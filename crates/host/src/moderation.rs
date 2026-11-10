//! Reports, moderation state and the audit log.
//!
//! The Host keeps its own moderation data in SQLite: public reports, the set of
//! hidden targets, the action history and the audit log. Moderation never
//! changes the underlying XMPP content; it only hides it from this Host.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use jid::Jid;
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use serde::Serialize;

use crate::auth::{hash_secret, new_secret, Authenticated, Role, TokenSummary, SESSION_TTL_HOURS};
use crate::content::{
    BlogProfile, CommunityPage, ContactProfile, ContentError, ContentSource, Post, SpacePage, Story,
};
use crate::settings::RuntimeSettings;

/// State of a report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReportStatus {
    Open,
    Hidden,
    Dismissed,
}

impl ReportStatus {
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Hidden => "hidden",
            Self::Dismissed => "dismissed",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "hidden" => Self::Hidden,
            "dismissed" => Self::Dismissed,
            _ => Self::Open,
        }
    }
}

/// One public report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    pub id: String,
    pub target: String,
    pub reason: String,
    pub details: Option<String>,
    pub reporter: Option<String>,
    pub created_at: DateTime<Utc>,
    pub status: ReportStatus,
}

/// One audit entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditEntry {
    pub id: i64,
    pub actor: String,
    pub action: String,
    pub target: String,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// SQLite-backed moderation store.
pub struct ModerationStore {
    connection: Mutex<Connection>,
}

impl ModerationStore {
    /// Open or create the moderation database.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot open or migrate the file.
    pub fn open(path: &str) -> Result<Self> {
        let connection = Connection::open(path)
            .with_context(|| format!("could not open moderation database {path}"))?;
        Self::from_connection(connection)
    }

    /// In-memory store, used by tests.
    ///
    /// # Errors
    ///
    /// Returns an error when the schema cannot be created.
    pub fn in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> Result<Self> {
        connection.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS reports (
                id TEXT PRIMARY KEY,
                target TEXT NOT NULL,
                reason TEXT NOT NULL,
                details TEXT,
                reporter TEXT,
                created_at TEXT NOT NULL,
                status TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS hidden_targets (
                target TEXT PRIMARY KEY,
                report_id TEXT,
                created_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS moderation_actions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                report_id TEXT NOT NULL,
                actor TEXT NOT NULL,
                action TEXT NOT NULL,
                note TEXT,
                created_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS audit_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                actor TEXT NOT NULL,
                action TEXT NOT NULL,
                target TEXT NOT NULL,
                note TEXT,
                created_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS admin_tokens (
                id TEXT PRIMARY KEY,
                token_hash TEXT NOT NULL UNIQUE,
                label TEXT NOT NULL,
                role TEXT NOT NULL,
                created_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS admin_sessions (
                id TEXT PRIMARY KEY,
                session_hash TEXT NOT NULL UNIQUE,
                token_id TEXT NOT NULL,
                label TEXT NOT NULL,
                role TEXT NOT NULL,
                created_at TEXT NOT NULL,
                expires_at TEXT NOT NULL
             );",
        )?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    /// Store one report and return it.
    ///
    /// # Errors
    ///
    /// Returns an error when the insert fails.
    pub fn create_report(
        &self,
        target: &str,
        reason: &str,
        details: Option<&str>,
        reporter: Option<&str>,
    ) -> Result<Report> {
        let report = Report {
            id: uuid::Uuid::new_v4().to_string(),
            target: target.to_string(),
            reason: reason.to_string(),
            details: details.map(str::to_string),
            reporter: reporter.map(str::to_string),
            created_at: Utc::now(),
            status: ReportStatus::Open,
        };
        self.connection.lock().execute(
            "INSERT INTO reports (id, target, reason, details, reporter, created_at, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                report.id,
                report.target,
                report.reason,
                report.details,
                report.reporter,
                report.created_at.to_rfc3339(),
                report.status.slug(),
            ],
        )?;
        Ok(report)
    }

    /// Read one report.
    #[must_use]
    pub fn report(&self, id: &str) -> Option<Report> {
        let connection = self.connection.lock();
        let report = connection
            .prepare(
                "SELECT id, target, reason, details, reporter, created_at, status
                 FROM reports WHERE id = ?1",
            )
            .ok()
            .and_then(|mut statement| statement.query_row([id], row_to_report).ok());
        drop(connection);
        report
    }

    /// All reports, newest first.
    #[must_use]
    pub fn reports(&self) -> Vec<Report> {
        let connection = self.connection.lock();
        let query = connection.prepare(
            "SELECT id, target, reason, details, reporter, created_at, status
             FROM reports ORDER BY created_at DESC",
        );
        let reports = query.map_or_else(
            |_| Vec::new(),
            |mut statement| {
                statement
                    .query_map([], row_to_report)
                    .map(|rows| rows.flatten().collect())
                    .unwrap_or_default()
            },
        );
        drop(connection);
        reports
    }

    /// Whether a target is hidden from this Host.
    #[must_use]
    pub fn is_hidden(&self, target: &str) -> bool {
        let connection = self.connection.lock();
        connection
            .query_row(
                "SELECT 1 FROM hidden_targets WHERE target = ?1",
                [target],
                |_| Ok(()),
            )
            .is_ok()
    }

    /// Apply a moderation action to one report.
    ///
    /// `hide`/`unhide` also update the hidden set; every action is audited.
    ///
    /// # Errors
    ///
    /// Returns an error when the report is unknown, the action is invalid or a
    /// write fails.
    pub fn apply(
        &self,
        actor: &str,
        report_id: &str,
        action: &str,
        note: Option<&str>,
    ) -> Result<Report> {
        let report = self
            .report(report_id)
            .with_context(|| format!("unknown report {report_id}"))?;
        let status = match action {
            "hide" => ReportStatus::Hidden,
            "dismiss" => ReportStatus::Dismissed,
            "reopen" | "unhide" => ReportStatus::Open,
            other => bail!("unknown moderation action {other}"),
        };
        let now = Utc::now().to_rfc3339();
        let connection = self.connection.lock();
        let transaction = connection.unchecked_transaction()?;
        transaction.execute(
            "UPDATE reports SET status = ?1 WHERE id = ?2",
            params![status.slug(), report.id],
        )?;
        match action {
            "hide" => {
                transaction.execute(
                    "INSERT INTO hidden_targets (target, report_id, created_at)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(target) DO UPDATE SET report_id = excluded.report_id",
                    params![report.target, report.id, now],
                )?;
            }
            "unhide" => {
                transaction.execute(
                    "DELETE FROM hidden_targets WHERE target = ?1",
                    params![report.target],
                )?;
            }
            _ => {}
        }
        transaction.execute(
            "INSERT INTO moderation_actions (report_id, actor, action, note, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![report.id, actor, action, note, now],
        )?;
        transaction.execute(
            "INSERT INTO audit_log (actor, action, target, note, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![actor, action, report.target, note, now],
        )?;
        transaction.commit()?;
        drop(connection);
        let updated = self.report(report_id).unwrap_or(report);
        Ok(updated)
    }

    /// Audit log, newest first.
    #[must_use]
    pub fn audit(&self) -> Vec<AuditEntry> {
        let connection = self.connection.lock();
        let entries = connection
            .prepare(
                "SELECT id, actor, action, target, note, created_at
                 FROM audit_log ORDER BY id DESC LIMIT 500",
            )
            .map_or_else(
                |_| Vec::new(),
                |mut statement| {
                    statement
                        .query_map([], |row| {
                            Ok(AuditEntry {
                                id: row.get(0)?,
                                actor: row.get(1)?,
                                action: row.get(2)?,
                                target: row.get(3)?,
                                note: row.get(4)?,
                                created_at: parse_time(&row.get::<_, String>(5)?),
                            })
                        })
                        .map(|rows| rows.flatten().collect())
                        .unwrap_or_default()
                },
            );
        drop(connection);
        entries
    }

    /// Stored runtime-settings overrides.
    #[must_use]
    pub fn settings_overrides(&self) -> HashMap<String, String> {
        let connection = self.connection.lock();
        let overrides = connection
            .prepare("SELECT key, value FROM settings")
            .map_or_else(
                |_| HashMap::new(),
                |mut statement| {
                    statement
                        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                        .map(|rows| rows.flatten().collect::<HashMap<String, String>>())
                        .unwrap_or_default()
                },
            );
        drop(connection);
        overrides
    }

    /// Persist runtime settings.
    ///
    /// # Errors
    ///
    /// Returns an error when the write fails.
    pub fn save_settings(&self, settings: &RuntimeSettings) -> Result<()> {
        let connection = self.connection.lock();
        let transaction = connection.unchecked_transaction()?;
        for (key, value) in settings.pairs() {
            transaction.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?;
        }
        transaction.commit()?;
        drop(connection);
        Ok(())
    }

    /// Import the bootstrap token when no admin token exists yet.
    ///
    /// Returns `true` when a token was inserted.
    ///
    /// # Errors
    ///
    /// Returns an error when the token cannot be written.
    pub fn bootstrap_admin_token(&self, token: &str) -> Result<bool> {
        if self.has_admin_tokens() {
            return Ok(false);
        }
        let created_at = Utc::now().to_rfc3339();
        let connection = self.connection.lock();
        connection.execute(
            "INSERT INTO admin_tokens (id, token_hash, label, role, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                uuid::Uuid::new_v4().to_string(),
                hash_secret(token),
                "bootstrap",
                Role::Admin.slug(),
                created_at,
            ],
        )?;
        drop(connection);
        Ok(true)
    }

    /// Whether any admin token exists. Without one every admin route is
    /// disabled.
    #[must_use]
    pub fn has_admin_tokens(&self) -> bool {
        let connection = self.connection.lock();
        let count = connection.query_row("SELECT COUNT(*) FROM admin_tokens", [], |row| {
            row.get::<_, i64>(0)
        });
        drop(connection);
        count.is_ok_and(|count| count > 0)
    }

    /// Admin tokens, newest first.
    #[must_use]
    pub fn tokens(&self) -> Vec<TokenSummary> {
        let connection = self.connection.lock();
        let tokens = connection
            .prepare(
                "SELECT id, label, role, created_at FROM admin_tokens ORDER BY created_at DESC",
            )
            .map_or_else(
                |_| Vec::new(),
                |mut statement| {
                    statement
                        .query_map([], |row| {
                            Ok(TokenSummary {
                                id: row.get(0)?,
                                label: row.get(1)?,
                                role: Role::parse(&row.get::<_, String>(2)?)
                                    .unwrap_or(Role::Moderator),
                                created_at: parse_time(&row.get::<_, String>(3)?),
                            })
                        })
                        .map(|rows| rows.flatten().collect())
                        .unwrap_or_default()
                },
            );
        drop(connection);
        tokens
    }

    /// Create a token and return its secret. The secret is shown once.
    ///
    /// # Errors
    ///
    /// Returns an error when the token cannot be written.
    pub fn create_token(&self, label: &str, role: Role) -> Result<(String, TokenSummary)> {
        let secret = new_secret("jabu_");
        let summary = TokenSummary {
            id: uuid::Uuid::new_v4().to_string(),
            label: label.to_string(),
            role,
            created_at: Utc::now(),
        };
        let connection = self.connection.lock();
        connection.execute(
            "INSERT INTO admin_tokens (id, token_hash, label, role, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                summary.id,
                hash_secret(&secret),
                summary.label,
                role.slug(),
                summary.created_at.to_rfc3339(),
            ],
        )?;
        drop(connection);
        Ok((secret, summary))
    }

    /// Revoke a token and every session created from it.
    ///
    /// # Errors
    ///
    /// Returns an error when the delete fails.
    pub fn revoke_token(&self, id: &str) -> Result<bool> {
        let connection = self.connection.lock();
        let transaction = connection.unchecked_transaction()?;
        transaction.execute("DELETE FROM admin_sessions WHERE token_id = ?1", [id])?;
        let removed = transaction.execute("DELETE FROM admin_tokens WHERE id = ?1", [id])?;
        transaction.commit()?;
        drop(connection);
        Ok(removed > 0)
    }

    /// Resolve a bearer token to its role.
    #[must_use]
    pub fn authenticate_token(&self, token: &str) -> Option<TokenSummary> {
        let connection = self.connection.lock();
        let found = connection
            .query_row(
                "SELECT id, label, role, created_at FROM admin_tokens WHERE token_hash = ?1",
                [hash_secret(token)],
                |row| {
                    Ok(TokenSummary {
                        id: row.get(0)?,
                        label: row.get(1)?,
                        role: Role::parse(&row.get::<_, String>(2)?).unwrap_or(Role::Moderator),
                        created_at: parse_time(&row.get::<_, String>(3)?),
                    })
                },
            )
            .ok();
        drop(connection);
        found
    }

    /// Create a session for a token and return its secret and expiry.
    ///
    /// # Errors
    ///
    /// Returns an error when the session cannot be written.
    pub fn create_session(&self, token: &TokenSummary) -> Result<(String, DateTime<Utc>)> {
        let secret = new_secret("");
        let now = Utc::now();
        let expires = now + chrono::Duration::hours(SESSION_TTL_HOURS);
        let connection = self.connection.lock();
        connection.execute(
            "INSERT INTO admin_sessions
             (id, session_hash, token_id, label, role, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                uuid::Uuid::new_v4().to_string(),
                hash_secret(&secret),
                token.id,
                token.label,
                token.role.slug(),
                now.to_rfc3339(),
                expires.to_rfc3339(),
            ],
        )?;
        drop(connection);
        Ok((secret, expires))
    }

    /// Resolve a session id, rejecting expired sessions.
    #[must_use]
    pub fn authenticate_session(&self, session: &str) -> Option<Authenticated> {
        let connection = self.connection.lock();
        let found = connection
            .query_row(
                "SELECT label, role, expires_at FROM admin_sessions WHERE session_hash = ?1",
                [hash_secret(session)],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .ok();
        drop(connection);
        let (label, role, expires_at) = found?;
        if parse_time(&expires_at) <= Utc::now() {
            let _ = self.delete_session(session);
            return None;
        }
        Some(Authenticated {
            label,
            role: Role::parse(&role).unwrap_or(Role::Moderator),
        })
    }

    /// Delete one session.
    ///
    /// # Errors
    ///
    /// Returns an error when the delete fails.
    pub fn delete_session(&self, session: &str) -> Result<()> {
        let connection = self.connection.lock();
        connection.execute(
            "DELETE FROM admin_sessions WHERE session_hash = ?1",
            [hash_secret(session)],
        )?;
        drop(connection);
        Ok(())
    }

    /// Delete expired sessions and return how many were removed.
    ///
    /// # Errors
    ///
    /// Returns an error when the query or delete fails.
    pub fn prune_sessions(&self) -> Result<usize> {
        let now = Utc::now();
        let connection = self.connection.lock();
        let expired = {
            let mut statement = connection.prepare("SELECT id, expires_at FROM admin_sessions")?;
            let rows = statement.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.flatten()
                .filter(|(_, expires_at)| parse_time(expires_at) <= now)
                .map(|(id, _)| id)
                .collect::<Vec<_>>()
        };
        let mut removed = 0;
        for id in &expired {
            removed += connection.execute("DELETE FROM admin_sessions WHERE id = ?1", [id])?;
        }
        drop(connection);
        Ok(removed)
    }

    /// Delete dismissed reports and audit entries older than `retention_days`.
    ///
    /// Hidden targets and open reports are kept. `0` disables pruning.
    ///
    /// # Errors
    ///
    /// Returns an error when the query or delete fails.
    pub fn prune(&self, retention_days: u32) -> Result<usize> {
        if retention_days == 0 {
            return Ok(0);
        }
        let cutoff = Utc::now() - chrono::Duration::days(i64::from(retention_days));
        let connection = self.connection.lock();
        let transaction = connection.unchecked_transaction()?;
        let reports = {
            let mut statement = transaction
                .prepare("SELECT id, created_at FROM reports WHERE status = 'dismissed'")?;
            let rows = statement.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.flatten()
                .filter(|(_, created_at)| parse_time(created_at) < cutoff)
                .map(|(id, _)| id)
                .collect::<Vec<_>>()
        };
        let mut removed = 0;
        for id in &reports {
            transaction.execute("DELETE FROM moderation_actions WHERE report_id = ?1", [id])?;
            removed += transaction.execute("DELETE FROM reports WHERE id = ?1", [id])?;
        }
        let audit = {
            let mut statement = transaction.prepare("SELECT id, created_at FROM audit_log")?;
            let rows = statement.query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.flatten()
                .filter(|(_, created_at)| parse_time(created_at) < cutoff)
                .map(|(id, _)| id)
                .collect::<Vec<_>>()
        };
        for id in &audit {
            removed += transaction.execute("DELETE FROM audit_log WHERE id = ?1", [id])?;
        }
        transaction.commit()?;
        drop(connection);
        Ok(removed)
    }

    /// Take a consistent SQLite snapshot of the whole database.
    ///
    /// # Errors
    ///
    /// Returns an error when the snapshot cannot be written or read.
    pub fn backup(&self) -> Result<Vec<u8>> {
        let path = std::env::temp_dir().join(format!(
            "jabu-host-backup-{}.sqlite3",
            uuid::Uuid::new_v4().simple()
        ));
        let target = path.to_string_lossy().to_string();
        let connection = self.connection.lock();
        connection
            .execute("VACUUM INTO ?1", [&target])
            .with_context(|| format!("could not snapshot the database to {target}"))?;
        drop(connection);
        let bytes = std::fs::read(&path)
            .with_context(|| format!("could not read the snapshot at {target}"))?;
        let _ = std::fs::remove_file(&path);
        Ok(bytes)
    }
}

fn parse_time(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value).map_or_else(|_| Utc::now(), |time| time.with_timezone(&Utc))
}

fn row_to_report(row: &rusqlite::Row<'_>) -> rusqlite::Result<Report> {
    Ok(Report {
        id: row.get(0)?,
        target: row.get(1)?,
        reason: row.get(2)?,
        details: row.get(3)?,
        reporter: row.get(4)?,
        created_at: parse_time(&row.get::<_, String>(5)?),
        status: ReportStatus::parse(&row.get::<_, String>(6)?),
    })
}

/// `post/{jid}/{id}` moderation target.
#[must_use]
pub fn post_target(jid: &str, id: &str) -> String {
    format!("post/{jid}/{id}")
}

/// `community/{service}/{node}` moderation target.
#[must_use]
pub fn community_target(service: &str, node: &str) -> String {
    format!("community/{service}/{node}")
}

/// `community/{service}/{node}/{id}` moderation target.
#[must_use]
pub fn community_post_target(service: &str, node: &str, id: &str) -> String {
    format!("community/{service}/{node}/{id}")
}

/// `space/{service}/{node}` moderation target.
#[must_use]
pub fn space_target(service: &str, node: &str) -> String {
    format!("space/{service}/{node}")
}

/// `story/{jid}/{id}` moderation target.
#[must_use]
pub fn story_target(jid: &str, id: &str) -> String {
    format!("story/{jid}/{id}")
}

/// `contact/{jid}` moderation target.
#[must_use]
pub fn contact_target(jid: &str) -> String {
    format!("contact/{jid}")
}

/// Hides moderated targets from a content source.
pub struct ModeratedContent {
    inner: Arc<dyn ContentSource>,
    store: Arc<ModerationStore>,
}

impl ModeratedContent {
    #[must_use]
    pub fn new(inner: Arc<dyn ContentSource>, store: Arc<ModerationStore>) -> Self {
        Self { inner, store }
    }

    fn hidden(&self, target: &str) -> bool {
        self.store.is_hidden(target)
    }
}

#[async_trait]
impl ContentSource for ModeratedContent {
    async fn blog(&self, jid: &Jid) -> Result<BlogProfile, ContentError> {
        let author = jid.to_bare().to_string();
        if self.hidden(&contact_target(&author)) {
            return Err(ContentError::NotFound);
        }
        let mut profile = self.inner.blog(jid).await?;
        profile
            .posts
            .retain(|post| !self.hidden(&post_target(&author, &post.id)));
        Ok(profile)
    }

    async fn post(&self, jid: &Jid, item_id: &str) -> Result<Post, ContentError> {
        let author = jid.to_bare().to_string();
        if self.hidden(&post_target(&author, item_id)) {
            return Err(ContentError::NotFound);
        }
        self.inner.post(jid, item_id).await
    }

    async fn community(&self, service: &Jid, node: &str) -> Result<CommunityPage, ContentError> {
        let service_bare = service.to_bare().to_string();
        if self.hidden(&community_target(&service_bare, node)) {
            return Err(ContentError::NotFound);
        }
        let mut page = self.inner.community(service, node).await?;
        page.posts
            .retain(|post| !self.hidden(&community_post_target(&service_bare, node, &post.id)));
        Ok(page)
    }

    async fn community_post(
        &self,
        service: &Jid,
        node: &str,
        item_id: &str,
    ) -> Result<Post, ContentError> {
        let service_bare = service.to_bare().to_string();
        if self.hidden(&community_post_target(&service_bare, node, item_id))
            || self.hidden(&community_target(&service_bare, node))
        {
            return Err(ContentError::NotFound);
        }
        self.inner.community_post(service, node, item_id).await
    }

    async fn space(&self, service: &Jid, node: &str) -> Result<SpacePage, ContentError> {
        if self.hidden(&space_target(&service.to_bare().to_string(), node)) {
            return Err(ContentError::NotFound);
        }
        self.inner.space(service, node).await
    }

    async fn story(&self, jid: &Jid, item_id: &str) -> Result<Story, ContentError> {
        if self.hidden(&story_target(&jid.to_bare().to_string(), item_id)) {
            return Err(ContentError::NotFound);
        }
        self.inner.story(jid, item_id).await
    }

    async fn contact(&self, jid: &Jid) -> Result<ContactProfile, ContentError> {
        if self.hidden(&contact_target(&jid.to_bare().to_string())) {
            return Err(ContentError::NotFound);
        }
        self.inner.contact(jid).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HostConfig;
    use chrono::TimeZone;

    fn store() -> ModerationStore {
        ModerationStore::in_memory().expect("store")
    }

    #[test]
    fn reports_are_created_and_listed() {
        let store = store();
        let report = store
            .create_report("post/alice@example.org/p1", "spam", Some("buy now"), None)
            .unwrap();
        assert_eq!(report.status, ReportStatus::Open);
        assert_eq!(store.report(&report.id).unwrap().reason, "spam");
        assert_eq!(store.reports().len(), 1);
    }

    #[test]
    fn hiding_and_unhiding_update_the_target_set_and_audit() {
        let store = store();
        let report = store
            .create_report("post/alice@example.org/p1", "abuse", None, Some("bob"))
            .unwrap();
        let updated = store
            .apply("admin", &report.id, "hide", Some("confirmed"))
            .unwrap();
        assert_eq!(updated.status, ReportStatus::Hidden);
        assert!(store.is_hidden("post/alice@example.org/p1"));
        assert_eq!(store.audit().len(), 1);

        store.apply("admin", &report.id, "unhide", None).unwrap();
        assert!(!store.is_hidden("post/alice@example.org/p1"));
        assert_eq!(store.reports()[0].status, ReportStatus::Open);
    }

    #[test]
    fn dismiss_keeps_the_content_visible() {
        let store = store();
        let report = store
            .create_report("contact/x@example.org", "other", None, None)
            .unwrap();
        store.apply("admin", &report.id, "dismiss", None).unwrap();
        assert!(!store.is_hidden("contact/x@example.org"));
        assert_eq!(store.reports()[0].status, ReportStatus::Dismissed);
    }

    #[test]
    fn admin_tokens_and_sessions_authenticate_and_revoke() {
        let store = store();
        assert!(!store.has_admin_tokens());
        assert!(store.bootstrap_admin_token("bootstrap-secret").unwrap());
        assert!(!store.bootstrap_admin_token("another").unwrap());
        assert!(store.has_admin_tokens());

        let token = store.authenticate_token("bootstrap-secret").unwrap();
        assert_eq!(token.role, Role::Admin);
        assert!(store.authenticate_token("wrong").is_none());

        let (session, expires) = store.create_session(&token).unwrap();
        assert!(expires > Utc::now());
        let identity = store.authenticate_session(&session).unwrap();
        assert_eq!(identity.role, Role::Admin);
        assert_eq!(identity.label, "bootstrap");

        let (secret, summary) = store.create_token("console", Role::Moderator).unwrap();
        assert_eq!(summary.role, Role::Moderator);
        assert_eq!(store.tokens().len(), 2);
        let moderator = store.authenticate_token(&secret).unwrap();
        assert_eq!(moderator.role, Role::Moderator);
        let (moderator_session, _) = store.create_session(&moderator).unwrap();
        assert!(store.authenticate_session(&moderator_session).is_some());

        assert!(store.revoke_token(&summary.id).unwrap());
        assert!(store.authenticate_token(&secret).is_none());
        assert!(store.authenticate_session(&moderator_session).is_none());
        assert!(store.authenticate_session(&session).is_some());
    }

    #[test]
    fn expired_sessions_are_rejected() {
        let store = store();
        store.bootstrap_admin_token("secret").unwrap();
        let token = store.authenticate_token("secret").unwrap();
        let (session, _) = store.create_session(&token).unwrap();
        let past = (Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        store
            .connection
            .lock()
            .execute("UPDATE admin_sessions SET expires_at = ?1", params![past])
            .unwrap();
        assert!(store.authenticate_session(&session).is_none());
        assert_eq!(store.prune_sessions().unwrap(), 0);
    }

    #[test]
    fn settings_round_trip_through_the_store() {
        let store = store();
        assert!(store.settings_overrides().is_empty());
        let settings = RuntimeSettings {
            site_name: "Example".to_string(),
            description: "A public instance".to_string(),
            featured_blogs: vec!["alice@example.org".to_string()],
            retention_days: 30,
            accent_color: "#112233".to_string(),
            footer_note: "Be kind".to_string(),
        };
        store.save_settings(&settings).unwrap();
        let overrides = store.settings_overrides();
        assert_eq!(
            RuntimeSettings::load(&overrides, &HostConfig::default()),
            settings
        );
    }

    #[test]
    fn pruning_removes_old_dismissed_reports_and_audit() {
        let store = store();
        let old_time = (Utc::now() - chrono::Duration::days(400)).to_rfc3339();
        let dismissed = store
            .create_report("post/alice@example.org/p1", "spam", None, None)
            .unwrap();
        store
            .apply("admin", &dismissed.id, "dismiss", None)
            .unwrap();
        let open = store
            .create_report("post/alice@example.org/p2", "spam", None, None)
            .unwrap();
        {
            let connection = store.connection.lock();
            connection
                .execute(
                    "UPDATE reports SET created_at = ?1 WHERE id = ?2",
                    params![old_time, dismissed.id],
                )
                .unwrap();
            connection
                .execute(
                    "UPDATE reports SET created_at = ?1 WHERE id = ?2",
                    params![old_time, open.id],
                )
                .unwrap();
            connection
                .execute("UPDATE audit_log SET created_at = ?1", params![old_time])
                .unwrap();
        }
        assert_eq!(store.prune(90).unwrap(), 2);
        assert_eq!(store.reports().len(), 1);
        assert_eq!(store.reports()[0].id, open.id);
        assert!(store.audit().is_empty());
        assert_eq!(store.prune(0).unwrap(), 0);
    }

    #[test]
    fn backups_reopen_with_their_reports() {
        let store = store();
        store
            .create_report("post/alice@example.org/p1", "spam", None, None)
            .unwrap();
        let bytes = store.backup().unwrap();
        assert!(bytes.starts_with(b"SQLite format 3\0"));
        let path = std::env::temp_dir().join(format!(
            "jabu-backup-test-{}.sqlite3",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::write(&path, &bytes).unwrap();
        let restored = ModerationStore::open(path.to_str().unwrap()).unwrap();
        assert_eq!(restored.reports().len(), 1);
        let _ = std::fs::remove_file(path);
    }

    struct FakePost;

    #[async_trait]
    impl ContentSource for FakePost {
        async fn blog(&self, jid: &Jid) -> Result<BlogProfile, ContentError> {
            Ok(BlogProfile {
                jid: jid.to_string(),
                name: None,
                description: None,
                posts: vec![Post {
                    id: "gone".to_string(),
                    author_jid: jid.to_string(),
                    author_name: None,
                    title: None,
                    body: "hidden".to_string(),
                    html: None,
                    published: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
                    tags: Vec::new(),
                    media: Vec::new(),
                }],
            })
        }

        async fn post(&self, jid: &Jid, item_id: &str) -> Result<Post, ContentError> {
            self.blog(jid)
                .await?
                .posts
                .into_iter()
                .find(|post| post.id == item_id)
                .ok_or(ContentError::NotFound)
        }
    }

    #[tokio::test]
    async fn moderated_content_filters_hidden_posts() {
        let store = Arc::new(store());
        let report = store
            .create_report("post/alice@example.org/gone", "spam", None, None)
            .unwrap();
        store.apply("admin", &report.id, "hide", None).unwrap();
        let content = ModeratedContent::new(Arc::new(FakePost), store);
        let jid: Jid = "alice@example.org".parse().unwrap();
        assert!(content.blog(&jid).await.unwrap().posts.is_empty());
        assert!(content.post(&jid, "gone").await.is_err());
    }
}
