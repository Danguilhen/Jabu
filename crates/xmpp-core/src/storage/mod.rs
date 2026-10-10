//! Local storage abstraction

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use jid::Jid;
use parking_lot::RwLock;
use redb::{Database, TableDefinition, ReadableTable, WriteTransaction};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::{ChatMessage, MucMessage, PubSubItem, Result, XmppError};

/// Storage backend trait
pub trait StorageBackend: Send + Sync {
    fn init(&self) -> Result<()>;
    fn close(&self) -> Result<()>;
}

/// Redb-based storage implementation
pub struct Storage {
    db: Arc<Database>,
    path: PathBuf,
}

impl Storage {
    pub fn new(path: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&path)?;

        let db_path = path.join("jabu.redb");
        let db = Database::create(&db_path)
            .map_err(|e| XmppError::StorageError(e.to_string()))?;

        let storage = Self {
            db: Arc::new(db),
            path,
        };

        storage.init_schema()?;
        Ok(storage)
    }

    fn init_schema(&self) -> Result<()> {
        let write_txn = self.db.begin_write()
            .map_err(|e| XmppError::StorageError(e.to_string()))?;

        // Accounts table
        let _ = write_txn.open_table(ACCOUNTS_TABLE)
            .map_err(|e| XmppError::StorageError(e.to_string()))?;

        // Contacts table
        let _ = write_txn.open_table(CONTACTS_TABLE)
            .map_err(|e| XmppError::StorageError(e.to_string()))?;

        // Chats table
        let _ = write_txn.open_table(CHATS_TABLE)
            .map_err(|e| XmppError::StorageError(e.to_string()))?;

        // Messages table
        let _ = write_txn.open_table(MESSAGES_TABLE)
            .map_err(|e| XmppError::StorageError(e.to_string()))?;

        // MUC rooms table
        let _ = write_txn.open_table(MUC_ROOMS_TABLE)
            .map_err(|e| XmppError::StorageError(e.to_string()))?;

        // MUC messages table
        let _ = write_txn.open_table(MUC_MESSAGES_TABLE)
            .map_err(|e| XmppError::StorageError(e.to_string()))?;

        // PubSub nodes table
        let _ = write_txn.open_table(PUBSUB_NODES_TABLE)
            .map_err(|e| XmppError::StorageError(e.to_string()))?;

        // PubSub items table
        let _ = write_txn.open_table(PUBSUB_ITEMS_TABLE)
            .map_err(|e| XmppError::StorageError(e.to_string()))?;

        // Settings table
        let _ = write_txn.open_table(SETTINGS_TABLE)
            .map_err(|e| XmppError::StorageError(e.to_string()))?;

        write_txn.commit()
            .map_err(|e| XmppError::StorageError(e.to_string()))?;

        Ok(())
    }

    // Account methods
    pub fn save_account(&self, account: &StoredAccount) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(ACCOUNTS_TABLE)?;
            let key = account.jid.as_str();
            let value = serde_json::to_vec(account)?;
            table.insert(key, value)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn get_account(&self, jid: &str) -> Result<Option<StoredAccount>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(ACCOUNTS_TABLE)?;
        if let Some(val) = table.get(jid)? {
            let account: StoredAccount = serde_json::from_slice(val.value())?;
            Ok(Some(account))
        } else {
            Ok(None)
        }
    }

    pub fn list_accounts(&self) -> Result<Vec<StoredAccount>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(ACCOUNTS_TABLE)?;
        let mut accounts = Vec::new();
        for entry in table.iter()? {
            let (_, val) = entry?;
            let account: StoredAccount = serde_json::from_slice(val.value())?;
            accounts.push(account);
        }
        Ok(accounts)
    }

    pub fn delete_account(&self, jid: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(ACCOUNTS_TABLE)?;
            table.remove(jid)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    // Contact methods
    pub fn save_contact(&self, contact: &StoredContact) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(CONTACTS_TABLE)?;
            let key = format!("{}@{}", contact.account_jid, contact.jid);
            let value = serde_json::to_vec(contact)?;
            table.insert(key.as_str(), value)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn get_contacts(&self, account_jid: &str) -> Result<Vec<StoredContact>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(CONTACTS_TABLE)?;
        let prefix = format!("{}@", account_jid);
        let mut contacts = Vec::new();
        for entry in table.iter()? {
            let (key, val) = entry?;
            let key_str = key.value();
            if key_str.starts_with(&prefix) {
                let contact: StoredContact = serde_json::from_slice(val.value())?;
                contacts.push(contact);
            }
        }
        Ok(contacts)
    }

    pub fn delete_contact(&self, account_jid: &str, contact_jid: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(CONTACTS_TABLE)?;
            let key = format!("{}@{}", account_jid, contact_jid);
            table.remove(key.as_str())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    // Chat methods
    pub fn save_chat(&self, chat: &StoredChat) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(CHATS_TABLE)?;
            let key = format!("{}@{}", chat.account_jid, chat.peer_jid);
            let value = serde_json::to_vec(chat)?;
            table.insert(key.as_str(), value)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn get_chats(&self, account_jid: &str) -> Result<Vec<StoredChat>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(CHATS_TABLE)?;
        let prefix = format!("{}@", account_jid);
        let mut chats = Vec::new();
        for entry in table.iter()? {
            let (key, val) = entry?;
            let key_str = key.value();
            if key_str.starts_with(&prefix) {
                let chat: StoredChat = serde_json::from_slice(val.value())?;
                chats.push(chat);
            }
        }
        Ok(chats)
    }

    // Message methods
    pub fn save_message(&self, message: &ChatMessage) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(MESSAGES_TABLE)?;
            let key = format!("{}@{}", message.bare_jid, message.id);
            let value = serde_json::to_vec(message)?;
            table.insert(key.as_str(), value)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn get_messages(&self, bare_jid: &str, limit: usize, before_id: Option<&str>) -> Result<Vec<ChatMessage>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(MESSAGES_TABLE)?;
        let prefix = format!("{}@", bare_jid);
        let mut messages = Vec::new();

        for entry in table.iter()? {
            let (key, val) = entry?;
            let key_str = key.value();
            if key_str.starts_with(&prefix) {
                let msg: ChatMessage = serde_json::from_slice(val.value())?;
                if let Some(before) = before_id {
                    if msg.id >= *before {
                        continue;
                    }
                }
                messages.push(msg);
            }
        }

        messages.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        messages.truncate(limit);
        Ok(messages)
    }

    pub fn delete_message(&self, bare_jid: &str, message_id: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(MESSAGES_TABLE)?;
            let key = format!("{}@{}", bare_jid, message_id);
            table.remove(key.as_str())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    // MUC methods
    pub fn save_muc_room(&self, room: &StoredMucRoom) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(MUC_ROOMS_TABLE)?;
            let key = format!("{}@{}", room.account_jid, room.jid);
            let value = serde_json::to_vec(room)?;
            table.insert(key.as_str(), value)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn get_muc_rooms(&self, account_jid: &str) -> Result<Vec<StoredMucRoom>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(MUC_ROOMS_TABLE)?;
        let prefix = format!("{}@", account_jid);
        let mut rooms = Vec::new();
        for entry in table.iter()? {
            let (key, val) = entry?;
            let key_str = key.value();
            if key_str.starts_with(&prefix) {
                let room: StoredMucRoom = serde_json::from_slice(val.value())?;
                rooms.push(room);
            }
        }
        Ok(rooms)
    }

    pub fn save_muc_message(&self, message: &MucMessage) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(MUC_MESSAGES_TABLE)?;
            let key = format!("{}@{}", message.room_jid, message.id);
            let value = serde_json::to_vec(message)?;
            table.insert(key.as_str(), value)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn get_muc_messages(&self, room_jid: &str, limit: usize, before_id: Option<&str>) -> Result<Vec<MucMessage>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(MUC_MESSAGES_TABLE)?;
        let prefix = format!("{}@", room_jid);
        let mut messages = Vec::new();

        for entry in table.iter()? {
            let (key, val) = entry?;
            let key_str = key.value();
            if key_str.starts_with(&prefix) {
                let msg: MucMessage = serde_json::from_slice(val.value())?;
                if let Some(before) = before_id {
                    if msg.id >= *before {
                        continue;
                    }
                }
                messages.push(msg);
            }
        }

        messages.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        messages.truncate(limit);
        Ok(messages)
    }

    // PubSub methods
    pub fn save_pubsub_node(&self, node: &StoredPubSubNode) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(PUBSUB_NODES_TABLE)?;
            let key = format!("{}@{}", node.service_jid, node.id);
            let value = serde_json::to_vec(node)?;
            table.insert(key.as_str(), value)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn get_pubsub_nodes(&self, service_jid: &str) -> Result<Vec<StoredPubSubNode>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(PUBSUB_NODES_TABLE)?;
        let prefix = format!("{}@", service_jid);
        let mut nodes = Vec::new();
        for entry in table.iter()? {
            let (key, val) = entry?;
            let key_str = key.value();
            if key_str.starts_with(&prefix) {
                let node: StoredPubSubNode = serde_json::from_slice(val.value())?;
                nodes.push(node);
            }
        }
        Ok(nodes)
    }

    pub fn save_pubsub_item(&self, item: &PubSubItem) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(PUBSUB_ITEMS_TABLE)?;
            let key = format!("{}@{}", item.node_id, item.id);
            let value = serde_json::to_vec(item)?;
            table.insert(key.as_str(), value)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn get_pubsub_items(&self, node_id: &str, limit: usize) -> Result<Vec<PubSubItem>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(PUBSUB_ITEMS_TABLE)?;
        let prefix = format!("{}@", node_id);
        let mut items = Vec::new();

        for entry in table.iter()? {
            let (key, val) = entry?;
            let key_str = key.value();
            if key_str.starts_with(&prefix) {
                let item: PubSubItem = serde_json::from_slice(val.value())?;
                items.push(item);
            }
        }

        items.sort_by(|a, b| b.published_at.cmp(&a.published_at));
        items.truncate(limit);
        Ok(items)
    }

    // Settings methods
    pub fn set_setting(&self, key: &str, value: &serde_json::Value) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SETTINGS_TABLE)?;
            let value = serde_json::to_vec(value)?;
            table.insert(key, value)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SETTINGS_TABLE)?;
        if let Some(val) = table.get(key)? {
            let value: serde_json::Value = serde_json::from_slice(val.value())?;
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }
}

/// Table definitions
const ACCOUNTS_TABLE: TableDefinition<&str, Vec<u8>> = TableDefinition::new("accounts");
const CONTACTS_TABLE: TableDefinition<&str, Vec<u8>> = TableDefinition::new("contacts");
const CHATS_TABLE: TableDefinition<&str, Vec<u8>> = TableDefinition::new("chats");
const MESSAGES_TABLE: TableDefinition<&str, Vec<u8>> = TableDefinition::new("messages");
const MUC_ROOMS_TABLE: TableDefinition<&str, Vec<u8>> = TableDefinition::new("muc_rooms");
const MUC_MESSAGES_TABLE: TableDefinition<&str, Vec<u8>> = TableDefinition::new("muc_messages");
const PUBSUB_NODES_TABLE: TableDefinition<&str, Vec<u8>> = TableDefinition::new("pubsub_nodes");
const PUBSUB_ITEMS_TABLE: TableDefinition<&str, Vec<u8>> = TableDefinition::new("pubsub_items");
const SETTINGS_TABLE: TableDefinition<&str, Vec<u8>> = TableDefinition::new("settings");

/// Stored account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAccount {
    pub jid: String,
    pub password_hash: Option<String>, // Encrypted
    pub resource: String,
    pub priority: i32,
    pub auto_reconnect: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_login: Option<chrono::DateTime<chrono::Utc>>,
}

/// Stored contact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredContact {
    pub account_jid: String,
    pub jid: String,
    pub name: Option<String>,
    pub groups: Vec<String>,
    pub subscription: crate::roster::SubscriptionState,
    pub avatar_hash: Option<String>,
    pub is_blocked: bool,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Stored chat
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredPubSubNode {
    pub service_jid: String,
    pub id: String,
    pub config: crate::pubsub::PubSubNodeConfig,
    pub last_sync: Option<chrono::DateTime<chrono::Utc>>,
}