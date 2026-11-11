//! Atom 1.0 and RSS 2.0 feeds for public blog and community pages.

#![allow(clippy::format_push_string)]

use crate::content::Post;
use crate::html::escape;

/// One feed entry with its absolute public URL.
pub struct FeedEntry<'a> {
    pub post: &'a Post,
    pub url: String,
}

fn title_of(post: &Post) -> String {
    post.title
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
        })
}

/// Render an Atom 1.0 feed.
#[must_use]
pub fn atom(
    title: &str,
    feed_id: &str,
    self_url: &str,
    site_url: &str,
    entries: &[FeedEntry<'_>],
) -> String {
    let updated = entries
        .iter()
        .map(|entry| entry.post.published)
        .max()
        .unwrap_or_else(chrono::Utc::now);
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>");
    xml.push_str("<feed xmlns=\"http://www.w3.org/2005/Atom\">");
    xml.push_str(&format!("<id>{}</id>", escape(feed_id)));
    xml.push_str(&format!("<title>{}</title>", escape(title)));
    xml.push_str(&format!("<updated>{}</updated>", updated.to_rfc3339()));
    xml.push_str(&format!(
        "<link rel=\"self\" href=\"{}\"/>",
        escape(self_url)
    ));
    xml.push_str(&format!(
        "<link rel=\"alternate\" href=\"{}\"/>",
        escape(site_url)
    ));
    for entry in entries {
        let post = entry.post;
        xml.push_str("<entry>");
        xml.push_str(&format!("<id>{}</id>", escape(&post.id)));
        xml.push_str(&format!("<title>{}</title>", escape(&title_of(post))));
        xml.push_str(&format!(
            "<published>{}</published>",
            post.published.to_rfc3339()
        ));
        xml.push_str(&format!(
            "<updated>{}</updated>",
            post.published.to_rfc3339()
        ));
        xml.push_str(&format!(
            "<author><name>{}</name><uri>xmpp:{}</uri></author>",
            escape(post.author_name.as_deref().unwrap_or(&post.author_jid)),
            escape(&post.author_jid)
        ));
        xml.push_str(&format!(
            "<link rel=\"alternate\" href=\"{}\"/>",
            escape(&entry.url)
        ));
        if let Some(via) = post.via.as_deref() {
            xml.push_str(&format!("<link rel=\"via\" href=\"{}\"/>", escape(via)));
        }
        if let Some(html) = post
            .html
            .as_deref()
            .and_then(xmpp_core::microblog::sanitize_html)
        {
            xml.push_str(&format!(
                "<content type=\"html\">{}</content>",
                escape(&html)
            ));
        } else {
            xml.push_str(&format!(
                "<content type=\"text\">{}</content>",
                escape(&post.body)
            ));
        }
        for tag in &post.tags {
            xml.push_str(&format!("<category term=\"{}\"/>", escape(tag)));
        }
        xml.push_str("</entry>");
    }
    xml.push_str("</feed>");
    xml
}

/// Render an RSS 2.0 feed.
#[must_use]
pub fn rss(
    title: &str,
    self_url: &str,
    site_url: &str,
    description: &str,
    entries: &[FeedEntry<'_>],
) -> String {
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>");
    xml.push_str("<rss version=\"2.0\"><channel>");
    xml.push_str(&format!("<title>{}</title>", escape(title)));
    xml.push_str(&format!("<link>{}</link>", escape(site_url)));
    xml.push_str(&format!(
        "<description>{}</description>",
        escape(description)
    ));
    xml.push_str(&format!(
        "<atom:link xmlns:atom=\"http://www.w3.org/2005/Atom\" \
rel=\"self\" href=\"{}\" type=\"application/rss+xml\"/>",
        escape(self_url)
    ));
    for entry in entries {
        let post = entry.post;
        xml.push_str("<item>");
        xml.push_str(&format!("<title>{}</title>", escape(&title_of(post))));
        xml.push_str(&format!("<link>{}</link>", escape(&entry.url)));
        xml.push_str(&format!(
            "<guid isPermaLink=\"true\">{}</guid>",
            escape(&entry.url)
        ));
        xml.push_str(&format!(
            "<pubDate>{}</pubDate>",
            post.published.to_rfc2822()
        ));
        let rich = post
            .html
            .as_deref()
            .and_then(xmpp_core::microblog::sanitize_html);
        xml.push_str(&format!(
            "<description>{}</description>",
            escape(rich.as_deref().unwrap_or(&post.body))
        ));
        xml.push_str("</item>");
    }
    xml.push_str("</channel></rss>");
    xml
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn post() -> Post {
        Post {
            id: "p1".to_string(),
            author_jid: "alice@example.org".to_string(),
            author_name: None,
            reposted_from: None,
            via: None,
            in_reply_to: None,
            related: Vec::new(),
            title: Some("Hello & goodbye".to_string()),
            body: "<script>alert(1)</script>".to_string(),
            html: None,
            published: chrono::Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap(),
            tags: vec!["rust".to_string()],
            media: Vec::new(),
        }
    }

    #[test]
    fn atom_escapes_content_and_carries_metadata() {
        let post = post();
        let entries = [FeedEntry {
            post: &post,
            url: "https://jabu.example/post/alice%40example.org/p1".to_string(),
        }];
        let xml = atom(
            "Alice",
            "https://jabu.example/blog/alice",
            "https://jabu.example/blog/alice/atom.xml",
            "https://jabu.example/blog/alice",
            &entries,
        );
        assert!(xml.contains("<title>Hello &amp; goodbye</title>"));
        assert!(xml.contains("&lt;script&gt;"));
        assert!(xml.contains("<category term=\"rust\"/>"));
        assert!(xml.starts_with("<?xml"));
    }

    #[test]
    fn atom_carries_sanitized_html() {
        let mut post = post();
        post.html = Some("<p>Hello <strong>world</strong></p>".to_string());
        let entries = [FeedEntry {
            post: &post,
            url: "https://jabu.example/post/alice%40example.org/p1".to_string(),
        }];
        let xml = atom(
            "Alice",
            "https://jabu.example/blog/alice",
            "https://jabu.example/blog/alice/atom.xml",
            "https://jabu.example/blog/alice",
            &entries,
        );
        assert!(xml.contains(
            "<content type=\"html\">&lt;p&gt;Hello &lt;strong&gt;world&lt;/strong&gt;&lt;/p&gt;</content>"
        ));
    }

    #[test]
    fn atom_carries_the_author_name_and_uri() {
        let mut post = post();
        post.author_name = Some("Alice".to_string());
        let entries = [FeedEntry {
            post: &post,
            url: "https://jabu.example/post/a/p1".to_string(),
        }];
        let xml = atom(
            "Alice",
            "https://jabu.example/blog/alice",
            "https://jabu.example/blog/alice/atom.xml",
            "https://jabu.example/blog/alice",
            &entries,
        );
        assert!(
            xml.contains("<author><name>Alice</name><uri>xmpp:alice@example.org</uri></author>")
        );
    }

    #[test]
    fn atom_carries_a_repost_via_link() {
        let mut post = post();
        post.via =
            Some("xmpp:alice@example.org?;node=urn%3Axmpp%3Amicroblog%3A0;item=p1".to_string());
        let entries = [FeedEntry {
            post: &post,
            url: "https://jabu.example/post/a/p1".to_string(),
        }];
        let xml = atom(
            "Alice",
            "https://jabu.example/blog/alice",
            "https://jabu.example/blog/alice/atom.xml",
            "https://jabu.example/blog/alice",
            &entries,
        );
        assert!(xml.contains(
            "<link rel=\"via\" href=\"xmpp:alice@example.org?;node=urn%3Axmpp%3Amicroblog%3A0;item=p1\"/>"
        ));
    }

    #[test]
    fn rss_uses_permalink_guids() {
        let post = post();
        let entries = [FeedEntry {
            post: &post,
            url: "https://jabu.example/post/a/p1".to_string(),
        }];
        let xml = rss(
            "Alice",
            "https://jabu.example/blog/a/rss.xml",
            "https://jabu.example/blog/a",
            "Posts",
            &entries,
        );
        assert!(xml.contains("<guid isPermaLink=\"true\">https://jabu.example/post/a/p1</guid>"));
        assert!(xml.contains("<pubDate>Thu"));
    }
}
