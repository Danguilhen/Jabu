//! Public content models and the source/cache boundary.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use jid::Jid;
use parking_lot::Mutex;

/// One attachment on a public post.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaLink {
    pub url: String,
    pub mime: Option<String>,
    pub name: Option<String>,
}

/// A public microblog post.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Post {
    pub id: String,
    pub author_jid: String,
    pub title: Option<String>,
    pub body: String,
    pub published: DateTime<Utc>,
    pub tags: Vec<String>,
    pub media: Vec<MediaLink>,
}

/// A blog profile and its most recent public posts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlogProfile {
    pub jid: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub posts: Vec<Post>,
}

/// Why public content could not be served.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContentError {
    #[error("no public content found")]
    NotFound,
    #[error("content is not publicly accessible")]
    NotPublic,
    #[error("content source unavailable: {0}")]
    Unavailable(String),
}

/// Where public pages read their content from.
#[async_trait]
pub trait ContentSource: Send + Sync {
    async fn blog(&self, jid: &Jid) -> Result<BlogProfile, ContentError>;
    async fn post(&self, jid: &Jid, item_id: &str) -> Result<Post, ContentError>;
}

enum Entry {
    Blog(Instant, Arc<BlogProfile>),
    Post(Instant, Arc<Post>),
}

impl Entry {
    const fn fetched_at(&self) -> Instant {
        match self {
            Self::Blog(at, _) | Self::Post(at, _) => *at,
        }
    }
}

/// TTL cache in front of a [`ContentSource`].
///
/// Public pages are read far more often than the underlying nodes change, so a
/// short-lived cache keeps request latency and XMPP traffic down.
pub struct CachedContent {
    inner: Arc<dyn ContentSource>,
    ttl: Duration,
    entries: Mutex<HashMap<String, Entry>>,
}

impl CachedContent {
    #[must_use]
    pub fn new(inner: Arc<dyn ContentSource>, ttl: Duration) -> Self {
        Self {
            inner,
            ttl,
            entries: Mutex::new(HashMap::new()),
        }
    }

    fn cached_blog(&self, key: &str) -> Option<BlogProfile> {
        let entries = self.entries.lock();
        match entries.get(key) {
            Some(Entry::Blog(at, value)) if at.elapsed() < self.ttl => Some((**value).clone()),
            _ => None,
        }
    }

    fn cached_post(&self, key: &str) -> Option<Post> {
        let entries = self.entries.lock();
        match entries.get(key) {
            Some(Entry::Post(at, value)) if at.elapsed() < self.ttl => Some((**value).clone()),
            _ => None,
        }
    }

    fn remember_blog(&self, key: String, value: &BlogProfile) {
        let mut entries = self.entries.lock();
        entries.retain(|_, entry| entry.fetched_at().elapsed() < self.ttl);
        entries.insert(key, Entry::Blog(Instant::now(), Arc::new(value.clone())));
    }

    fn remember_post(&self, key: String, value: &Post) {
        let mut entries = self.entries.lock();
        entries.retain(|_, entry| entry.fetched_at().elapsed() < self.ttl);
        entries.insert(key, Entry::Post(Instant::now(), Arc::new(value.clone())));
    }
}

#[async_trait]
impl ContentSource for CachedContent {
    async fn blog(&self, jid: &Jid) -> Result<BlogProfile, ContentError> {
        let key = format!("blog:{}", jid.to_bare());
        if let Some(value) = self.cached_blog(&key) {
            return Ok(value);
        }
        let value = self.inner.blog(jid).await?;
        self.remember_blog(key, &value);
        Ok(value)
    }

    async fn post(&self, jid: &Jid, item_id: &str) -> Result<Post, ContentError> {
        let key = format!("post:{}:{item_id}", jid.to_bare());
        if let Some(value) = self.cached_post(&key) {
            return Ok(value);
        }
        let value = self.inner.post(jid, item_id).await?;
        self.remember_post(key, &value);
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Counting {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl ContentSource for Counting {
        async fn blog(&self, jid: &Jid) -> Result<BlogProfile, ContentError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(BlogProfile {
                jid: jid.to_string(),
                name: None,
                description: None,
                posts: Vec::new(),
            })
        }

        async fn post(&self, jid: &Jid, item_id: &str) -> Result<Post, ContentError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(Post {
                id: item_id.to_string(),
                author_jid: jid.to_string(),
                title: None,
                body: "body".to_string(),
                published: Utc::now(),
                tags: Vec::new(),
                media: Vec::new(),
            })
        }
    }

    #[tokio::test]
    async fn caches_blog_and_post_lookups() {
        let inner = Arc::new(Counting {
            calls: AtomicUsize::new(0),
        });
        let cached = CachedContent::new(inner.clone(), Duration::from_secs(60));
        let jid: Jid = "alice@example.org".parse().unwrap();

        cached.blog(&jid).await.unwrap();
        cached.blog(&jid).await.unwrap();
        cached.post(&jid, "p1").await.unwrap();
        cached.post(&jid, "p1").await.unwrap();
        assert_eq!(inner.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn expires_entries_after_the_ttl() {
        let inner = Arc::new(Counting {
            calls: AtomicUsize::new(0),
        });
        let cached = CachedContent::new(inner.clone(), Duration::from_millis(1));
        let jid: Jid = "alice@example.org".parse().unwrap();
        cached.blog(&jid).await.unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        cached.blog(&jid).await.unwrap();
        assert_eq!(inner.calls.load(Ordering::SeqCst), 2);
    }
}
