//! View models for XEP-0501 stories.

use std::collections::{HashMap, HashSet};

use chrono::Utc;
use storage::{load_story_seen, story_item_key, StorageBackend};
use xmpp_core::{microblog::is_active_story, Jid, PubSubItem, XmppClient, STORIES_NODE};

/// Output size of a published story image.
pub const STORY_IMAGE_WIDTH: u32 = 1080;
pub const STORY_IMAGE_HEIGHT: u32 = 1920;
/// Preview box used by the crop editor.
pub const STORY_PREVIEW_WIDTH: f32 = 270.0;
pub const STORY_PREVIEW_HEIGHT: f32 = 480.0;

/// Pan and zoom applied to a story image before publishing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StoryCrop {
    pub zoom: f32,
    pub offset_x: f32,
    pub offset_y: f32,
}

impl Default for StoryCrop {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            offset_x: 0.0,
            offset_y: 0.0,
        }
    }
}

/// Compute the crop settings that keep a preview box covered.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn story_crop_for(width: u32, height: u32, zoom: f32, pan_x: f32, pan_y: f32) -> StoryCrop {
    let zoom = zoom.max(1.0);
    let cover = (STORY_PREVIEW_WIDTH / width.max(1) as f32)
        .max(STORY_PREVIEW_HEIGHT / height.max(1) as f32);
    let display_width = width.max(1) as f32 * cover * zoom;
    let display_height = height.max(1) as f32 * cover * zoom;
    let max_x = ((display_width - STORY_PREVIEW_WIDTH) / 2.0).max(0.0);
    let max_y = ((display_height - STORY_PREVIEW_HEIGHT) / 2.0).max(0.0);
    StoryCrop {
        zoom,
        offset_x: pan_x.clamp(-1.0, 1.0) * max_x,
        offset_y: pan_y.clamp(-1.0, 1.0) * max_y,
    }
}

/// Decode the source dimensions without keeping the decoded image.
#[must_use]
pub fn story_image_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    image::io::Reader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

/// Crop, resize and encode a story image as a 1080x1920 JPEG.
///
/// # Errors
///
/// Returns an error when the source is not a supported image or cannot be
/// encoded.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn crop_story_image(bytes: &[u8], crop: StoryCrop) -> anyhow::Result<Vec<u8>> {
    let source = image::load_from_memory(bytes)?;
    let source = source.to_rgb8();
    let (width, height) = (source.width(), source.height());
    if width == 0 || height == 0 {
        anyhow::bail!("story image is empty");
    }
    let zoom = crop.zoom.max(1.0);
    let cover = (STORY_PREVIEW_WIDTH / width as f32).max(STORY_PREVIEW_HEIGHT / height as f32);
    let scale = cover * zoom;
    let visible_width = (STORY_PREVIEW_WIDTH / scale).min(width as f32);
    let visible_height = (STORY_PREVIEW_HEIGHT / scale).min(height as f32);
    let left = ((width as f32 - visible_width) / 2.0 - crop.offset_x / scale)
        .clamp(0.0, width as f32 - visible_width);
    let top = ((height as f32 - visible_height) / 2.0 - crop.offset_y / scale)
        .clamp(0.0, height as f32 - visible_height);
    let crop_width = visible_width.round().max(1.0) as u32;
    let crop_height = visible_height.round().max(1.0) as u32;
    let left = (left.round() as u32).min(width.saturating_sub(crop_width));
    let top = (top.round() as u32).min(height.saturating_sub(crop_height));

    let cropped = image::imageops::crop_imm(&source, left, top, crop_width, crop_height).to_image();
    let resized = image::imageops::resize(
        &cropped,
        STORY_IMAGE_WIDTH,
        STORY_IMAGE_HEIGHT,
        image::imageops::FilterType::Lanczos3,
    );
    let mut output = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 85).encode_image(&resized)?;
    Ok(output)
}

/// The global story timeline shown by the viewer.
#[derive(Clone, PartialEq)]
pub struct StoryView {
    pub items: Vec<PubSubItem>,
    pub start: usize,
}

/// Story ring state for one contact.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContactStoryState {
    pub count: usize,
    pub all_seen: bool,
    pub first_unseen: Option<usize>,
    pub first: usize,
}

/// A snapshot of active stories plus the account's seen state.
#[derive(Clone, Default)]
pub struct StoryFeed {
    pub items: Vec<PubSubItem>,
    seen: HashMap<String, i64>,
}

impl StoryFeed {
    /// Collect own stories and those of mutual roster contacts.
    #[must_use]
    pub fn load(client: &XmppClient, account: &str, storage: &dyn StorageBackend) -> Self {
        let mut items = Vec::new();
        let mut seen_services = HashSet::new();
        let own = client.connection.jid().map(|jid| Jid::from(jid.to_bare()));
        if let Some(own) = own.as_ref() {
            collect_service(client, own, &mut items);
            seen_services.insert(own.to_string());
        }
        for entry in client.roster.list() {
            if entry.subscription != xmpp_core::SubscriptionState::Both {
                continue;
            }
            let service = Jid::from(entry.jid.to_bare());
            if !seen_services.insert(service.to_string()) {
                continue;
            }
            collect_service(client, &service, &mut items);
        }

        let now = Utc::now();
        items.retain(|item| is_active_story(item, now));
        items.sort_by_key(|item| std::cmp::Reverse(item.published_at));
        let mut seen_ids = HashSet::new();
        items.retain(|item| seen_ids.insert(story_item_key(item)));
        items.truncate(200);

        Self {
            items,
            seen: load_story_seen(storage, account),
        }
    }

    /// Whether one story has already been opened by this account.
    #[must_use]
    pub fn is_seen(&self, item: &PubSubItem) -> bool {
        self.seen.contains_key(&story_item_key(item))
    }

    /// Ring state for a contact, or `None` when it has no active story.
    #[must_use]
    pub fn contact_state(&self, jid: &str) -> Option<ContactStoryState> {
        let mut count = 0;
        let mut all_seen = true;
        let mut first = None;
        let mut first_unseen = None;
        for (index, item) in self.items.iter().enumerate() {
            if item.publisher.to_bare().to_string() != jid {
                continue;
            }
            if first.is_none() {
                first = Some(index);
            }
            count += 1;
            if self.is_seen(item) {
                continue;
            }
            all_seen = false;
            if first_unseen.is_none() {
                first_unseen = Some(index);
            }
        }
        let first = first?;
        Some(ContactStoryState {
            count,
            all_seen,
            first_unseen,
            first,
        })
    }

    /// Find a story by its service and item id.
    #[must_use]
    pub fn index_of(&self, service: &Jid, item_id: &str) -> Option<usize> {
        let service = service.to_bare().to_string();
        self.items
            .iter()
            .position(|item| item.id == item_id && item.publisher.to_bare().to_string() == service)
    }

    /// Build a viewer snapshot starting at one story.
    #[must_use]
    pub fn view_at(&self, index: usize) -> Option<StoryView> {
        if self.items.is_empty() {
            return None;
        }
        Some(StoryView {
            items: self.items.clone(),
            start: index.min(self.items.len().saturating_sub(1)),
        })
    }
}

fn collect_service(client: &XmppClient, service: &Jid, items: &mut Vec<PubSubItem>) {
    let Some(node) = client
        .pubsub
        .get(service)
        .and_then(|service| service.get_node(STORIES_NODE))
    else {
        return;
    };
    items.extend(node.items());
}

#[cfg(test)]
mod tests {
    use super::*;
    use xmpp_core::MediaType;

    fn story(id: &str, publisher: &str, minutes_ago: i64) -> PubSubItem {
        let mut item = PubSubItem::new(
            STORIES_NODE.to_string(),
            publisher.parse().unwrap(),
            serde_json::json!({}),
        );
        item.id = id.to_string();
        item.published_at = Utc::now() - chrono::Duration::minutes(minutes_ago);
        item.expires_at = Some(item.published_at + chrono::Duration::hours(24));
        item.media.push(xmpp_core::MediaAttachment {
            id: id.to_string(),
            url: format!("https://files.example/{id}.jpg"),
            media_type: MediaType::Image,
            mime: Some("image/jpeg".to_string()),
            name: None,
            size: None,
            width: None,
            height: None,
            duration: None,
            thumbnail_url: None,
            blur_hash: None,
        });
        item
    }

    fn feed(items: Vec<PubSubItem>, seen: &[&str]) -> StoryFeed {
        StoryFeed {
            items,
            seen: seen
                .iter()
                .map(|key| ((*key).to_string(), i64::MAX))
                .collect(),
        }
    }

    #[test]
    fn contact_ring_reports_the_first_unseen_story() {
        let feed = feed(
            vec![
                story("new", "alice@example.com", 1),
                story("old", "alice@example.com", 5),
                story("bob", "bob@example.com", 3),
            ],
            &["alice@example.com|old"],
        );
        let state = feed.contact_state("alice@example.com").expect("state");
        assert_eq!(state.count, 2);
        assert!(!state.all_seen);
        assert_eq!(state.first, 0);
        assert_eq!(state.first_unseen, Some(0));

        let state = feed.contact_state("bob@example.com").expect("state");
        assert_eq!(state.count, 1);
        assert!(!state.all_seen);

        assert!(feed.contact_state("carol@example.com").is_none());
    }

    #[test]
    fn contact_ring_is_seen_when_every_story_was_opened() {
        let feed = feed(
            vec![story("one", "alice@example.com", 1)],
            &["alice@example.com|one"],
        );
        let state = feed.contact_state("alice@example.com").expect("state");
        assert!(state.all_seen);
        assert_eq!(state.first_unseen, None);
    }

    #[test]
    fn index_of_matches_service_and_item() {
        let feed = feed(
            vec![
                story("one", "alice@example.com", 1),
                story("two", "bob@example.com", 2),
            ],
            &[],
        );
        let service: Jid = "bob@example.com".parse().unwrap();
        assert_eq!(feed.index_of(&service, "two"), Some(1));
        assert_eq!(feed.index_of(&service, "one"), None);
    }

    #[test]
    fn crops_and_resizes_to_the_story_size() {
        let source = image::RgbImage::from_fn(300, 200, |x, y| {
            image::Rgb([(x % 255) as u8, (y % 255) as u8, 120])
        });
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(source)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        assert_eq!(story_image_dimensions(&png), Some((300, 200)));

        let jpeg = crop_story_image(
            &png,
            StoryCrop {
                zoom: 1.5,
                offset_x: 12.0,
                offset_y: -8.0,
            },
        )
        .expect("crop");
        let decoded = image::load_from_memory(&jpeg).expect("decode");
        assert_eq!(decoded.width(), STORY_IMAGE_WIDTH);
        assert_eq!(decoded.height(), STORY_IMAGE_HEIGHT);
    }
}
