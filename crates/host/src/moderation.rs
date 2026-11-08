//! Reports, moderation state and the audit log.
//!
//! The Host keeps its own moderation data in SQLite: public reports, the set of
//! hidden targets, the action history and the audit log. Moderation never
//! changes the underlying XMPP content; it only hides it from this Host.

use std::sync::Arc;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use jid::Jid;
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use serde::Serialize;

use crate::content::{
    BlogProfile, CommunityPage, ContactProfile, ContentError, ContentSource, Post, SpacePage, Story,
};

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
                    title: None,
                    body: "hidden".to_string(),
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
