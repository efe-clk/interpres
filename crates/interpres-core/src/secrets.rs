//! API keys stored in the OS credential store (Secret Service, Windows Credential
//! Manager, Keychain). Keys never touch the settings file or logs.

use crate::EngineId;

const SERVICE: &str = "interpres";

#[derive(Debug, thiserror::Error)]
#[error("credential store error: {0}")]
pub struct SecretError(#[from] keyring::Error);

fn entry(engine: EngineId) -> Result<keyring::Entry, SecretError> {
    Ok(keyring::Entry::new(SERVICE, engine.as_str())?)
}

/// Returns the stored key, or `None` if none is stored.
pub fn get_key(engine: EngineId) -> Result<Option<String>, SecretError> {
    match entry(engine)?.get_password() {
        Ok(k) => Ok(Some(k)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn set_key(engine: EngineId, key: &str) -> Result<(), SecretError> {
    Ok(entry(engine)?.set_password(key)?)
}

/// Deletes the stored key. Deleting a missing key is not an error.
pub fn delete_key(engine: EngineId) -> Result<(), SecretError> {
    match entry(engine)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}
