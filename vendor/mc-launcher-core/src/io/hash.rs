//! Hashing helpers for installed files.

use std::{
    fs::File,
    io::{self, BufReader, Read},
    path::Path,
};

use sha1::{Digest as Sha1Digest, Sha1};
use sha2::{Digest as Sha2Digest, Sha256};

use crate::Result;

/// Calculates the SHA-1 digest of a file as lowercase hexadecimal.
///
/// # Errors
///
/// Returns [`crate::LauncherError`] if the file cannot be read.
pub fn sha1_file(path: impl AsRef<Path>) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha1::new();
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(hex(&Sha1Digest::finalize_reset(&mut hasher)))
}

/// Digest algorithm supported while streaming bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashAlgorithm {
    /// SHA-1, used by Mojang version manifests and asset indexes.
    Sha1,
    /// SHA-256, used by modpack manifests.
    Sha256,
}

/// Reader wrapper that hashes bytes as they pass through.
///
/// The digest is computed during the read itself, so a downloaded file does
/// not need a second pass over the disk to validate its checksum.
pub struct HashingReader<R> {
    inner: R,
    hasher: Option<Hasher>,
    bytes_read: u64,
}

enum Hasher {
    Sha1(Sha1),
    Sha256(Sha256),
}

impl Hasher {
    fn new(algorithm: HashAlgorithm) -> Self {
        match algorithm {
            HashAlgorithm::Sha1 => Self::Sha1(Sha1::new()),
            HashAlgorithm::Sha256 => Self::Sha256(Sha256::new()),
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        match self {
            Self::Sha1(hasher) => hasher.update(bytes),
            Self::Sha256(hasher) => hasher.update(bytes),
        }
    }

    fn finish_hex(&mut self) -> String {
        match self {
            Self::Sha1(hasher) => hex(&Sha1Digest::finalize_reset(hasher)),
            Self::Sha256(hasher) => hex(&Sha2Digest::finalize_reset(hasher)),
        }
    }
}

fn hex(digest: &[u8]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

impl<R> HashingReader<R> {
    /// Wraps `inner`, computing `algorithm` over the bytes read.
    pub fn new(inner: R, algorithm: Option<HashAlgorithm>) -> Self {
        Self {
            inner,
            hasher: algorithm.map(Hasher::new),
            bytes_read: 0,
        }
    }

    /// Bytes read through the wrapper so far.
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    /// Finishes hashing, returning the bytes read and the lowercase
    /// hexadecimal digest, if an algorithm was requested.
    pub fn finish(&mut self) -> (u64, Option<String>) {
        let digest = self.hasher.as_mut().map(Hasher::finish_hex);
        (self.bytes_read, digest)
    }
}

impl<R: Read> Read for HashingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read = self.inner.read(buf)?;
        if let Some(hasher) = &mut self.hasher {
            hasher.update(&buf[..read]);
        }
        self.bytes_read += read as u64;
        Ok(read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn hashing_reader_computes_sha1_while_copying() {
        let mut reader = HashingReader::new(Cursor::new(b"minecraft"), Some(HashAlgorithm::Sha1));
        let mut output = Vec::new();
        io::copy(&mut reader, &mut output).unwrap();

        let (bytes, digest) = reader.finish();
        assert_eq!(bytes, 9);
        assert_eq!(output, b"minecraft");
        assert_eq!(
            digest.as_deref(),
            Some("624c22a8c8f8c93f18fe5ecd4713100c8d754507")
        );
    }

    #[test]
    fn hashing_reader_computes_sha256_while_copying() {
        let mut reader = HashingReader::new(Cursor::new(b"minecraft"), Some(HashAlgorithm::Sha256));
        let mut output = Vec::new();
        io::copy(&mut reader, &mut output).unwrap();

        let (bytes, digest) = reader.finish();
        assert_eq!(bytes, 9);
        assert_eq!(
            digest.as_deref(),
            Some("9970626666560a32465d4ce10d28f3233365af833e15eed59884d9477862c379")
        );
    }

    #[test]
    fn hashing_reader_counts_bytes_without_an_algorithm() {
        let mut reader = HashingReader::new(Cursor::new(b"minecraft"), None);
        let mut output = Vec::new();
        io::copy(&mut reader, &mut output).unwrap();

        let (bytes, digest) = reader.finish();
        assert_eq!(bytes, 9);
        assert_eq!(digest, None);
    }
}
