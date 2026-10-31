//! XEP-0277 microblogging / XEP-0472 social feed atom payloads.

use chrono::{DateTime, Utc};
use jid::Jid;
use minidom::{Element, NSChoice};

use crate::{types::MediaType, MediaAttachment, PubSubItem};

pub const NS_ATOM: &str = "http://www.w3.org/2005/Atom";
pub const MICROBLOG_NODE: &str = "urn:xmpp:microblog:0";

/// Parse a PubSub `<item>` (wrapping an Atom `<entry>`) into a [`PubSubItem`].
///
/// `fallback_publisher` is used when the entry carries no usable author URI.
pub fn parse_item(item: &Element, fallback_publisher: &Jid) -> Option<PubSubItem> {
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

    let mut pubsub_item =
        PubSubItem::new(MICROBLOG_NODE.to_string(), publisher, serde_json::json!({}));
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
}
