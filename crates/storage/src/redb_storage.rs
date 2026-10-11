//! Redb storage implementation

use anyhow::Result;
use redb::{Database, ReadableTable, TableDefinition, WriteTransaction};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info};

use crate::{
    StorageBackend, StoredAccount, StoredChat, StoredContact, StoredMucRoom, StoredPubSubNode,
};
use xmpp_core::{ChatMessage, MucMessage, PubSubItem};

/// Redb storage implementation
pub struct RedbStorage {
    db: Arc<Database>,
    path: PathBuf,
}

impl RedbStorage {
    pub fn new(path: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&path)?;

        let db_path = path.join("jabu.redb");
        let db = Database::create(&db_path)
            .map_err(|e| anyhow::anyhow!("Failed to create database: {}", e))?;

        let storage = Self {
            db: Arc::new(db),
            path,
        };

        storage.init_schema()?;
        Ok(storage)
    }

    fn init_schema(&self) -> Result<()> {
        let write_txn = self.db.begin_write()?;

        // Accounts table
        let _ = write_txn.open_table(ACCOUNTS_TABLE)?;

        // Contacts table
        let _ = write_txn.open_table(CONTACTS_TABLE)?;

        // Chats table
        let _ = write_txn.open_table(CHATS_TABLE)?;

        // Messages table
        let _ = write_txn.open_table(MESSAGES_TABLE)?;

        // MUC rooms table
        let _ = write_txn.open_table(MUC_ROOMS_TABLE)?;

        // MUC messages table
        let _ = write_txn.open_table(MUC_MESSAGES_TABLE)?;

        // PubSub nodes table
        let _ = write_txn.open_table(PUBSUB_NODES_TABLE)?;

        // PubSub items table
        let _ = write_txn.open_table(PUBSUB_ITEMS_TABLE)?;

        // Settings table
        let _ = write_txn.open_table(SETTINGS_TABLE)?;

        write_txn.commit()?;
        Ok(())
    }

    fn serialize<T: Serialize>(&self, value: &T) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(value)?)
    }

    fn deserialize<T: for<'de> Deserialize<'de>>(&self, data: &[u8]) -> Result<T> {
        Ok(serde_json::from_slice(data)?)
    }
}

fn scoped_key(account_jid: &str, peer_jid: &str) -> String {
    format!("{account_jid}\u{1f}{peer_jid}")
}

impl StorageBackend for RedbStorage {
    fn init(&self) -> Result<()> {
        self.init_schema()
    }

    fn close(&self) -> Result<()> {
        // Redb handles closing automatically on drop
        Ok(())
    }

    fn save_account(&self, account: &StoredAccount) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(ACCOUNTS_TABLE)?;
            let key = account.jid.as_str();
            let value = self.serialize(account)?;
            table.insert(key, value)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn get_account(&self, jid: &str) -> Result<Option<StoredAccount>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(ACCOUNTS_TABLE)?;
        if let Some(val) = table.get(jid)? {
            let account = self.deserialize(&val.value())?;
            Ok(Some(account))
        } else {
            Ok(None)
        }
    }

    fn list_accounts(&self) -> Result<Vec<StoredAccount>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(ACCOUNTS_TABLE)?;
        let mut accounts = Vec::new();
        for entry in table.iter()? {
            let (_, val) = entry?;
            let account = self.deserialize(&val.value())?;
            accounts.push(account);
        }
        Ok(accounts)
    }

    fn delete_account(&self, jid: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(ACCOUNTS_TABLE)?;
            table.remove(jid)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn save_contact(&self, contact: &StoredContact) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(CONTACTS_TABLE)?;
            let key = format!("{}@{}", contact.account_jid, contact.jid);
            let value = self.serialize(contact)?;
            table.insert(key.as_str(), value)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn get_contacts(&self, account_jid: &str) -> Result<Vec<StoredContact>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(CONTACTS_TABLE)?;
        let prefix = format!("{}@", account_jid);
        let mut contacts = Vec::new();
        for entry in table.iter()? {
            let (key, val) = entry?;
            let key_str = key.value();
            if key_str.starts_with(&prefix) {
                let contact = self.deserialize(&val.value())?;
                contacts.push(contact);
            }
        }
        Ok(contacts)
    }

    fn delete_contact(&self, account_jid: &str, contact_jid: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(CONTACTS_TABLE)?;
            let key = format!("{}@{}", account_jid, contact_jid);
            table.remove(key.as_str())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn save_chat(&self, chat: &StoredChat) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(CHATS_TABLE)?;
            let key = format!("{}@{}", chat.account_jid, chat.peer_jid);
            let value = self.serialize(chat)?;
            table.insert(key.as_str(), value)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn get_chats(&self, account_jid: &str) -> Result<Vec<StoredChat>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(CHATS_TABLE)?;
        let prefix = format!("{}@", account_jid);
        let mut chats = Vec::new();
        for entry in table.iter()? {
            let (key, val) = entry?;
            let key_str = key.value();
            if key_str.starts_with(&prefix) {
                let chat = self.deserialize(&val.value())?;
                chats.push(chat);
            }
        }
        Ok(chats)
    }

    fn save_message(&self, message: &ChatMessage) -> Result<()> {
        self.save_message_for("", message)
    }

    fn get_messages(
        &self,
        bare_jid: &str,
        limit: usize,
        before_id: Option<&str>,
    ) -> Result<Vec<ChatMessage>> {
        self.get_messages_for("", bare_jid, limit, before_id)
    }

    fn save_message_for(&self, account_jid: &str, message: &ChatMessage) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(MESSAGES_TABLE)?;
            let key = format!(
                "{}@{}",
                scoped_key(account_jid, &message.bare_jid),
                message.id
            );
            let value = self.serialize(message)?;
            table.insert(key.as_str(), value)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn get_messages_for(
        &self,
        account_jid: &str,
        bare_jid: &str,
        limit: usize,
        before_id: Option<&str>,
    ) -> Result<Vec<ChatMessage>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(MESSAGES_TABLE)?;
        let prefix = format!("{}@", scoped_key(account_jid, bare_jid));
        let mut messages = Vec::new();

        for entry in table.iter()? {
            let (key, val) = entry?;
            let key_str = key.value();
            if key_str.starts_with(&prefix) {
                let msg: ChatMessage = self.deserialize(&val.value())?;
                if let Some(before) = before_id {
                    if msg.id >= before.to_string() {
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

    fn clear_messages_for(&self, account_jid: &str, bare_jid: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(MESSAGES_TABLE)?;
            let prefix = format!("{}@", scoped_key(account_jid, bare_jid));
            let keys: Vec<String> = table
                .iter()?
                .filter_map(|entry| entry.ok().map(|(key, _)| key.value().to_string()))
                .filter(|key| key.starts_with(&prefix))
                .collect();
            for key in keys {
                table.remove(key.as_str())?;
            }
        }
        write_txn.commit()?;
        Ok(())
    }

    fn delete_message(&self, bare_jid: &str, message_id: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(MESSAGES_TABLE)?;
            let key = format!("{}@{}", scoped_key("", bare_jid), message_id);
            table.remove(key.as_str())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn save_muc_room(&self, room: &StoredMucRoom) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(MUC_ROOMS_TABLE)?;
            let key = format!("{}@{}", room.account_jid, room.jid);
            let value = self.serialize(room)?;
            table.insert(key.as_str(), value)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn get_muc_rooms(&self, account_jid: &str) -> Result<Vec<StoredMucRoom>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(MUC_ROOMS_TABLE)?;
        let prefix = format!("{}@", account_jid);
        let mut rooms = Vec::new();
        for entry in table.iter()? {
            let (key, val) = entry?;
            let key_str = key.value();
            if key_str.starts_with(&prefix) {
                let room = self.deserialize(&val.value())?;
                rooms.push(room);
            }
        }
        Ok(rooms)
    }

    fn save_muc_message(&self, message: &MucMessage) -> Result<()> {
        self.save_muc_message_for("", message)
    }

    fn get_muc_messages(
        &self,
        room_jid: &str,
        limit: usize,
        before_id: Option<&str>,
    ) -> Result<Vec<MucMessage>> {
        self.get_muc_messages_for("", room_jid, limit, before_id)
    }

    fn save_muc_message_for(&self, account_jid: &str, message: &MucMessage) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(MUC_MESSAGES_TABLE)?;
            let key = format!(
                "{}@{}",
                scoped_key(account_jid, &message.room_jid),
                message.id
            );
            let value = self.serialize(message)?;
            table.insert(key.as_str(), value)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn get_muc_messages_for(
        &self,
        account_jid: &str,
        room_jid: &str,
        limit: usize,
        before_id: Option<&str>,
    ) -> Result<Vec<MucMessage>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(MUC_MESSAGES_TABLE)?;
        let prefix = format!("{}@", scoped_key(account_jid, room_jid));
        let mut messages = Vec::new();

        for entry in table.iter()? {
            let (key, val) = entry?;
            let key_str = key.value();
            if key_str.starts_with(&prefix) {
                let msg: MucMessage = self.deserialize(&val.value())?;
                if let Some(before) = before_id {
                    if msg.id >= before.to_string() {
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

    fn clear_muc_messages_for(&self, account_jid: &str, room_jid: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(MUC_MESSAGES_TABLE)?;
            let prefix = format!("{}@", scoped_key(account_jid, room_jid));
            let keys: Vec<String> = table
                .iter()?
                .filter_map(|entry| entry.ok().map(|(key, _)| key.value().to_string()))
                .filter(|key| key.starts_with(&prefix))
                .collect();
            for key in keys {
                table.remove(key.as_str())?;
            }
        }
        write_txn.commit()?;
        Ok(())
    }

    fn save_pubsub_node(&self, node: &StoredPubSubNode) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(PUBSUB_NODES_TABLE)?;
            let key = format!("{}@{}", node.service_jid, node.id);
            let value = self.serialize(node)?;
            table.insert(key.as_str(), value)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn get_pubsub_nodes(&self, service_jid: &str) -> Result<Vec<StoredPubSubNode>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(PUBSUB_NODES_TABLE)?;
        let prefix = format!("{}@", service_jid);
        let mut nodes = Vec::new();
        for entry in table.iter()? {
            let (key, val) = entry?;
            let key_str = key.value();
            if key_str.starts_with(&prefix) {
                let node = self.deserialize(&val.value())?;
                nodes.push(node);
            }
        }
        Ok(nodes)
    }

    fn save_pubsub_item(&self, item: &PubSubItem) -> Result<()> {
        self.save_pubsub_item_for("", item)
    }

    fn get_pubsub_items(&self, node_id: &str, limit: usize) -> Result<Vec<PubSubItem>> {
        self.get_pubsub_items_for("", node_id, limit)
    }

    fn save_pubsub_item_for(&self, account_jid: &str, item: &PubSubItem) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(PUBSUB_ITEMS_TABLE)?;
            let key = format!("{}@{}", scoped_key(account_jid, &item.node_id), item.id);
            let value = self.serialize(item)?;
            table.insert(key.as_str(), value)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn get_pubsub_items_for(
        &self,
        account_jid: &str,
        node_id: &str,
        limit: usize,
    ) -> Result<Vec<PubSubItem>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(PUBSUB_ITEMS_TABLE)?;
        let prefix = format!("{}@", scoped_key(account_jid, node_id));
        let mut items = Vec::new();

        for entry in table.iter()? {
            let (key, val) = entry?;
            let key_str = key.value();
            if key_str.starts_with(&prefix) {
                let item: PubSubItem = self.deserialize(&val.value())?;
                items.push(item);
            }
        }

        items.sort_by(|a, b| b.published_at.cmp(&a.published_at));
        items.truncate(limit);
        Ok(items)
    }

    fn set_setting(&self, key: &str, value: &serde_json::Value) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SETTINGS_TABLE)?;
            let value = self.serialize(value)?;
            table.insert(key, value)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SETTINGS_TABLE)?;
        if let Some(val) = table.get(key)? {
            let value = self.deserialize(&val.value())?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use jid::Jid;
    use tempfile::TempDir;
    use xmpp_core::{ChatMessage, MessageType, MucMessage, PubSubItem};

    fn create_test_storage() -> (RedbStorage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let storage = RedbStorage::new(temp_dir.path().to_path_buf()).unwrap();
        (storage, temp_dir)
    }

    #[test]
    fn test_storage_new() {
        let (_storage, _temp_dir) = create_test_storage();
    }

    #[test]
    fn test_save_and_load_account() {
        let (storage, _temp_dir) = create_test_storage();

        let account = StoredAccount {
            jid: "user@example.com".to_string(),
            password_hash: Some("password".to_string()),
            resource: "jabu".to_string(),
            priority: 0,
            auto_reconnect: true,
            created_at: chrono::Utc::now(),
            last_login: Some(chrono::Utc::now()),
        };

        storage.save_account(&account).unwrap();
        let loaded = storage.get_account("user@example.com").unwrap();

        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.jid, "user@example.com");
        assert_eq!(loaded.password_hash, Some("password".to_string()));
    }

    #[test]
    fn test_save_and_load_contact() {
        let (storage, _temp_dir) = create_test_storage();

        let contact = StoredContact {
            account_jid: "user@example.com".to_string(),
            jid: "contact@example.com".to_string(),
            name: Some("Test Contact".to_string()),
            groups: vec!["Friends".to_string()],
            subscription: xmpp_core::roster::SubscriptionState::Both,
            avatar_hash: None,
            is_blocked: false,
            updated_at: chrono::Utc::now(),
        };

        storage.save_contact(&contact).unwrap();
        let contacts = storage.get_contacts("user@example.com").unwrap();

        assert_eq!(contacts.len(), 1);
        assert_eq!(contacts[0].jid, "contact@example.com");
        assert_eq!(contacts[0].name, Some("Test Contact".to_string()));
    }

    #[test]
    fn test_save_and_load_chat() {
        let (storage, _temp_dir) = create_test_storage();

        let chat = StoredChat {
            account_jid: "user@example.com".to_string(),
            peer_jid: "chat@example.com".to_string(),
            unread_count: 0,
            last_message_id: None,
            last_activity: chrono::Utc::now(),
            is_archived: false,
            is_pinned: false,
            notifications_enabled: true,
        };

        storage.save_chat(&chat).unwrap();
        let chats = storage.get_chats("user@example.com").unwrap();

        assert_eq!(chats.len(), 1);
        assert_eq!(chats[0].peer_jid, "chat@example.com");
    }

    #[test]
    fn test_save_and_load_messages() {
        let (storage, _temp_dir) = create_test_storage();

        let jid = "chat@example.com".to_string();
        let messages = vec![
            ChatMessage::new(
                jid.clone(),
                jid.parse::<Jid>().unwrap(),
                jid.parse::<Jid>().unwrap(),
                false,
                Some("Message 1".to_string()),
            ),
            ChatMessage::new(
                jid.clone(),
                jid.parse::<Jid>().unwrap(),
                jid.parse::<Jid>().unwrap(),
                true,
                Some("Message 2".to_string()),
            ),
        ];

        for msg in &messages {
            storage.save_message(msg).unwrap();
        }
        let loaded = storage.get_messages(&jid, 100, None).unwrap();

        assert_eq!(loaded.len(), 2);
        // Messages may be returned in any order, check both exist
        let bodies: Vec<_> = loaded
            .iter()
            .map(|m| m.body.as_ref().unwrap().clone())
            .collect();
        assert!(bodies.contains(&"Message 1".to_string()));
        assert!(bodies.contains(&"Message 2".to_string()));
    }

    #[test]
    fn test_save_and_load_muc_room() {
        let (storage, _temp_dir) = create_test_storage();

        let room = StoredMucRoom {
            account_jid: "user@example.com".to_string(),
            jid: "room@conference.example.com".to_string(),
            name: "Test Room".to_string(),
            nick: "mynick".to_string(),
            is_joined: true,
            is_favorite: false,
            notifications_enabled: true,
            last_activity: chrono::Utc::now(),
        };

        storage.save_muc_room(&room).unwrap();
        let rooms = storage.get_muc_rooms("user@example.com").unwrap();

        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].jid, "room@conference.example.com");
        assert_eq!(rooms[0].name, "Test Room");
    }

    #[test]
    fn test_save_and_load_muc_messages() {
        let (storage, _temp_dir) = create_test_storage();

        let jid = "room@conference.example.com".to_string();
        let messages = vec![
            MucMessage::new(
                jid.clone(),
                "sender1".to_string(),
                None,
                false,
                Some("MUC Message 1".to_string()),
            ),
            MucMessage::new(
                jid.clone(),
                "sender2".to_string(),
                None,
                false,
                Some("MUC Message 2".to_string()),
            ),
        ];

        for msg in &messages {
            storage.save_muc_message(msg).unwrap();
        }
        let loaded = storage.get_muc_messages(&jid, 100, None).unwrap();

        assert_eq!(loaded.len(), 2);
        // Messages may be returned in any order, check both exist
        let nicks: Vec<_> = loaded.iter().map(|m| m.from_nick.clone()).collect();
        assert!(nicks.contains(&"sender1".to_string()));
        assert!(nicks.contains(&"sender2".to_string()));
    }

    #[test]
    fn test_settings() {
        let (storage, _temp_dir) = create_test_storage();

        storage
            .set_setting("theme", &serde_json::json!("dark"))
            .unwrap();
        let value = storage.get_setting("theme").unwrap();

        assert!(value.is_some());
        assert_eq!(value.unwrap(), serde_json::json!("dark"));

        // Test non-existent key
        let missing = storage.get_setting("nonexistent").unwrap();
        assert!(missing.is_none());
    }
}
