//! Secrets in the OS credential store (Windows Credential Manager, macOS
//! Keychain, Linux kernel keyring). Account tokens never go to files.

const SERVICE: &str = "OrbMC";

#[derive(Debug, thiserror::Error)]
#[error("Хранилище паролей ОС недоступно: {0}")]
pub struct CredentialError(#[from] keyring::Error);

pub fn store(key: &str, secret: &str) -> Result<(), CredentialError> {
    Ok(keyring::Entry::new(SERVICE, key)?.set_password(secret)?)
}

/// The stored secret, or `None` when there is none.
pub fn load(key: &str) -> Result<Option<String>, CredentialError> {
    match keyring::Entry::new(SERVICE, key)?.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Removes the secret; a missing one is not an error.
pub fn delete(key: &str) -> Result<(), CredentialError> {
    match keyring::Entry::new(SERVICE, key)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error.into()),
    }
}
