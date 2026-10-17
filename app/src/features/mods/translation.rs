//! Translation of mod descriptions into the launcher language.
//!
//! Uses Google's free `client=gtx` web endpoint, which requires no API key.
//! Results are cached on disk (`platform::translation_cache`) keyed by the
//! project and a hash of the original text, so once a page has been
//! translated it keeps working offline and without hitting the translator
//! again until the provider description changes.

use std::{fmt, sync::Arc};

use gpui_kit::http_client::{AsyncBody, HttpClient, Request};
use serde_json::Value;

use super::catalog::ModProject;
use crate::features::settings::Locale;

const TRANSLATE_ENDPOINT: &str = "https://translate.googleapis.com/translate_a/single";

/// Maximum single-request length. Keeps the GET URL well within limits.
pub const MAX_CHUNK: usize = 1500;

#[derive(Debug, Clone)]
pub struct TranslationError {
    pub message: String,
}

impl fmt::Display for TranslationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// Translates `text` into `target`. English is a no-op: catalog texts are
/// already written in English, so the provider is never contacted.
pub async fn translate_text(
    client: Arc<dyn HttpClient>,
    text: &str,
    target: Locale,
) -> Result<String, TranslationError> {
    match target {
        Locale::En => Ok(text.to_owned()),
        Locale::Ru => {
            let prepared = normalize(text);
            if prepared.trim().is_empty() {
                return Ok(prepared);
            }
            let chunks = split_chunks(&prepared, MAX_CHUNK);
            let mut translated = String::new();
            for (index, chunk) in chunks.iter().enumerate() {
                if index > 0 {
                    translated.push('\n');
                }
                translated
                    .push_str(&translate_single_chunk(client.clone(), chunk, "en", "ru").await?);
            }
            let translated = translated.trim().to_owned();
            if translated.is_empty() {
                return Err(TranslationError {
                    message: "The translation service returned an empty text.".into(),
                });
            }
            Ok(translated)
        }
    }
}

/// Translates a mod card description, consulting the persistent cache first.
pub async fn translate_card_description(
    client: Arc<dyn HttpClient>,
    project: &ModProject,
    target: Locale,
) -> Result<String, TranslationError> {
    let original = normalize(&project.description);
    if original.trim().is_empty() {
        return Ok(project.description.clone());
    }
    let source = project.source.api_name();
    if let Some(cached) = crate::platform::translation_cache::read(source, target.tag(), &original)
    {
        return Ok(cached);
    }
    let translated = translate_text(client, &original, target).await?;
    crate::platform::translation_cache::write(source, target.tag(), &original, &translated);
    Ok(translated)
}

/// One GET request per chunk.
async fn translate_single_chunk(
    client: Arc<dyn HttpClient>,
    chunk: &str,
    from: &str,
    to: &str,
) -> Result<String, TranslationError> {
    let url = format!(
        "{TRANSLATE_ENDPOINT}?client=gtx&sl={from}&tl={to}&dt=t&ie=UTF-8&oe=UTF-8&q={}",
        encode_query_component(chunk)
    );
    let request = Request::builder()
        .uri(url)
        .header("User-Agent", "Mozilla/5.0 (rs-mc-launcher)")
        .body(AsyncBody::empty())
        .map_err(|error| TranslationError {
            message: format!("Could not prepare translation request: {error}"),
        })?;
    let response = client
        .send(request)
        .await
        .map_err(|error| TranslationError {
            message: format!("Translation request failed: {error}"),
        })?;
    let status = response.status();
    if !status.is_success() {
        return Err(TranslationError {
            message: format!("Translation service returned HTTP {status}"),
        });
    }
    let mut body = response.into_body();
    let mut bytes = Vec::new();
    futures::io::AsyncReadExt::read_to_end(&mut body, &mut bytes)
        .await
        .map_err(|error| TranslationError {
            message: format!("Could not read translation response: {error}"),
        })?;
    parse_response(&String::from_utf8_lossy(&bytes))
}

/// Extracts the joined translated segments from the gtx response.
///
/// The payload is a loosely structured array like
/// `[[["перевод","original",null,...],["...","..."]],null,"en",...]`; each
/// segment first element is a chunk of the translated text.
fn parse_response(body: &str) -> Result<String, TranslationError> {
    let value: Value = serde_json::from_str(body).map_err(|error| TranslationError {
        message: format!("Translation response is not valid JSON: {error}"),
    })?;
    let segments = value
        .get(0)
        .and_then(Value::as_array)
        .ok_or_else(|| TranslationError {
            message: "Translation response has no segments.".into(),
        })?;
    let mut text = String::new();
    for segment in segments {
        let Some(part) = segment.get(0).and_then(Value::as_str) else {
            continue;
        };
        text.push_str(part);
    }
    if text.trim().is_empty() {
        return Err(TranslationError {
            message: "Translation response is empty.".into(),
        });
    }
    Ok(text)
}

/// Percent-encodes a query value.
fn encode_query_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

/// Cleans up provider text before sending it to the translator: strips simple
/// markup and decodes common HTML entities that search snippets leak through.
pub fn normalize(text: &str) -> String {
    let stripped = {
        let mut out = String::with_capacity(text.len());
        let mut inside_tag = false;
        for ch in text.chars() {
            match ch {
                '<' => inside_tag = true,
                '>' => inside_tag = false,
                _ if !inside_tag => out.push(ch),
                _ => {}
            }
        }
        out
    };
    stripped
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Splits text along word boundaries into chunks no longer than `max`
/// characters.
///
/// A single uncommonly long token (a URL, for instance) is hard-split; the
/// chunks reassemble into the full text (joined with newlines).
pub fn split_chunks(text: &str, max: usize) -> Vec<String> {
    let mut chunks = Vec::<String>::new();
    let mut current = String::new();
    let mut current_len = 0usize;

    for word in text.split_whitespace() {
        for piece in split_long_word(word, max) {
            let piece_len = piece.chars().count();
            if current_len > 0 && current_len + 1 + piece_len > max {
                chunks.push(std::mem::take(&mut current));
                current_len = 0;
            }
            if current_len > 0 {
                current.push(' ');
                current_len += 1;
            }
            current.push_str(&piece);
            current_len += piece_len;
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    if chunks.is_empty() {
        vec![text.to_owned()]
    } else {
        chunks
    }
}

/// Splits one whitespace-free token if it exceeds `max` characters.
fn split_long_word(word: &str, max: usize) -> Vec<String> {
    if word.chars().count() <= max {
        return vec![word.to_owned()];
    }
    let mut parts = Vec::new();
    let mut rest = word.to_owned();
    while rest.chars().count() > max {
        let split = rest.char_indices().nth(max).map_or(rest.len(), |(i, _)| i);
        let head = rest.split_off(split);
        parts.push(std::mem::replace(&mut rest, head));
    }
    parts.push(rest);
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_never_exceed_the_limit_and_reassemble_the_text() {
        let text = "Adds new ores and crafted tools to the game. ".repeat(300);
        let text = text.trim();
        let chunks = split_chunks(text, 1500);
        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(
                chunk.chars().count() <= 1500,
                "chunk of {} chars",
                chunk.chars().count()
            );
        }
        let joined = join_with_newlines(&chunks);
        for word in ["ores", "crafted", "game."] {
            assert!(joined.contains(word), "lost word {word}");
        }
    }

    #[test]
    fn short_text_stays_in_one_chunk() {
        let chunks = split_chunks("Mod adds new ores and tools.", MAX_CHUNK);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Mod adds new ores and tools.");
    }

    #[test]
    fn long_single_tokens_are_hard_split() {
        let token = "a".repeat(4000);
        let chunks = split_chunks(&token, 1500);
        assert!(chunks.iter().all(|c| c.chars().count() <= 1500));
        assert_eq!(join_with_newlines(&chunks).replace('\n', ""), token);
    }

    fn join_with_newlines(chunks: &[String]) -> String {
        chunks.join("\n")
    }

    #[test]
    fn html_entities_are_normalized_and_tags_stripped() {
        assert_eq!(normalize("<b>Cat &amp; Mouse</b>"), "Cat & Mouse");
        assert_eq!(normalize("A &#39;quoted&#39; line"), "A 'quoted' line");
        assert!(!normalize("<li>Everything</li>").contains('<'));
    }

    #[test]
    fn response_segments_are_joined() {
        let body = serde_json::to_string(&serde_json::json!([
            [
                ["Первая часть", "first", null, null, 1],
                [", вторая", ", second", null, null, 2]
            ],
            null,
            "en"
        ]))
        .unwrap();
        assert_eq!(parse_response(&body).unwrap(), "Первая часть, вторая");
    }

    #[test]
    fn empty_and_broken_responses_error_out() {
        let body = serde_json::to_string(&serde_json::json!([[], null])).unwrap();
        assert!(parse_response(&body).is_err());
        let body = serde_json::to_string(&serde_json::json!("not an array")).unwrap();
        assert!(parse_response(&body).is_err());
    }
}
