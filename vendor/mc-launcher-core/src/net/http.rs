//! Blocking HTTP helpers with the crate user agent.

use std::time::Duration;

use once_cell::sync::OnceCell;
use reqwest::blocking::Client;

use crate::Result;

/// Returns the user agent used by crate-managed HTTP requests.
pub fn user_agent() -> String {
    format!("mc-launcher-core/{}", env!("CARGO_PKG_VERSION"))
}

/// Process-wide shared blocking client.
///
/// Installs issue thousands of small requests (asset objects, libraries) in
/// quick succession. Reusing one client keeps pooled connections and TLS
/// sessions warm instead of paying fresh handshakes for every request.
/// Cloning the handle is cheap and shares the same connection pool.
static CLIENT: OnceCell<Client> = OnceCell::new();

/// Builds the shared client with sane connection defaults.
fn build_client() -> Result<Client> {
    Ok(Client::builder()
        .user_agent(user_agent())
        .connect_timeout(Duration::from_secs(15))
        .tcp_keepalive(Duration::from_secs(15))
        .build()?)
}

/// Returns the shared blocking reqwest client.
///
/// The first caller builds the client; concurrent first callers may each
/// build one, and the first finished client is kept.
///
/// # Errors
///
/// Returns [`crate::LauncherError`] if the client cannot be constructed.
pub fn client() -> Result<Client> {
    if let Some(client) = CLIENT.get() {
        return Ok(client.clone());
    }
    let client = build_client()?;
    let _ = CLIENT.set(client.clone());
    Ok(client)
}

/// Fetches a URL as text.
///
/// # Errors
///
/// Returns [`crate::LauncherError`] if the request fails, returns an error
/// status, or the body cannot be decoded as text.
pub fn get_text(url: &str) -> Result<String> {
    Ok(client()?.get(url).send()?.error_for_status()?.text()?)
}

/// Fetches a URL and decodes the JSON body.
///
/// # Errors
///
/// Returns [`crate::LauncherError`] if the request fails, returns an error
/// status, or the body cannot be decoded as `T`.
pub fn get_json<T>(url: &str) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    Ok(client()?.get(url).send()?.error_for_status()?.json()?)
}
