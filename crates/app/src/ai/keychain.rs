//! The OS keychain behind the AI keys (#1945): macOS Keychain, Windows
//! Credential Manager, or the freedesktop Secret Service, through the
//! `keyring` crate. Native only. A missing entry is `Ok(None)`; any
//! other failure is named (never with the key in it).
//!
//! Provenance: the `keyring` crate's public API documentation.

use super::secret::{ApiKey, KEYCHAIN_SERVICE, KeyName};

fn entry(name: KeyName) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, name.account())
        .map_err(|e| format!("the OS keychain is not available ({e})"))
}

/// Read the stored key for `name`, if any.
pub fn load(name: KeyName) -> Result<Option<ApiKey>, String> {
    match entry(name)?.get_password() {
        Ok(text) => ApiKey::new(&text).map(Some).map_err(|e| format!("keychain {} key: {e}", name.label())),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("the {} key could not be read from the keychain ({e})", name.label())),
    }
}

/// Store `key` for `name`, replacing any earlier one.
pub fn store(name: KeyName, key: &ApiKey) -> Result<(), String> {
    entry(name)?
        .set_password(key.expose())
        .map_err(|e| format!("the {} key could not be stored in the keychain ({e})", name.label()))
}

/// Remove the stored key for `name`; removing a missing one is fine.
pub fn forget(name: KeyName) -> Result<(), String> {
    match entry(name)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("the {} key could not be removed from the keychain ({e})", name.label())),
    }
}
