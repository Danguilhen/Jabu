//! XEP-0277 microblogging / XEP-0472 social feed atom payloads.

use chrono::{DateTime, Utc};
use jid::Jid;
use minidom::{Element, NSChoice, Node};
use serde::{Deserialize, Serialize};

use crate::{types::MediaType, MediaAttachment, PubSubItem};

pub const NS_ATOM: &str = "http://www.w3.org/2005/Atom";
pub const NS_XHTML: &str = "http://www.w3.org/1999/xhtml";
/// Atom threading extension namespace (RFC 4685).
pub const NS_THREADING: &str = "http://purl.org/syndication/thread/1.0";
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

/// Build an XMPP URI that references one microblog item.
#[must_use]
pub fn post_href(service: &Jid, item_id: &str) -> String {
    format!(
        "xmpp:{}?;node={};item={}",
        service.to_bare(),
        percent_encode(MICROBLOG_NODE),
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
#[must_use]
pub fn parse_item(item: &Element, fallback_publisher: &Jid) -> Option<PubSubItem> {
    parse_item_for_node(item, fallback_publisher, MICROBLOG_NODE)
}

/// Parse a feed item and tag it with the owning node.
///
/// Stories use the same Atom payload as a microblog entry but live on
/// [`STORIES_NODE`]; items there expire after the XEP-0501 default of 24 hours.
#[allow(clippy::too_many_lines)]
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

    let author = entry.get_child("author", NSChoice::Any);
    let author_display = author
        .and_then(|a| a.get_child("name", NSChoice::Any))
        .map(|name| name.text().trim().to_string())
        .filter(|name| !name.is_empty());
    let author_jid = author
        .and_then(|a| a.get_child("uri", NSChoice::Any))
        .map(|u| u.text())
        .and_then(|u| {
            let trimmed = u.trim().trim_start_matches("xmpp:");
            trimmed
                .parse::<Jid>()
                .ok()
                .map(|jid| Jid::from(jid.to_bare()))
        });
    let item_publisher = item
        .attr("publisher")
        .and_then(|p| p.parse::<Jid>().ok())
        .map(|jid| Jid::from(jid.to_bare()));
    let author_is_publisher = item_publisher
        .as_ref()
        .is_none_or(|publisher| author_jid.as_ref() == Some(publisher));
    let publisher = item_publisher
        .or_else(|| author_jid.clone())
        .unwrap_or_else(|| Jid::from(fallback_publisher.to_bare()));

    let mut pubsub_item = PubSubItem::new(node.to_string(), publisher, serde_json::json!({}));
    pubsub_item.id = id;
    if author_is_publisher {
        pubsub_item.author_name.clone_from(&author_display);
    }

    pubsub_item.title = entry
        .get_child("title", NSChoice::Any)
        .map(|e| e.text())
        .filter(|s| !s.is_empty());

    let contents: Vec<&Element> = entry
        .children()
        .filter(|child| child.name() == "content")
        .collect();
    let text_content = contents.iter().copied().find(|content| {
        content
            .attr("type")
            .is_none_or(|kind| kind.eq_ignore_ascii_case("text"))
    });
    let rich_content = contents.iter().copied().find(|content| {
        content.attr("type").is_some_and(|kind| {
            kind.eq_ignore_ascii_case("xhtml") || kind.eq_ignore_ascii_case("html")
        })
    });
    if let Some(content) = rich_content {
        let (html, derived) = parse_atom_text(content);
        pubsub_item.html = html;
        pubsub_item.body = text_content
            .and_then(|text| {
                let text = text.text();
                (!text.is_empty()).then_some(text)
            })
            .or(derived);
    } else if let Some(content) = text_content.or_else(|| contents.first().copied()) {
        let (html, body) = parse_atom_text(content);
        pubsub_item.html = html;
        pubsub_item.body = body;
    }

    if let Some(summary) = entry.get_child("summary", NSChoice::Any) {
        let (parsed_html, summary_body) = parse_atom_text(summary);
        if let Some(summary_body) = summary_body {
            let has_rich_content = pubsub_item.html.is_some();
            let has_any_content = has_rich_content || pubsub_item.body.is_some();
            if has_rich_content || !has_any_content {
                let summary_html = parsed_html.unwrap_or_else(|| {
                    let mut escaped = String::new();
                    escape_into(&mut escaped, &summary_body);
                    format!("<p>{escaped}</p>")
                });
                pubsub_item.html = Some(match pubsub_item.html.take() {
                    Some(html) if !html.is_empty() => format!("{summary_html}{html}"),
                    _ => summary_html,
                });
            }
            pubsub_item.body = Some(match pubsub_item.body.take() {
                Some(body) if !body.is_empty() => format!("{summary_body}\n\n{body}"),
                _ => summary_body,
            });
        }
    }

    let updated = entry
        .get_child("updated", NSChoice::Any)
        .and_then(|updated| DateTime::parse_from_rfc3339(updated.text().trim()).ok())
        .map(|ts| ts.with_timezone(&Utc));
    let published_at = entry
        .get_child("published", NSChoice::Any)
        .and_then(|published| DateTime::parse_from_rfc3339(published.text().trim()).ok())
        .map(|ts| ts.with_timezone(&Utc));
    if let Some(published_at) = published_at.or(updated) {
        pubsub_item.published_at = published_at;
    }
    if node == STORIES_NODE {
        pubsub_item.expires_at = Some(pubsub_item.published_at + chrono::Duration::hours(24));
    }
    pubsub_item.updated_at = updated;

    for category in entry.children().filter(|c| c.name() == "category") {
        if let Some(term) = category.attr("term") {
            pubsub_item.tags.push(term.to_string());
        }
    }

    let mut via = None;
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
        } else if link.attr("rel") == Some("via") {
            if let Some(href) = link.attr("href").and_then(safe_href) {
                via = Some(href.to_string());
            }
        } else if link.attr("rel") == Some("related") {
            if let Some(href) = link.attr("href").and_then(safe_web_url) {
                pubsub_item.related.push(crate::pubsub::RelatedLink {
                    url: href.to_string(),
                    title: link
                        .attr("title")
                        .map(str::trim)
                        .filter(|title| !title.is_empty())
                        .map(str::to_string),
                    description: link
                        .attr("description")
                        .map(str::trim)
                        .filter(|description| !description.is_empty())
                        .map(str::to_string),
                    logo: link.attr("logo").and_then(safe_web_url).map(str::to_string),
                });
            }
        }
    }

    if let Some(reply) = entry.children().find(|child| {
        child.name() == "in-reply-to" && (child.ns() == NS_THREADING || child.ns().is_empty())
    }) {
        if let Some(href) = reply.attr("href").and_then(safe_href) {
            pubsub_item.in_reply_to = Some(href.to_string());
        }
    }

    if via.is_some() && !author_is_publisher {
        pubsub_item.original_author_name = author_display;
        pubsub_item.original_author_jid = author_jid.map(|jid| jid.to_string());
    }
    pubsub_item.via = via;

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
    build_entry_rich(
        id, body, None, title, author, published, updated, tags, media,
    )
}

/// Build an Atom entry with an optional rich XHTML body.
///
/// XEP-0277 requires two `content` elements when XHTML is published: one
/// `text` and one `xhtml`. The HTML is sanitized before it is embedded.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn build_entry_rich(
    id: &str,
    body: &str,
    html: Option<&str>,
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
        .build();
    if let Some(title) = title {
        entry.append_child(
            Element::builder("title", NS_ATOM)
                .append(title.to_string())
                .build(),
        );
    }
    entry = append_content(entry, body, html);
    append_entry_metadata(entry, tags, media)
}

/// Convert a plain-text body into a safe XHTML fragment.
///
/// Blank lines become paragraphs and single line breaks become `<br/>`. All
/// text is escaped, so the result is safe to pass to [`build_entry_rich`].
#[must_use]
pub fn text_to_xhtml(text: &str) -> String {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut html = String::new();
    for paragraph in normalized.split("\n\n") {
        let lines: Vec<&str> = paragraph.lines().collect();
        let Some(first) = lines.iter().position(|line| !line.trim().is_empty()) else {
            continue;
        };
        let last = lines
            .iter()
            .rposition(|line| !line.trim().is_empty())
            .unwrap_or(first);
        html.push_str("<p>");
        for (index, line) in lines[first..=last].iter().enumerate() {
            if index > 0 {
                html.push_str("<br/>");
            }
            escape_into(&mut html, line);
        }
        html.push_str("</p>");
    }
    html
}

fn append_content(mut entry: Element, body: &str, html: Option<&str>) -> Element {
    entry.append_child(
        Element::builder("content", NS_ATOM)
            .attr("type", "text")
            .append(body.to_string())
            .build(),
    );
    if let Some(html) = html.and_then(sanitize_html).filter(|html| !html.is_empty()) {
        let wrapped = format!("<div xmlns=\"{NS_XHTML}\">{html}</div>");
        if let Ok(div) = wrapped.parse::<Element>() {
            entry.append_child(
                Element::builder("content", NS_ATOM)
                    .attr("type", "xhtml")
                    .append(div)
                    .build(),
            );
        }
    }
    entry
}

fn append_entry_metadata(
    mut entry: Element,
    tags: &[String],
    media: &[MediaAttachment],
) -> Element {
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

/// Build a repeated-post entry (XEP-0277).
///
/// The Atom author is the original author, while the publishing item is
/// attributed to the repeater by the server. A `rel="via"` link points back at
/// the original item.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn build_repost_entry(
    id: &str,
    body: &str,
    original_jid: &str,
    original_name: Option<&str>,
    published: DateTime<Utc>,
    via: &str,
    tags: &[String],
    media: &[MediaAttachment],
) -> Element {
    let name = original_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(original_jid);
    let entry = Element::builder("entry", NS_ATOM)
        .append(Element::builder("id", NS_ATOM).append(id.to_string()))
        .append(Element::builder("published", NS_ATOM).append(published.to_rfc3339()))
        .append(Element::builder("updated", NS_ATOM).append(published.to_rfc3339()))
        .append(
            Element::builder("author", NS_ATOM)
                .append(Element::builder("name", NS_ATOM).append(name.to_string()))
                .append(Element::builder("uri", NS_ATOM).append(format!("xmpp:{original_jid}"))),
        )
        .append(
            Element::builder("link", NS_ATOM)
                .attr("rel", "via")
                .attr("href", via.to_string()),
        )
        .build();
    let entry = append_content(entry, body, Some(&text_to_xhtml(body)));
    append_entry_metadata(entry, tags, media)
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

/// Return a safe HTML fragment for a stored post body.
///
/// Only well-formed XML fragments are accepted. The sanitizer emits a small
/// allowlist of block and inline tags, escapes all text, and keeps only
/// `http`, `https`, `mailto` and `xmpp` links. Anything else is dropped, and a
/// fragment that cannot be parsed returns `None` so the caller can fall back
/// to the plain-text body.
#[must_use]
pub fn sanitize_html(raw: &str) -> Option<String> {
    sanitize_html_fragment(raw).map(|(html, _)| html)
}

fn sanitize_html_fragment(raw: &str) -> Option<(String, String)> {
    let wrapped = format!("<div xmlns=\"{NS_XHTML}\">{raw}</div>");
    let root: Element = wrapped.parse().ok()?;
    let (html, text) = render_rich(&root);
    (!html.is_empty()).then_some((html, text))
}

/// Parse an Atom text construct (`content` or `summary`) into a safe HTML
/// fragment plus a plain-text body.
///
/// `text` and untyped constructs are treated as plain text; `html` is
/// sanitized and `xhtml` is rendered through the same allowlist.
fn parse_atom_text(element: &Element) -> (Option<String>, Option<String>) {
    match element.attr("type") {
        Some(kind) if kind.eq_ignore_ascii_case("xhtml") => {
            let root = element
                .children()
                .find(|child| child.name() == "div" && child.ns() == NS_XHTML)
                .or_else(|| element.children().next());
            if let Some(root) = root {
                let (html, text) = render_rich(root);
                (
                    (!html.is_empty()).then_some(html),
                    (!text.is_empty()).then_some(text),
                )
            } else {
                let text = element.text();
                (None, (!text.is_empty()).then_some(text))
            }
        }
        Some(kind) if kind.eq_ignore_ascii_case("html") => {
            let raw = element.text();
            let (html, text) = sanitize_html_fragment(&raw).map_or_else(
                || (None, strip_tags(&raw)),
                |(html, text)| (Some(html), text),
            );
            (html, (!text.is_empty()).then_some(text))
        }
        _ => {
            let text = element.text();
            (None, (!text.is_empty()).then_some(text))
        }
    }
}

fn render_rich(root: &Element) -> (String, String) {
    let mut html = String::new();
    let mut texts = String::new();
    render_rich_nodes(root, &mut html, &mut texts, false, 0);
    (html.trim().to_string(), normalize_rich_text(&texts))
}

const MAX_RICH_DEPTH: usize = 32;

fn render_rich_nodes(
    parent: &Element,
    html: &mut String,
    texts: &mut String,
    pre: bool,
    depth: usize,
) {
    if depth > MAX_RICH_DEPTH {
        return;
    }
    for node in parent.nodes() {
        match node {
            Node::Text(raw) => {
                if pre {
                    escape_into(html, raw);
                    texts.push_str(raw);
                } else {
                    let collapsed = collapse_whitespace(raw);
                    escape_into(html, &collapsed);
                    texts.push_str(&collapsed);
                }
            }
            Node::Element(element) => render_rich_element(element, html, texts, pre, depth + 1),
        }
    }
}

fn render_rich_element(
    element: &Element,
    html: &mut String,
    texts: &mut String,
    pre: bool,
    depth: usize,
) {
    let name = element.name();
    if !element.ns().is_empty() && element.ns() != NS_XHTML {
        if matches!(name, "script" | "style") {
            return;
        }
        render_rich_nodes(element, html, texts, pre, depth);
        return;
    }
    match name {
        "script" | "style" | "iframe" | "object" | "embed" | "svg" | "math" => {}
        "br" => {
            html.push_str("<br/>");
            texts.push('\n');
        }
        "img" => {
            if let Some(alt) = element
                .attr("alt")
                .map(str::trim)
                .filter(|alt| !alt.is_empty())
            {
                texts.push(' ');
                texts.push_str(alt);
            }
        }
        "a" => {
            let href = element.attr("href").and_then(safe_href);
            if let Some(href) = href {
                html.push_str("<a href=\"");
                escape_into(html, href);
                html.push_str("\" target=\"_blank\" rel=\"noopener noreferrer\">");
            }
            render_rich_nodes(element, html, texts, pre, depth);
            if href.is_some() {
                html.push_str("</a>");
            }
        }
        "strong" | "b" => render_wrapped(element, html, texts, pre, depth, "strong"),
        "em" | "i" => render_wrapped(element, html, texts, pre, depth, "em"),
        "u" => render_wrapped(element, html, texts, pre, depth, "u"),
        "code" => render_wrapped(element, html, texts, pre, depth, "code"),
        "p" | "div" | "blockquote" | "pre" | "ul" | "ol" | "li" | "h1" | "h2" | "h3" | "h4"
        | "h5" | "h6" => {
            if name == "li" {
                ensure_newline(texts);
                texts.push_str("• ");
            } else {
                ensure_block_break(texts);
            }
            html.push('<');
            html.push_str(name);
            html.push('>');
            render_rich_nodes(element, html, texts, pre || name == "pre", depth);
            html.push_str("</");
            html.push_str(name);
            html.push('>');
            texts.push('\n');
        }
        _ => render_rich_nodes(element, html, texts, pre, depth),
    }
}

fn render_wrapped(
    element: &Element,
    html: &mut String,
    texts: &mut String,
    pre: bool,
    depth: usize,
    tag: &str,
) {
    html.push('<');
    html.push_str(tag);
    html.push('>');
    render_rich_nodes(element, html, texts, pre, depth);
    html.push_str("</");
    html.push_str(tag);
    html.push('>');
}

fn ensure_newline(texts: &mut String) {
    if !texts.is_empty() && !texts.ends_with('\n') {
        texts.push('\n');
    }
}

fn ensure_block_break(texts: &mut String) {
    ensure_newline(texts);
    if !texts.is_empty() && !texts.ends_with("\n\n") {
        texts.push('\n');
    }
}

fn collapse_whitespace(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut in_space = false;
    for ch in value.chars() {
        if ch.is_whitespace() {
            if !in_space {
                out.push(' ');
                in_space = true;
            }
        } else {
            out.push(ch);
            in_space = false;
        }
    }
    out
}

fn normalize_rich_text(value: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut blank = false;
    for line in value.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            if !blank && !lines.is_empty() {
                lines.push(String::new());
                blank = true;
            }
        } else {
            lines.push(line);
            blank = false;
        }
    }
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines.join("\n")
}

fn escape_into(out: &mut String, value: &str) {
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
}

fn safe_href(value: &str) -> Option<&str> {
    let value = value.trim();
    if value
        .chars()
        .any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return None;
    }
    let lower = value.to_ascii_lowercase();
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with("xmpp:")
    {
        Some(value)
    } else {
        None
    }
}

/// Restrict a URL to the HTTP(S) schemes used by link previews.
fn safe_web_url(value: &str) -> Option<&str> {
    let value = value.trim();
    if value
        .chars()
        .any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return None;
    }
    let lower = value.to_ascii_lowercase();
    (lower.starts_with("http://") || lower.starts_with("https://")).then_some(value)
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
        assert_eq!(item.author_name.as_deref(), Some("Alice"));
        assert_eq!(item.body.as_deref(), Some("Hi all"));
        assert!(item
            .html
            .as_deref()
            .unwrap()
            .contains("<strong>all</strong>"));
        assert_eq!(item.tags, vec!["rust".to_string()]);
        assert_eq!(item.media.len(), 1);
        assert_eq!(item.media[0].media_type, MediaType::Image);
        assert_eq!(item.media[0].size, Some(1234));
    }

    #[test]
    fn trusts_the_item_publisher_over_a_spoofed_author() {
        let xml = r#"<item xmlns="http://jabber.org/protocol/pubsub" id="post-4" publisher="bob@example.com"><entry xmlns="http://www.w3.org/2005/Atom"><id>post-4</id><published>2024-05-01T12:00:00Z</published><author><name>Alice</name><uri>xmpp:alice@example.com</uri></author><content>Hi</content></entry></item>"#;
        let elem = Element::from_str(xml).unwrap();
        let fallback: Jid = "carol@example.com".parse().unwrap();
        let item = parse_item(&elem, &fallback).unwrap();
        assert_eq!(item.publisher.to_string(), "bob@example.com");
        assert!(item.author_name.is_none());
    }

    #[test]
    fn accepts_an_author_name_that_matches_the_publisher() {
        let xml = r#"<item xmlns="http://jabber.org/protocol/pubsub" id="post-5" publisher="alice@example.com/phone"><entry xmlns="http://www.w3.org/2005/Atom"><id>post-5</id><published>2024-05-01T12:00:00Z</published><author><name>Alice</name><uri>xmpp:alice@example.com</uri></author><content>Hi</content></entry></item>"#;
        let elem = Element::from_str(xml).unwrap();
        let fallback: Jid = "bob@example.com".parse().unwrap();
        let item = parse_item(&elem, &fallback).unwrap();
        assert_eq!(item.publisher.to_string(), "alice@example.com");
        assert_eq!(item.author_name.as_deref(), Some("Alice"));
    }

    #[test]
    fn prepends_a_summary_to_the_content() {
        let xml = r#"<item xmlns="http://jabber.org/protocol/pubsub" id="post-6"><entry xmlns="http://www.w3.org/2005/Atom"><id>post-6</id><published>2024-05-01T12:00:00Z</published><author><uri>xmpp:alice@example.com</uri></author><summary>Short</summary><content type="html">&lt;p&gt;Long&lt;/p&gt;</content></entry></item>"#;
        let elem = Element::from_str(xml).unwrap();
        let fallback: Jid = "bob@example.com".parse().unwrap();
        let item = parse_item(&elem, &fallback).unwrap();
        assert_eq!(item.body.as_deref(), Some("Short\n\nLong"));
        assert_eq!(item.html.as_deref(), Some("<p>Short</p><p>Long</p>"));
    }

    #[test]
    fn keeps_plain_summary_and_content_as_text() {
        let xml = r#"<item xmlns="http://jabber.org/protocol/pubsub" id="post-8"><entry xmlns="http://www.w3.org/2005/Atom"><id>post-8</id><published>2024-05-01T12:00:00Z</published><author><uri>xmpp:alice@example.com</uri></author><summary>Short</summary><content>Long</content></entry></item>"#;
        let elem = Element::from_str(xml).unwrap();
        let fallback: Jid = "bob@example.com".parse().unwrap();
        let item = parse_item(&elem, &fallback).unwrap();
        assert_eq!(item.body.as_deref(), Some("Short\n\nLong"));
        assert!(item.html.is_none());
    }

    #[test]
    fn falls_back_to_updated_without_published() {
        let xml = r#"<item xmlns="http://jabber.org/protocol/pubsub" id="post-7"><entry xmlns="http://www.w3.org/2005/Atom"><id>post-7</id><updated>2024-05-02T03:04:05Z</updated><author><uri>xmpp:alice@example.com</uri></author><content>Hi</content></entry></item>"#;
        let elem = Element::from_str(xml).unwrap();
        let fallback: Jid = "bob@example.com".parse().unwrap();
        let item = parse_item(&elem, &fallback).unwrap();
        let expected: DateTime<Utc> = "2024-05-02T03:04:05Z".parse().unwrap();
        assert_eq!(item.published_at, expected);
        assert_eq!(item.updated_at, Some(expected));
    }

    #[test]
    fn parses_related_link_previews() {
        let xml = r#"<item xmlns="http://jabber.org/protocol/pubsub" id="post-11"><entry xmlns="http://www.w3.org/2005/Atom"><id>post-11</id><published>2024-05-01T12:00:00Z</published><author><uri>xmpp:alice@example.com</uri></author><content>Look at this</content><link rel="related" href="https://example.com/story" title="A story" description="A longer description" logo="https://example.com/logo.png"/><link rel="related" href="http://example.org/second"/><link rel="related" href="javascript:alert(1)" logo="javascript:alert(2)"/></entry></item>"#;
        let elem = Element::from_str(xml).unwrap();
        let fallback: Jid = "bob@example.com".parse().unwrap();
        let item = parse_item(&elem, &fallback).unwrap();
        assert_eq!(item.related.len(), 2);
        assert_eq!(item.related[0].url, "https://example.com/story");
        assert_eq!(item.related[0].title.as_deref(), Some("A story"));
        assert_eq!(
            item.related[0].description.as_deref(),
            Some("A longer description")
        );
        assert_eq!(
            item.related[0].logo.as_deref(),
            Some("https://example.com/logo.png")
        );
        assert_eq!(item.related[1].url, "http://example.org/second");
        assert!(item.related[1].logo.is_none());
    }

    #[test]
    fn parses_rich_xhtml_content() {
        let xml = r#"<item xmlns="http://jabber.org/protocol/pubsub" id="post-2"><entry xmlns="http://www.w3.org/2005/Atom"><id>post-2</id><published>2024-05-01T12:00:00Z</published><author><uri>xmpp:alice@example.com</uri></author><content type="xhtml"><div xmlns="http://www.w3.org/1999/xhtml"><p>Hello <strong>world</strong>.</p><ul><li>One</li><li>Two</li></ul></div></content></entry></item>"#;
        let elem = Element::from_str(xml).unwrap();
        let fallback: Jid = "bob@example.com".parse().unwrap();
        let item = parse_item(&elem, &fallback).unwrap();
        assert_eq!(item.body.as_deref(), Some("Hello world.\n\n• One\n• Two"));
        let html = item.html.as_deref().expect("rich html");
        assert!(html.contains("<p>Hello <strong>world</strong>.</p>"));
        assert!(html.contains("<ul><li>One</li><li>Two</li></ul>"));
    }

    #[test]
    fn rich_xhtml_drops_unsafe_markup() {
        let xml = r#"<item xmlns="http://jabber.org/protocol/pubsub" id="post-3"><entry xmlns="http://www.w3.org/2005/Atom"><id>post-3</id><published>2024-05-01T12:00:00Z</published><author><uri>xmpp:alice@example.com</uri></author><content type="xhtml"><div xmlns="http://www.w3.org/1999/xhtml"><p>Hello<script>alert(1)</script><a href="javascript:alert(1)" onclick="bad()">link</a><img src="https://tracker.example/pixel" onerror="bad()"/></p></div></content></entry></item>"#;
        let elem = Element::from_str(xml).unwrap();
        let fallback: Jid = "bob@example.com".parse().unwrap();
        let item = parse_item(&elem, &fallback).unwrap();
        let html = item.html.as_deref().expect("rich html");
        assert!(html.contains("Hello"));
        assert!(html.contains("link"));
        assert!(!html.contains("script"));
        assert!(!html.contains("javascript:"));
        assert!(!html.contains("onclick"));
        assert!(!html.contains("onerror"));
        assert!(!html.contains("<img"));
    }

    #[test]
    fn sanitize_html_accepts_its_own_output() {
        let safe = sanitize_html(r#"<p>Hi <a href="https://example.com">there</a></p>"#)
            .expect("sanitized");
        assert_eq!(sanitize_html(&safe).as_deref(), Some(safe.as_str()));
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
    fn keeps_a_reposts_original_author_and_via_link() {
        let xml = r#"<item xmlns="http://jabber.org/protocol/pubsub" id="post-9" publisher="bob@example.com"><entry xmlns="http://www.w3.org/2005/Atom"><id>post-9</id><published>2024-05-01T12:00:00Z</published><author><name>Alice</name><uri>xmpp:alice@example.com</uri></author><content>Original body</content><link rel="via" href="xmpp:alice@example.com?;node=urn%3Axmpp%3Amicroblog%3A0;item=post-1"/></entry></item>"#;
        let elem = Element::from_str(xml).unwrap();
        let fallback: Jid = "carol@example.com".parse().unwrap();
        let item = parse_item(&elem, &fallback).unwrap();
        assert_eq!(item.publisher.to_string(), "bob@example.com");
        assert!(item.author_name.is_none());
        assert_eq!(item.original_author_name.as_deref(), Some("Alice"));
        assert_eq!(
            item.original_author_jid.as_deref(),
            Some("alice@example.com")
        );
        assert_eq!(
            item.via.as_deref(),
            Some("xmpp:alice@example.com?;node=urn%3Axmpp%3Amicroblog%3A0;item=post-1")
        );
    }

    #[test]
    fn parses_a_threaded_reply() {
        let xml = r#"<item xmlns="http://jabber.org/protocol/pubsub" id="post-10"><entry xmlns="http://www.w3.org/2005/Atom" xmlns:thr="http://purl.org/syndication/thread/1.0"><id>post-10</id><published>2024-05-01T12:00:00Z</published><author><uri>xmpp:alice@example.com</uri></author><content>A reply</content><thr:in-reply-to href="xmpp:bob@example.com?;node=urn%3Axmpp%3Amicroblog%3A0;item=post-1"/></entry></item>"#;
        let elem = Element::from_str(xml).unwrap();
        let fallback: Jid = "carol@example.com".parse().unwrap();
        let item = parse_item(&elem, &fallback).unwrap();
        assert_eq!(
            item.in_reply_to.as_deref(),
            Some("xmpp:bob@example.com?;node=urn%3Axmpp%3Amicroblog%3A0;item=post-1")
        );
    }

    #[test]
    fn builds_text_content_with_a_type() {
        let entry = build_entry("id-5", "Body", None, "alice@example.com", Utc::now());
        let content = entry.get_child("content", NS_ATOM).expect("content");
        assert_eq!(content.attr("type"), Some("text"));
        assert_eq!(content.text(), "Body");
    }

    #[test]
    fn builds_repost_metadata() {
        let entry = build_repost_entry(
            "id-6",
            "Repeated",
            "alice@example.com",
            Some("Alice"),
            Utc::now(),
            "xmpp:alice@example.com?;node=urn%3Axmpp%3Amicroblog%3A0;item=post-1",
            &["rust".to_string()],
            &[],
        );
        let author = entry.get_child("author", NS_ATOM).expect("author");
        assert_eq!(
            author
                .get_child("name", NS_ATOM)
                .map(Element::text)
                .as_deref(),
            Some("Alice")
        );
        assert_eq!(
            author
                .get_child("uri", NS_ATOM)
                .map(Element::text)
                .as_deref(),
            Some("xmpp:alice@example.com")
        );
        let via = entry
            .children()
            .find(|child| child.attr("rel") == Some("via"))
            .expect("via link");
        assert_eq!(
            via.attr("href"),
            Some("xmpp:alice@example.com?;node=urn%3Axmpp%3Amicroblog%3A0;item=post-1")
        );
        assert_eq!(
            entry
                .get_child("content", NS_ATOM)
                .and_then(|content| content.attr("type")),
            Some("text")
        );
    }

    #[test]
    fn repost_entry_round_trips_through_the_parser() {
        let via = "xmpp:alice@example.com?;node=urn%3Axmpp%3Amicroblog%3A0;item=post-1";
        let entry = build_repost_entry(
            "id-7",
            "Repeated",
            "alice@example.com",
            Some("Alice"),
            Utc::now(),
            via,
            &[],
            &[],
        );
        let item = Element::builder("item", "http://jabber.org/protocol/pubsub")
            .attr("publisher", "bob@example.com")
            .append(entry)
            .build();
        let fallback: Jid = "carol@example.com".parse().unwrap();
        let parsed = parse_item(&item, &fallback).unwrap();
        assert_eq!(parsed.publisher.to_string(), "bob@example.com");
        assert_eq!(parsed.original_author_name.as_deref(), Some("Alice"));
        assert_eq!(
            parsed.original_author_jid.as_deref(),
            Some("alice@example.com")
        );
        assert_eq!(parsed.via.as_deref(), Some(via));
    }

    #[test]
    fn post_href_points_at_the_microblog_node() {
        let service: Jid = "alice@example.com".parse().unwrap();
        assert_eq!(
            post_href(&service, "post/one"),
            "xmpp:alice@example.com?;node=urn%3Axmpp%3Amicroblog%3A0;item=post%2Fone"
        );
    }

    #[test]
    fn converts_plain_text_to_safe_xhtml() {
        assert_eq!(
            text_to_xhtml("Hello <world>\nline two\n\nNew paragraph & more"),
            "<p>Hello &lt;world&gt;<br/>line two</p><p>New paragraph &amp; more</p>"
        );
        assert_eq!(text_to_xhtml("\n\n"), "");
    }

    #[test]
    fn builds_two_content_elements_for_rich_posts() {
        let entry = build_entry_rich(
            "id-8",
            "Plain body",
            Some("<p>Rich <strong>body</strong></p>"),
            None,
            "alice@example.com",
            Utc::now(),
            Utc::now(),
            &[],
            &[],
        );
        let contents: Vec<_> = entry
            .children()
            .filter(|child| child.name() == "content")
            .collect();
        assert_eq!(contents.len(), 2);
        assert_eq!(contents[0].attr("type"), Some("text"));
        assert_eq!(contents[0].text(), "Plain body");
        assert_eq!(contents[1].attr("type"), Some("xhtml"));
        let div = contents[1].children().next().expect("xhtml div");
        assert_eq!(div.ns(), NS_XHTML);
        assert!(String::from(div).contains("<strong>body</strong>"));
    }

    #[test]
    fn prefers_the_text_body_from_a_rich_post() {
        let entry = build_entry_rich(
            "id-9",
            "Plain body",
            Some("<p>Rich body</p>"),
            None,
            "alice@example.com",
            Utc::now(),
            Utc::now(),
            &[],
            &[],
        );
        let item = Element::builder("item", "http://jabber.org/protocol/pubsub")
            .append(entry)
            .build();
        let fallback: Jid = "bob@example.com".parse().unwrap();
        let parsed = parse_item(&item, &fallback).unwrap();
        assert_eq!(parsed.body.as_deref(), Some("Plain body"));
        assert_eq!(parsed.html.as_deref(), Some("<p>Rich body</p>"));
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
