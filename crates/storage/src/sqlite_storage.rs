//! SQLite storage backend.

use anyhow::Result;
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};

use crate::{
    StorageBackend, StoredAccount, StoredChat, StoredContact, StoredMucRoom, StoredPubSubNode,
};

pub struct SqliteStorage {
    conn: Mutex<Connection>,
    path: PathBuf,
}

impl SqliteStorage {
    pub fn new(path: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&path)?;
        let db_path = if path.extension().and_then(|e| e.to_str()) == Some("sqlite") {
            path.clone()
        } else {
            path.join("jabu.sqlite")
        };
        let conn = Connection::open(&db_path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let storage = Self {
            conn: Mutex::new(conn),
            path,
        };
        storage.init_schema()?;
        Ok(storage)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS accounts (
                jid TEXT PRIMARY KEY,
                data BLOB NOT NULL
            );
            CREATE TABLE IF NOT EXISTS contacts (
                account_jid TEXT NOT NULL,
                jid TEXT NOT NULL,
                data BLOB NOT NULL,
                PRIMARY KEY (account_jid, jid)
            );
            CREATE TABLE IF NOT EXISTS chats (
                account_jid TEXT NOT NULL,
                peer_jid TEXT NOT NULL,
                data BLOB NOT NULL,
                PRIMARY KEY (account_jid, peer_jid)
            );
            CREATE TABLE IF NOT EXISTS messages (
                bare_jid TEXT NOT NULL,
                id TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                body TEXT NOT NULL DEFAULT '',
                data BLOB NOT NULL,
                PRIMARY KEY (bare_jid, id)
            );
            CREATE INDEX IF NOT EXISTS messages_bare_time
                ON messages (bare_jid, timestamp DESC);
            CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
                bare_jid UNINDEXED,
                message_id UNINDEXED,
                body,
                tokenize = 'unicode61'
            );
            CREATE TABLE IF NOT EXISTS muc_rooms (
                account_jid TEXT NOT NULL,
                jid TEXT NOT NULL,
                data BLOB NOT NULL,
                PRIMARY KEY (account_jid, jid)
            );
            CREATE TABLE IF NOT EXISTS muc_messages (
                room_jid TEXT NOT NULL,
                id TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                body TEXT NOT NULL DEFAULT '',
                data BLOB NOT NULL,
                PRIMARY KEY (room_jid, id)
            );
            CREATE INDEX IF NOT EXISTS muc_messages_room_time
                ON muc_messages (room_jid, timestamp DESC);
            CREATE VIRTUAL TABLE IF NOT EXISTS muc_messages_fts USING fts5(
                room_jid UNINDEXED,
                message_id UNINDEXED,
                body,
                tokenize = 'unicode61'
            );
            CREATE TABLE IF NOT EXISTS pubsub_nodes (
                service_jid TEXT NOT NULL,
                id TEXT NOT NULL,
                data BLOB NOT NULL,
                PRIMARY KEY (service_jid, id)
            );
            CREATE TABLE IF NOT EXISTS pubsub_items (
                node_id TEXT NOT NULL,
                id TEXT NOT NULL,
                published_at INTEGER NOT NULL,
                data BLOB NOT NULL,
                PRIMARY KEY (node_id, id)
            );
            CREATE INDEX IF NOT EXISTS pubsub_items_node_time
                ON pubsub_items (node_id, published_at DESC);
            CREATE TABLE IF NOT EXISTS spaces (
                account_jid TEXT NOT NULL,
                server TEXT NOT NULL,
                node TEXT NOT NULL,
                data BLOB NOT NULL,
                PRIMARY KEY (account_jid, server, node)
            );
            CREATE TABLE IF NOT EXISTS calls (
                account_jid TEXT NOT NULL,
                id TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                data BLOB NOT NULL,
                PRIMARY KEY (account_jid, id)
            );
            CREATE INDEX IF NOT EXISTS calls_account_time
                ON calls (account_jid, started_at DESC);
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value BLOB NOT NULL
            );
            "#,
        )?;
        Ok(())
    }
}

fn timestamp_millis(t: &chrono::DateTime<chrono::Utc>) -> i64 {
    t.timestamp_millis()
}

fn scoped_key(account_jid: &str, peer_jid: &str) -> String {
    format!("{account_jid}\u{1f}{peer_jid}")
}

fn fts_query(query: &str) -> String {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect();
    if terms.is_empty() {
        "\"\"".to_string()
    } else {
        terms.join(" ")
    }
}

impl StorageBackend for SqliteStorage {
    fn init(&self) -> Result<()> {
        self.init_schema()
    }

    fn close(&self) -> Result<()> {
        Ok(())
    }

    fn save_account(&self, account: &StoredAccount) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO accounts (jid, data) VALUES (?1, ?2)
             ON CONFLICT(jid) DO UPDATE SET data = excluded.data",
            params![account.jid, serde_json::to_vec(account)?],
        )?;
        Ok(())
    }

    fn get_account(&self, jid: &str) -> Result<Option<StoredAccount>> {
        let conn = self.conn.lock();
        let data: Option<Vec<u8>> = conn
            .query_row("SELECT data FROM accounts WHERE jid = ?1", [jid], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(data.map(|d| serde_json::from_slice(&d)).transpose()?)
    }

    fn list_accounts(&self) -> Result<Vec<StoredAccount>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT data FROM accounts ORDER BY jid")?;
        let rows = stmt.query_map([], |r| r.get::<_, Vec<u8>>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_slice(&row?)?);
        }
        Ok(out)
    }

    fn delete_account(&self, jid: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM accounts WHERE jid = ?1", [jid])?;
        Ok(())
    }

    fn save_contact(&self, contact: &StoredContact) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO contacts (account_jid, jid, data) VALUES (?1, ?2, ?3)
             ON CONFLICT(account_jid, jid) DO UPDATE SET data = excluded.data",
            params![
                contact.account_jid,
                contact.jid,
                serde_json::to_vec(contact)?
            ],
        )?;
        Ok(())
    }

    fn get_contacts(&self, account_jid: &str) -> Result<Vec<StoredContact>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT data FROM contacts WHERE account_jid = ?1")?;
        let rows = stmt.query_map([account_jid], |r| r.get::<_, Vec<u8>>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_slice(&row?)?);
        }
        Ok(out)
    }

    fn delete_contact(&self, account_jid: &str, contact_jid: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM contacts WHERE account_jid = ?1 AND jid = ?2",
            params![account_jid, contact_jid],
        )?;
        Ok(())
    }

    fn save_chat(&self, chat: &StoredChat) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO chats (account_jid, peer_jid, data) VALUES (?1, ?2, ?3)
             ON CONFLICT(account_jid, peer_jid) DO UPDATE SET data = excluded.data",
            params![chat.account_jid, chat.peer_jid, serde_json::to_vec(chat)?],
        )?;
        Ok(())
    }

    fn get_chats(&self, account_jid: &str) -> Result<Vec<StoredChat>> {
        let conn = self.conn.lock();
        let mut stmt =
            conn.prepare("SELECT data FROM chats WHERE account_jid = ?1 ORDER BY peer_jid")?;
        let rows = stmt.query_map([account_jid], |r| r.get::<_, Vec<u8>>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_slice(&row?)?);
        }
        Ok(out)
    }

    fn save_call(&self, call: &crate::StoredCall) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO calls (account_jid, id, started_at, data) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(account_jid, id) DO UPDATE SET started_at = excluded.started_at, data = excluded.data",
            params![
                call.account_jid,
                call.id,
                call.started_at.timestamp(),
                serde_json::to_vec(call)?
            ],
        )?;
        Ok(())
    }

    fn get_calls(&self, account_jid: &str, limit: usize) -> Result<Vec<crate::StoredCall>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT data FROM calls WHERE account_jid = ?1 ORDER BY started_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![account_jid, limit as i64], |r| {
            r.get::<_, Vec<u8>>(0)
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_slice(&row?)?);
        }
        Ok(out)
    }

    fn save_message(&self, message: &xmpp_core::ChatMessage) -> Result<()> {
        self.save_message_for("", message)
    }

    fn get_messages(
        &self,
        bare_jid: &str,
        limit: usize,
        before_id: Option<&str>,
    ) -> Result<Vec<xmpp_core::ChatMessage>> {
        self.get_messages_for("", bare_jid, limit, before_id)
    }

    fn save_message_for(&self, account_jid: &str, message: &xmpp_core::ChatMessage) -> Result<()> {
        let conn = self.conn.lock();
        let key = scoped_key(account_jid, &message.bare_jid);
        conn.execute(
            "INSERT INTO messages (bare_jid, id, timestamp, body, data) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(bare_jid, id) DO UPDATE SET
                timestamp = excluded.timestamp, body = excluded.body, data = excluded.data",
            params![
                key,
                message.id,
                timestamp_millis(&message.timestamp),
                message.display_body(),
                serde_json::to_vec(message)?
            ],
        )?;
        conn.execute(
            "DELETE FROM messages_fts WHERE bare_jid = ?1 AND message_id = ?2",
            params![key, message.id],
        )?;
        conn.execute(
            "INSERT INTO messages_fts (bare_jid, message_id, body) VALUES (?1, ?2, ?3)",
            params![key, message.id, message.display_body()],
        )?;
        Ok(())
    }

    fn get_messages_for(
        &self,
        account_jid: &str,
        bare_jid: &str,
        limit: usize,
        before_id: Option<&str>,
    ) -> Result<Vec<xmpp_core::ChatMessage>> {
        let conn = self.conn.lock();
        let key = scoped_key(account_jid, bare_jid);
        let before_ts: Option<i64> = match before_id {
            Some(id) => conn
                .query_row(
                    "SELECT timestamp FROM messages WHERE bare_jid = ?1 AND id = ?2",
                    params![key, id],
                    |r| r.get(0),
                )
                .optional()?,
            None => None,
        };
        let mut out = Vec::new();
        if let Some(ts) = before_ts {
            let mut stmt = conn.prepare(
                "SELECT data FROM messages
                 WHERE bare_jid = ?1 AND timestamp < ?2
                 ORDER BY timestamp DESC LIMIT ?3",
            )?;
            let rows =
                stmt.query_map(params![key, ts, limit as i64], |r| r.get::<_, Vec<u8>>(0))?;
            for row in rows {
                out.push(serde_json::from_slice(&row?)?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT data FROM messages
                 WHERE bare_jid = ?1
                 ORDER BY timestamp DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![key, limit as i64], |r| r.get::<_, Vec<u8>>(0))?;
            for row in rows {
                out.push(serde_json::from_slice(&row?)?);
            }
        }
        Ok(out)
    }

    fn delete_message(&self, bare_jid: &str, message_id: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM messages WHERE bare_jid = ?1 AND id = ?2",
            params![bare_jid, message_id],
        )?;
        Ok(())
    }

    fn search_messages(&self, query: &str, limit: usize) -> Result<Vec<xmpp_core::ChatMessage>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT m.data FROM messages_fts f
             JOIN messages m ON m.bare_jid = f.bare_jid AND m.id = f.message_id
             WHERE messages_fts MATCH ?1
             ORDER BY rank LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![fts_query(query), limit as i64], |r| {
            r.get::<_, Vec<u8>>(0)
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_slice(&row?)?);
        }
        Ok(out)
    }

    fn search_messages_for(
        &self,
        account_jid: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<xmpp_core::ChatMessage>> {
        let conn = self.conn.lock();
        let prefix = format!(
            "{}\u{1f}",
            account_jid.split('/').next().unwrap_or(account_jid)
        );
        let mut stmt = conn.prepare(
            "SELECT m.data FROM messages_fts f
             JOIN messages m ON m.bare_jid = f.bare_jid AND m.id = f.message_id
             WHERE messages_fts MATCH ?1
               AND substr(f.bare_jid, 1, length(?2)) = ?2
             ORDER BY rank LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![fts_query(query), prefix, limit as i64], |r| {
            r.get::<_, Vec<u8>>(0)
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_slice(&row?)?);
        }
        Ok(out)
    }

    fn clear_messages_for(&self, account_jid: &str, bare_jid: &str) -> Result<()> {
        let conn = self.conn.lock();
        let key = scoped_key(account_jid, bare_jid);
        conn.execute("DELETE FROM messages_fts WHERE bare_jid = ?1", [&key])?;
        conn.execute("DELETE FROM messages WHERE bare_jid = ?1", [&key])?;
        Ok(())
    }

    fn save_muc_room(&self, room: &StoredMucRoom) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO muc_rooms (account_jid, jid, data) VALUES (?1, ?2, ?3)
             ON CONFLICT(account_jid, jid) DO UPDATE SET data = excluded.data",
            params![room.account_jid, room.jid, serde_json::to_vec(room)?],
        )?;
        Ok(())
    }

    fn get_muc_rooms(&self, account_jid: &str) -> Result<Vec<StoredMucRoom>> {
        let conn = self.conn.lock();
        let mut stmt =
            conn.prepare("SELECT data FROM muc_rooms WHERE account_jid = ?1 ORDER BY jid")?;
        let rows = stmt.query_map([account_jid], |r| r.get::<_, Vec<u8>>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_slice(&row?)?);
        }
        Ok(out)
    }

    fn save_muc_message(&self, message: &xmpp_core::MucMessage) -> Result<()> {
        self.save_muc_message_for("", message)
    }

    fn get_muc_messages(
        &self,
        room_jid: &str,
        limit: usize,
        before_id: Option<&str>,
    ) -> Result<Vec<xmpp_core::MucMessage>> {
        self.get_muc_messages_for("", room_jid, limit, before_id)
    }

    fn save_muc_message_for(
        &self,
        account_jid: &str,
        message: &xmpp_core::MucMessage,
    ) -> Result<()> {
        let conn = self.conn.lock();
        let key = scoped_key(account_jid, &message.room_jid);
        conn.execute(
            "INSERT INTO muc_messages (room_jid, id, timestamp, body, data) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(room_jid, id) DO UPDATE SET
                timestamp = excluded.timestamp, body = excluded.body, data = excluded.data",
            params![
                key,
                message.id,
                timestamp_millis(&message.timestamp),
                message.body.clone().unwrap_or_default(),
                serde_json::to_vec(message)?
            ],
        )?;
        conn.execute(
            "DELETE FROM muc_messages_fts WHERE room_jid = ?1 AND message_id = ?2",
            params![key, message.id],
        )?;
        conn.execute(
            "INSERT INTO muc_messages_fts (room_jid, message_id, body) VALUES (?1, ?2, ?3)",
            params![key, message.id, message.body.clone().unwrap_or_default()],
        )?;
        Ok(())
    }

    fn get_muc_messages_for(
        &self,
        account_jid: &str,
        room_jid: &str,
        limit: usize,
        before_id: Option<&str>,
    ) -> Result<Vec<xmpp_core::MucMessage>> {
        let conn = self.conn.lock();
        let key = scoped_key(account_jid, room_jid);
        let before_ts: Option<i64> = match before_id {
            Some(id) => conn
                .query_row(
                    "SELECT timestamp FROM muc_messages WHERE room_jid = ?1 AND id = ?2",
                    params![key, id],
                    |r| r.get(0),
                )
                .optional()?,
            None => None,
        };
        let mut out = Vec::new();
        if let Some(ts) = before_ts {
            let mut stmt = conn.prepare(
                "SELECT data FROM muc_messages
                 WHERE room_jid = ?1 AND timestamp < ?2
                 ORDER BY timestamp DESC LIMIT ?3",
            )?;
            let rows =
                stmt.query_map(params![key, ts, limit as i64], |r| r.get::<_, Vec<u8>>(0))?;
            for row in rows {
                out.push(serde_json::from_slice(&row?)?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT data FROM muc_messages
                 WHERE room_jid = ?1
                 ORDER BY timestamp DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![key, limit as i64], |r| r.get::<_, Vec<u8>>(0))?;
            for row in rows {
                out.push(serde_json::from_slice(&row?)?);
            }
        }
        Ok(out)
    }

    fn search_muc_messages(&self, query: &str, limit: usize) -> Result<Vec<xmpp_core::MucMessage>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT m.data FROM muc_messages_fts f
             JOIN muc_messages m ON m.room_jid = f.room_jid AND m.id = f.message_id
             WHERE muc_messages_fts MATCH ?1
             ORDER BY rank LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![fts_query(query), limit as i64], |r| {
            r.get::<_, Vec<u8>>(0)
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_slice(&row?)?);
        }
        Ok(out)
    }

    fn search_muc_messages_for(
        &self,
        account_jid: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<xmpp_core::MucMessage>> {
        let conn = self.conn.lock();
        let prefix = format!(
            "{}\u{1f}",
            account_jid.split('/').next().unwrap_or(account_jid)
        );
        let mut stmt = conn.prepare(
            "SELECT m.data FROM muc_messages_fts f
             JOIN muc_messages m ON m.room_jid = f.room_jid AND m.id = f.message_id
             WHERE muc_messages_fts MATCH ?1
               AND substr(f.room_jid, 1, length(?2)) = ?2
             ORDER BY rank LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![fts_query(query), prefix, limit as i64], |r| {
            r.get::<_, Vec<u8>>(0)
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_slice(&row?)?);
        }
        Ok(out)
    }

    fn clear_muc_messages_for(&self, account_jid: &str, room_jid: &str) -> Result<()> {
        let conn = self.conn.lock();
        let key = scoped_key(account_jid, room_jid);
        conn.execute("DELETE FROM muc_messages_fts WHERE room_jid = ?1", [&key])?;
        conn.execute("DELETE FROM muc_messages WHERE room_jid = ?1", [&key])?;
        Ok(())
    }

    fn save_pubsub_node(&self, node: &StoredPubSubNode) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO pubsub_nodes (service_jid, id, data) VALUES (?1, ?2, ?3)
             ON CONFLICT(service_jid, id) DO UPDATE SET data = excluded.data",
            params![node.service_jid, node.id, serde_json::to_vec(node)?],
        )?;
        Ok(())
    }

    fn get_pubsub_nodes(&self, service_jid: &str) -> Result<Vec<StoredPubSubNode>> {
        let conn = self.conn.lock();
        let mut stmt =
            conn.prepare("SELECT data FROM pubsub_nodes WHERE service_jid = ?1 ORDER BY id")?;
        let rows = stmt.query_map([service_jid], |r| r.get::<_, Vec<u8>>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_slice(&row?)?);
        }
        Ok(out)
    }

    fn save_pubsub_item(&self, item: &xmpp_core::PubSubItem) -> Result<()> {
        self.save_pubsub_item_for("", item)
    }

    fn get_pubsub_items(&self, node_id: &str, limit: usize) -> Result<Vec<xmpp_core::PubSubItem>> {
        self.get_pubsub_items_for("", node_id, limit)
    }

    fn save_pubsub_item_for(&self, account_jid: &str, item: &xmpp_core::PubSubItem) -> Result<()> {
        let conn = self.conn.lock();
        let key = scoped_key(account_jid, &item.node_id);
        conn.execute(
            "INSERT INTO pubsub_items (node_id, id, published_at, data) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(node_id, id) DO UPDATE SET
                published_at = excluded.published_at, data = excluded.data",
            params![
                key,
                item.id,
                timestamp_millis(&item.published_at),
                serde_json::to_vec(item)?
            ],
        )?;
        Ok(())
    }

    fn get_pubsub_items_for(
        &self,
        account_jid: &str,
        node_id: &str,
        limit: usize,
    ) -> Result<Vec<xmpp_core::PubSubItem>> {
        let conn = self.conn.lock();
        let key = scoped_key(account_jid, node_id);
        let mut stmt = conn.prepare(
            "SELECT data FROM pubsub_items
             WHERE node_id = ?1 ORDER BY published_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![key, limit as i64], |r| r.get::<_, Vec<u8>>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_slice(&row?)?);
        }
        Ok(out)
    }

    fn save_space(&self, account_jid: &str, space: &xmpp_core::Space) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO spaces (account_jid, server, node, data) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(account_jid, server, node) DO UPDATE SET data = excluded.data",
            params![
                account_jid,
                space.id.server,
                space.id.node,
                serde_json::to_vec(space)?
            ],
        )?;
        Ok(())
    }

    fn get_spaces(&self, account_jid: &str) -> Result<Vec<xmpp_core::Space>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT data FROM spaces WHERE account_jid = ?1")?;
        let rows = stmt.query_map([account_jid], |row| row.get::<_, Vec<u8>>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_slice(&row?)?);
        }
        Ok(out)
    }

    fn delete_space(&self, account_jid: &str, server: &str, node: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM spaces WHERE account_jid = ?1 AND server = ?2 AND node = ?3",
            params![account_jid, server, node],
        )?;
        Ok(())
    }

    fn set_setting(&self, key: &str, value: &serde_json::Value) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, serde_json::to_vec(value)?],
        )?;
        Ok(())
    }

    fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>> {
        let conn = self.conn.lock();
        let data: Option<Vec<u8>> = conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(data.map(|d| serde_json::from_slice(&d)).transpose()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{StoredChat, StoredContact};
    use jid::Jid;
    use xmpp_core::roster::SubscriptionState;
    use xmpp_core::{ChatMessage, MucMessage};

    fn storage() -> (SqliteStorage, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let storage = SqliteStorage::new(dir.path().to_path_buf()).unwrap();
        (storage, dir)
    }

    #[test]
    fn call_history_roundtrip() {
        let (s, _dir) = storage();
        let call = crate::StoredCall {
            account_jid: "bob@localhost".into(),
            id: "call-1".into(),
            peer_jid: "alice@localhost".into(),
            direction: "outgoing".into(),
            kind: "audio".into(),
            state: "success".into(),
            started_at: chrono::Utc::now(),
            ended_at: Some(chrono::Utc::now()),
            duration_secs: 42,
            reason: Some("success".into()),
        };
        s.save_call(&call).unwrap();
        let calls = s.get_calls("bob@localhost", 10).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].peer_jid, "alice@localhost");
        assert_eq!(calls[0].duration_secs, 42);
    }

    #[test]
    fn settings_roundtrip() {
        let (s, _dir) = storage();
        s.set_setting("ui.theme", &serde_json::json!("dark"))
            .unwrap();
        assert_eq!(
            s.get_setting("ui.theme").unwrap(),
            Some(serde_json::json!("dark"))
        );
    }

    #[test]
    fn contact_roundtrip() {
        let (s, _dir) = storage();
        let contact = StoredContact {
            account_jid: "me@example.com".into(),
            jid: "alice@example.com".into(),
            name: Some("Alice".into()),
            groups: vec!["Friends".into()],
            subscription: SubscriptionState::Both,
            avatar_hash: None,
            is_blocked: false,
            updated_at: chrono::Utc::now(),
        };
        s.save_contact(&contact).unwrap();
        let loaded = s.get_contacts("me@example.com").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name.as_deref(), Some("Alice"));
        assert_eq!(loaded[0].groups, vec!["Friends".to_string()]);
        assert_eq!(loaded[0].subscription, SubscriptionState::Both);
    }

    #[test]
    fn messages_are_paginated_newest_first() {
        let (s, _dir) = storage();
        let peer: Jid = "alice@example.com".parse().unwrap();
        for i in 0..5 {
            let mut message = ChatMessage::new(
                peer.to_bare().to_string(),
                peer.clone(),
                peer.clone(),
                false,
                Some(format!("message {i}")),
            );
            message.id = format!("m{i}");
            message.timestamp = chrono::Utc::now() + chrono::Duration::seconds(i);
            s.save_message(&message).unwrap();
        }
        let page = s
            .get_messages(&peer.to_bare().to_string(), 2, None)
            .unwrap();
        assert_eq!(page.len(), 2);
        assert_eq!(page[0].id, "m4");
        assert_eq!(page[1].id, "m3");

        let older = s
            .get_messages(&peer.to_bare().to_string(), 10, Some("m3"))
            .unwrap();
        assert_eq!(
            older.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            vec!["m2", "m1", "m0"]
        );
    }

    #[test]
    fn messages_are_scoped_per_account() {
        let (s, _dir) = storage();
        let peer: Jid = "alice@example.com".parse().unwrap();
        for (account, body) in [
            ("one@example.com", "from one"),
            ("two@example.com", "from two"),
        ] {
            let mut message = ChatMessage::new(
                peer.to_bare().to_string(),
                peer.clone(),
                peer.clone(),
                false,
                Some(body.into()),
            );
            message.id = "same-id".into();
            s.save_message_for(account, &message).unwrap();
        }

        let one = s
            .get_messages_for("one@example.com", &peer.to_bare().to_string(), 10, None)
            .unwrap();
        let two = s
            .get_messages_for("two@example.com", &peer.to_bare().to_string(), 10, None)
            .unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(two.len(), 1);
        assert_eq!(one[0].body.as_deref(), Some("from one"));
        assert_eq!(two[0].body.as_deref(), Some("from two"));
    }

    #[test]
    fn message_search_uses_fts() {
        let (s, _dir) = storage();
        let peer: Jid = "alice@example.com".parse().unwrap();
        let mut message = ChatMessage::new(
            peer.to_bare().to_string(),
            peer.clone(),
            peer.clone(),
            false,
            Some("the quick brown fox jumps".into()),
        );
        message.id = "search-1".into();
        s.save_message(&message).unwrap();
        let found = s.search_messages("brown", 10).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "search-1");
    }

    #[test]
    fn search_and_clear_are_account_scoped() {
        let (s, _dir) = storage();
        let peer: Jid = "alice@example.com".parse().unwrap();
        for (account, body, id) in [
            ("one@example.com", "shared needle one", "one-1"),
            ("two@example.com", "shared needle two", "two-1"),
        ] {
            let mut message = ChatMessage::new(
                peer.to_bare().to_string(),
                peer.clone(),
                peer.clone(),
                false,
                Some(body.into()),
            );
            message.id = id.into();
            s.save_message_for(account, &message).unwrap();
        }
        let one = s
            .search_messages_for("one@example.com", "needle", 10)
            .unwrap();
        let two = s
            .search_messages_for("two@example.com", "needle", 10)
            .unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(two.len(), 1);
        assert_eq!(one[0].id, "one-1");
        assert_eq!(two[0].id, "two-1");

        s.clear_messages_for("one@example.com", &peer.to_bare().to_string())
            .unwrap();
        assert!(s
            .get_messages_for("one@example.com", &peer.to_bare().to_string(), 10, None)
            .unwrap()
            .is_empty());
        assert_eq!(
            s.get_messages_for("two@example.com", &peer.to_bare().to_string(), 10, None)
                .unwrap()
                .len(),
            1
        );
        assert!(s
            .search_messages_for("one@example.com", "needle", 10)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn muc_search_and_clear_are_account_scoped() {
        let (s, _dir) = storage();
        let room = "room@conference.example.com";
        for (account, body, id) in [
            ("one@example.com", "room needle one", "m-one"),
            ("two@example.com", "room needle two", "m-two"),
        ] {
            let mut message =
                MucMessage::new(room.into(), "alice".into(), None, false, Some(body.into()));
            message.id = id.into();
            s.save_muc_message_for(account, &message).unwrap();
        }
        let one = s
            .search_muc_messages_for("one@example.com", "needle", 10)
            .unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].id, "m-one");
        s.clear_muc_messages_for("one@example.com", room).unwrap();
        assert!(s
            .get_muc_messages_for("one@example.com", room, 10, None)
            .unwrap()
            .is_empty());
        assert_eq!(
            s.get_muc_messages_for("two@example.com", room, 10, None)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn chat_roundtrip() {
        let (s, _dir) = storage();
        let chat = StoredChat {
            account_jid: "me@example.com".into(),
            peer_jid: "alice@example.com".into(),
            unread_count: 2,
            last_message_id: Some("m1".into()),
            last_activity: chrono::Utc::now(),
            is_archived: false,
            is_pinned: true,
            notifications_enabled: true,
        };
        s.save_chat(&chat).unwrap();
        let loaded = s.get_chats("me@example.com").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].unread_count, 2);
        assert!(loaded[0].is_pinned);
        let one = s
            .get_chat("me@example.com", "alice@example.com")
            .unwrap()
            .unwrap();
        assert_eq!(one.last_message_id.as_deref(), Some("m1"));
    }

    #[test]
    fn drafts_roundtrip_and_clear() {
        let (s, _dir) = storage();
        crate::save_draft(&s, "me@example.com/res", "alice@example.com", "hello").unwrap();
        assert_eq!(
            crate::load_draft(&s, "me@example.com", "alice@example.com").as_deref(),
            Some("hello")
        );
        crate::save_draft(&s, "me@example.com", "alice@example.com", "   ").unwrap();
        assert!(crate::load_draft(&s, "me@example.com", "alice@example.com").is_none());
        crate::save_draft(&s, "me@example.com", "alice@example.com", "again").unwrap();
        crate::save_draft(&s, "other@example.com", "alice@example.com", "separate").unwrap();
        assert_eq!(
            crate::load_draft(&s, "other@example.com", "alice@example.com").as_deref(),
            Some("separate")
        );
        crate::clear_draft(&s, "me@example.com", "alice@example.com").unwrap();
        assert!(crate::load_draft(&s, "me@example.com", "alice@example.com").is_none());
        assert_eq!(
            crate::load_draft(&s, "other@example.com", "alice@example.com").as_deref(),
            Some("separate")
        );
    }

    #[test]
    fn test_space_roundtrip() {
        let (storage, _temp_dir) = storage();
        let space = xmpp_core::Space {
            id: xmpp_core::SpaceId::new("spaces.example.org", "dev"),
            name: "Developers".to_string(),
            description: "The developers corner".to_string(),
            rooms: vec![xmpp_core::SpaceRoom {
                conference: "dev@conference.example.org".to_string(),
                name: "dev".to_string(),
                autojoin: true,
                ..xmpp_core::SpaceRoom::default()
            }],
            ..xmpp_core::Space::default()
        };

        storage.save_space("user@example.com", &space).unwrap();
        let spaces = storage.get_spaces("user@example.com").unwrap();
        assert_eq!(spaces.len(), 1);
        assert_eq!(spaces[0].name, "Developers");
        assert_eq!(spaces[0].rooms.len(), 1);
        assert!(spaces[0].rooms[0].autojoin);

        storage
            .delete_space("user@example.com", "spaces.example.org", "dev")
            .unwrap();
        assert!(storage.get_spaces("user@example.com").unwrap().is_empty());
    }
}
