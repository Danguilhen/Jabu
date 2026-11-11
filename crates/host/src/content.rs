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

/// One `rel="related"` link preview on a public post.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkCard {
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub logo: Option<String>,
}

/// A public microblog post.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Post {
    pub id: String,
    pub author_jid: String,
    /// Display name advertised by the post author.
    pub author_name: Option<String>,
    /// Original author when this post repeats another one.
    pub reposted_from: Option<String>,
    /// Link back to the repeated post.
    pub via: Option<String>,
    /// Threading link when this post replies to another.
    pub in_reply_to: Option<String>,
    /// `rel="related"` link previews advertised by the post.
    pub related: Vec<LinkCard>,
    pub title: Option<String>,
    pub body: String,
    /// Sanitized rich body, when the source entry carried XHTML.
    pub html: Option<String>,
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

/// A community node and its recent articles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityPage {
    pub service: String,
    pub node: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub posts: Vec<Post>,
}

/// One room listed in a Space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpaceRoomLink {
    pub jid: String,
    pub name: String,
    pub directory: Option<String>,
    pub voice: bool,
}

/// A public Space and its rooms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpacePage {
    pub service: String,
    pub node: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub rooms: Vec<SpaceRoomLink>,
}

/// One public Story.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Story {
    pub id: String,
    pub author_jid: String,
    pub caption: String,
    pub media: Option<MediaLink>,
    pub published: DateTime<Utc>,
    pub expires: Option<DateTime<Utc>>,
}

/// A public contact profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactProfile {
    pub jid: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub subscriptions: Vec<SubscriptionLink>,
}

/// One node published in a contact's XEP-0330 subscription list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionLink {
    pub server: String,
    pub node: String,
    pub title: Option<String>,
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

    async fn community(&self, _service: &Jid, _node: &str) -> Result<CommunityPage, ContentError> {
        Err(ContentError::NotFound)
    }

    async fn community_post(
        &self,
        _service: &Jid,
        _node: &str,
        _item_id: &str,
    ) -> Result<Post, ContentError> {
        Err(ContentError::NotFound)
    }

    async fn space(&self, _service: &Jid, _node: &str) -> Result<SpacePage, ContentError> {
        Err(ContentError::NotFound)
    }

    async fn story(&self, _jid: &Jid, _item_id: &str) -> Result<Story, ContentError> {
        Err(ContentError::NotFound)
    }

    async fn contact(&self, _jid: &Jid) -> Result<ContactProfile, ContentError> {
        Err(ContentError::NotFound)
    }
}

enum Entry {
    Blog(Instant, Arc<BlogProfile>),
    Post(Instant, Arc<Post>),
    Community(Instant, Arc<CommunityPage>),
    Space(Instant, Arc<SpacePage>),
    Story(Instant, Arc<Story>),
    Contact(Instant, Arc<ContactProfile>),
}

impl Entry {
    const fn fetched_at(&self) -> Instant {
        match self {
            Self::Blog(at, _)
            | Self::Post(at, _)
            | Self::Community(at, _)
            | Self::Space(at, _)
            | Self::Story(at, _)
            | Self::Contact(at, _) => *at,
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

    fn cached_community(&self, key: &str) -> Option<CommunityPage> {
        let entries = self.entries.lock();
        match entries.get(key) {
            Some(Entry::Community(at, value)) if at.elapsed() < self.ttl => Some((**value).clone()),
            _ => None,
        }
    }

    fn remember_community(&self, key: String, value: &CommunityPage) {
        let mut entries = self.entries.lock();
        entries.retain(|_, entry| entry.fetched_at().elapsed() < self.ttl);
        entries.insert(
            key,
            Entry::Community(Instant::now(), Arc::new(value.clone())),
        );
    }

    fn cached_space(&self, key: &str) -> Option<SpacePage> {
        let entries = self.entries.lock();
        match entries.get(key) {
            Some(Entry::Space(at, value)) if at.elapsed() < self.ttl => Some((**value).clone()),
            _ => None,
        }
    }

    fn remember_space(&self, key: String, value: &SpacePage) {
        let mut entries = self.entries.lock();
        entries.retain(|_, entry| entry.fetched_at().elapsed() < self.ttl);
        entries.insert(key, Entry::Space(Instant::now(), Arc::new(value.clone())));
    }

    fn cached_story(&self, key: &str) -> Option<Story> {
        let entries = self.entries.lock();
        match entries.get(key) {
            Some(Entry::Story(at, value)) if at.elapsed() < self.ttl => Some((**value).clone()),
            _ => None,
        }
    }

    fn remember_story(&self, key: String, value: &Story) {
        let mut entries = self.entries.lock();
        entries.retain(|_, entry| entry.fetched_at().elapsed() < self.ttl);
        entries.insert(key, Entry::Story(Instant::now(), Arc::new(value.clone())));
    }

    fn cached_contact(&self, key: &str) -> Option<ContactProfile> {
        let entries = self.entries.lock();
        match entries.get(key) {
            Some(Entry::Contact(at, value)) if at.elapsed() < self.ttl => Some((**value).clone()),
            _ => None,
        }
    }

    fn remember_contact(&self, key: String, value: &ContactProfile) {
        let mut entries = self.entries.lock();
        entries.retain(|_, entry| entry.fetched_at().elapsed() < self.ttl);
        entries.insert(key, Entry::Contact(Instant::now(), Arc::new(value.clone())));
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

    async fn community(&self, service: &Jid, node: &str) -> Result<CommunityPage, ContentError> {
        let key = format!("community:{}:{node}", service.to_bare());
        if let Some(value) = self.cached_community(&key) {
            return Ok(value);
        }
        let value = self.inner.community(service, node).await?;
        self.remember_community(key, &value);
        Ok(value)
    }

    async fn community_post(
        &self,
        service: &Jid,
        node: &str,
        item_id: &str,
    ) -> Result<Post, ContentError> {
        let key = format!("community-post:{}:{node}:{item_id}", service.to_bare());
        if let Some(value) = self.cached_post(&key) {
            return Ok(value);
        }
        let value = self.inner.community_post(service, node, item_id).await?;
        self.remember_post(key, &value);
        Ok(value)
    }

    async fn space(&self, service: &Jid, node: &str) -> Result<SpacePage, ContentError> {
        let key = format!("space:{}:{node}", service.to_bare());
        if let Some(value) = self.cached_space(&key) {
            return Ok(value);
        }
        let value = self.inner.space(service, node).await?;
        self.remember_space(key, &value);
        Ok(value)
    }

    async fn story(&self, jid: &Jid, item_id: &str) -> Result<Story, ContentError> {
        let key = format!("story:{}:{item_id}", jid.to_bare());
        if let Some(value) = self.cached_story(&key) {
            return Ok(value);
        }
        let value = self.inner.story(jid, item_id).await?;
        self.remember_story(key, &value);
        Ok(value)
    }

    async fn contact(&self, jid: &Jid) -> Result<ContactProfile, ContentError> {
        let key = format!("contact:{}", jid.to_bare());
        if let Some(value) = self.cached_contact(&key) {
            return Ok(value);
        }
        let value = self.inner.contact(jid).await?;
        self.remember_contact(key, &value);
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
                author_name: None,
                reposted_from: None,
                via: None,
                in_reply_to: None,
                related: Vec::new(),
                title: None,
                body: "body".to_string(),
                html: None,
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
