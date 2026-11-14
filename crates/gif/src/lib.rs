// SPDX-License-Identifier: MIT

//! A provider-neutral GIF picker API for Jabu.
//!
//! The first implementation is [`WeftProvider`], which talks to a Weft node.
//! UI code depends on the [`GifProvider`] trait so another provider can be
//! added without changing the picker.

use async_trait::async_trait;

mod weft;
pub use weft::WeftProvider;

/// One picker query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GifQuery {
    pub text: String,
    pub cursor: Option<String>,
    pub limit: u32,
}

impl Default for GifQuery {
    fn default() -> Self {
        Self {
            text: String::new(),
            cursor: None,
            limit: 24,
        }
    }
}

impl GifQuery {
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn with_cursor(mut self, cursor: impl Into<String>) -> Self {
        self.cursor = Some(cursor.into());
        self
    }

    #[must_use]
    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = limit.clamp(1, 100);
        self
    }
}

/// One page of results.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GifPage {
    pub items: Vec<GifItem>,
    pub next_cursor: Option<String>,
}

/// Licence information shown with every result.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GifLicence {
    pub spdx: Option<String>,
    pub status: String,
    pub url: Option<String>,
}

/// A result in the picker grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GifItem {
    /// Stable provider identifier (a Weft content hash for Weft).
    pub id: String,
    pub title: String,
    pub media_url: String,
    /// A lighter preview when the provider has one. Weft currently serves the
    /// original here because its public API has no thumbnail route yet.
    pub preview_url: String,
    pub mime: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_ms: Option<u64>,
    pub tags: Vec<String>,
    pub source: Option<String>,
    pub licence: GifLicence,
    pub node: String,
}

/// One tag in the provider vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GifTag {
    pub tag: String,
    pub count: i64,
}

/// Original bytes ready for an XEP-0363 upload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GifAsset {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub filename: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_ms: Option<u64>,
    pub source: Option<String>,
    pub licence: GifLicence,
}

/// A GIF source. Implementations must be safe to share between UI tasks.
#[async_trait]
pub trait GifProvider: Send + Sync {
    /// Stable provider id, such as `weft`.
    fn id(&self) -> &'static str;

    /// Human-readable provider or node name.
    fn name(&self) -> &str;

    /// Search the provider.
    async fn search(&self, query: GifQuery) -> Result<GifPage, GifError>;

    /// List trending or featured items.
    async fn trending(&self, query: GifQuery) -> Result<GifPage, GifError>;

    /// List the provider's tags.
    async fn tags(&self) -> Result<Vec<GifTag>, GifError>;

    /// Download the original bytes for a result.
    async fn fetch(&self, item: &GifItem) -> Result<GifAsset, GifError>;
}

/// Provider errors that the picker can render.
#[derive(Debug, thiserror::Error)]
pub enum GifError {
    #[error("the node is rate limiting this client{}", retry_suffix(.retry_after.as_ref()))]
    RateLimited { retry_after: Option<u64> },
    #[error("the GIF is no longer available")]
    NotFound,
    #[error("the provider is not configured")]
    NotConfigured,
    #[error("the node could not be reached: {0}")]
    Unavailable(String),
    #[error("the provider returned an invalid item: {0}")]
    InvalidItem(String),
    #[error("the provider request failed: {0}")]
    Provider(String),
}

fn retry_suffix(retry_after: Option<&u64>) -> String {
    retry_after.map_or_else(String::new, |seconds| format!("; retry in {seconds}s"))
}
