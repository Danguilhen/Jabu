//! Mobile platform implementation (Android/iOS)

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use xmpp_core::events::EventBus;

use crate::{
    BackgroundService, Notification, NotificationPermission, NotificationService, PlatformStorage,
};

/// Mobile notifications using the Android notification service.
pub struct MobileNotifications {
    fcm_token: Option<String>,
    apns_token: Option<String>,
}

impl MobileNotifications {
    pub fn new() -> Result<Self> {
        #[cfg(target_os = "android")]
        if let Err(error) = android::ensure_channel() {
            tracing::warn!(%error, "could not create the Android notification channel");
        }
        Ok(Self {
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
        #[cfg(target_os = "android")]
        return android::request_permission();
        #[cfg(target_os = "ios")]
        return Ok(NotificationPermission::Granted);
    }

    fn show_notification(&self, notification: &Notification) -> Result<()> {
        #[cfg(target_os = "android")]
        return android::show(notification);
        #[cfg(target_os = "ios")]
        {
            tracing::info!(
                "Showing mobile notification: {} - {}",
                notification.title,
                notification.body
            );
            Ok(())
        }
    }

    fn close_notification(&self, id: &str) -> Result<()> {
        #[cfg(target_os = "android")]
        return android::close(id);
        #[cfg(target_os = "ios")]
        {
            let _ = id;
            Ok(())
        }
    }

    fn get_permission(&self) -> NotificationPermission {
        #[cfg(target_os = "android")]
        return android::permission();
        #[cfg(target_os = "ios")]
        return NotificationPermission::Granted;
    }
}

#[cfg(target_os = "android")]
mod android {
    use std::panic::AssertUnwindSafe;

    use anyhow::{anyhow, Context, Result};
    use jni::objects::{JObject, JString, JValue};
    use jni::JavaVM;

    use crate::{Notification, NotificationPermission};

    const CHANNEL_ID: &str = "jabu_messages";

    fn with_env<T>(
        operation: impl FnOnce(&mut jni::JNIEnv<'_>, &JObject<'_>) -> Result<T>,
    ) -> Result<T> {
        std::panic::catch_unwind(AssertUnwindSafe(|| {
            let context = ndk_context::android_context();
            // SAFETY: android-activity owns these handles for the process lifetime.
            let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }
                .context("could not get the Android VM")?;
            let mut env = vm
                .attach_current_thread()
                .context("could not attach the Android thread")?;
            let activity = unsafe { JObject::from_raw(context.context().cast()) };
            operation(&mut env, &activity)
        }))
        .map_err(|_| anyhow!("Android notification JNI call panicked"))?
    }

    fn sdk_int(env: &mut jni::JNIEnv<'_>) -> Result<i32> {
        Ok(env
            .get_static_field("android/os/Build$VERSION", "SDK_INT", "I")
            .context("could not read the Android SDK level")?
            .i()?)
    }

    fn notification_manager<'local>(
        env: &mut jni::JNIEnv<'local>,
        activity: &JObject<'_>,
    ) -> Result<JObject<'local>> {
        let service = env.new_string("notification")?;
        Ok(env
            .call_method(
                activity,
                "getSystemService",
                "(Ljava/lang/String;)Ljava/lang/Object;",
                &[JValue::Object(&service)],
            )?
            .l()?)
    }

    fn channel_id<'local>(env: &mut jni::JNIEnv<'local>) -> Result<JString<'local>> {
        Ok(env.new_string(CHANNEL_ID)?)
    }

    pub(super) fn ensure_channel() -> Result<()> {
        with_env(|env, activity| {
            if sdk_int(env)? < 26 {
                return Ok(());
            }
            let id = channel_id(env)?;
            let name = env.new_string("Messages")?;
            let channel = env.new_object(
                "android/app/NotificationChannel",
                "(Ljava/lang/String;Ljava/lang/CharSequence;I)V",
                &[JValue::Object(&id), JValue::Object(&name), JValue::Int(3)],
            )?;
            let manager = notification_manager(env, activity)?;
            env.call_method(
                &manager,
                "createNotificationChannel",
                "(Landroid/app/NotificationChannel;)V",
                &[JValue::Object(&channel)],
            )?;
            Ok(())
        })
    }

    pub(super) fn permission() -> NotificationPermission {
        with_env(|env, activity| {
            if sdk_int(env)? < 33 {
                return Ok(NotificationPermission::Granted);
            }
            let permission = env.new_string("android.permission.POST_NOTIFICATIONS")?;
            let granted = env
                .call_method(
                    activity,
                    "checkSelfPermission",
                    "(Ljava/lang/String;)I",
                    &[JValue::Object(&permission)],
                )?
                .i()?
                == 0;
            Ok(if granted {
                NotificationPermission::Granted
            } else {
                NotificationPermission::Denied
            })
        })
        .unwrap_or(NotificationPermission::Denied)
    }

    pub(super) fn request_permission() -> Result<NotificationPermission> {
        with_env(|env, activity| {
            if sdk_int(env)? < 33 {
                return Ok(NotificationPermission::Granted);
            }
            let permission = env.new_string("android.permission.POST_NOTIFICATIONS")?;
            let granted = env
                .call_method(
                    activity,
                    "checkSelfPermission",
                    "(Ljava/lang/String;)I",
                    &[JValue::Object(&permission)],
                )?
                .i()?
                == 0;
            if granted {
                return Ok(NotificationPermission::Granted);
            }
            let permissions = env.new_object_array(1, "java/lang/String", JObject::null())?;
            env.set_object_array_element(&permissions, 0, &permission)?;
            env.call_method(
                activity,
                "requestPermissions",
                "([Ljava/lang/String;I)V",
                &[JValue::Object(&permissions), JValue::Int(42)],
            )?;
            Ok(NotificationPermission::Default)
        })
    }

    pub(super) fn show(notification: &Notification) -> Result<()> {
        if permission() != NotificationPermission::Granted {
            return Err(anyhow!("Android notification permission is not granted"));
        }
        with_env(|env, activity| {
            let id = channel_id(env)?;
            let sdk = sdk_int(env)?;
            let builder = if sdk >= 26 {
                env.new_object(
                    "android/app/Notification$Builder",
                    "(Landroid/content/Context;Ljava/lang/String;)V",
                    &[JValue::Object(activity), JValue::Object(&id)],
                )?
            } else {
                env.new_object(
                    "android/app/Notification$Builder",
                    "(Landroid/content/Context;)V",
                    &[JValue::Object(activity)],
                )?
            };

            let title = env.new_string(&notification.title)?;
            let body = env.new_string(&notification.body)?;
            let category = env.new_string("msg")?;
            let app_info = env
                .call_method(
                    activity,
                    "getApplicationInfo",
                    "()Landroid/content/pm/ApplicationInfo;",
                    &[],
                )?
                .l()?;
            let icon = env.get_field(&app_info, "icon", "I")?.i()?;

            let class = "android/app/Notification$Builder";
            env.call_method(
                &builder,
                "setSmallIcon",
                format!("(I)L{class};"),
                &[JValue::Int(icon)],
            )?;
            env.call_method(
                &builder,
                "setContentTitle",
                format!("(Ljava/lang/CharSequence;)L{class};"),
                &[JValue::Object(&title)],
            )?;
            env.call_method(
                &builder,
                "setContentText",
                format!("(Ljava/lang/CharSequence;)L{class};"),
                &[JValue::Object(&body)],
            )?;
            env.call_method(
                &builder,
                "setCategory",
                format!("(Ljava/lang/String;)L{class};"),
                &[JValue::Object(&category)],
            )?;
            env.call_method(
                &builder,
                "setAutoCancel",
                format!("(Z)L{class};"),
                &[JValue::Bool(1)],
            )?;
            env.call_method(
                &builder,
                "setShowWhen",
                format!("(Z)L{class};"),
                &[JValue::Bool(1)],
            )?;
            env.call_method(
                &builder,
                "setWhen",
                format!("(J)L{class};"),
                &[JValue::Long(notification.timestamp.timestamp_millis())],
            )?;
            let built = env
                .call_method(&builder, "build", "()Landroid/app/Notification;", &[])?
                .l()?;
            let manager = notification_manager(env, activity)?;
            let id = notification_id(&notification.id);
            if let Some(tag) = notification.tag.as_deref() {
                let tag = env.new_string(tag)?;
                env.call_method(
                    &manager,
                    "notify",
                    "(Ljava/lang/String;ILandroid/app/Notification;)V",
                    &[
                        JValue::Object(&tag),
                        JValue::Int(id),
                        JValue::Object(&built),
                    ],
                )?;
            } else {
                env.call_method(
                    &manager,
                    "notify",
                    "(ILandroid/app/Notification;)V",
                    &[JValue::Int(id), JValue::Object(&built)],
                )?;
            }
            Ok(())
        })
    }

    pub(super) fn close(id: &str) -> Result<()> {
        with_env(|env, activity| {
            let manager = notification_manager(env, activity)?;
            env.call_method(
                &manager,
                "cancel",
                "(I)V",
                &[JValue::Int(notification_id(id))],
            )?;
            Ok(())
        })
    }

    fn notification_id(id: &str) -> i32 {
        let mut hash = 2_166_136_261_u32;
        for byte in id.bytes() {
            hash ^= u32::from(byte);
            hash = hash.wrapping_mul(16_777_619);
        }
        (hash & 0x7fff_ffff) as i32
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
