//! Secret storage for credentials.
//!
//! Desktop builds use the OS keyring. When no keyring service is available
//! (headless session, locked keyring, unsupported platform) the value falls
//! back to the SQLite settings table so sign-in still works. The fallback is
//! explicit in the log and in `docs/STATUS.md`; it is not silent encryption.

use std::sync::OnceLock;

use crate::StorageBackend;

/// Service name shown by the desktop keyring UI.
const SERVICE: &str = "jabu";

fn fallback_key(key: &str) -> String {
    format!("secret:{key}")
}

/// Whether the OS keyring can be used for this process.
///
/// Tests always use the settings fallback so they never touch the developer's
/// real keyring. A failed probe means the service is unavailable, not that the
/// requested secret is missing.
fn keyring_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        if cfg!(test) {
            return false;
        }
        keyring_probe()
    })
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn keyring_probe() -> bool {
    match keyring::Entry::new(SERVICE, "__jabu_probe__") {
        Ok(entry) => match entry.get_password() {
            Ok(_) | Err(keyring::Error::NoEntry) => true,
            Err(error) => {
                tracing::warn!("OS keyring unavailable; secrets fall back to SQLite: {error}");
                false
            }
        },
        Err(error) => {
            tracing::warn!("OS keyring unavailable; secrets fall back to SQLite: {error}");
            false
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
const fn keyring_probe() -> bool {
    false
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn keyring_entry(key: &str) -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new(SERVICE, key)
}

/// Store a secret, preferring the OS keyring.
pub(crate) fn save(storage: &dyn StorageBackend, key: &str, value: &str) -> anyhow::Result<()> {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    if keyring_available() {
        match keyring_entry(key).and_then(|entry| entry.set_password(value)) {
            Ok(()) => return Ok(()),
            Err(error) => {
                tracing::warn!("Could not write {key} to the OS keyring: {error}");
            }
        }
    }
    storage.set_setting(
        &fallback_key(key),
        &serde_json::Value::String(value.to_string()),
    )
}

/// Load a secret, preferring the OS keyring.
pub(crate) fn load(storage: &dyn StorageBackend, key: &str) -> Option<String> {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    if keyring_available() {
        match keyring_entry(key).and_then(|entry| entry.get_password()) {
            Ok(value) => return Some(value),
            Err(keyring::Error::NoEntry) => {}
            Err(error) => {
                tracing::warn!("Could not read {key} from the OS keyring: {error}");
            }
        }
    }
    storage
        .get_setting(&fallback_key(key))
        .ok()
        .flatten()
        .and_then(|value| value.as_str().map(str::to_string))
}

/// Delete a secret from both the keyring and the fallback store.
pub(crate) fn delete(storage: &dyn StorageBackend, key: &str) -> anyhow::Result<()> {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    if keyring_available() {
        if let Ok(entry) = keyring_entry(key) {
            let _ = entry.delete_credential();
        }
    }
    storage.set_setting(&fallback_key(key), &serde_json::Value::Null)
}
