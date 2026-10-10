//! Desktop platform implementation

use anyhow::Result;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use notify_debouncer_full::new_debouncer;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use xmpp_core::events::{EventBus, XmppEvent};

use crate::{
    BackgroundService, Notification, NotificationPermission, NotificationService, PlatformStorage,
};

/// Desktop notifications using system tray / native notifications
pub struct DesktopNotifications {
    permission: NotificationPermission,
}

impl DesktopNotifications {
    pub fn new() -> Result<Self> {
        // Try to initialize native notifications
        // This would use tauri-plugin-notification or similar
        Ok(Self {
            permission: NotificationPermission::Default,
        })
    }
}

impl NotificationService for DesktopNotifications {
    fn request_permission(&self) -> Result<NotificationPermission> {
        // On desktop, permission is usually granted by default
        Ok(NotificationPermission::Granted)
    }

    fn show_notification(&self, notification: &Notification) -> Result<()> {
        // Use system notifications (libnotify on Linux, UserNotifications on macOS, WinRT on Windows)
        tracing::info!(
            "Showing desktop notification: {} - {}",
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

    fn schedule_sync(&self, interval: Duration) -> Result<()> {
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

    pub fn set_icon(&self, icon: &[u8]) -> Result<()> {
        Ok(())
    }

    pub fn set_tooltip(&self, tooltip: &str) -> Result<()> {
        Ok(())
    }

    pub fn show_menu(&self) -> Result<()> {
        Ok(())
    }
}

/// Auto-start management
pub struct AutoStart {
    app_name: String,
}

impl AutoStart {
    pub fn new(app_name: String) -> Self {
        Self { app_name }
    }

    pub fn enable(&self) -> Result<()> {
        // Add to startup applications
        Ok(())
    }

    pub fn disable(&self) -> Result<()> {
        // Remove from startup applications
        Ok(())
    }

    pub fn is_enabled(&self) -> Result<bool> {
        Ok(false)
    }
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

        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .mode(0o600)
            .open(&self.lock_file)?;

        // Try to lock
        use std::os::unix::fs::FileExt;
        // This is simplified - real implementation would use flock
        Ok(true)
    }

    pub fn unlock(&self) -> Result<()> {
        std::fs::remove_file(&self.lock_file).ok();
        Ok(())
    }
}
