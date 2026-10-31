//! XEP-0277 microblogging / XEP-0472 social feed atom payloads.

use chrono::{DateTime, Utc};
use jid::Jid;
use minidom::{Element, NSChoice};
use serde::{Deserialize, Serialize};

use crate::{types::MediaType, MediaAttachment, PubSubItem};

pub const NS_ATOM: &str = "http://www.w3.org/2005/Atom";
pub const MICROBLOG_NODE: &str = "urn:xmpp:microblog:0";
pub const STORIES_NODE: &str = "urn:xmpp:pubsub-social-feed:stories:0";
pub const COMMENTS_PREFIX: &str = "urn:xmpp:microblog:0:comments/";
pub const STORY_SHARE_BODY: &str = "Shared a story";

/// A reference to one story item, carried in chat messages as an XMPP URI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoryRef {
    pub service: Jid,
    pub item_id: String,
}

impl StoryRef {
    /// Build a reference pointing at `item`.
    #[must_use]
    pub fn from_item(item: &PubSubItem) -> Self {
        Self {
            service: Jid::from(item.publisher.to_bare()),
            item_id: item.id.clone(),
        }
    }
}

/// Parse the comments link advertised by a post.
#[must_use]
pub fn parse_comment_ref(href: &str) -> Option<crate::pubsub::CommentRef> {
    let href = href.trim().strip_prefix("xmpp:")?;
    let (target, query) = href.split_once('?')?;
    let service = target.trim().parse::<Jid>().ok()?;
    let query = query.trim_start_matches(';');
    let node = query
        .split('&')
        .find_map(|part| part.strip_prefix("node="))
        .map(percent_decode)
        .filter(|node| !node.is_empty())?;
    Some(crate::pubsub::CommentRef { service, node })
}

/// Build the XMPP URI that advertises a comments node.
#[must_use]
pub fn comments_href(service: &Jid, node: &str) -> String {
    format!("xmpp:{}?;node={}", service.to_bare(), percent_encode(node))
}

/// Build an XMPP URI that references one story item.
#[must_use]
pub fn story_href(service: &Jid, item_id: &str) -> String {
    format!(
        "xmpp:{}?;node={};item={}",
        service.to_bare(),
        percent_encode(STORIES_NODE),
        percent_encode(item_id)
    )
}

/// Parse a story reference from an XMPP URI.
///
/// Only URIs that point at [`STORIES_NODE`] with an item id are accepted.
#[must_use]
pub fn parse_story_uri(uri: &str) -> Option<StoryRef> {
    let href = uri.trim().strip_prefix("xmpp:")?;
    let (target, query) = href.split_once('?')?;
    let service = target.trim().trim_end_matches('/').parse::<Jid>().ok()?;
    let mut node = None;
    let mut item = None;
    for part in query.trim_start_matches(';').split([';', '&']) {
        if let Some(value) = part.strip_prefix("node=") {
            node = Some(percent_decode(value));
        } else if let Some(value) = part.strip_prefix("item=") {
            item = Some(percent_decode(value));
        }
    }
    if node.as_deref() != Some(STORIES_NODE) {
        return None;
    }
    let item_id = item.filter(|value| !value.is_empty())?;
    Some(StoryRef {
        service: Jid::from(service.to_bare()),
        item_id,
    })
}

/// Whether a story item still falls inside its 24 hour lifetime.
#[must_use]
pub fn is_active_story(item: &PubSubItem, now: DateTime<Utc>) -> bool {
    item.node_id == STORIES_NODE && item.expires_at.is_none_or(|expires| expires > now)
}

/// Append the XEP-0277 comments link to an Atom entry.
#[must_use]
pub fn add_comments_link(mut entry: Element, service: &Jid, node: &str) -> Element {
    entry.append_child(
        Element::builder("link", NS_ATOM)
            .attr("rel", "replies")
            .attr("title", "comments")
            .attr("href", comments_href(service, node))
            .build(),
    );
    entry
}

fn percent_encode(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(byte as char);
        } else {
            output.push_str(&format!("%{byte:02X}"));
        }
    }
    output
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = &value[index + 1..index + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                output.push(byte);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

/// Parse a PubSub `<item>` (wrapping an Atom `<entry>`) into a [`PubSubItem`].
///
/// `fallback_publisher` is used when the entry carries no usable author URI.
pub fn parse_item(item: &Element, fallback_publisher: &Jid) -> Option<PubSubItem> {
    parse_item_for_node(item, fallback_publisher, MICROBLOG_NODE)
}

/// Parse a feed item and tag it with the owning node.
///
/// Stories use the same Atom payload as a microblog entry but live on
/// [`STORIES_NODE`]; items there expire after the XEP-0501 default of 24 hours.
pub fn parse_item_for_node(
    item: &Element,
    fallback_publisher: &Jid,
    node: &str,
) -> Option<PubSubItem> {
    let entry = item.get_child("entry", NS_ATOM)?;
    let entry_id = entry
        .get_child("id", NSChoice::Any)
        .map(|e| e.text())
        .filter(|s| !s.is_empty());
    let id = item
        .attr("id")
        .map(|s| s.to_string())
        .or(entry_id)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let publisher = entry
        .get_child("author", NSChoice::Any)
        .and_then(|a| a.get_child("uri", NSChoice::Any))
        .map(|u| u.text())
        .and_then(|u| {
            let trimmed = u.trim().trim_start_matches("xmpp:");
            trimmed.parse::<Jid>().ok()
        })
        .unwrap_or_else(|| fallback_publisher.clone());

    let mut pubsub_item = PubSubItem::new(node.to_string(), publisher, serde_json::json!({}));
    pubsub_item.id = id;

    pubsub_item.title = entry
        .get_child("title", NSChoice::Any)
        .map(|e| e.text())
        .filter(|s| !s.is_empty());

    if let Some(content) = entry.get_child("content", NSChoice::Any) {
        let text = content.text();
        let is_html = content.attr("type") == Some("html");
        if is_html {
            pubsub_item.html = Some(text.clone());
            pubsub_item.body = Some(strip_tags(&text));
        } else {
            pubsub_item.body = Some(text);
        }
    }

    if let Some(published) = entry.get_child("published", NSChoice::Any) {
        if let Ok(ts) = DateTime::parse_from_rfc3339(published.text().trim()) {
            pubsub_item.published_at = ts.with_timezone(&Utc);
        }
    }
    if node == STORIES_NODE {
        pubsub_item.expires_at = Some(pubsub_item.published_at + chrono::Duration::hours(24));
    }
    if let Some(updated) = entry.get_child("updated", NSChoice::Any) {
        if let Ok(ts) = DateTime::parse_from_rfc3339(updated.text().trim()) {
            pubsub_item.updated_at = Some(ts.with_timezone(&Utc));
        }
    }

    for category in entry.children().filter(|c| c.name() == "category") {
        if let Some(term) = category.attr("term") {
            pubsub_item.tags.push(term.to_string());
        }
    }

    for link in entry.children().filter(|c| c.name() == "link") {
        if link.attr("rel") == Some("enclosure") {
            if let Some(href) = link.attr("href") {
                let mime = link.attr("type").unwrap_or("");
                pubsub_item.media.push(MediaAttachment {
                    id: uuid::Uuid::new_v4().to_string(),
                    url: href.to_string(),
                    media_type: media_type_from_mime(mime),
                    mime: (!mime.is_empty()).then(|| mime.to_string()),
                    name: link.attr("title").map(|s| s.to_string()),
                    size: link.attr("length").and_then(|l| l.parse().ok()),
                    width: None,
                    height: None,
                    duration: None,
                    thumbnail_url: None,
                    blur_hash: None,
                });
            }
        } else if link.attr("rel") == Some("replies") && link.attr("title") == Some("comments") {
            if let Some(comment) = link.attr("href").and_then(parse_comment_ref) {
                pubsub_item.comments = Some(comment);
            }
        }
    }

    Some(pubsub_item)
}

/// Build an Atom `<entry>` for publishing to a microblog node.
pub fn build_entry(
    id: &str,
    body: &str,
    title: Option<&str>,
    author: &str,
    published: DateTime<Utc>,
) -> Element {
    build_entry_with(id, body, title, author, published, &[], &[])
}

/// Build an Atom entry with categories and enclosure links.
pub fn build_entry_with(
    id: &str,
    body: &str,
    title: Option<&str>,
    author: &str,
    published: DateTime<Utc>,
    tags: &[String],
    media: &[MediaAttachment],
) -> Element {
    build_entry_updated(id, body, title, author, published, published, tags, media)
}

/// Build an Atom entry with distinct published and updated timestamps.
#[allow(clippy::too_many_arguments)]
pub fn build_entry_updated(
    id: &str,
    body: &str,
    title: Option<&str>,
    author: &str,
    published: DateTime<Utc>,
    updated: DateTime<Utc>,
    tags: &[String],
    media: &[MediaAttachment],
) -> Element {
    let mut entry = Element::builder("entry", NS_ATOM)
        .append(Element::builder("id", NS_ATOM).append(id.to_string()))
        .append(Element::builder("published", NS_ATOM).append(published.to_rfc3339()))
        .append(Element::builder("updated", NS_ATOM).append(updated.to_rfc3339()))
        .append(
            Element::builder("author", NS_ATOM)
                .append(Element::builder("name", NS_ATOM).append(author.to_string()))
                .append(Element::builder("uri", NS_ATOM).append(format!("xmpp:{}", author))),
        )
        .append(Element::builder("content", NS_ATOM).append(body.to_string()))
        .build();
    if let Some(title) = title {
        entry.append_child(
            Element::builder("title", NS_ATOM)
                .append(title.to_string())
                .build(),
        );
    }
    for tag in tags.iter().filter(|tag| !tag.trim().is_empty()) {
        entry.append_child(
            Element::builder("category", NS_ATOM)
                .attr("term", tag.trim().to_string())
                .build(),
        );
    }
    for attachment in media {
        let mut link = Element::builder("link", NS_ATOM)
            .attr("rel", "enclosure")
            .attr("href", attachment.url.clone())
            .attr(
                "type",
                attachment
                    .mime
                    .as_deref()
                    .unwrap_or_else(|| mime_for_media_type(attachment.media_type)),
            );
        if let Some(name) = attachment.name.as_ref().filter(|name| !name.is_empty()) {
            link = link.attr("title", name.clone());
        }
        if let Some(size) = attachment.size {
            link = link.attr("length", size.to_string());
        }
        entry.append_child(link.build());
    }
    entry
}

fn mime_for_media_type(media_type: MediaType) -> &'static str {
    match media_type {
        MediaType::Image => "image/*",
        MediaType::Video => "video/*",
        MediaType::Audio => "audio/*",
        _ => "application/octet-stream",
    }
}

fn media_type_from_mime(mime: &str) -> MediaType {
    if mime.starts_with("image/") {
        MediaType::Image
    } else if mime.starts_with("video/") {
        MediaType::Video
    } else if mime.starts_with("audio/") {
        MediaType::Audio
    } else {
        MediaType::File
    }
}

/// Very small HTML-to-text helper for feed previews.
fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn parses_microblog_entry() {
        let xml = r#"<item xmlns="http://jabber.org/protocol/pubsub" id="post-1"><entry xmlns="http://www.w3.org/2005/Atom"><id>post-1</id><title>Hello</title><published>2024-05-01T12:00:00Z</published><updated>2024-05-01T12:00:00Z</updated><author><name>Alice</name><uri>xmpp:alice@example.com</uri></author><content type="html">&lt;p&gt;Hi &lt;b&gt;all&lt;/b&gt;&lt;/p&gt;</content><category term="rust"/><link rel="enclosure" href="https://files.example/a.png" type="image/png" title="a.png" length="1234"/></entry></item>"#;
        let elem = Element::from_str(xml).unwrap();
        let fallback: Jid = "bob@example.com".parse().unwrap();
        let item = parse_item(&elem, &fallback).unwrap();
        assert_eq!(item.id, "post-1");
        assert_eq!(item.publisher.to_string(), "alice@example.com");
        assert_eq!(item.title.as_deref(), Some("Hello"));
        assert_eq!(item.body.as_deref(), Some("Hi all"));
        assert!(item.html.as_deref().unwrap().contains("<b>all</b>"));
        assert_eq!(item.tags, vec!["rust".to_string()]);
        assert_eq!(item.media.len(), 1);
        assert_eq!(item.media[0].media_type, MediaType::Image);
        assert_eq!(item.media[0].size, Some(1234));
    }

    #[test]
    fn parses_a_story_item_with_expiry() {
        let xml = r#"<item xmlns="http://jabber.org/protocol/pubsub" id="story-1"><entry xmlns="http://www.w3.org/2005/Atom"><id>story-1</id><published>2024-05-01T12:00:00Z</published><author><name>Alice</name><uri>xmpp:alice@example.com</uri></author><content>Sunset</content><link rel="enclosure" href="https://files.example/sunset.jpg" type="image/jpeg"/></entry></item>"#;
        let elem = Element::from_str(xml).unwrap();
        let fallback: Jid = "bob@example.com".parse().unwrap();
        let item = parse_item_for_node(&elem, &fallback, STORIES_NODE).unwrap();
        assert_eq!(item.node_id, STORIES_NODE);
        assert_eq!(item.body.as_deref(), Some("Sunset"));
        assert_eq!(item.media.len(), 1);
        assert_eq!(item.media[0].media_type, MediaType::Image);
        let expires = item.expires_at.expect("story expiry");
        assert_eq!((expires - item.published_at).num_hours(), 24);
    }

    #[test]
    fn builds_entry_roundtrips() {
        let entry = build_entry(
            "id-1",
            "Body text",
            Some("Title"),
            "alice@example.com",
            Utc::now(),
        );
        let item = Element::builder("item", "http://jabber.org/protocol/pubsub")
            .append(entry)
            .build();
        let fallback: Jid = "bob@example.com".parse().unwrap();
        let parsed = parse_item(&item, &fallback).unwrap();
        assert_eq!(parsed.id, "id-1");
        assert_eq!(parsed.title.as_deref(), Some("Title"));
        assert_eq!(parsed.body.as_deref(), Some("Body text"));
        assert_eq!(parsed.publisher.to_string(), "alice@example.com");
    }

    #[test]
    fn comments_href_roundtrips() {
        let service: Jid = "pubsub.example.com".parse().unwrap();
        let node = "urn:xmpp:microblog:0:comments/post-1";
        let href = comments_href(&service, node);
        let parsed = parse_comment_ref(&href).expect("comment ref");
        assert_eq!(parsed.service, service);
        assert_eq!(parsed.node, node);
    }

    #[test]
    fn parses_a_comments_link() {
        let xml = r#"<item xmlns="http://jabber.org/protocol/pubsub" id="post-1"><entry xmlns="http://www.w3.org/2005/Atom"><id>post-1</id><published>2024-05-01T12:00:00Z</published><author><uri>xmpp:alice@example.com</uri></author><content>Hi</content><link rel="replies" title="comments" href="xmpp:pubsub.example.com?;node=urn%3Axmpp%3Amicroblog%3A0%3Acomments%2Fpost-1"/></entry></item>"#;
        let elem = Element::from_reader(xml.as_bytes()).unwrap();
        let fallback: Jid = "bob@example.com".parse().unwrap();
        let item = parse_item(&elem, &fallback).unwrap();
        let comments = item.comments.expect("comments");
        assert_eq!(comments.service.to_string(), "pubsub.example.com");
        assert_eq!(
            comments.node,
            "urn:xmpp:microblog:0:comments/post-1".to_string()
        );
    }

    #[test]
    fn builds_distinct_published_and_updated_times() {
        let published: DateTime<Utc> = "2026-11-01T09:00:00Z".parse().expect("published");
        let updated: DateTime<Utc> = "2026-11-07T10:00:00Z".parse().expect("updated");
        let entry = build_entry_updated(
            "id-3",
            "Edited body",
            Some("Title"),
            "alice@example.com",
            published,
            updated,
            &[],
            &[],
        );
        let item = Element::builder("item", "http://jabber.org/protocol/pubsub")
            .append(entry)
            .build();
        let fallback: Jid = "bob@example.com".parse().unwrap();
        let parsed = parse_item(&item, &fallback).unwrap();
        assert_eq!(parsed.published_at, published);
        assert_eq!(parsed.updated_at, Some(updated));
    }

    #[test]
    fn builds_categories_and_enclosures() {
        let media = MediaAttachment {
            id: "m1".to_string(),
            url: "https://files.example/a.png".to_string(),
            media_type: MediaType::Image,
            mime: Some("image/png".to_string()),
            name: Some("a.png".to_string()),
            size: Some(1234),
            width: None,
            height: None,
            duration: None,
            thumbnail_url: None,
            blur_hash: None,
        };
        let entry = build_entry_with(
            "id-2",
            "Body",
            Some("Title"),
            "alice@example.com",
            Utc::now(),
            &["rust".to_string(), "xmpp".to_string()],
            std::slice::from_ref(&media),
        );
        let item = Element::builder("item", "http://jabber.org/protocol/pubsub")
            .append(entry)
            .build();
        let fallback: Jid = "bob@example.com".parse().unwrap();
        let parsed = parse_item(&item, &fallback).unwrap();
        assert_eq!(parsed.tags, vec!["rust".to_string(), "xmpp".to_string()]);
        assert_eq!(parsed.media.len(), 1);
        assert_eq!(parsed.media[0].url, "https://files.example/a.png");
        assert_eq!(parsed.media[0].name.as_deref(), Some("a.png"));
        assert_eq!(parsed.media[0].size, Some(1234));
    }

    #[test]
    fn story_href_roundtrips() {
        let service: Jid = "alice@example.com".parse().unwrap();
        let href = story_href(&service, "story/one");
        assert_eq!(
            href,
            "xmpp:alice@example.com?;node=urn%3Axmpp%3Apubsub-social-feed%3Astories%3A0;item=story%2Fone"
        );
        let parsed = parse_story_uri(&href).expect("story ref");
        assert_eq!(parsed.service, service);
        assert_eq!(parsed.item_id, "story/one");
    }

    #[test]
    fn story_uri_accepts_unencoded_query_and_rejects_other_nodes() {
        let parsed = parse_story_uri(
            "xmpp:alice@example.com?;node=urn:xmpp:pubsub-social-feed:stories:0&item=abc",
        )
        .expect("story ref");
        assert_eq!(parsed.item_id, "abc");

        assert!(
            parse_story_uri("xmpp:alice@example.com?;node=urn:xmpp:microblog:0;item=abc").is_none()
        );
        assert!(parse_story_uri(
            "xmpp:alice@example.com?;node=urn:xmpp:pubsub-social-feed:stories:0"
        )
        .is_none());
    }

    #[test]
    fn active_story_honours_expiry() {
        let now = Utc::now();
        let mut item = PubSubItem::new(
            STORIES_NODE.to_string(),
            Jid::from_str("alice@example.com").unwrap(),
            serde_json::json!({}),
        );
        item.expires_at = Some(now + chrono::Duration::hours(1));
        assert!(is_active_story(&item, now));

        item.expires_at = Some(now - chrono::Duration::seconds(1));
        assert!(!is_active_story(&item, now));

        item.node_id = MICROBLOG_NODE.to_string();
        item.expires_at = Some(now + chrono::Duration::hours(1));
        assert!(!is_active_story(&item, now));
    }
}
