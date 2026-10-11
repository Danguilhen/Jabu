//! Storage abstraction for Jabu

pub mod redb_storage;

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use xmpp_core::{ChatMessage, MucMessage, PubSubItem};

/// Storage backend trait
pub trait StorageBackend: Send + Sync {
    fn init(&self) -> Result<()>;
    fn close(&self) -> Result<()>;

    // Account methods
    fn save_account(&self, account: &StoredAccount) -> Result<()>;
    fn get_account(&self, jid: &str) -> Result<Option<StoredAccount>>;
    fn list_accounts(&self) -> Result<Vec<StoredAccount>>;
    fn delete_account(&self, jid: &str) -> Result<()>;

    // Contact methods
    fn save_contact(&self, contact: &StoredContact) -> Result<()>;
    fn get_contacts(&self, account_jid: &str) -> Result<Vec<StoredContact>>;
    fn delete_contact(&self, account_jid: &str, contact_jid: &str) -> Result<()>;

    // Chat methods
    fn save_chat(&self, chat: &StoredChat) -> Result<()>;
    fn get_chats(&self, account_jid: &str) -> Result<Vec<StoredChat>>;

    // Message methods
    fn save_message(&self, message: &ChatMessage) -> Result<()>;
    fn get_messages(
        &self,
        bare_jid: &str,
        limit: usize,
        before_id: Option<&str>,
    ) -> Result<Vec<ChatMessage>>;
    fn delete_message(&self, bare_jid: &str, message_id: &str) -> Result<()>;

    // MUC methods
    fn save_muc_room(&self, room: &StoredMucRoom) -> Result<()>;
    fn get_muc_rooms(&self, account_jid: &str) -> Result<Vec<StoredMucRoom>>;
    fn save_muc_message(&self, message: &MucMessage) -> Result<()>;
    fn get_muc_messages(
        &self,
        room_jid: &str,
        limit: usize,
        before_id: Option<&str>,
    ) -> Result<Vec<MucMessage>>;

    // PubSub methods
    fn save_pubsub_node(&self, node: &StoredPubSubNode) -> Result<()>;
    fn get_pubsub_nodes(&self, service_jid: &str) -> Result<Vec<StoredPubSubNode>>;
    fn save_pubsub_item(&self, item: &PubSubItem) -> Result<()>;
    fn get_pubsub_items(&self, node_id: &str, limit: usize) -> Result<Vec<PubSubItem>>;

    // Settings
    fn set_setting(&self, key: &str, value: &serde_json::Value) -> Result<()>;
    fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>>;
}

/// Settings key holding the saved account credentials.
pub const CREDENTIALS_KEY: &str = "account.credentials";

/// Account credentials persisted across launches.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredCredentials {
    pub jid: String,
    pub password: Option<String>,
    pub resource: Option<String>,
}

pub fn load_credentials(storage: &dyn StorageBackend) -> Option<StoredCredentials> {
    storage
        .get_setting(CREDENTIALS_KEY)
        .ok()
        .flatten()
        .and_then(|value| serde_json::from_value(value).ok())
}

pub fn save_credentials(
    storage: &dyn StorageBackend,
    credentials: &StoredCredentials,
) -> anyhow::Result<()> {
    storage.set_setting(CREDENTIALS_KEY, &serde_json::to_value(credentials)?)
}

pub fn clear_credentials(storage: &dyn StorageBackend) -> anyhow::Result<()> {
    storage.set_setting(CREDENTIALS_KEY, &serde_json::Value::Null)
}

/// OMEMO state persisted in the storage backend's settings table.
pub struct SettingsOmemoStore {
    storage: Arc<dyn StorageBackend>,
    key: String,
}

impl SettingsOmemoStore {
    pub fn new(storage: Arc<dyn StorageBackend>, jid: &str) -> Self {
        Self {
            storage,
            key: format!("omemo:{}", jid),
        }
    }
}

impl xmpp_core::omemo::OmemoStore for SettingsOmemoStore {
    fn load(&self) -> Option<String> {
        self.storage
            .get_setting(&self.key)
            .ok()
            .flatten()
            .and_then(|v| v.as_str().map(String::from))
    }

    fn save(&self, data: &str) -> xmpp_core::Result<()> {
        self.storage
            .set_setting(&self.key, &serde_json::Value::String(data.to_string()))
            .map_err(|e| xmpp_core::XmppError::StorageError(e.to_string()))
    }
}

/// Storage factory
pub struct StorageFactory;

impl StorageFactory {
    pub fn create(path: PathBuf) -> Result<Arc<dyn StorageBackend>> {
        Ok(Arc::new(redb_storage::RedbStorage::new(path)?))
    }
}

/// Stored account
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredAccount {
    pub jid: String,
    pub password_hash: Option<String>,
    pub resource: String,
    pub priority: i32,
    pub auto_reconnect: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_login: Option<chrono::DateTime<chrono::Utc>>,
}

/// Stored contact
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredContact {
    pub account_jid: String,
    pub jid: String,
    pub name: Option<String>,
    pub groups: Vec<String>,
    pub subscription: xmpp_core::roster::SubscriptionState,
    pub avatar_hash: Option<String>,
    pub is_blocked: bool,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Stored chat
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredChat {
    pub account_jid: String,
    pub peer_jid: String,
    pub unread_count: usize,
    pub last_message_id: Option<String>,
    pub last_activity: chrono::DateTime<chrono::Utc>,
    pub is_archived: bool,
    pub is_pinned: bool,
    pub notifications_enabled: bool,
}

/// Stored MUC room
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredMucRoom {
    pub account_jid: String,
    pub jid: String,
    pub name: String,
    pub nick: String,
    pub is_joined: bool,
    pub is_favorite: bool,
    pub notifications_enabled: bool,
    pub last_activity: chrono::DateTime<chrono::Utc>,
}

/// Stored PubSub node
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredPubSubNode {
    pub service_jid: String,
    pub id: String,
    pub config: xmpp_core::pubsub::PubSubNodeConfig,
    pub last_sync: Option<chrono::DateTime<chrono::Utc>>,
}

impl StorageBackend for Arc<dyn StorageBackend> {
    fn init(&self) -> Result<()> {
        self.as_ref().init()
    }
    fn close(&self) -> Result<()> {
        self.as_ref().close()
    }
    fn save_account(&self, account: &StoredAccount) -> Result<()> {
        self.as_ref().save_account(account)
    }
    fn get_account(&self, jid: &str) -> Result<Option<StoredAccount>> {
        self.as_ref().get_account(jid)
    }
    fn list_accounts(&self) -> Result<Vec<StoredAccount>> {
        self.as_ref().list_accounts()
    }
    fn delete_account(&self, jid: &str) -> Result<()> {
        self.as_ref().delete_account(jid)
    }
    fn save_contact(&self, contact: &StoredContact) -> Result<()> {
        self.as_ref().save_contact(contact)
    }
    fn get_contacts(&self, account_jid: &str) -> Result<Vec<StoredContact>> {
        self.as_ref().get_contacts(account_jid)
    }
    fn delete_contact(&self, account_jid: &str, contact_jid: &str) -> Result<()> {
        self.as_ref().delete_contact(account_jid, contact_jid)
    }
    fn save_chat(&self, chat: &StoredChat) -> Result<()> {
        self.as_ref().save_chat(chat)
    }
    fn get_chats(&self, account_jid: &str) -> Result<Vec<StoredChat>> {
        self.as_ref().get_chats(account_jid)
    }
    fn save_message(&self, message: &ChatMessage) -> Result<()> {
        self.as_ref().save_message(message)
    }
    fn get_messages(
        &self,
        bare_jid: &str,
        limit: usize,
        before_id: Option<&str>,
    ) -> Result<Vec<ChatMessage>> {
        self.as_ref().get_messages(bare_jid, limit, before_id)
    }
    fn delete_message(&self, bare_jid: &str, message_id: &str) -> Result<()> {
        self.as_ref().delete_message(bare_jid, message_id)
    }
    fn save_muc_room(&self, room: &StoredMucRoom) -> Result<()> {
        self.as_ref().save_muc_room(room)
    }
    fn get_muc_rooms(&self, account_jid: &str) -> Result<Vec<StoredMucRoom>> {
        self.as_ref().get_muc_rooms(account_jid)
    }
    fn save_muc_message(&self, message: &MucMessage) -> Result<()> {
        self.as_ref().save_muc_message(message)
    }
    fn get_muc_messages(
        &self,
        room_jid: &str,
        limit: usize,
        before_id: Option<&str>,
    ) -> Result<Vec<MucMessage>> {
        self.as_ref().get_muc_messages(room_jid, limit, before_id)
    }
    fn save_pubsub_node(&self, node: &StoredPubSubNode) -> Result<()> {
        self.as_ref().save_pubsub_node(node)
    }
    fn get_pubsub_nodes(&self, service_jid: &str) -> Result<Vec<StoredPubSubNode>> {
        self.as_ref().get_pubsub_nodes(service_jid)
    }
    fn save_pubsub_item(&self, item: &PubSubItem) -> Result<()> {
        self.as_ref().save_pubsub_item(item)
    }
    fn get_pubsub_items(&self, node_id: &str, limit: usize) -> Result<Vec<PubSubItem>> {
        self.as_ref().get_pubsub_items(node_id, limit)
    }
    fn set_setting(&self, key: &str, value: &serde_json::Value) -> Result<()> {
        self.as_ref().set_setting(key, value)
    }
    fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>> {
        self.as_ref().get_setting(key)
    }
}
