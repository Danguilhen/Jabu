//! Server-rendered HTML for the public pages.
//!
//! Templates are written from scratch for Jabu; no Movim markup or assets are
//! used. User text is escaped, and rich HTML bodies are rendered as plain text
//! until a sanitizer is added.

use std::fmt::Write as _;

use chrono::{DateTime, Utc};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};

use crate::config::HostConfig;
use crate::content::{
    BlogProfile, CommunityPage, ContactProfile, MediaLink, Post, SpacePage, Story,
};

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
    /// Absolute image URL for social cards.
    pub image: Option<&'a str>,
    /// Atom feed URL for autodiscovery.
    pub feed: Option<&'a str>,
}

fn layout(config: &HostConfig, meta: &PageMeta<'_>, body: &str) -> String {
    let site = escape(&config.site_name);
    let title = escape(meta.title);
    let description = escape(meta.description);
    let canonical = escape(meta.canonical);
    let image = meta
        .image
        .filter(|url| is_safe_url(url))
        .map(|url| format!("<meta property=\"og:image\" content=\"{}\">", escape(url)))
        .unwrap_or_default();
    let feed = meta
        .feed
        .map(|url| {
            format!(
                "<link rel=\"alternate\" type=\"application/atom+xml\" href=\"{}\">",
                escape(url)
            )
        })
        .unwrap_or_default();
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<title>{title} · {site}</title>\
<meta name=\"description\" content=\"{description}\">\
<link rel=\"canonical\" href=\"{canonical}\">\
{feed}\
<meta property=\"og:site_name\" content=\"{site}\">\
<meta property=\"og:title\" content=\"{title}\">\
<meta property=\"og:description\" content=\"{description}\">\
<meta property=\"og:type\" content=\"{og_type}\">\
<meta property=\"og:url\" content=\"{canonical}\">\
{image}\
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
.media video{max-width:100%;border-radius:.5rem}\
.rooms{list-style:none;padding:0;margin:0}.rooms li{border-top:1px solid #1f2937;padding:.6rem 0}\
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

/// `jabu://` deep link for a Host path.
#[must_use]
pub fn deep_link(path: &str) -> String {
    format!("jabu://{}", path.trim_start_matches('/'))
}

fn share_html(canonical: &str, deep: &str) -> String {
    format!(
        "<p class=\"meta\">Share: <a href=\"{}\">{}</a> · <a href=\"{}\">Open in Jabu</a></p>",
        escape(canonical),
        escape(canonical),
        escape(deep)
    )
}

fn tags_html(tags: &[String]) -> String {
    let mut html = String::new();
    for tag in tags {
        let _ = write!(html, "<span class=\"tag\">#{}</span>", escape(tag));
    }
    html
}

fn post_summary(post: &Post, _config: &HostConfig) -> String {
    post_summary_with(post, &post_path(post))
}

fn post_summary_with(post: &Post, href: &str) -> String {
    let title = post
        .title
        .as_deref()
        .filter(|title| !title.is_empty())
        .map_or_else(|| post.body.lines().next().unwrap_or("Post"), |title| title);
    let tags = tags_html(&post.tags);
    format!(
        "<article><h2><a href=\"{}\">{}</a></h2>\
<p class=\"meta\">{} · {}</p>{}<div class=\"tags\">{}</div></article>",
        escape(href),
        escape(title),
        escape(&post.author_jid),
        format_time(post.published),
        paragraphs(&post.body),
        tags,
    )
}

fn community_post_path(service: &str, node: &str, item_id: &str) -> String {
    format!(
        "/community/{}/{}/{}",
        segment(service),
        segment(node),
        segment(item_id)
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
    let share = share_html(
        &canonical,
        &deep_link(&format!("blog/{}", segment(&profile.jid))),
    );
    let body = format!(
        "<section><h1>{}</h1><p class=\"meta\">{}</p>{}{share}</section>{}",
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
    let feed = config.absolute(&format!("/blog/{}/atom.xml", segment(&profile.jid)));
    layout(
        config,
        &PageMeta {
            title: &title,
            description: &description,
            canonical: &canonical,
            og_type: "profile",
            image: None,
            feed: Some(&feed),
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
    let share = share_html(&canonical, &deep_link(&path));
    let body = format!(
        "<article><h1>{}</h1><p class=\"meta\">By <a href=\"/blog/{}\">{}</a> · {}</p>{}{}<div class=\"tags\">{}</div>{share}</article>",
        escape(&title),
        escape(&segment(&post.author_jid)),
        escape(&post.author_jid),
        format_time(post.published),
        paragraphs(&post.body),
        media_html(&post.media),
        tags,
    );
    let image = post
        .media
        .iter()
        .find(|media| {
            media
                .mime
                .as_deref()
                .is_some_and(|mime| mime.starts_with("image/"))
                && is_safe_url(&media.url)
        })
        .map(|media| media.url.as_str());
    layout(
        config,
        &PageMeta {
            title: &title,
            description: &description,
            canonical: &canonical,
            og_type: "article",
            image,
            feed: None,
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
            image: None,
            feed: None,
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
            image: None,
            feed: None,
        },
        &body,
    )
}

/// A public contact profile.
#[must_use]
pub fn render_contact(profile: &ContactProfile, config: &HostConfig) -> String {
    let canonical = config.absolute(&format!("/contact/{}", segment(&profile.jid)));
    let title = profile.name.clone().unwrap_or_else(|| profile.jid.clone());
    let description = profile
        .description
        .clone()
        .unwrap_or_else(|| format!("Public profile of {title}"));
    let blog = config.absolute(&format!("/blog/{}", segment(&profile.jid)));
    let share = share_html(
        &canonical,
        &deep_link(&format!("contact/{}", segment(&profile.jid))),
    );
    let mut subscriptions = String::new();
    for subscription in &profile.subscriptions {
        let label = subscription
            .title
            .clone()
            .unwrap_or_else(|| format!("{} / {}", subscription.server, subscription.node));
        let _ = write!(
            subscriptions,
            "<li><p><strong>{}</strong></p><p class=\"meta\">{} / {}</p></li>",
            escape(&label),
            escape(&subscription.server),
            escape(&subscription.node)
        );
    }
    let subscriptions_html = if profile.subscriptions.is_empty() {
        "<p>No public subscriptions.</p>".to_string()
    } else {
        format!("<ul class=\"rooms\">{subscriptions}</ul>")
    };
    let body = format!(
        "<section><h1>{}</h1><p class=\"meta\">{}</p>{}{share}\
<p class=\"meta\"><a href=\"{}\">Public blog</a></p></section>\
<section><h2>Public subscriptions</h2>{subscriptions_html}</section>",
        escape(&title),
        escape(&profile.jid),
        profile
            .description
            .as_deref()
            .map_or_else(String::new, paragraphs),
        escape(&blog),
    );
    layout(
        config,
        &PageMeta {
            title: &title,
            description: &description,
            canonical: &canonical,
            og_type: "profile",
            image: None,
            feed: None,
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
            image: None,
            feed: None,
        },
        body,
    )
}

/// Community page.
#[must_use]
pub fn render_community(page: &CommunityPage, config: &HostConfig) -> String {
    let canonical = config.absolute(&format!(
        "/community/{}/{}",
        segment(&page.service),
        segment(&page.node)
    ));
    let feed = config.absolute(&format!(
        "/community/{}/{}/atom.xml",
        segment(&page.service),
        segment(&page.node)
    ));
    let title = page
        .title
        .clone()
        .unwrap_or_else(|| format!("{} / {}", page.service, page.node));
    let description = page
        .description
        .clone()
        .unwrap_or_else(|| format!("Public community {title}"));
    let posts: String = page
        .posts
        .iter()
        .map(|post| {
            post_summary_with(
                post,
                &community_post_path(&page.service, &page.node, &post.id),
            )
        })
        .collect();
    let share = share_html(
        &canonical,
        &deep_link(&format!(
            "community/{}/{}",
            segment(&page.service),
            segment(&page.node)
        )),
    );
    let body = format!(
        "<section><h1>{}</h1><p class=\"meta\">{} / {}</p>{}\
<p class=\"meta\"><a href=\"{}\">Atom</a> · <a href=\"{}\">RSS</a></p>{share}</section>{}",
        escape(&title),
        escape(&page.service),
        escape(&page.node),
        page.description
            .as_deref()
            .map_or_else(String::new, paragraphs),
        escape(&feed),
        escape(&feed.replace("atom.xml", "rss.xml")),
        if posts.is_empty() {
            "<p>No public articles yet.</p>".to_string()
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
            og_type: "website",
            image: None,
            feed: Some(&feed),
        },
        &body,
    )
}

/// One community article.
#[must_use]
pub fn render_community_post(
    post: &Post,
    service: &str,
    node: &str,
    config: &HostConfig,
) -> String {
    let path = community_post_path(service, node, &post.id);
    let canonical = config.absolute(&path);
    let community = config.absolute(&format!(
        "/community/{}/{}",
        segment(service),
        segment(node)
    ));
    let title = post
        .title
        .clone()
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| {
            post.body
                .lines()
                .next()
                .unwrap_or("Article")
                .chars()
                .take(80)
                .collect()
        });
    let description: String = post.body.chars().take(160).collect();
    let image = post
        .media
        .iter()
        .find(|media| {
            media
                .mime
                .as_deref()
                .is_some_and(|mime| mime.starts_with("image/"))
                && is_safe_url(&media.url)
        })
        .map(|media| media.url.as_str());
    let share = share_html(&canonical, &deep_link(&path));
    let body = format!(
        "<article><h1>{}</h1><p class=\"meta\">By {} · {} · <a href=\"{}\">Community</a></p>{}{}<div class=\"tags\">{}</div>{share}</article>",
        escape(&title),
        escape(&post.author_jid),
        format_time(post.published),
        escape(&community),
        paragraphs(&post.body),
        media_html(&post.media),
        tags_html(&post.tags),
    );
    layout(
        config,
        &PageMeta {
            title: &title,
            description: &description,
            canonical: &canonical,
            og_type: "article",
            image,
            feed: None,
        },
        &body,
    )
}

/// Space page.
#[must_use]
pub fn render_space(page: &SpacePage, config: &HostConfig) -> String {
    let canonical = config.absolute(&format!(
        "/space/{}/{}",
        segment(&page.service),
        segment(&page.node)
    ));
    let title = page
        .title
        .clone()
        .unwrap_or_else(|| format!("{} / {}", page.service, page.node));
    let description = page
        .description
        .clone()
        .unwrap_or_else(|| format!("Public Space {title}"));
    let mut rooms = String::new();
    for room in &page.rooms {
        let voice = if room.voice {
            "<span class=\"tag\">voice</span>"
        } else {
            ""
        };
        let directory = room
            .directory
            .as_deref()
            .map(|directory| format!("<span class=\"tag\">{}</span>", escape(directory)))
            .unwrap_or_default();
        let _ = write!(
            rooms,
            "<li><p><strong>{}</strong>{voice}{directory}</p><p class=\"meta\">{}</p></li>",
            escape(&room.name),
            escape(&room.jid)
        );
    }
    let share = share_html(
        &canonical,
        &deep_link(&format!(
            "space/{}/{}",
            segment(&page.service),
            segment(&page.node)
        )),
    );
    let body = format!(
        "<section><h1>{}</h1><p class=\"meta\">{} / {}</p>{share}{}</section>\
<section><h2>Rooms</h2>{}</section>",
        escape(&title),
        escape(&page.service),
        escape(&page.node),
        page.description
            .as_deref()
            .map_or_else(String::new, paragraphs),
        if page.rooms.is_empty() {
            "<p>No public rooms listed.</p>".to_string()
        } else {
            format!("<ul class=\"rooms\">{rooms}</ul>")
        }
    );
    layout(
        config,
        &PageMeta {
            title: &title,
            description: &description,
            canonical: &canonical,
            og_type: "website",
            image: None,
            feed: None,
        },
        &body,
    )
}

/// One public Story.
#[must_use]
pub fn render_story(story: &Story, config: &HostConfig) -> String {
    let canonical = config.absolute(&format!(
        "/story/{}/{}",
        segment(&story.author_jid),
        segment(&story.id)
    ));
    let caption = if story.caption.trim().is_empty() {
        "Story".to_string()
    } else {
        story.caption.clone()
    };
    let title: String = caption
        .lines()
        .next()
        .unwrap_or("Story")
        .chars()
        .take(80)
        .collect();
    let image = story
        .media
        .as_ref()
        .filter(|media| {
            media
                .mime
                .as_deref()
                .is_some_and(|mime| mime.starts_with("image/"))
                && is_safe_url(&media.url)
        })
        .map(|media| media.url.as_str());
    let media = story
        .media
        .as_ref()
        .filter(|media| is_safe_url(&media.url))
        .map(|media| {
            let url = escape(&media.url);
            if media
                .mime
                .as_deref()
                .is_some_and(|mime| mime.starts_with("video/"))
            {
                format!("<video controls preload=\"metadata\" src=\"{url}\"></video>")
            } else {
                format!(
                    "<img src=\"{url}\" alt=\"{}\" loading=\"lazy\">",
                    escape(media.name.as_deref().unwrap_or("Story"))
                )
            }
        })
        .unwrap_or_default();
    let share = share_html(
        &canonical,
        &deep_link(&format!(
            "story/{}/{}",
            segment(&story.author_jid),
            segment(&story.id)
        )),
    );
    let body = format!(
        "<article><h1>{}</h1><p class=\"meta\">By {} · {}</p>\
<div class=\"media\">{}</div>{}<p class=\"meta\">A Story disappears after 24 hours.</p>{share}</article>",
        escape(&title),
        escape(&story.author_jid),
        format_time(story.published),
        media,
        paragraphs(&story.caption),
    );
    layout(
        config,
        &PageMeta {
            title: &title,
            description: &caption.chars().take(160).collect::<String>(),
            canonical: &canonical,
            og_type: "article",
            image,
            feed: None,
        },
        &body,
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
