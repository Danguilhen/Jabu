//! Privacy-conscious link previews.
//!
//! Previews are fetched only when the user asks for one (or when the UI opts
//! in), never while loading history. The fetcher rejects local/private hosts
//! so a message cannot make the client probe the local network, caps the
//! response size and only reads a small HTML subset.

use std::collections::HashMap;
use std::sync::OnceLock;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use url::Url;

/// Maximum number of previews kept in the in-memory cache.
const CACHE_LIMIT: usize = 200;
/// Maximum HTML body read for metadata.
const MAX_HTML_BYTES: usize = 512 * 1024;

/// Metadata extracted from a link.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkPreview {
    pub url: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub site_name: Option<String>,
}

impl LinkPreview {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.title.is_none() && self.description.is_none() && self.image.is_none()
    }
}

/// Find the first http(s) URL in message text.
#[must_use]
pub fn first_url(text: &str) -> Option<String> {
    let mut starts: Vec<usize> = [text.find("https://"), text.find("http://")]
        .into_iter()
        .flatten()
        .collect();
    starts.sort_unstable();
    for start in starts {
        let candidate = &text[start..];
        let end = candidate
            .char_indices()
            .find(|(_, ch)| ch.is_whitespace() || matches!(ch, '<' | '>' | '"' | '\''))
            .map(|(index, _)| index)
            .unwrap_or(candidate.len());
        let url = candidate[..end].trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']', '}']);
        if let Ok(parsed) = Url::parse(url) {
            if matches!(parsed.scheme(), "http" | "https") {
                return Some(url.to_string());
            }
        }
    }
    None
}

/// Whether a URL may be fetched without touching a local or private host.
#[must_use]
pub fn is_fetchable(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        return false;
    }
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host = host
        .trim_matches(['[', ']'])
        .trim_end_matches('.')
        .to_lowercase();
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") {
        return false;
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return match ip {
            std::net::IpAddr::V4(ip) => {
                !(ip.is_private()
                    || ip.is_loopback()
                    || ip.is_link_local()
                    || ip.is_broadcast()
                    || ip.is_unspecified())
            }
            std::net::IpAddr::V6(ip) => {
                !(ip.is_loopback() || ip.is_unspecified() || ip.is_unique_local())
            }
        };
    }
    true
}

/// Parse OpenGraph/Twitter/HTML metadata from a page.
#[must_use]
pub fn parse_html(page_url: &str, html: &str) -> LinkPreview {
    let mut preview = LinkPreview {
        url: page_url.to_string(),
        ..LinkPreview::default()
    };
    let mut og_title = None;
    let mut og_description = None;
    let mut og_image = None;
    let mut og_site = None;
    let mut twitter_title = None;
    let mut twitter_description = None;
    let mut twitter_image = None;
    let mut meta_description = None;

    for tag in meta_tags(html) {
        let Some(key) = tag.get("property").or_else(|| tag.get("name")) else {
            continue;
        };
        let Some(content) = tag.get("content") else {
            continue;
        };
        let content = decode_entities(content).trim().to_string();
        if content.is_empty() {
            continue;
        }
        match key.to_lowercase().as_str() {
            "og:title" => og_title = Some(content),
            "og:description" => og_description = Some(content),
            "og:image" | "og:image:url" => og_image = Some(content),
            "og:site_name" => og_site = Some(content),
            "twitter:title" => twitter_title = Some(content),
            "twitter:description" => twitter_description = Some(content),
            "twitter:image" => twitter_image = Some(content),
            "description" => meta_description = Some(content),
            _ => {}
        }
    }

    preview.title = og_title
        .or(twitter_title)
        .or_else(|| html_title(html).map(|title| decode_entities(&title).trim().to_string()));
    preview.description = og_description.or(twitter_description).or(meta_description);
    preview.image = og_image
        .or(twitter_image)
        .and_then(|image| resolve_url(page_url, &image));
    preview.site_name = og_site;
    preview
}

fn resolve_url(base: &str, candidate: &str) -> Option<String> {
    if candidate.trim().is_empty() {
        None
    } else if candidate.starts_with("http://") || candidate.starts_with("https://") {
        Some(candidate.to_string())
    } else {
        Url::parse(base)
            .ok()
            .and_then(|base| base.join(candidate).ok())
            .map(|url| url.to_string())
    }
}

fn html_title(html: &str) -> Option<String> {
    let start = find_ascii_ci(html, "<title")?;
    let open_end = html[start..].find('>')? + start + 1;
    let close = find_ascii_ci(&html[open_end..], "</title>")? + open_end;
    Some(html[open_end..close].to_string())
}

fn find_ascii_ci(haystack: &str, needle: &str) -> Option<usize> {
    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle))
}

/// Collect `<meta ...>` attributes. This is deliberately a small scanner, not
/// a full HTML parser; it only needs attribute name/value pairs.
fn meta_tags(html: &str) -> Vec<HashMap<String, String>> {
    let mut tags = Vec::new();
    let mut offset = 0;
    while let Some(index) = find_ascii_ci(&html[offset..], "<meta") {
        let start = offset + index;
        let after_name = start + "<meta".len();
        if html[after_name..]
            .chars()
            .next()
            .is_some_and(|ch| !ch.is_whitespace() && ch != '>' && ch != '/')
        {
            offset = after_name;
            continue;
        }
        let Some(len) = html[after_name..].find('>') else {
            break;
        };
        let tag = &html[after_name..after_name + len];
        let mut attributes = HashMap::new();
        let mut cursor = 0;
        while let Some(eq) = tag[cursor..].find('=') {
            let eq = cursor + eq;
            let name = tag[..eq]
                .trim_end()
                .rsplit(|ch: char| ch.is_whitespace() || ch == '/' || ch == '<')
                .next()
                .unwrap_or("")
                .to_lowercase();
            let rest = tag[eq + 1..].trim_start();
            let Some(quote) = rest.chars().next().filter(|ch| *ch == '"' || *ch == '\'') else {
                cursor = eq + 1;
                continue;
            };
            let value_start = eq + 1 + (tag[eq + 1..].len() - rest.len()) + 1;
            let Some(end) = tag[value_start..].find(quote) else {
                break;
            };
            let value = &tag[value_start..value_start + end];
            if !name.is_empty() {
                attributes.insert(name, value.to_string());
            }
            cursor = value_start + end + 1;
        }
        tags.push(attributes);
        offset = after_name + len + 1;
    }
    tags
}

fn decode_entities(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find('&') {
        result.push_str(&rest[..index]);
        let tail = &rest[index..];
        let Some(end) = tail.find(';') else {
            result.push_str(tail);
            return result;
        };
        let entity = &tail[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(ch) => {
                result.push(ch);
                rest = &tail[end + 1..];
            }
            None => {
                result.push('&');
                rest = &tail[1..];
            }
        }
    }
    result.push_str(rest);
    result
}

static CACHE: OnceLock<RwLock<HashMap<String, LinkPreview>>> = OnceLock::new();

fn cache() -> &'static RwLock<HashMap<String, LinkPreview>> {
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Fetch a link preview, reusing the in-memory cache.
///
/// # Errors
///
/// Returns an error for an invalid/private URL, a non-HTML response, a network
/// failure or a response larger than the metadata limit.
#[cfg(not(target_arch = "wasm32"))]
pub async fn fetch(url: &str) -> crate::Result<LinkPreview> {
    let url = first_url(url).unwrap_or_else(|| url.to_string());
    if !is_fetchable(&url) {
        return Err(crate::XmppError::ProtocolError(
            "link previews are limited to public http(s) URLs".into(),
        ));
    }
    if let Some(cached) = cache().read().get(&url).cloned() {
        return Ok(cached);
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(6))
        .redirect(reqwest::redirect::Policy::limited(3))
        .user_agent("Jabu link preview")
        .build()
        .map_err(|error| crate::XmppError::ProtocolError(error.to_string()))?;
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|error| crate::XmppError::ProtocolError(error.to_string()))?;
    if !response.status().is_success() {
        return Err(crate::XmppError::ProtocolError(format!(
            "link preview returned HTTP {}",
            response.status()
        )));
    }
    if let Some(content_type) = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
    {
        if !content_type.to_lowercase().contains("text/html") {
            return Err(crate::XmppError::ProtocolError(
                "link preview target is not HTML".into(),
            ));
        }
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| crate::XmppError::ProtocolError(error.to_string()))?;
    if bytes.len() > MAX_HTML_BYTES {
        return Err(crate::XmppError::ProtocolError(
            "link preview page is too large".into(),
        ));
    }
    let html = String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_HTML_BYTES)]);
    let preview = parse_html(&url, &html);

    let mut cache = cache().write();
    if cache.len() >= CACHE_LIMIT {
        cache.clear();
    }
    cache.insert(url, preview.clone());
    Ok(preview)
}

/// Link previews are not available in the wasm build yet.
#[cfg(target_arch = "wasm32")]
pub async fn fetch(_url: &str) -> crate::Result<LinkPreview> {
    Err(crate::XmppError::ProtocolError(
        "link previews are not available on this platform".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_first_url_and_trims_punctuation() {
        assert_eq!(
            first_url("look at https://example.org/a?b=1, and this").as_deref(),
            Some("https://example.org/a?b=1")
        );
        assert_eq!(
            first_url("(see http://example.org/path).").as_deref(),
            Some("http://example.org/path")
        );
        assert_eq!(first_url("no link here"), None);
        assert_eq!(first_url("ftp://example.org/file"), None);
    }

    #[test]
    fn rejects_local_and_private_targets() {
        assert!(!is_fetchable("http://localhost/admin"));
        assert!(!is_fetchable("http://127.0.0.1/"));
        assert!(!is_fetchable("http://192.168.1.1/"));
        assert!(!is_fetchable("http://[::1]/"));
        assert!(!is_fetchable("file:///etc/passwd"));
        assert!(is_fetchable("https://example.org/post"));
    }

    #[test]
    fn parses_open_graph_and_resolves_relative_images() {
        let html = r#"<html><head>
            <title>Fallback title</title>
            <meta property="og:title" content="A &amp; B">
            <meta property="og:description" content="Hello &quot;world&quot;">
            <meta property="og:site_name" content="Example">
            <meta property="og:image" content="/images/card.png">
        </head></html>"#;
        let preview = parse_html("https://example.org/post/1", html);
        assert_eq!(preview.title.as_deref(), Some("A & B"));
        assert_eq!(preview.description.as_deref(), Some("Hello \"world\""));
        assert_eq!(preview.site_name.as_deref(), Some("Example"));
        assert_eq!(
            preview.image.as_deref(),
            Some("https://example.org/images/card.png")
        );
    }

    #[test]
    fn falls_back_to_title_and_description() {
        let html = r#"<html><head><title>Page &#39;title&#39;</title>
            <meta name="description" content="Plain description">
        </head></html>"#;
        let preview = parse_html("https://example.org/", html);
        assert_eq!(preview.title.as_deref(), Some("Page 'title'"));
        assert_eq!(preview.description.as_deref(), Some("Plain description"));
        assert!(preview.image.is_none());
    }

    #[test]
    fn handles_meta_tags_in_any_attribute_order() {
        let html = r#"<meta content="First" property="og:title">
            <meta name="twitter:description" content="Second">"#;
        let preview = parse_html("https://example.org/", html);
        assert_eq!(preview.title.as_deref(), Some("First"));
        assert_eq!(preview.description.as_deref(), Some("Second"));
    }

    #[test]
    fn non_ascii_text_before_meta_does_not_shift_offsets() {
        let html = "<html><body>İstanbul — café</body><head><title>Şehir</title><meta property=\"og:description\" content=\"Türkçe\"></head></html>";
        let preview = parse_html("https://example.org/", html);
        assert_eq!(preview.title.as_deref(), Some("Şehir"));
        assert_eq!(preview.description.as_deref(), Some("Türkçe"));
    }
}
