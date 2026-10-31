//! Platform-specific functionality for Jabu

pub mod desktop;
pub mod single_instance;

#[cfg(target_arch = "wasm32")]
pub mod web;

#[cfg(any(target_os = "android", target_os = "ios"))]
pub mod mobile;

use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use xmpp_core::events::EventBus;

/// Platform capabilities
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Desktop,
    Mobile,
    Web,
}

impl Platform {
    pub fn current() -> Self {
        #[cfg(target_arch = "wasm32")]
        return Platform::Web;

        #[cfg(all(
            not(target_arch = "wasm32"),
            any(target_os = "android", target_os = "ios")
        ))]
        return Platform::Mobile;

        #[cfg(all(
            not(target_arch = "wasm32"),
            not(any(target_os = "android", target_os = "ios"))
        ))]
        return Platform::Desktop;
    }

    pub fn supports_push(&self) -> bool {
        matches!(self, Platform::Mobile | Platform::Web)
    }

    pub fn supports_background(&self) -> bool {
        matches!(self, Platform::Desktop | Platform::Mobile)
    }

    pub fn supports_system_tray(&self) -> bool {
        matches!(self, Platform::Desktop)
    }

    pub fn supports_file_system(&self) -> bool {
        matches!(self, Platform::Desktop | Platform::Mobile)
    }

    pub fn supports_notifications(&self) -> bool {
        true // All platforms support some form of notifications
    }
}

/// Platform context for accessing platform-specific features
#[derive(Clone)]
pub struct PlatformContext {
    pub platform: Platform,
    pub notifications: Arc<dyn NotificationService>,
    pub storage: Arc<dyn PlatformStorage>,
    pub background: Arc<dyn BackgroundService>,
    pub events: Arc<EventBus>,
}

impl PlatformContext {
    pub fn new(events: Arc<EventBus>) -> Result<Self> {
        let platform = Platform::current();

        let notifications = create_notifications(platform)?;
        let storage = create_storage(platform)?;
        let background = create_background(platform, events.clone())?;

        Ok(Self {
            platform,
            notifications,
            storage,
            background,
            events,
        })
    }
}

// Platform-specific constructors using #[cfg] attributes

#[cfg(target_arch = "wasm32")]
fn create_notifications(_: Platform) -> Result<Arc<dyn NotificationService>> {
    Ok(Arc::new(web::WebNotifications::new()?))
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn create_notifications(_: Platform) -> Result<Arc<dyn NotificationService>> {
    Ok(Arc::new(mobile::MobileNotifications::new()?))
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(any(target_os = "android", target_os = "ios"))
))]
fn create_notifications(_: Platform) -> Result<Arc<dyn NotificationService>> {
    Ok(Arc::new(desktop::DesktopNotifications::new()?))
}

#[cfg(target_arch = "wasm32")]
fn create_storage(_: Platform) -> Result<Arc<dyn PlatformStorage>> {
    Ok(Arc::new(web::WebStorage::new()?))
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn create_storage(_: Platform) -> Result<Arc<dyn PlatformStorage>> {
    Ok(Arc::new(mobile::MobileStorage::new()?))
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(any(target_os = "android", target_os = "ios"))
))]
fn create_storage(_: Platform) -> Result<Arc<dyn PlatformStorage>> {
    Ok(Arc::new(desktop::DesktopStorage::new()?))
}

#[cfg(target_arch = "wasm32")]
fn create_background(_: Platform, events: Arc<EventBus>) -> Result<Arc<dyn BackgroundService>> {
    Ok(Arc::new(web::WebBackground::new(events)?))
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn create_background(_: Platform, events: Arc<EventBus>) -> Result<Arc<dyn BackgroundService>> {
    Ok(Arc::new(mobile::MobileBackground::new(events)?))
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(any(target_os = "android", target_os = "ios"))
))]
fn create_background(_: Platform, events: Arc<EventBus>) -> Result<Arc<dyn BackgroundService>> {
    Ok(Arc::new(desktop::DesktopBackground::new(events)?))
}

/// Notification service trait
pub trait NotificationService: Send + Sync {
    fn request_permission(&self) -> Result<NotificationPermission>;
    fn show_notification(&self, notification: &Notification) -> Result<()>;
    fn close_notification(&self, id: &str) -> Result<()>;
    fn get_permission(&self) -> NotificationPermission;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationPermission {
    Granted,
    Denied,
    Default,
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub id: String,
    pub title: String,
    pub body: String,
    pub icon: Option<String>,
    pub tag: Option<String>,
    pub data: Option<serde_json::Value>,
    pub actions: Vec<NotificationAction>,
    pub require_interaction: bool,
    pub silent: bool,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct NotificationAction {
    pub action: String,
    pub title: String,
    pub icon: Option<String>,
}

/// Platform storage trait
pub trait PlatformStorage: Send + Sync {
    fn get_data_dir(&self) -> Result<std::path::PathBuf>;
    fn get_cache_dir(&self) -> Result<std::path::PathBuf>;
    fn get_config_dir(&self) -> Result<std::path::PathBuf>;
}

/// Background service trait
pub trait BackgroundService: Send + Sync {
    fn start(&self) -> Result<()>;
    fn stop(&self) -> Result<()>;
    fn schedule_sync(&self, interval: Duration) -> Result<()>;
    fn cancel_sync(&self) -> Result<()>;
}

/// Platform initialization
pub fn init_platform(events: Arc<EventBus>) -> Result<PlatformContext> {
    PlatformContext::new(events)
}
