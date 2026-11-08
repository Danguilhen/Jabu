//! Server-rendered HTML for the public pages.
//!
//! Templates are written from scratch for Jabu; no Movim markup or assets are
//! used. User text is escaped, and rich HTML bodies are rendered as plain text
//! until a sanitizer is added.

use std::fmt::Write as _;

use chrono::{DateTime, Utc};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};

use crate::config::HostConfig;
use crate::content::{BlogProfile, MediaLink, Post};

/// Escapes text for HTML element content and attribute values.
#[must_use]
pub fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
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
    out
}

/// Encodes one URL path segment.
#[must_use]
pub fn segment(value: &str) -> String {
    utf8_percent_encode(value, NON_ALPHANUMERIC).to_string()
}

/// Page metadata used by the layout and the share cards.
pub struct PageMeta<'a> {
    pub title: &'a str,
    pub description: &'a str,
    pub canonical: &'a str,
    pub og_type: &'a str,
}

fn layout(config: &HostConfig, meta: &PageMeta<'_>, body: &str) -> String {
    let site = escape(&config.site_name);
    let title = escape(meta.title);
    let description = escape(meta.description);
    let canonical = escape(meta.canonical);
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<title>{title} · {site}</title>\
<meta name=\"description\" content=\"{description}\">\
<link rel=\"canonical\" href=\"{canonical}\">\
<meta property=\"og:site_name\" content=\"{site}\">\
<meta property=\"og:title\" content=\"{title}\">\
<meta property=\"og:description\" content=\"{description}\">\
<meta property=\"og:type\" content=\"{og_type}\">\
<meta property=\"og:url\" content=\"{canonical}\">\
<meta name=\"twitter:card\" content=\"summary\">\
<style>{style}</style></head>\
<body><header><a href=\"/\" class=\"brand\">{site}</a>\
<nav><a href=\"/about\">About</a></nav></header>\
<main>{body}</main>\
<footer>Served by {site} · <a href=\"/about\">About this instance</a></footer>\
</body></html>",
        og_type = escape(meta.og_type),
        style = STYLE,
    )
}

const STYLE: &str = "\
:root{color-scheme:dark}*{box-sizing:border-box}\
body{margin:0;background:#0b1220;color:#e5e7eb;font:16px/1.55 system-ui,sans-serif}\
a{color:#7dd3fc;text-decoration:none}a:hover{text-decoration:underline}\
header,footer{display:flex;gap:1rem;align-items:center;justify-content:space-between;\
padding:1rem 1.25rem;background:#111827;border-bottom:1px solid #1f2937}\
footer{border-top:1px solid #1f2937;border-bottom:0;color:#9ca3af;font-size:.85rem}\
.brand{font-weight:700;color:#f9fafb}main{max-width:46rem;margin:0 auto;padding:1.5rem 1.25rem}\
article,section{background:#111827;border:1px solid #1f2937;border-radius:.75rem;\
padding:1rem 1.25rem;margin-bottom:1rem}h1{font-size:1.6rem;margin:.2rem 0 .6rem}\
h2{font-size:1.15rem;margin:0 0 .4rem}.meta{color:#9ca3af;font-size:.85rem}\
.tags{display:flex;gap:.35rem;flex-wrap:wrap;margin:.6rem 0 0}.tag{background:#1f2937;\
border-radius:999px;padding:.1rem .55rem;font-size:.75rem;color:#cbd5e1}\
.media{margin-top:.7rem;display:flex;flex-wrap:wrap;gap:.5rem}\
.media img{max-width:100%;border-radius:.5rem}.media a{font-size:.85rem}\
p{margin:.4rem 0}";

fn paragraphs(text: &str) -> String {
    let mut html = String::new();
    for block in text.split("\n\n") {
        let trimmed = block.trim();
        if trimmed.is_empty() {
            continue;
        }
        html.push_str("<p>");
        for (index, line) in trimmed.lines().enumerate() {
            if index > 0 {
                html.push_str("<br>");
            }
            html.push_str(&escape(line));
        }
        html.push_str("</p>");
    }
    html
}

fn format_time(time: DateTime<Utc>) -> String {
    time.format("%Y-%m-%d %H:%M UTC").to_string()
}

fn media_html(media: &[MediaLink]) -> String {
    let mut html = String::new();
    for link in media {
        if !is_safe_url(&link.url) {
            continue;
        }
        let url = escape(&link.url);
        if link
            .mime
            .as_deref()
            .is_some_and(|mime| mime.starts_with("image/"))
        {
            let alt = escape(link.name.as_deref().unwrap_or(""));
            let _ = write!(
                html,
                "<a href=\"{url}\" rel=\"noreferrer noopener\"><img src=\"{url}\" alt=\"{alt}\" loading=\"lazy\"></a>"
            );
        } else {
            let name = escape(link.name.as_deref().unwrap_or(&link.url));
            let _ = write!(
                html,
                "<a href=\"{url}\" rel=\"noreferrer noopener\">{name}</a>"
            );
        }
    }
    if html.is_empty() {
        String::new()
    } else {
        format!("<div class=\"media\">{html}</div>")
    }
}

fn is_safe_url(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

fn post_path(post: &Post) -> String {
    format!("/post/{}/{}", segment(&post.author_jid), segment(&post.id))
}

fn tags_html(tags: &[String]) -> String {
    let mut html = String::new();
    for tag in tags {
        let _ = write!(html, "<span class=\"tag\">#{}</span>", escape(tag));
    }
    html
}

fn post_summary(post: &Post, _config: &HostConfig) -> String {
    let title = post
        .title
        .as_deref()
        .filter(|title| !title.is_empty())
        .map_or_else(|| post.body.lines().next().unwrap_or("Post"), |title| title);
    let tags = tags_html(&post.tags);
    format!(
        "<article><h2><a href=\"{}\">{}</a></h2>\
<p class=\"meta\">{} · {}</p>{}<div class=\"tags\">{}</div></article>",
        escape(&post_path(post)),
        escape(title),
        escape(&post.author_jid),
        format_time(post.published),
        paragraphs(&post.body),
        tags,
    )
}

/// Blog profile page.
#[must_use]
pub fn render_blog(profile: &BlogProfile, config: &HostConfig) -> String {
    let canonical = config.absolute(&format!("/blog/{}", segment(&profile.jid)));
    let title = profile.name.clone().unwrap_or_else(|| profile.jid.clone());
    let description = profile
        .description
        .clone()
        .unwrap_or_else(|| format!("Public posts by {title}"));
    let posts: String = profile
        .posts
        .iter()
        .map(|post| post_summary(post, config))
        .collect();
    let body = format!(
        "<section><h1>{}</h1><p class=\"meta\">{}</p>{}</section>{}",
        escape(&title),
        escape(&profile.jid),
        profile
            .description
            .as_deref()
            .map_or_else(String::new, paragraphs),
        if posts.is_empty() {
            "<p>No public posts yet.</p>".to_string()
        } else {
            posts
        }
    );
    layout(
        config,
        &PageMeta {
            title: &title,
            description: &description,
            canonical: &canonical,
            og_type: "profile",
        },
        &body,
    )
}

/// One public post.
#[must_use]
pub fn render_post(post: &Post, config: &HostConfig) -> String {
    let path = post_path(post);
    let canonical = config.absolute(&path);
    let title = post
        .title
        .clone()
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| {
            post.body
                .lines()
                .next()
                .unwrap_or("Post")
                .chars()
                .take(80)
                .collect()
        });
    let description: String = post.body.chars().take(160).collect();
    let tags = tags_html(&post.tags);
    let body = format!(
        "<article><h1>{}</h1><p class=\"meta\">By <a href=\"/blog/{}\">{}</a> · {}</p>{}{}<div class=\"tags\">{}</div></article>",
        escape(&title),
        escape(&segment(&post.author_jid)),
        escape(&post.author_jid),
        format_time(post.published),
        paragraphs(&post.body),
        media_html(&post.media),
        tags,
    );
    layout(
        config,
        &PageMeta {
            title: &title,
            description: &description,
            canonical: &canonical,
            og_type: "article",
        },
        &body,
    )
}

/// Instance landing page.
#[must_use]
pub fn render_index(config: &HostConfig) -> String {
    let body = format!(
        "<section><h1>{}</h1>{}<p>Open a public blog with <code>/blog/&lt;jid&gt;</code>.</p></section>",
        escape(&config.site_name),
        paragraphs(&config.description),
    );
    layout(
        config,
        &PageMeta {
            title: &config.site_name,
            description: &config.description,
            canonical: &config.absolute("/"),
            og_type: "website",
        },
        &body,
    )
}

/// About page.
#[must_use]
pub fn render_about(config: &HostConfig) -> String {
    let body = format!(
        "<section><h1>About {}</h1>{}<p>This instance serves public XMPP content only. \
Chats, private posts and account data are never exposed here.</p></section>",
        escape(&config.site_name),
        paragraphs(&config.description),
    );
    layout(
        config,
        &PageMeta {
            title: "About",
            description: &config.description,
            canonical: &config.absolute("/about"),
            og_type: "website",
        },
        &body,
    )
}

/// Minimal 404 page.
#[must_use]
pub fn render_not_found(config: &HostConfig) -> String {
    let body = "<section><h1>Not found</h1><p>This page does not exist or the content is not public.</p></section>";
    layout(
        config,
        &PageMeta {
            title: "Not found",
            description: "This page does not exist or the content is not public.",
            canonical: &config.absolute("/"),
            og_type: "website",
        },
        body,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> HostConfig {
        HostConfig {
            public_url: Some("https://jabu.example".to_string()),
            ..HostConfig::default()
        }
    }

    fn post() -> Post {
        Post {
            id: "p1".to_string(),
            author_jid: "alice@example.org".to_string(),
            title: Some("Hello".to_string()),
            body: "<script>alert(1)</script>".to_string(),
            published: Utc::now(),
            tags: vec!["rust".to_string()],
            media: Vec::new(),
        }
    }

    #[test]
    fn escapes_user_text() {
        let page = render_post(&post(), &config());
        assert!(page.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!page.contains("<script>alert(1)</script>"));
    }

    #[test]
    fn adds_open_graph_and_canonical_tags() {
        let page = render_post(&post(), &config());
        assert!(page.contains("property=\"og:type\" content=\"article\""));
        assert!(page.contains(
            "rel=\"canonical\" href=\"https://jabu.example/post/alice%40example%2Eorg/p1\""
        ));
    }

    #[test]
    fn skips_unsafe_media_urls() {
        let mut post = post();
        post.media = vec![MediaLink {
            url: "javascript:alert(1)".to_string(),
            mime: Some("image/png".to_string()),
            name: None,
        }];
        let page = render_post(&post, &config());
        assert!(!page.contains("javascript:"));
    }
}
