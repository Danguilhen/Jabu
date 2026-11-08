//! Secret storage for credentials.
//!
//! Desktop builds use the OS keyring. When no keyring service is available
//! (headless session, locked keyring, unsupported platform) credentials fall
//! back to the SQLite settings table so sign-in still works. The fallback is
//! explicit in the log and in `docs/STATUS.md`; it is not silent encryption.
//!
//! A secret is never left in both places: writing to the keyring clears any
//! older plaintext copy, and a fallback copy found while the keyring works is
//! migrated into the keyring and removed.

use std::sync::OnceLock;
use std::time::Duration;

use crate::StorageBackend;

/// Keyring calls can block on a locked Secret Service. The app must still
/// start, so every keyring operation is bounded and falls back on timeout.
const KEYRING_TIMEOUT: Duration = Duration::from_secs(2);

fn with_timeout<T, F>(operation: F) -> Option<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .spawn(move || {
            let _ = sender.send(operation());
        })
        .ok()?;
    receiver.recv_timeout(KEYRING_TIMEOUT).ok()
}

/// Service name shown by the desktop keyring UI.
const SERVICE: &str = "jabu";

fn fallback_key(key: &str) -> String {
    format!("secret:{key}")
}

/// Install an empty in-memory keyring for the current test thread.
#[cfg(test)]
#[allow(clippy::redundant_pub_crate)]
pub(crate) fn install_test_keyring() {
    fake::install();
}

/// Remove the in-memory keyring, simulating a locked or unavailable service.
#[cfg(test)]
#[allow(clippy::redundant_pub_crate)]
pub(crate) fn clear_test_keyring() {
    fake::remove();
}

/// Whether the OS keyring can be used for this process.
///
/// Tests use an explicit in-memory keyring when installed, and the SQLite
/// fallback otherwise, so they never touch the developer's real keyring.
fn keyring_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    #[cfg(test)]
    if fake::active() {
        return true;
    }
    *AVAILABLE.get_or_init(|| {
        if cfg!(test) && std::env::var_os("JABU_REAL_KEYRING").is_none() {
            return false;
        }
        with_timeout(keyring_probe).unwrap_or_else(|| {
            tracing::warn!("OS keyring did not respond; secrets fall back to SQLite");
            false
        })
    })
}

/// Whether the OS keyring can be used for this process.
pub fn is_available() -> bool {
    keyring_available()
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
fn native_get(key: &str) -> Option<String> {
    let owned_key = key.to_string();
    let label = key.to_string();
    match with_timeout(move || {
        keyring::Entry::new(SERVICE, &owned_key).and_then(|entry| entry.get_password())
    }) {
        Some(Ok(value)) => Some(value),
        Some(Err(keyring::Error::NoEntry)) => None,
        Some(Err(error)) => {
            tracing::warn!("Could not read {label} from the OS keyring: {error}");
            None
        }
        None => {
            tracing::warn!("OS keyring did not respond while reading {label}");
            None
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn native_get(_key: &str) -> Option<String> {
    None
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn native_set(key: &str, value: &str) -> Result<(), String> {
    let owned_key = key.to_string();
    let owned_value = value.to_string();
    with_timeout(move || {
        keyring::Entry::new(SERVICE, &owned_key)
            .and_then(|entry| entry.set_password(&owned_value))
            .map_err(|error| error.to_string())
    })
    .unwrap_or_else(|| Err("OS keyring did not respond".to_string()))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn native_set(_key: &str, _value: &str) -> Result<(), String> {
    Err("no OS keyring on this platform".to_string())
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn native_delete(key: &str) -> Result<(), String> {
    let owned_key = key.to_string();
    with_timeout(move || {
        keyring::Entry::new(SERVICE, &owned_key)
            .and_then(|entry| entry.delete_credential())
            .map_err(|error| error.to_string())
    })
    .unwrap_or_else(|| Err("OS keyring did not respond".to_string()))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn native_delete(_key: &str) -> Result<(), String> {
    Ok(())
}

fn keyring_get(key: &str) -> Option<String> {
    #[cfg(test)]
    if let Some(value) = fake::get(key) {
        return value;
    }
    native_get(key)
}

fn keyring_set(key: &str, value: &str) -> Result<(), String> {
    #[cfg(test)]
    if fake::set(key, value) {
        return Ok(());
    }
    native_set(key, value)
}

fn keyring_delete(key: &str) -> Result<(), String> {
    #[cfg(test)]
    if fake::delete(key) {
        return Ok(());
    }
    native_delete(key)
}

/// Save only to the OS keyring; used for encryption keys that must not fall
/// back to SQLite.
///
/// # Errors
///
/// Returns an error when no keyring is available or it rejects the write.
pub fn keyring_save(key: &str, value: &str) -> Result<(), String> {
    keyring_set(key, value)
}

/// Load only from the OS keyring.
pub fn keyring_load(key: &str) -> Option<String> {
    keyring_get(key)
}

fn clear_fallback(storage: &dyn StorageBackend, key: &str) {
    let _ = storage.set_setting(&fallback_key(key), &serde_json::Value::Null);
}

fn load_fallback(storage: &dyn StorageBackend, key: &str) -> Option<String> {
    storage
        .get_setting(&fallback_key(key))
        .ok()
        .flatten()
        .and_then(|value| value.as_str().map(str::to_string))
}

/// Store a secret, preferring the OS keyring.
///
/// # Errors
///
/// Returns an error when neither the keyring nor the fallback store accepts the
/// value.
pub fn save(storage: &dyn StorageBackend, key: &str, value: &str) -> anyhow::Result<()> {
    if keyring_available() {
        match keyring_set(key, value) {
            Ok(()) => {
                // Never keep an older plaintext copy next to the keyring value.
                clear_fallback(storage, key);
                return Ok(());
            }
            Err(error) => {
                tracing::warn!("Could not write {key} to the OS keyring: {error}");
                // An older keyring value must not shadow the new fallback.
                let _ = keyring_delete(key);
            }
        }
    }
    storage.set_setting(
        &fallback_key(key),
        &serde_json::Value::String(value.to_string()),
    )
}

/// Load a secret, preferring the OS keyring.
///
/// When the keyring is available but empty, a fallback copy is migrated into it
/// and removed from SQLite.
pub fn load(storage: &dyn StorageBackend, key: &str) -> Option<String> {
    if keyring_available() {
        if let Some(value) = keyring_get(key) {
            clear_fallback(storage, key);
            return Some(value);
        }
    }
    let value = load_fallback(storage, key)?;
    if keyring_available() && keyring_set(key, &value).is_ok() {
        clear_fallback(storage, key);
    }
    Some(value)
}

/// Delete a secret from both the keyring and the fallback store.
///
/// # Errors
///
/// Returns an error when the fallback store cannot be updated.
pub fn delete(storage: &dyn StorageBackend, key: &str) -> anyhow::Result<()> {
    if keyring_available() {
        if let Err(error) = keyring_delete(key) {
            tracing::warn!("Could not delete {key} from the OS keyring: {error}");
        }
    }
    storage.set_setting(&fallback_key(key), &serde_json::Value::Null)
}

/// In-memory keyring used by unit tests on the current thread.
///
/// It is thread-local so parallel tests never see each other's secrets.
#[cfg(test)]
mod fake {
    use std::cell::RefCell;
    use std::collections::HashMap;

    thread_local! {
        static MAP: RefCell<Option<HashMap<String, String>>> = const { RefCell::new(None) };
    }

    pub(super) fn install() {
        MAP.with(|map| *map.borrow_mut() = Some(HashMap::new()));
    }

    pub(super) fn remove() {
        MAP.with(|map| *map.borrow_mut() = None);
    }

    pub(super) fn active() -> bool {
        MAP.with(|map| map.borrow().is_some())
    }

    #[allow(clippy::option_option)]
    pub(super) fn get(key: &str) -> Option<Option<String>> {
        MAP.with(|map| map.borrow().as_ref().map(|values| values.get(key).cloned()))
    }

    pub(super) fn set(key: &str, value: &str) -> bool {
        MAP.with(|map| {
            map.borrow_mut().as_mut().is_some_and(|values| {
                values.insert(key.to_string(), value.to_string());
                true
            })
        })
    }

    pub(super) fn delete(key: &str) -> bool {
        MAP.with(|map| {
            map.borrow_mut().as_mut().is_some_and(|values| {
                values.remove(key);
                true
            })
        })
    }
}
