//! Web platform implementation (WASM/PWA)

use anyhow::Result;
use gloo_storage::{IndexedDbStorage, Storage as GlooStorage};
use std::path::PathBuf;
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use web_sys::{Notification, PushManager, PushSubscription, ServiceWorkerRegistration};
use xmpp_core::events::{EventBus, XmppEvent};

use crate::{
    BackgroundService, Notification, NotificationPermission, NotificationService, PlatformStorage,
};

/// Web notifications using Notification API and Push API
pub struct WebNotifications {
    permission: NotificationPermission,
    sw_registration: Option<ServiceWorkerRegistration>,
    vapid_public_key: Option<String>,
}

impl WebNotifications {
    pub fn new() -> Result<Self> {
        let permission = match Notification::permission().as_str() {
            "granted" => NotificationPermission::Granted,
            "denied" => NotificationPermission::Denied,
            _ => NotificationPermission::Default,
        };

        Ok(Self {
            permission,
            sw_registration: None,
            vapid_public_key: None,
        })
    }

    pub async fn set_vapid_key(&mut self, key: String) -> Result<()> {
        self.vapid_public_key = Some(key);
        Ok(())
    }

    pub async fn subscribe_push(&self) -> Result<PushSubscription> {
        let registration = self.get_sw_registration().await?;
        let push_manager = registration.push_manager()?;

        let options = web_sys::PushSubscriptionOptions::new();
        options.set_user_visible_only(true);

        if let Some(key) = &self.vapid_public_key {
            let key_bytes = base64::decode_config(key, base64::URL_SAFE_NO_PAD)?;
            let array = js_sys::Uint8Array::new(&key_bytes.into());
            options.set_application_server_key(&array);
        }

        let subscription = push_manager.subscribe_with_options(&options)?;
        let promise = wasm_bindgen_futures::JsFuture::from(subscription);
        let result = promise.await?;
        Ok(result.unchecked_into::<PushSubscription>())
    }

    async fn get_sw_registration(&self) -> Result<ServiceWorkerRegistration> {
        if let Some(reg) = &self.sw_registration {
            return Ok(reg.clone());
        }

        let navigator = web_sys::window().unwrap().navigator();
        let registration = navigator.service_worker().ready()?;
        let promise = wasm_bindgen_futures::JsFuture::from(registration);
        let result = promise.await?;
        Ok(result.unchecked_into::<ServiceWorkerRegistration>())
    }
}

impl NotificationService for WebNotifications {
    fn request_permission(&self) -> Result<NotificationPermission> {
        let window = web_sys::window().unwrap();
        let promise = Notification::request_permission();
        let future = wasm_bindgen_futures::JsFuture::from(promise);
        let result = wasm_bindgen_futures::spawn_local(async move {
            let permission = future.await.unwrap();
            match permission.as_string().unwrap().as_str() {
                "granted" => NotificationPermission::Granted,
                "denied" => NotificationPermission::Denied,
                _ => NotificationPermission::Default,
            }
        });
        // This is async, would need proper handling
        Ok(NotificationPermission::Default)
    }

    fn show_notification(&self, notification: &Notification) -> Result<()> {
        if self.permission != NotificationPermission::Granted {
            return Err(anyhow::anyhow!("Notification permission not granted"));
        }

        let options = web_sys::NotificationOptions::new();
        options.set_body(&notification.body);
        if let Some(icon) = &notification.icon {
            options.set_icon(icon);
        }
        if let Some(tag) = &notification.tag {
            options.set_tag(tag);
        }
        options.set_require_interaction(notification.require_interaction);
        options.set_silent(notification.silent);

        let notif = Notification::new_with_options(&notification.title, &options)?;
        // Auto-close after delay unless require_interaction
        if !notification.require_interaction {
            let notif_clone = notif.clone();
            gloo_timers::callback::Timeout::new(5000, move || {
                notif_clone.close();
            })
            .forget();
        }

        Ok(())
    }

    fn close_notification(&self, id: &str) -> Result<()> {
        // Would need to track notifications by ID
        Ok(())
    }

    fn get_permission(&self) -> NotificationPermission {
        self.permission
    }
}

/// Web storage using IndexedDB
pub struct WebStorage {
    db: IndexedDbStorage,
}

impl WebStorage {
    pub fn new() -> Result<Self> {
        let db = IndexedDbStorage::open("jabu")?;
        Ok(Self { db })
    }
}

impl PlatformStorage for WebStorage {
    fn get_data_dir(&self) -> Result<PathBuf> {
        // Not applicable on web
        Err(anyhow::anyhow!("No filesystem access on web"))
    }

    fn get_cache_dir(&self) -> Result<PathBuf> {
        Err(anyhow::anyhow!("No filesystem access on web"))
    }

    fn get_config_dir(&self) -> Result<PathBuf> {
        Err(anyhow::anyhow!("No filesystem access on web"))
    }
}

impl WebStorage {
    pub fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self.db.get(key)?)
    }

    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        Ok(self.db.set(key, value)?)
    }

    pub fn delete(&self, key: &str) -> Result<()> {
        Ok(self.db.delete(key)?)
    }

    pub fn keys(&self) -> Result<Vec<String>> {
        Ok(self.db.keys()?)
    }

    pub fn clear(&self) -> Result<()> {
        Ok(self.db.clear()?)
    }
}

/// Web background service using Service Workers and Background Sync
pub struct WebBackground {
    events: Arc<EventBus>,
    sync_interval: Option<gloo_timers::callback::Interval>,
}

impl WebBackground {
    pub fn new(events: Arc<EventBus>) -> Result<Self> {
        Ok(Self {
            events,
            sync_interval: None,
        })
    }
}

impl BackgroundService for WebBackground {
    fn start(&self) -> Result<()> {
        // Register service worker
        // Set up periodic background sync if supported
        Ok(())
    }

    fn stop(&self) -> Result<()> {
        if let Some(interval) = &self.sync_interval {
            interval.cancel();
        }
        Ok(())
    }

    fn schedule_sync(&self, interval: std::time::Duration) -> Result<()> {
        let events = self.events.clone();
        let interval_ms = interval.as_millis() as u32;

        let timer = gloo_timers::callback::Interval::new(interval_ms, move || {
            // Trigger background sync
            let _ = events.send(XmppEvent::ChatChanged {
                jid: "background_sync".to_string(),
            });
        });

        // Store interval (would need interior mutability)
        Ok(())
    }

    fn cancel_sync(&self) -> Result<()> {
        Ok(())
    }
}

/// PWA install prompt handling
pub struct PwaInstallPrompt {
    deferred_prompt: Option<web_sys::BeforeInstallPromptEvent>,
}

impl PwaInstallPrompt {
    pub fn new() -> Self {
        Self {
            deferred_prompt: None,
        }
    }

    pub fn set_deferred_prompt(&mut self, event: web_sys::BeforeInstallPromptEvent) {
        event.prevent_default().ok();
        self.deferred_prompt = Some(event);
    }

    pub async fn prompt_install(&self) -> Result<bool> {
        if let Some(prompt) = &self.deferred_prompt {
            prompt.prompt().ok();
            let choice = wasm_bindgen_futures::JsFuture::from(prompt.user_choice()).await?;
            let outcome = choice.get("outcome").unwrap();
            Ok(outcome.as_string().unwrap() == "accepted")
        }
        Ok(false)
    }

    pub fn can_install(&self) -> bool {
        self.deferred_prompt.is_some()
    }
}

/// Web file handling (drag-drop, file picker)
pub struct WebFileHandler;

impl WebFileHandler {
    pub fn new() -> Self {
        Self
    }

    pub async fn pick_files(&self, accept: &str, multiple: bool) -> Result<Vec<WebFile>> {
        // Use File System Access API or <input type="file">
        Ok(vec![])
    }

    pub async fn save_file(&self, name: &str, data: &[u8], mime: &str) -> Result<()> {
        // Use File System Access API or download link
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct WebFile {
    pub name: String,
    pub mime_type: String,
    pub size: u64,
    pub data: Vec<u8>,
}

/// Web clipboard access
pub struct WebClipboard;

impl WebClipboard {
    pub fn new() -> Self {
        Self
    }

    pub async fn read_text(&self) -> Result<String> {
        let navigator = web_sys::window().unwrap().navigator();
        let clipboard = navigator.clipboard()?;
        let promise = clipboard.read_text()?;
        let text = wasm_bindgen_futures::JsFuture::from(promise).await?;
        Ok(text.as_string().unwrap_or_default())
    }

    pub async fn write_text(&self, text: &str) -> Result<()> {
        let navigator = web_sys::window().unwrap().navigator();
        let clipboard = navigator.clipboard()?;
        let promise = clipboard.write_text(text)?;
        wasm_bindgen_futures::JsFuture::from(promise).await?;
        Ok(())
    }
}

/// Web share API
pub struct WebShare;

impl WebShare {
    pub fn new() -> Self {
        Self
    }

    pub async fn share(&self, data: ShareData) -> Result<()> {
        let navigator = web_sys::window().unwrap().navigator();
        let share_data = web_sys::ShareData::new();
        share_data.set_title(&data.title);
        share_data.set_text(&data.text);
        if let Some(url) = data.url {
            share_data.set_url(&url);
        }

        let promise = navigator.share(&share_data)?;
        wasm_bindgen_futures::JsFuture::from(promise).await?;
        Ok(())
    }

    pub fn can_share(&self) -> bool {
        let navigator = web_sys::window().unwrap().navigator();
        js_sys::Reflect::has(&navigator, &"share".into()).unwrap()
    }
}

#[derive(Debug, Clone)]
pub struct ShareData {
    pub title: String,
    pub text: String,
    pub url: Option<String>,
}
