//! Desktop platform implementation

use anyhow::Result;
use notify::RecommendedWatcher;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use xmpp_core::events::EventBus;

use crate::{
    BackgroundService, Notification, NotificationPermission, NotificationService, PlatformStorage,
};

/// Desktop notifications using system tray / native notifications
pub struct DesktopNotifications {
    permission: NotificationPermission,
}

impl DesktopNotifications {
    pub fn new() -> Result<Self> {
        Ok(Self {
            permission: NotificationPermission::Granted,
        })
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn show_native(notification: &Notification) -> Result<()> {
    let mut native = notify_rust::Notification::new();
    native
        .summary(&notification.title)
        .body(&notification.body)
        .appname("Jabu")
        .timeout(notify_rust::Timeout::Milliseconds(6000));
    if let Some(icon) = notification.icon.as_deref() {
        native.icon(icon);
    }
    native
        .show()
        .map_err(|error| anyhow::anyhow!("desktop notification failed: {error}"))?;
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn show_native(notification: &Notification) -> Result<()> {
    tracing::info!(
        "Desktop notifications are unavailable on this platform: {} - {}",
        notification.title,
        notification.body
    );
    Ok(())
}

impl NotificationService for DesktopNotifications {
    fn request_permission(&self) -> Result<NotificationPermission> {
        // Desktops do not have an app-level permission prompt. The compositor
        // or notification daemon may still suppress the toast.
        Ok(NotificationPermission::Granted)
    }

    fn show_notification(&self, notification: &Notification) -> Result<()> {
        show_native(notification)
    }

    fn close_notification(&self, _id: &str) -> Result<()> {
        Ok(())
    }

    fn get_permission(&self) -> NotificationPermission {
        self.permission
    }
}

/// Desktop storage using standard directories
pub struct DesktopStorage {
    data_dir: PathBuf,
    cache_dir: PathBuf,
    config_dir: PathBuf,
}

impl DesktopStorage {
    pub fn new() -> Result<Self> {
        let data_dir = dirs::data_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find data directory"))?
            .join("jabu");

        let cache_dir = dirs::cache_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find cache directory"))?
            .join("jabu");

        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?
            .join("jabu");

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

impl PlatformStorage for DesktopStorage {
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

/// Desktop background service for sync and file watching
#[allow(dead_code)]
pub struct DesktopBackground {
    events: Arc<EventBus>,
    watcher: Option<RecommendedWatcher>,
    sync_handle: Option<JoinHandle<()>>,
}

impl DesktopBackground {
    pub fn new(events: Arc<EventBus>) -> Result<Self> {
        Ok(Self {
            events,
            watcher: None,
            sync_handle: None,
        })
    }
}

impl BackgroundService for DesktopBackground {
    fn start(&self) -> Result<()> {
        // Start file watcher for config changes
        // Start periodic sync
        Ok(())
    }

    fn stop(&self) -> Result<()> {
        // Stop file watcher
        // Cancel sync
        Ok(())
    }

    fn schedule_sync(&self, _interval: Duration) -> Result<()> {
        // Schedule periodic background sync
        Ok(())
    }

    fn cancel_sync(&self) -> Result<()> {
        Ok(())
    }
}

/// System tray integration
pub struct SystemTray {
    // Tray icon and menu
}

impl SystemTray {
    pub fn new() -> Result<Self> {
        Ok(Self {})
    }

    pub fn set_icon(&self, _icon: &[u8]) -> Result<()> {
        Ok(())
    }

    pub fn set_tooltip(&self, _tooltip: &str) -> Result<()> {
        Ok(())
    }

    pub fn show_menu(&self) -> Result<()> {
        Ok(())
    }
}

/// Auto-start management
#[allow(dead_code)]
pub struct AutoStart {
    app_name: String,
}

impl AutoStart {
    pub fn new(app_name: String) -> Self {
        Self { app_name }
    }

    fn path(&self) -> Result<PathBuf> {
        let config =
            dirs::config_dir().ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;
        Ok(autostart_path(&config, &self.app_name))
    }

    pub fn enable(&self) -> Result<()> {
        let path = self.path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let executable = std::env::current_exe()?;
        let contents = desktop_entry(&executable);
        std::fs::write(&path, contents)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644));
        }
        Ok(())
    }

    pub fn disable(&self) -> Result<()> {
        let path = self.path()?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn is_enabled(&self) -> Result<bool> {
        Ok(self.path()?.exists())
    }
}

fn autostart_path(config_dir: &std::path::Path, app_name: &str) -> PathBuf {
    config_dir
        .join("autostart")
        .join(format!("{app_name}.desktop"))
}

fn desktop_entry(executable: &std::path::Path) -> String {
    let quoted = executable
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Jabu\n\
         Comment=Jabu XMPP client\n\
         Exec=\"{quoted}\"\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n"
    )
}

/// Single instance lock
pub struct SingleInstance {
    lock_file: PathBuf,
}

impl SingleInstance {
    pub fn new(app_name: &str) -> Result<Self> {
        let lock_file = dirs::runtime_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join(format!("{}.lock", app_name));

        Ok(Self { lock_file })
    }

    pub fn try_lock(&self) -> Result<bool> {
        // Try to create lock file
        use std::fs::OpenOptions;
        use std::os::unix::fs::OpenOptionsExt;

        let _file = OpenOptions::new()
            .create(true)
            .write(true)
            .mode(0o600)
            .open(&self.lock_file)?;

        // This is simplified - real implementation would use flock
        Ok(true)
    }

    pub fn unlock(&self) -> Result<()> {
        std::fs::remove_file(&self.lock_file).ok();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_notification_permission_is_granted() {
        let notifications = DesktopNotifications::new().expect("desktop notifications");
        assert_eq!(
            notifications.get_permission(),
            NotificationPermission::Granted
        );
        assert_eq!(
            notifications.request_permission().expect("permission"),
            NotificationPermission::Granted
        );
    }

    #[test]
    fn autostart_entry_quotes_the_executable() {
        let path = autostart_path(std::path::Path::new("/home/alice/.config"), "jabu");
        assert_eq!(
            path,
            std::path::PathBuf::from("/home/alice/.config/autostart/jabu.desktop")
        );
        let entry = desktop_entry(std::path::Path::new("/opt/Jabu/jabu"));
        assert!(entry.contains("Exec=\"/opt/Jabu/jabu\""));
        assert!(entry.contains("Type=Application"));
    }
}
