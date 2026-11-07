//! Mobile platform implementation (Android/iOS)

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use xmpp_core::events::{EventBus, XmppEvent};

use crate::{
    BackgroundService, Notification, NotificationPermission, NotificationService, PlatformStorage,
};

/// Mobile notifications using FCM/APNs
pub struct MobileNotifications {
    permission: NotificationPermission,
    fcm_token: Option<String>,
    apns_token: Option<String>,
}

impl MobileNotifications {
    pub fn new() -> Result<Self> {
        Ok(Self {
            permission: NotificationPermission::Default,
            fcm_token: None,
            apns_token: None,
        })
    }

    pub fn register_fcm(&mut self, token: String) {
        self.fcm_token = Some(token);
    }

    pub fn register_apns(&mut self, token: String) {
        self.apns_token = Some(token);
    }
}

impl NotificationService for MobileNotifications {
    fn request_permission(&self) -> Result<NotificationPermission> {
        // Request permission via platform APIs
        Ok(NotificationPermission::Default)
    }

    fn show_notification(&self, notification: &Notification) -> Result<()> {
        // Show local notification or forward to push service
        tracing::info!(
            "Showing mobile notification: {} - {}",
            notification.title,
            notification.body
        );
        Ok(())
    }

    fn close_notification(&self, id: &str) -> Result<()> {
        Ok(())
    }

    fn get_permission(&self) -> NotificationPermission {
        self.permission
    }
}

/// Mobile storage using app-specific directories
pub struct MobileStorage {
    data_dir: PathBuf,
    cache_dir: PathBuf,
    config_dir: PathBuf,
}

impl MobileStorage {
    pub fn new() -> Result<Self> {
        // On mobile, use app-specific directories. Android does not expose
        // XDG paths, so ask the activity for its files/cache directories.
        let data_root = mobile_data_root()?;
        let cache_root = mobile_cache_root().unwrap_or_else(|| data_root.join("cache"));
        let data_dir = data_root.join("jabu");
        let cache_dir = cache_root.join("jabu");
        let config_dir = data_root.join("jabu-config");

        std::fs::create_dir_all(&data_dir)?;
        std::fs::create_dir_all(&cache_dir)?;
        std::fs::create_dir_all(&config_dir)?;

        Ok(Self {
            data_dir,
            cache_dir,
            config_dir,
        })
    }
}

fn mobile_data_root() -> Result<PathBuf> {
    #[cfg(target_os = "android")]
    if let Some(path) = android_dir("getFilesDir") {
        return Ok(path);
    }
    dirs::data_dir().ok_or_else(|| anyhow::anyhow!("Could not find data directory"))
}

fn mobile_cache_root() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    if let Some(path) = android_dir("getCacheDir") {
        return Some(path);
    }
    dirs::cache_dir()
}

#[cfg(target_os = "android")]
fn android_dir(method: &str) -> Option<PathBuf> {
    std::panic::catch_unwind(|| {
        use jni::objects::{JObject, JString};
        use jni::JavaVM;

        let context = ndk_context::android_context();
        // SAFETY: android-activity owns these handles for the process lifetime.
        let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }.ok()?;
        let mut env = vm.attach_current_thread().ok()?;
        let activity = unsafe { JObject::from_raw(context.context().cast()) };
        let directory = env
            .call_method(activity, method, "()Ljava/io/File;", &[])
            .ok()?
            .l()
            .ok()?;
        let path = env
            .call_method(directory, "getAbsolutePath", "()Ljava/lang/String;", &[])
            .ok()?
            .l()
            .ok()?;
        let path: String = env
            .get_string(&JString::from(path))
            .ok()?
            .to_string_lossy()
            .into_owned();
        Some(PathBuf::from(path))
    })
    .ok()
    .flatten()
}

impl PlatformStorage for MobileStorage {
    fn get_data_dir(&self) -> Result<PathBuf> {
        Ok(self.data_dir.clone())
    }

    fn get_cache_dir(&self) -> Result<PathBuf> {
        Ok(self.cache_dir.clone())
    }

    fn get_config_dir(&self) -> Result<PathBuf> {
        Ok(self.config_dir.clone())
    }
}

/// Mobile background service using WorkManager (Android) / BGAppRefresh (iOS)
pub struct MobileBackground {
    events: Arc<EventBus>,
    sync_handle: Option<tokio::task::JoinHandle<()>>,
}

impl MobileBackground {
    pub fn new(events: Arc<EventBus>) -> Result<Self> {
        Ok(Self {
            events,
            sync_handle: None,
        })
    }
}

impl BackgroundService for MobileBackground {
    fn start(&self) -> Result<()> {
        // Register background tasks with platform
        #[cfg(target_os = "android")]
        {
            // Register WorkManager periodic task
        }

        #[cfg(target_os = "ios")]
        {
            // Register BGAppRefresh task
        }

        Ok(())
    }

    fn stop(&self) -> Result<()> {
        // Cancel background tasks
        Ok(())
    }

    fn schedule_sync(&self, interval: std::time::Duration) -> Result<()> {
        // Schedule periodic background sync
        Ok(())
    }

    fn cancel_sync(&self) -> Result<()> {
        Ok(())
    }
}

/// Biometric authentication
pub struct BiometricAuth;

impl BiometricAuth {
    pub fn new() -> Self {
        Self
    }

    pub async fn authenticate(&self, reason: &str) -> Result<bool> {
        // Use platform biometric APIs (FaceID, TouchID, Fingerprint, Face Unlock)
        #[cfg(target_os = "ios")]
        {
            // Use LocalAuthentication framework
        }

        #[cfg(target_os = "android")]
        {
            // Use BiometricPrompt API
        }

        Ok(true)
    }

    pub async fn is_available(&self) -> Result<bool> {
        // Check if biometric hardware is available and enrolled
        Ok(true)
    }
}

/// Secure storage for keys (Keychain/Keystore)
pub struct SecureStorage;

impl SecureStorage {
    pub fn new() -> Self {
        Self
    }

    pub fn set(&self, key: &str, value: &[u8]) -> Result<()> {
        // Store in Keychain (iOS) / Keystore (Android)
        Ok(())
    }

    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        // Retrieve from Keychain/Keystore
        Ok(None)
    }

    pub fn delete(&self, key: &str) -> Result<()> {
        // Delete from Keychain/Keystore
        Ok(())
    }

    pub fn exists(&self, key: &str) -> Result<bool> {
        Ok(false)
    }
}

/// Deep link handling
pub struct DeepLinkHandler;

impl DeepLinkHandler {
    pub fn new() -> Self {
        Self
    }

    pub fn handle_url(&self, url: &str) -> Result<DeepLinkAction> {
        // Parse xmpp: URLs, invite links, etc.
        if url.starts_with("xmpp:") {
            // Handle XMPP URI
            return Ok(DeepLinkAction::OpenChat {
                jid: url[5..].to_string(),
            });
        }

        Ok(DeepLinkAction::None)
    }
}

#[derive(Debug, Clone)]
pub enum DeepLinkAction {
    None,
    OpenChat {
        jid: String,
    },
    JoinRoom {
        jid: String,
        nick: String,
        password: Option<String>,
    },
    AddContact {
        jid: String,
    },
    OpenSettings,
}

/// Share extension handling
pub struct ShareExtension;

impl ShareExtension {
    pub fn new() -> Self {
        Self
    }

    pub fn handle_shared_content(&self, content: SharedContent) -> Result<()> {
        // Handle content shared from other apps
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SharedContent {
    pub text: Option<String>,
    pub urls: Vec<String>,
    pub images: Vec<PathBuf>,
    pub files: Vec<PathBuf>,
}
