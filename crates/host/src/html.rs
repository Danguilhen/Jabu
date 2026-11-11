//! Server-rendered HTML for the public pages.
//!
//! Templates are written from scratch for Jabu; no Movim markup or assets are
//! used. User text is escaped, and rich post bodies are sanitized by
//! `xmpp-core` before they are inserted.

use std::fmt::Write as _;

use chrono::{DateTime, Utc};
use jabu_i18n::Translator;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};

use crate::auth::{Role, TokenSummary};
use crate::config::HostConfig;
use crate::content::{
    BlogProfile, CommunityPage, ContactProfile, MediaLink, Post, SpacePage, Story,
};
use crate::moderation::{AuditEntry, Report, ReportStatus};
use crate::settings::RuntimeSettings;

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

fn layout(config: &HostConfig, meta: &PageMeta<'_>, body: &str, i18n: &Translator) -> String {
    let site = escape(&config.site_name);
    let lang = i18n.tag();
    let dir = if i18n.is_rtl() { "rtl" } else { "ltr" };
    let nav_about = i18n.t("host-nav-about");
    let footer_about = i18n.t("host-footer-about");
    let mut served_args = jabu_i18n::Args::new();
    served_args.set("site", site.clone());
    let served = i18n.t_args("host-footer-served", &served_args);
    let accent = if crate::settings::is_hex_color(&config.accent_color) {
        config.accent_color.as_str()
    } else {
        crate::settings::DEFAULT_ACCENT
    };
    let footer_note = if config.footer_note.trim().is_empty() {
        String::new()
    } else {
        format!(" · {}", escape(config.footer_note.trim()))
    };
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
        "<!doctype html><html lang=\"{lang}\" dir=\"{dir}\"><head><meta charset=\"utf-8\">\
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
<style>:root{{--accent:{accent}}}{style}</style></head>\
<body><header><a href=\"/\" class=\"brand\">{site}</a>\
<nav><a href=\"/about\">{nav_about}</a></nav></header>\
<main>{body}</main>\
<footer>{served}{footer_note} · <a href=\"/about\">{footer_about}</a></footer>\
</body></html>",
        og_type = escape(meta.og_type),
        style = STYLE,
    )
}

const STYLE: &str = "\
:root{color-scheme:dark}*{box-sizing:border-box}\
body{margin:0;background:#0b1220;color:#e5e7eb;font:16px/1.55 system-ui,sans-serif}\
a{color:var(--accent,#7dd3fc);text-decoration:none}a:hover{text-decoration:underline}\
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
form{display:flex;flex-direction:column;gap:.5rem;margin:.8rem 0}\
label{display:flex;flex-direction:column;gap:.25rem;font-size:.85rem;color:#cbd5e1}\
input,select,textarea,button{font:inherit;color:#e5e7eb;background:#0b1220;border:1px solid #374151;\
border-radius:.45rem;padding:.45rem .6rem}button{background:#1f2937;cursor:pointer;align-self:flex-start}\
button:hover{background:#374151}.error{color:#fca5a5}.notice{color:#86efac}\
p{margin:.4rem 0}\
.rich-text p{margin:0 0 .7rem}.rich-text p:last-child{margin-bottom:0}\
.rich-text ul,.rich-text ol{margin:.4rem 0 .7rem;padding-left:1.25rem}\
.rich-text li{margin:.2rem 0}.rich-text blockquote{margin:.5rem 0;padding-left:.8rem;\
border-left:2px solid #374151;color:#cbd5e1}\
.rich-text code{background:#0b1220;border-radius:.25rem;padding:.05rem .25rem;\
font-family:ui-monospace,monospace;font-size:.9em}\
.rich-text pre{background:#0b1220;border-radius:.4rem;padding:.6rem;overflow-x:auto}\
.rich-text h1,.rich-text h2,.rich-text h3{font-size:1.05rem;margin:.6rem 0 .3rem}";

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

fn format_time(time: DateTime<Utc>, i18n: &Translator) -> String {
    i18n.date_time(time)
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

fn share_html(canonical: &str, deep: &str, target: &str, i18n: &Translator) -> String {
    let report = utf8_percent_encode(target, NON_ALPHANUMERIC).to_string();
    format!(
        "<p class=\"meta\">{} <a href=\"{}\">{}</a> · <a href=\"{}\">{}</a> · \
<a href=\"/report?target={}\">{}</a></p>",
        escape(&i18n.t("host-share")),
        escape(canonical),
        escape(canonical),
        escape(deep),
        escape(&i18n.t("host-open-in-jabu")),
        escape(&report),
        escape(&i18n.t("host-report-page"))
    )
}

fn tags_html(tags: &[String]) -> String {
    let mut html = String::new();
    for tag in tags {
        let _ = write!(html, "<span class=\"tag\">#{}</span>", escape(tag));
    }
    html
}

/// Attribution lines for a repeated or threaded post.
fn attribution_html(post: &Post, i18n: &Translator) -> String {
    let mut html = String::new();
    if let Some(via) = post
        .via
        .as_deref()
        .filter(|url| is_safe_url(url) || url.starts_with("xmpp:"))
    {
        let name = post.reposted_from.as_deref().unwrap_or_default();
        let _ = write!(
            html,
            "<p class=\"meta repost\"><a href=\"{}\" rel=\"noopener noreferrer\">↻ {}</a></p>",
            escape(via),
            escape(name)
        );
    }
    if let Some(reply) = post
        .in_reply_to
        .as_deref()
        .filter(|url| is_safe_url(url) || url.starts_with("xmpp:"))
    {
        let _ = write!(
            html,
            "<p class=\"meta reply\"><a href=\"{}\" rel=\"noopener noreferrer\">↳ {}</a></p>",
            escape(reply),
            escape(&i18n.t("feed-in-reply-to"))
        );
    }
    html
}

fn post_summary(post: &Post, _config: &HostConfig, i18n: &Translator) -> String {
    post_summary_with(post, &post_path(post), i18n)
}

fn post_summary_with(post: &Post, href: &str, i18n: &Translator) -> String {
    let fallback = i18n.t("host-post-fallback");
    let title = post
        .title
        .as_deref()
        .filter(|title| !title.is_empty())
        .map_or_else(
            || post.body.lines().next().unwrap_or(&fallback),
            |title| title,
        );
    let tags = tags_html(&post.tags);
    format!(
        "<article><h2><a href=\"{}\">{}</a></h2>\
<p class=\"meta\">{} · {}</p>{}<div class=\"tags\">{}</div></article>",
        escape(href),
        escape(title),
        escape(post.author_name.as_deref().unwrap_or(&post.author_jid)),
        format_time(post.published, i18n),
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
pub fn render_blog(profile: &BlogProfile, config: &HostConfig, i18n: &Translator) -> String {
    let canonical = config.absolute(&format!("/blog/{}", segment(&profile.jid)));
    let title = profile.name.clone().unwrap_or_else(|| profile.jid.clone());
    let description = profile.description.clone().unwrap_or_else(|| {
        let mut args = jabu_i18n::Args::new();
        args.set("title", title.clone());
        i18n.t_args("host-blog-description", &args)
    });
    let posts: String = profile
        .posts
        .iter()
        .map(|post| post_summary(post, config, i18n))
        .collect();
    let share = share_html(
        &canonical,
        &deep_link(&format!("blog/{}", segment(&profile.jid))),
        &crate::moderation::contact_target(&profile.jid),
        i18n,
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
            format!("<p>{}</p>", escape(&i18n.t("host-blog-empty")))
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
        i18n,
    )
}

/// One public post.
#[must_use]
pub fn render_post(post: &Post, config: &HostConfig, i18n: &Translator) -> String {
    let path = post_path(post);
    let canonical = config.absolute(&path);
    let fallback = i18n.t("host-post-fallback");
    let title = post
        .title
        .clone()
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| {
            post.body
                .lines()
                .next()
                .unwrap_or(&fallback)
                .chars()
                .take(80)
                .collect()
        });
    let description: String = post.body.chars().take(160).collect();
    let tags = tags_html(&post.tags);
    let attribution = attribution_html(post, i18n);
    let body_html = post
        .html
        .as_deref()
        .and_then(xmpp_core::microblog::sanitize_html)
        .map_or_else(
            || paragraphs(&post.body),
            |html| format!("<div class=\"rich-text\">{html}</div>"),
        );
    let share = share_html(
        &canonical,
        &deep_link(&path),
        &crate::moderation::post_target(&post.author_jid, &post.id),
        i18n,
    );
    let body = format!(
        "<article><h1>{}</h1><p class=\"meta\">{} <a href=\"/blog/{}\">{}</a> · {}</p>{attribution}{}{}<div class=\"tags\">{}</div>{share}</article>",
        escape(&title),
        escape(&i18n.t("host-by")),
        escape(&segment(&post.author_jid)),
        escape(post.author_name.as_deref().unwrap_or(&post.author_jid)),
        format_time(post.published, i18n),
        body_html,
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
        i18n,
    )
}

/// Instance landing page.
#[must_use]
pub fn render_index(config: &HostConfig, i18n: &Translator) -> String {
    let mut hint_args = jabu_i18n::Args::new();
    hint_args.set("code", "<code>/blog/&lt;jid&gt;</code>");
    let hint = i18n.t_args("host-index-hint", &hint_args);
    let body = format!(
        "<section><h1>{}</h1>{}<p>{hint}</p></section>",
        escape(&config.site_name),
        paragraphs(&config.description)
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
        i18n,
    )
}

/// About page.
#[must_use]
pub fn render_about(config: &HostConfig, i18n: &Translator) -> String {
    let mut args = jabu_i18n::Args::new();
    args.set("site", config.site_name.clone());
    let heading = i18n.t_args("host-about-title", &args);
    let note = i18n.t("host-about-body");
    let body = format!(
        "<section><h1>{heading}</h1>{}<p>{note}</p></section>",
        paragraphs(&config.description),
    );
    layout(
        config,
        &PageMeta {
            title: &i18n.t("host-nav-about"),
            description: &config.description,
            canonical: &config.absolute("/about"),
            og_type: "website",
            image: None,
            feed: None,
        },
        &body,
        i18n,
    )
}

/// A public contact profile.
#[must_use]
pub fn render_contact(profile: &ContactProfile, config: &HostConfig, i18n: &Translator) -> String {
    let canonical = config.absolute(&format!("/contact/{}", segment(&profile.jid)));
    let title = profile.name.clone().unwrap_or_else(|| profile.jid.clone());
    let description = profile.description.clone().unwrap_or_else(|| {
        let mut args = jabu_i18n::Args::new();
        args.set("title", title.clone());
        i18n.t_args("host-contact-description", &args)
    });
    let blog = config.absolute(&format!("/blog/{}", segment(&profile.jid)));
    let share = share_html(
        &canonical,
        &deep_link(&format!("contact/{}", segment(&profile.jid))),
        &crate::moderation::contact_target(&profile.jid),
        i18n,
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
        format!(
            "<p>{}</p>",
            escape(&i18n.t("host-contact-no-subscriptions"))
        )
    } else {
        format!("<ul class=\"rooms\">{subscriptions}</ul>")
    };
    let body = format!(
        "<section><h1>{}</h1><p class=\"meta\">{}</p>{}{share}\
<p class=\"meta\"><a href=\"{}\">{}</a></p></section>\
<section><h2>{}</h2>{subscriptions_html}</section>",
        escape(&title),
        escape(&profile.jid),
        profile
            .description
            .as_deref()
            .map_or_else(String::new, paragraphs),
        escape(&blog),
        escape(&i18n.t("host-public-blog")),
        escape(&i18n.t("host-public-subscriptions")),
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
        i18n,
    )
}

/// Public report form.
#[must_use]
pub fn render_report_form(target: &str, config: &HostConfig, i18n: &Translator) -> String {
    let body = format!(
        "<section><h1>{}</h1>\
<p class=\"meta\">{} <code>{}</code></p>\
<form method=\"post\" action=\"/report\">\
<input type=\"hidden\" name=\"target\" value=\"{}\">\
<label>{}<select name=\"reason\">\
<option value=\"spam\">{}</option>\
<option value=\"abuse\">{}</option>\
<option value=\"illegal\">{}</option>\
<option value=\"other\">{}</option>\
</select></label>\
<label>{}<textarea name=\"details\" rows=\"4\" maxlength=\"2000\"></textarea></label>\
<label>{}<input type=\"text\" name=\"reporter\" maxlength=\"200\"></label>\
<button type=\"submit\">{}</button>\
</form>\
<p class=\"meta\">{}</p></section>",
        escape(&i18n.t("host-report-page-title")),
        escape(&i18n.t("host-target")),
        escape(target),
        escape(target),
        escape(&i18n.t("report-reason")),
        escape(&i18n.t("report-spam")),
        escape(&i18n.t("report-abuse")),
        escape(&i18n.t("report-illegal")),
        escape(&i18n.t("report-other")),
        escape(&i18n.t("host-report-details")),
        escape(&i18n.t("host-report-contact")),
        escape(&i18n.t("report-send")),
        escape(&i18n.t("host-report-note")),
    );
    layout(
        config,
        &PageMeta {
            title: &i18n.t("host-report-page-title"),
            description: &i18n.t("host-report-description"),
            canonical: &config.absolute("/report"),
            og_type: "website",
            image: None,
            feed: None,
        },
        &body,
        i18n,
    )
}

/// Receipt shown after a report is stored.
#[must_use]
pub fn render_report_receipt(report: &Report, config: &HostConfig, i18n: &Translator) -> String {
    let status_url = config.absolute(&format!("/report/{}", segment(&report.id)));
    let mut args = jabu_i18n::Args::new();
    args.set("id", escape(&report.id));
    let thanks = i18n.t_args("host-report-thanks", &args);
    let body = format!(
        "<section><h1>{}</h1>\
<p>{thanks}</p>\
<p class=\"meta\">{} {}</p>\
<p class=\"meta\">{} {}</p>\
<p><a href=\"{}\">{}</a></p></section>",
        escape(&i18n.t("host-report-received")),
        escape(&i18n.t("host-target")),
        escape(&report.target),
        escape(&i18n.t("host-status")),
        report.status.slug(),
        escape(&status_url),
        escape(&i18n.t("host-report-check")),
    );
    layout(
        config,
        &PageMeta {
            title: &i18n.t("host-report-received"),
            description: &i18n.t("host-report-received"),
            canonical: &status_url,
            og_type: "website",
            image: None,
            feed: None,
        },
        &body,
        i18n,
    )
}

/// Reporter-facing report status.
#[must_use]
pub fn render_report_status(report: &Report, config: &HostConfig, i18n: &Translator) -> String {
    let outcome = i18n.t(match report.status {
        ReportStatus::Open => "host-report-open",
        ReportStatus::Hidden => "host-report-hidden",
        ReportStatus::Dismissed => "host-report-dismissed",
    });
    let mut created_args = jabu_i18n::Args::new();
    created_args.set("time", format_time(report.created_at, i18n));
    let created = i18n.t_args("host-report-created", &created_args);
    let body = format!(
        "<section><h1>{}</h1>\
<p class=\"meta\">{} {}</p><p>{}</p>\
<p class=\"meta\">{created}</p></section>",
        escape(&i18n.t("host-report-status-title")),
        escape(&i18n.t("host-target")),
        escape(&report.target),
        escape(&outcome),
    );
    layout(
        config,
        &PageMeta {
            title: &i18n.t("host-report-status-title"),
            description: &i18n.t("host-report-status-description"),
            canonical: &config.absolute(&format!("/report/{}", segment(&report.id))),
            og_type: "website",
            image: None,
            feed: None,
        },
        &body,
        i18n,
    )
}

/// Admin login form.
#[must_use]
pub fn render_admin_login(config: &HostConfig, i18n: &Translator, error: Option<&str>) -> String {
    let error = error
        .map(|error| format!("<p class=\"error\">{}</p>", escape(error)))
        .unwrap_or_default();
    let body = format!(
        "<section><h1>{}</h1>{error}\
<form method=\"post\" action=\"/admin/login\">\
<label>{}<input type=\"password\" name=\"token\" autocomplete=\"current-password\"></label>\
<button type=\"submit\">{}</button></form></section>",
        escape(&i18n.t("host-admin-signin-title")),
        escape(&i18n.t("host-admin-token-label")),
        escape(&i18n.t("host-admin-signin")),
    );
    layout(
        config,
        &PageMeta {
            title: &i18n.t("host-admin-title"),
            description: &i18n.t("host-admin-description"),
            canonical: &config.absolute("/admin"),
            og_type: "website",
            image: None,
            feed: None,
        },
        &body,
        i18n,
    )
}

/// Admin report queue and recent audit entries.
#[must_use]
pub fn render_admin(
    reports: &[Report],
    audit: &[AuditEntry],
    role: Role,
    config: &HostConfig,
    i18n: &Translator,
) -> String {
    let admin_links = if role.allows(Role::Admin) {
        format!(
            " · <a href=\"/admin/settings\">{}</a> · <a href=\"/admin/tokens\">{}</a> · \
<a href=\"/admin/api/backup\">{}</a>",
            escape(&i18n.t("host-admin-settings")),
            escape(&i18n.t("host-admin-tokens")),
            escape(&i18n.t("host-admin-backup")),
        )
    } else {
        String::new()
    };
    let mut queue = String::new();
    for report in reports {
        let details = report
            .details
            .as_deref()
            .map(paragraphs)
            .unwrap_or_default();
        let reporter = report
            .reporter
            .as_deref()
            .map(|reporter| {
                let mut args = jabu_i18n::Args::new();
                args.set("name", reporter.to_string());
                format!(" · {}", escape(&i18n.t_args("host-admin-reporter", &args)))
            })
            .unwrap_or_default();
        let _ = write!(
            queue,
            "<li><p><strong>{}</strong> <span class=\"tag\">{}</span></p>\
<p class=\"meta\">{} · {}{}</p>{details}\
<form method=\"post\" action=\"/admin/reports/{}/action\">\
<select name=\"action\">\
<option value=\"hide\">{}</option>\
<option value=\"unhide\">{}</option>\
<option value=\"dismiss\">{}</option>\
<option value=\"reopen\">{}</option>\
</select>\
<input type=\"text\" name=\"note\" placeholder=\"{}\" maxlength=\"500\">\
<button type=\"submit\">{}</button></form></li>",
            escape(&report.target),
            escape(&report_status_label(&i18n, report.status.slug())),
            escape(&report.reason),
            format_time(report.created_at, &i18n),
            reporter,
            escape(&report.id),
            escape(&i18n.t("host-admin-action-hide")),
            escape(&i18n.t("host-admin-action-unhide")),
            escape(&i18n.t("host-admin-action-dismiss")),
            escape(&i18n.t("host-admin-action-reopen")),
            escape(&i18n.t("host-admin-note-placeholder")),
            escape(&i18n.t("host-admin-apply")),
        );
    }
    let mut log = String::new();
    for entry in audit.iter().take(20) {
        let _ = write!(
            log,
            "<li><p><strong>{}</strong> {}</p><p class=\"meta\">{} · {}{}</p></li>",
            escape(&admin_action_label(&i18n, &entry.action)),
            escape(&entry.target),
            format_time(entry.created_at, &i18n),
            escape(&entry.actor),
            entry
                .note
                .as_deref()
                .map(|note| format!(" · {}", escape(note)))
                .unwrap_or_default()
        );
    }
    let body = format!(
        "<section><h1>{}</h1>\
<p class=\"meta\">{} · <a href=\"/admin/logout\">{}</a> · \
<a href=\"/admin/api/reports\">JSON</a>{admin_links}</p>\
{}</section><section><h2>{}</h2>{}</section>",
        escape(&i18n.t("host-admin-queue-title")),
        escape(&i18n.tn("host-admin-report-count", reports.len() as i64)),
        escape(&i18n.t("host-admin-signout")),
        if queue.is_empty() {
            format!("<p>{}</p>", escape(&i18n.t("host-admin-no-reports")))
        } else {
            format!("<ul class=\"rooms\">{queue}</ul>")
        },
        escape(&i18n.t("host-admin-audit")),
        if log.is_empty() {
            format!("<p>{}</p>", escape(&i18n.t("host-admin-no-actions")))
        } else {
            format!("<ul class=\"rooms\">{log}</ul>")
        }
    );
    layout(
        config,
        &PageMeta {
            title: &i18n.t("host-admin-moderation-title"),
            description: &i18n.t("host-admin-moderation-description"),
            canonical: &config.absolute("/admin"),
            og_type: "website",
            image: None,
            feed: None,
        },
        &body,
        i18n,
    )
}

/// Instance settings form.
#[must_use]
pub fn render_admin_settings(
    settings: &RuntimeSettings,
    config: &HostConfig,
    i18n: &Translator,
    error: Option<&str>,
    notice: Option<&str>,
) -> String {
    let message = error
        .map(|error| format!("<p class=\"error\">{}</p>", escape(error)))
        .or_else(|| notice.map(|notice| format!("<p class=\"notice\">{}</p>", escape(notice))))
        .unwrap_or_default();
    let featured = settings.featured_blogs.join("\n");
    let body = format!(
        "<section><h1>{}</h1>{message}\
<form method=\"post\" action=\"/admin/settings\">\
<label>{}<input type=\"text\" name=\"site_name\" maxlength=\"80\" \
value=\"{}\" required></label>\
<label>{}<textarea name=\"description\" rows=\"3\" maxlength=\"500\">{}</textarea></label>\
<label>{}<input type=\"text\" name=\"accent_color\" maxlength=\"7\" \
value=\"{}\" placeholder=\"#7dd3fc\"></label>\
<label>{}<input type=\"text\" name=\"footer_note\" maxlength=\"500\" \
value=\"{}\" placeholder=\"{}\"></label>\
<label>{}<textarea name=\"featured_blogs\" rows=\"4\">{}</textarea></label>\
<label>{}\
<input type=\"number\" name=\"retention_days\" min=\"0\" max=\"3650\" value=\"{}\"></label>\
<button type=\"submit\">{}</button></form>\
<p class=\"meta\"><a href=\"/admin\">{}</a> · \
{}</p>\
</section>",
        escape(&i18n.t("host-admin-settings-title")),
        escape(&i18n.t("host-admin-site-name")),
        escape(&settings.site_name),
        escape(&i18n.t("host-admin-description-label")),
        escape(&settings.description),
        escape(&i18n.t("host-admin-accent")),
        escape(&settings.accent_color),
        escape(&i18n.t("host-admin-footer-note")),
        escape(&settings.footer_note),
        escape(&i18n.t("host-admin-footer-placeholder")),
        escape(&i18n.t("host-admin-featured")),
        escape(&featured),
        escape(&i18n.t("host-admin-retention")),
        settings.retention_days,
        escape(&i18n.t("host-admin-save-settings")),
        escape(&i18n.t("host-admin-back")),
        escape(&i18n.t("host-admin-config-note")),
    );
    layout(
        config,
        &PageMeta {
            title: &i18n.t("host-admin-settings"),
            description: &i18n.t("host-admin-settings-description"),
            canonical: &config.absolute("/admin/settings"),
            og_type: "website",
            image: None,
            feed: None,
        },
        &body,
        i18n,
    )
}

/// Admin token management page.
#[must_use]
pub fn render_admin_tokens(
    tokens: &[TokenSummary],
    config: &HostConfig,
    i18n: &Translator,
    new_token: Option<&str>,
    error: Option<&str>,
) -> String {
    let message = error
        .map(|error| format!("<p class=\"error\">{}</p>", escape(error)))
        .unwrap_or_default();
    let revealed = new_token.map_or_else(String::new, |token| {
        format!(
            "<p class=\"notice\">{}</p>\
<p><code>{}</code></p>",
            escape(&i18n.t("host-admin-new-token")),
            escape(token)
        )
    });
    let mut list = String::new();
    for token in tokens {
        let _ = write!(
            list,
            "<li><p><strong>{}</strong> <span class=\"tag\">{}</span></p>\
<p class=\"meta\">{}</p>\
<form method=\"post\" action=\"/admin/tokens/{}/revoke\">\
<button type=\"submit\">{}</button></form></li>",
            escape(&token.label),
            escape(&role_label(&i18n, token.role)),
            {
                let mut args = jabu_i18n::Args::new();
                args.set("time", format_time(token.created_at, &i18n));
                escape(&i18n.t_args("host-admin-created", &args))
            },
            escape(&token.id),
            escape(&i18n.t("host-admin-revoke")),
        );
    }
    let body = format!(
        "<section><h1>{}</h1>\
<p class=\"meta\"><a href=\"/admin\">{}</a></p>{message}{revealed}\
<ul class=\"rooms\">{}</ul></section>\
<section><h2>{}</h2>\
<form method=\"post\" action=\"/admin/tokens\">\
<label>{}<input type=\"text\" name=\"label\" maxlength=\"60\" \
placeholder=\"{}\" required></label>\
<label>{}<select name=\"role\">\
<option value=\"moderator\">{}</option>\
<option value=\"admin\">{}</option>\
</select></label>\
<button type=\"submit\">{}</button></form></section>",
        escape(&i18n.t("host-admin-tokens-title")),
        escape(&i18n.t("host-admin-back")),
        if list.is_empty() {
            format!(
                "<li><p>{}</p></li>",
                escape(&i18n.t("host-admin-no-tokens"))
            )
        } else {
            list
        },
        escape(&i18n.t("host-admin-create-title")),
        escape(&i18n.t("host-admin-label")),
        escape(&i18n.t("host-admin-label-placeholder")),
        escape(&i18n.t("host-admin-role")),
        escape(&i18n.t("host-admin-role-moderator")),
        escape(&i18n.t("host-admin-role-admin")),
        escape(&i18n.t("host-admin-create")),
    );
    layout(
        config,
        &PageMeta {
            title: &i18n.t("host-admin-tokens"),
            description: &i18n.t("host-admin-tokens-description"),
            canonical: &config.absolute("/admin/tokens"),
            og_type: "website",
            image: None,
            feed: None,
        },
        &body,
        i18n,
    )
}

/// Minimal 404 page.
#[must_use]
pub fn render_not_found(config: &HostConfig, i18n: &Translator) -> String {
    let body = format!(
        "<section><h1>{}</h1><p>{}</p></section>",
        escape(&i18n.t("host-not-found")),
        escape(&i18n.t("host-not-found-detail"))
    );
    let body = body.as_str();
    layout(
        config,
        &PageMeta {
            title: &i18n.t("host-not-found"),
            description: &i18n.t("host-not-found-detail"),
            canonical: &config.absolute("/"),
            og_type: "website",
            image: None,
            feed: None,
        },
        body,
        i18n,
    )
}

/// Community page.
#[must_use]
pub fn render_community(page: &CommunityPage, config: &HostConfig, i18n: &Translator) -> String {
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
    let description = page.description.clone().unwrap_or_else(|| {
        let mut args = jabu_i18n::Args::new();
        args.set("title", title.clone());
        i18n.t_args("host-community-description", &args)
    });
    let posts: String = page
        .posts
        .iter()
        .map(|post| {
            post_summary_with(
                post,
                &community_post_path(&page.service, &page.node, &post.id),
                i18n,
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
        &crate::moderation::community_target(&page.service, &page.node),
        i18n,
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
            format!("<p>{}</p>", escape(&i18n.t("host-community-empty")))
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
        i18n,
    )
}

/// One community article.
#[must_use]
pub fn render_community_post(
    post: &Post,
    service: &str,
    node: &str,
    config: &HostConfig,
    i18n: &Translator,
) -> String {
    let path = community_post_path(service, node, &post.id);
    let canonical = config.absolute(&path);
    let community = config.absolute(&format!(
        "/community/{}/{}",
        segment(service),
        segment(node)
    ));
    let fallback = i18n.t("host-article-fallback");
    let title = post
        .title
        .clone()
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| {
            post.body
                .lines()
                .next()
                .unwrap_or(&fallback)
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
    let share = share_html(
        &canonical,
        &deep_link(&path),
        &crate::moderation::community_post_target(service, node, &post.id),
        i18n,
    );
    let attribution = attribution_html(post, i18n);
    let body = format!(
        "<article><h1>{}</h1><p class=\"meta\">{} {} · {} · <a href=\"{}\">{}</a></p>{attribution}{}{}<div class=\"tags\">{}</div>{share}</article>",
        escape(&title),
        escape(&i18n.t("host-by")),
        escape(post.author_name.as_deref().unwrap_or(&post.author_jid)),
        format_time(post.published, i18n),
        escape(&community),
        escape(&i18n.t("host-community-label")),
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
        i18n,
    )
}

/// Space page.
#[must_use]
pub fn render_space(page: &SpacePage, config: &HostConfig, i18n: &Translator) -> String {
    let canonical = config.absolute(&format!(
        "/space/{}/{}",
        segment(&page.service),
        segment(&page.node)
    ));
    let title = page
        .title
        .clone()
        .unwrap_or_else(|| format!("{} / {}", page.service, page.node));
    let description = page.description.clone().unwrap_or_else(|| {
        let mut args = jabu_i18n::Args::new();
        args.set("title", title.clone());
        i18n.t_args("host-space-description", &args)
    });
    let mut rooms = String::new();
    for room in &page.rooms {
        let voice = if room.voice {
            format!(
                "<span class=\"tag\">{}</span>",
                escape(&i18n.t("host-voice"))
            )
        } else {
            String::new()
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
        &crate::moderation::space_target(&page.service, &page.node),
        i18n,
    );
    let body = format!(
        "<section><h1>{}</h1><p class=\"meta\">{} / {}</p>{share}{}</section>\
<section><h2>{}</h2>{}</section>",
        escape(&title),
        escape(&page.service),
        escape(&page.node),
        page.description
            .as_deref()
            .map_or_else(String::new, paragraphs),
        escape(&i18n.t("host-space-rooms")),
        if page.rooms.is_empty() {
            format!("<p>{}</p>", escape(&i18n.t("host-space-empty")))
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
        i18n,
    )
}

/// One public Story.
#[must_use]
pub fn render_story(story: &Story, config: &HostConfig, i18n: &Translator) -> String {
    let canonical = config.absolute(&format!(
        "/story/{}/{}",
        segment(&story.author_jid),
        segment(&story.id)
    ));
    let caption = if story.caption.trim().is_empty() {
        i18n.t("host-story")
    } else {
        story.caption.clone()
    };
    let story_fallback = i18n.t("host-story");
    let title: String = caption
        .lines()
        .next()
        .unwrap_or(&story_fallback)
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
            let alt = i18n.t("host-story");
            if media
                .mime
                .as_deref()
                .is_some_and(|mime| mime.starts_with("video/"))
            {
                format!("<video controls preload=\"metadata\" src=\"{url}\"></video>")
            } else {
                format!(
                    "<img src=\"{url}\" alt=\"{}\" loading=\"lazy\">",
                    escape(media.name.as_deref().unwrap_or(&alt))
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
        &crate::moderation::story_target(&story.author_jid, &story.id),
        i18n,
    );
    let body = format!(
        "<article><h1>{}</h1><p class=\"meta\">{} {} · {}</p>\
<div class=\"media\">{}</div>{}<p class=\"meta\">{}</p>{share}</article>",
        escape(&title),
        escape(&i18n.t("host-by")),
        escape(&story.author_jid),
        format_time(story.published, i18n),
        media,
        paragraphs(&story.caption),
        escape(&i18n.t("host-story-hint")),
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
        i18n,
    )
}

fn report_status_label(i18n: &Translator, slug: &str) -> String {
    match slug {
        "open" => i18n.t("host-admin-status-open"),
        "hidden" => i18n.t("host-admin-status-hidden"),
        "dismissed" => i18n.t("host-admin-status-dismissed"),
        other => other.to_string(),
    }
}

fn admin_action_label(i18n: &Translator, action: &str) -> String {
    match action {
        "hide" => i18n.t("host-admin-action-hide"),
        "unhide" => i18n.t("host-admin-action-unhide"),
        "dismiss" => i18n.t("host-admin-action-dismiss"),
        "reopen" => i18n.t("host-admin-action-reopen"),
        other => other.to_string(),
    }
}

fn role_label(i18n: &Translator, role: Role) -> String {
    match role {
        Role::Admin => i18n.t("host-admin-role-admin"),
        Role::Moderator => i18n.t("host-admin-role-moderator"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn en() -> Translator {
        Translator::for_tag("en")
    }

    fn fr() -> Translator {
        Translator::for_tag("fr")
    }

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
            author_name: None,
            reposted_from: None,
            via: None,
            in_reply_to: None,
            title: Some("Hello".to_string()),
            body: "<script>alert(1)</script>".to_string(),
            html: None,
            published: Utc::now(),
            tags: vec!["rust".to_string()],
            media: Vec::new(),
        }
    }

    #[test]
    fn escapes_user_text() {
        let page = render_post(&post(), &config(), &en());
        assert!(page.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!page.contains("<script>alert(1)</script>"));
    }

    #[test]
    fn renders_the_author_display_name() {
        let mut post = post();
        post.author_name = Some("Alice".to_string());
        let page = render_post(&post, &config(), &en());
        assert!(page.contains(">Alice</a>"));
        assert!(!page.contains(">alice@example.org</a>"));
    }

    #[test]
    fn renders_repost_and_reply_attribution() {
        let mut post = post();
        post.reposted_from = Some("Alice".to_string());
        post.via =
            Some("xmpp:alice@example.org?;node=urn%3Axmpp%3Amicroblog%3A0;item=p1".to_string());
        post.in_reply_to =
            Some("xmpp:bob@example.org?;node=urn%3Axmpp%3Amicroblog%3A0;item=p2".to_string());
        let page = render_post(&post, &config(), &en());
        assert!(page.contains("↻ Alice"));
        assert!(page.contains("↳ In reply to a post"));
        assert!(page.contains("item=p1"));
        assert!(page.contains("item=p2"));
    }

    #[test]
    fn renders_a_sanitized_rich_body() {
        let mut post = post();
        post.body = "Hello world".to_string();
        post.html = Some("<p>Hello <strong>world</strong></p>".to_string());
        let page = render_post(&post, &config(), &en());
        assert!(page.contains("<div class=\"rich-text\"><p>Hello <strong>world</strong></p></div>"));
        assert!(!page.contains("<p>Hello world</p>"));
    }

    #[test]
    fn sanitizes_an_untrusted_rich_body() {
        let mut post = post();
        post.html = Some(
            "<p>Hello<script>alert(1)</script><a href=\"javascript:bad()\">link</a></p>"
                .to_string(),
        );
        let page = render_post(&post, &config(), &en());
        assert!(page.contains("Hello"));
        assert!(page.contains("link"));
        assert!(!page.contains("<script>"));
        assert!(!page.contains("javascript:"));
    }

    #[test]
    fn renders_public_pages_in_the_requested_language() {
        let page = render_index(&config(), &fr());
        assert!(page.contains("lang=\"fr\""));
        assert!(page.contains("Ouvrez un blog public"));
        assert!(page.contains("À propos"));

        let page = render_about(&config(), &fr());
        assert!(page.contains("Cette instance ne diffuse que du contenu XMPP public"));
        assert!(!page.contains("This instance serves public XMPP content only"));

        let page = render_index(&config(), &Translator::for_tag("zh-Hans"));
        assert!(page.contains("lang=\"zh-Hans\""));
        assert!(page.contains("打开公开博客"));
    }

    #[test]
    fn renders_admin_pages_in_the_requested_language() {
        let page = render_admin_login(&config(), &fr(), Some("Jeton invalide"));
        assert!(page.contains("Connexion administrateur"));
        assert!(page.contains("Jeton d&#39;administration"));
        assert!(!page.contains("Admin sign in"));

        let tokens = render_admin_tokens(&[], &config(), &fr(), None, None);
        assert!(tokens.contains("Aucun jeton"));
        assert!(tokens.contains("Créer un jeton"));
    }

    #[test]
    fn adds_open_graph_and_canonical_tags() {
        let page = render_post(&post(), &config(), &en());
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
        let page = render_post(&post, &config(), &en());
        assert!(!page.contains("javascript:"));
    }
}
