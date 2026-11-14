// SPDX-License-Identifier: MIT

use async_trait::async_trait;
use weft_client::{Client, Error as ClientError, ListQuery, Window};

pub use weft_client::DiscoveryDocument;

/// Discover a Weft node advertised by an XMPP or community domain.
pub async fn discover_from_domain(domain: &str) -> Result<DiscoveryDocument, GifError> {
    weft_client::discover(domain).await.map_err(map_error)
}

use crate::{GifAsset, GifError, GifItem, GifLicence, GifPage, GifProvider, GifQuery, GifTag};

/// A GIF provider backed by one Weft node.
#[derive(Debug, Clone)]
pub struct WeftProvider {
    client: Client,
    node: String,
    name: String,
}

impl WeftProvider {
    /// Connect to a node and load its descriptor.
    ///
    /// # Errors
    ///
    /// Returns [`GifError`] when the URL is invalid, the node cannot be
    /// reached, or its descriptor is malformed.
    pub async fn connect(node_url: impl Into<String>) -> Result<Self, GifError> {
        let node = node_url.into();
        let client = Client::new(&node).map_err(map_error)?;
        let descriptor = client.node().await.map_err(map_error)?;
        Ok(Self {
            client,
            node: descriptor.public_url,
            name: descriptor.name,
        })
    }

    /// Create a provider from an existing client and display name.
    #[must_use]
    pub fn from_client(client: Client, node: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            client,
            node: node.into(),
            name: name.into(),
        }
    }

    fn page(&self, page: weft_client::Page<weft_client::ContentSummary>) -> GifPage {
        GifPage {
            items: page
                .items
                .into_iter()
                .map(|item| map_item(item, &self.node))
                .collect(),
            next_cursor: page.next_cursor,
        }
    }
}

#[async_trait]
impl GifProvider for WeftProvider {
    fn id(&self) -> &'static str {
        "weft"
    }

    fn name(&self) -> &str {
        &self.name
    }

    async fn search(&self, query: GifQuery) -> Result<GifPage, GifError> {
        let mut list = ListQuery::text(query.text).with_limit(query.limit);
        if let Some(cursor) = query.cursor {
            list = list.with_cursor(cursor);
        }
        self.client
            .search(list)
            .await
            .map(|page| self.page(page))
            .map_err(map_error)
    }

    async fn trending(&self, query: GifQuery) -> Result<GifPage, GifError> {
        let mut list = ListQuery::trending(Window::Day).with_limit(query.limit);
        if let Some(cursor) = query.cursor {
            list = list.with_cursor(cursor);
        }
        self.client
            .trending(list)
            .await
            .map(|page| self.page(page))
            .map_err(map_error)
    }

    async fn tags(&self) -> Result<Vec<GifTag>, GifError> {
        self.client
            .tags()
            .await
            .map(|tags| {
                tags.into_iter()
                    .map(|tag| GifTag {
                        tag: tag.tag,
                        count: tag.count,
                    })
                    .collect()
            })
            .map_err(map_error)
    }

    async fn fetch(&self, item: &GifItem) -> Result<GifAsset, GifError> {
        let media = self.client.download(&item.id).await.map_err(map_error)?;
        let mime = media
            .content_type
            .or_else(|| (!item.mime.is_empty()).then(|| item.mime.clone()))
            .ok_or_else(|| GifError::InvalidItem("the node did not return a media type".into()))?;
        let filename = filename_for(&item.title, &mime, &item.id);
        Ok(GifAsset {
            bytes: media.bytes,
            mime,
            filename,
            width: item.width,
            height: item.height,
            duration_ms: item.duration_ms,
            source: item.source.clone(),
            licence: item.licence.clone(),
        })
    }
}

fn map_item(item: weft_client::ContentSummary, node: &str) -> GifItem {
    GifItem {
        id: item.hash,
        title: if item.title.trim().is_empty() {
            "Weft GIF".to_string()
        } else {
            item.title
        },
        preview_url: item
            .thumbnail_url
            .clone()
            .unwrap_or_else(|| item.media_url.clone()),
        media_url: item.media_url,
        mime: item.mime,
        width: item.width,
        height: item.height,
        duration_ms: item.duration_ms,
        tags: item.tags,
        source: item.source,
        licence: GifLicence {
            spdx: item.licence.spdx,
            status: item.licence.status,
            url: item.licence.url,
        },
        node: node.to_string(),
    }
}

fn filename_for(title: &str, mime: &str, id: &str) -> String {
    let mut slug = String::new();
    for character in title.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    let stem = if slug.is_empty() {
        id.get(..12).unwrap_or("weft").to_string()
    } else {
        slug.to_string()
    };
    let extension = match mime {
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/apng" | "image/png" => "png",
        "image/jpeg" => "jpg",
        _ => "bin",
    };
    format!("{stem}.{extension}")
}

fn map_error(error: ClientError) -> GifError {
    match error {
        ClientError::Api {
            status: 429,
            retry_after,
            ..
        } => GifError::RateLimited { retry_after },
        ClientError::Api {
            status: 404 | 410, ..
        } => GifError::NotFound,
        ClientError::InvalidHash(hash) => GifError::InvalidItem(hash),
        ClientError::Http(error) => GifError::Unavailable(error.to_string()),
        other => GifError::Provider(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::filename_for;

    #[test]
    fn filenames_are_safe_and_keep_the_mime_extension() {
        assert_eq!(
            filename_for("Surprised Cat!", "image/gif", "abc"),
            "surprised-cat.gif"
        );
        assert_eq!(
            filename_for("  ", "image/webp", "0123456789abcdef"),
            "0123456789ab.webp"
        );
        assert_eq!(
            filename_for("../../etc/passwd", "image/gif", "abc"),
            "etc-passwd.gif"
        );
    }
}
