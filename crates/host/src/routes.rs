//! HTTP routes for the public Host.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{ConnectInfo, Form, Path, Query, Request, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use jid::Jid;
use serde::Deserialize;
use sha1::{Digest, Sha1};

use crate::config::HostConfig;
use crate::content::{ContentError, ContentSource};
use crate::feed::{self, FeedEntry};
use crate::html;
use crate::moderation::ModerationStore;
use crate::ratelimit::RateLimiter;

#[derive(Clone)]
struct AppState {
    content: Arc<dyn ContentSource>,
    config: HostConfig,
    limiter: Arc<RateLimiter>,
    moderation: Arc<ModerationStore>,
}

/// Build the public router. The content source is injectable so tests do not
/// need an XMPP connection.
pub fn router(
    content: Arc<dyn ContentSource>,
    config: HostConfig,
    moderation: Arc<ModerationStore>,
) -> Router {
    let state = Arc::new(AppState {
        content,
        limiter: Arc::new(RateLimiter::new(
            config.rate_limit_per_minute,
            Duration::from_secs(60),
        )),
        moderation,
        config,
    });
    Router::new()
        .route("/healthz", get(health))
        .route("/", get(index))
        .route("/about", get(about))
        .route("/robots.txt", get(robots))
        .route("/sitemap.xml", get(sitemap))
        .route("/blog/{jid}", get(blog))
        .route("/blog/{jid}/atom.xml", get(blog_atom))
        .route("/blog/{jid}/rss.xml", get(blog_rss))
        .route("/post/{jid}/{item}", get(post_page))
        .route("/contact/{jid}", get(contact))
        .route("/community/{service}/{node}", get(community))
        .route("/community/{service}/{node}/{item}", get(community_post))
        .route("/community/{service}/{node}/atom.xml", get(community_atom))
        .route("/community/{service}/{node}/rss.xml", get(community_rss))
        .route("/space/{service}/{node}", get(space))
        .route("/story/{jid}/{item}", get(story))
        .route("/report", get(report_form).post(report_submit))
        .route("/report/{id}", get(report_status))
        .route("/admin/login", get(admin_login_page).post(admin_login))
        .route("/admin/logout", get(admin_logout))
        .route("/admin", get(admin_page))
        .route("/admin/reports/{id}/action", post(admin_action))
        .route("/admin/api/reports", get(api_reports))
        .route("/admin/api/audit", get(api_audit))
        .fallback(not_found)
        .layer(middleware::from_fn_with_state(state.clone(), rate_limit))
        .with_state(state)
}

async fn rate_limit(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    if state.config.rate_limit_per_minute > 0 {
        // Health probes must not consume a client's budget.
        if request.uri().path() == "/healthz" {
            return next.run(request).await;
        }
        let key = client_key(&request, state.config.trust_proxy);
        if !state.limiter.allow(&key) {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                [(header::RETRY_AFTER, "60")],
                "Too many requests",
            )
                .into_response();
        }
    }
    next.run(request).await
}

fn client_key(request: &Request, trust_proxy: bool) -> String {
    if trust_proxy {
        if let Some(forwarded) = request
            .headers()
            .get("x-forwarded-for")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(',').next())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return forwarded.to_string();
        }
    }
    request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map_or_else(|| "-".to_string(), |info| info.0.ip().to_string())
}

async fn health() -> &'static str {
    "ok"
}

async fn index(State(state): State<Arc<AppState>>) -> Response {
    page(&html::render_index(&state.config), &HeaderMap::new())
}

async fn about(State(state): State<Arc<AppState>>) -> Response {
    page(&html::render_about(&state.config), &HeaderMap::new())
}

async fn blog(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(jid): Path<String>,
) -> Response {
    let Ok(jid) = jid.parse::<Jid>() else {
        return not_found_page(&state);
    };
    match state.content.blog(&jid).await {
        Ok(profile) => page(&html::render_blog(&profile, &state.config), &headers),
        Err(error) => content_error(error, &state),
    }
}

async fn post_page(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((jid, item)): Path<(String, String)>,
) -> Response {
    let (Ok(jid), false) = (jid.parse::<Jid>(), item.is_empty()) else {
        return not_found_page(&state);
    };
    match state.content.post(&jid, &item).await {
        Ok(post) => page(&html::render_post(&post, &state.config), &headers),
        Err(error) => content_error(error, &state),
    }
}

async fn contact(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(jid): Path<String>,
) -> Response {
    let Ok(jid) = jid.parse::<Jid>() else {
        return not_found_page(&state);
    };
    match state.content.contact(&jid).await {
        Ok(profile) => page(&html::render_contact(&profile, &state.config), &headers),
        Err(error) => content_error(error, &state),
    }
}

async fn community(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((service, node)): Path<(String, String)>,
) -> Response {
    let Ok(service) = service.parse::<Jid>() else {
        return not_found_page(&state);
    };
    if node.is_empty() {
        return not_found_page(&state);
    }
    match state.content.community(&service, &node).await {
        Ok(community_page) => page(
            &html::render_community(&community_page, &state.config),
            &headers,
        ),
        Err(error) => content_error(error, &state),
    }
}

async fn community_post(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((service, node, item)): Path<(String, String, String)>,
) -> Response {
    let Ok(service) = service.parse::<Jid>() else {
        return not_found_page(&state);
    };
    if node.is_empty() || item.is_empty() {
        return not_found_page(&state);
    }
    match state.content.community_post(&service, &node, &item).await {
        Ok(post) => page(
            &html::render_community_post(&post, &service.to_string(), &node, &state.config),
            &headers,
        ),
        Err(error) => content_error(error, &state),
    }
}

async fn space(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((service, node)): Path<(String, String)>,
) -> Response {
    let Ok(service) = service.parse::<Jid>() else {
        return not_found_page(&state);
    };
    if node.is_empty() {
        return not_found_page(&state);
    }
    match state.content.space(&service, &node).await {
        Ok(space_page) => page(&html::render_space(&space_page, &state.config), &headers),
        Err(error) => content_error(error, &state),
    }
}

async fn story(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((jid, item)): Path<(String, String)>,
) -> Response {
    let Ok(jid) = jid.parse::<Jid>() else {
        return not_found_page(&state);
    };
    if item.is_empty() {
        return not_found_page(&state);
    }
    match state.content.story(&jid, &item).await {
        Ok(story) => page(&html::render_story(&story, &state.config), &headers),
        Err(error) => content_error(error, &state),
    }
}

async fn blog_atom(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(jid): Path<String>,
) -> Response {
    blog_feed(&state, &headers, &jid, FeedFormat::Atom).await
}

async fn blog_rss(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(jid): Path<String>,
) -> Response {
    blog_feed(&state, &headers, &jid, FeedFormat::Rss).await
}

enum FeedFormat {
    Atom,
    Rss,
}

async fn blog_feed(
    state: &AppState,
    headers: &HeaderMap,
    jid: &str,
    format: FeedFormat,
) -> Response {
    let Ok(jid) = jid.parse::<Jid>() else {
        return not_found_page(state);
    };
    let profile = match state.content.blog(&jid).await {
        Ok(profile) => profile,
        Err(error) => return content_error(error, state),
    };
    let feed_url = state.config.absolute(&format!(
        "{}/{}",
        blog_path(&jid.to_string()),
        match format {
            FeedFormat::Atom => "atom.xml",
            FeedFormat::Rss => "rss.xml",
        }
    ));
    let site_url = state.config.absolute(&blog_path(&jid.to_string()));
    let entries: Vec<FeedEntry<'_>> = profile
        .posts
        .iter()
        .map(|post| FeedEntry {
            post,
            url: post_url(state, &post.author_jid, &post.id),
        })
        .collect();
    let title = profile.name.clone().unwrap_or_else(|| profile.jid.clone());
    let body = match format {
        FeedFormat::Atom => feed::atom(&title, &feed_url, &feed_url, &site_url, &entries),
        FeedFormat::Rss => feed::rss(
            &title,
            &feed_url,
            &site_url,
            &profile.description.clone().unwrap_or_default(),
            &entries,
        ),
    };
    let content_type = match format {
        FeedFormat::Atom => "application/atom+xml; charset=utf-8",
        FeedFormat::Rss => "application/rss+xml; charset=utf-8",
    };
    text_page(&body, content_type, headers)
}

async fn community_atom(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((service, node)): Path<(String, String)>,
) -> Response {
    community_feed(&state, &headers, &service, &node, FeedFormat::Atom).await
}

async fn community_rss(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((service, node)): Path<(String, String)>,
) -> Response {
    community_feed(&state, &headers, &service, &node, FeedFormat::Rss).await
}

async fn community_feed(
    state: &AppState,
    headers: &HeaderMap,
    service: &str,
    node: &str,
    format: FeedFormat,
) -> Response {
    let Ok(service_jid) = service.parse::<Jid>() else {
        return not_found_page(state);
    };
    let page = match state.content.community(&service_jid, node).await {
        Ok(page) => page,
        Err(error) => return content_error(error, state),
    };
    let feed_url = state.config.absolute(&format!(
        "{}/{}",
        community_path(service, node),
        match format {
            FeedFormat::Atom => "atom.xml",
            FeedFormat::Rss => "rss.xml",
        }
    ));
    let site_url = state.config.absolute(&community_path(service, node));
    let entries: Vec<FeedEntry<'_>> = page
        .posts
        .iter()
        .map(|post| FeedEntry {
            post,
            url: community_post_url(state, service, node, &post.id),
        })
        .collect();
    let title = page
        .title
        .clone()
        .unwrap_or_else(|| format!("{service} / {node}"));
    let body = match format {
        FeedFormat::Atom => feed::atom(&title, &feed_url, &feed_url, &site_url, &entries),
        FeedFormat::Rss => feed::rss(
            &title,
            &feed_url,
            &site_url,
            &page.description.clone().unwrap_or_default(),
            &entries,
        ),
    };
    let content_type = match format {
        FeedFormat::Atom => "application/atom+xml; charset=utf-8",
        FeedFormat::Rss => "application/rss+xml; charset=utf-8",
    };
    text_page(&body, content_type, headers)
}

async fn sitemap(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    use std::fmt::Write as _;
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">",
    );
    for path in ["/", "/about"] {
        let _ = write!(
            xml,
            "<url><loc>{}</loc></url>",
            html::escape(&state.config.absolute(path))
        );
    }
    for jid in &state.config.featured_blogs {
        let Ok(jid) = jid.parse::<Jid>() else {
            continue;
        };
        let _ = write!(
            xml,
            "<url><loc>{}</loc></url>",
            html::escape(&state.config.absolute(&blog_path(&jid.to_string())))
        );
    }
    xml.push_str("</urlset>");
    text_page(&xml, "application/xml; charset=utf-8", &headers)
}

async fn robots(State(state): State<Arc<AppState>>) -> Response {
    let sitemap_url = state.config.absolute("/sitemap.xml");
    let body = format!(
        "User-agent: *\nDisallow: /admin\nSitemap: {}\n",
        html::escape(&sitemap_url)
    );
    text_page(&body, "text/plain; charset=utf-8", &HeaderMap::new())
}

fn blog_path(jid: &str) -> String {
    format!("/blog/{}", html::segment(jid))
}

fn post_url(state: &AppState, jid: &str, item_id: &str) -> String {
    state.config.absolute(&format!(
        "/post/{}/{}",
        html::segment(jid),
        html::segment(item_id)
    ))
}

fn community_path(service: &str, node: &str) -> String {
    format!(
        "/community/{}/{}",
        html::segment(service),
        html::segment(node)
    )
}

fn community_post_url(state: &AppState, service: &str, node: &str, item_id: &str) -> String {
    state.config.absolute(&format!(
        "{}/{}",
        community_path(service, node),
        html::segment(item_id)
    ))
}

#[derive(Deserialize)]
struct ReportQuery {
    target: Option<String>,
}

#[derive(Deserialize)]
struct ReportForm {
    target: String,
    reason: String,
    details: Option<String>,
    reporter: Option<String>,
}

#[derive(Deserialize)]
struct LoginForm {
    token: String,
}

#[derive(Deserialize)]
struct ActionForm {
    action: String,
    note: Option<String>,
}

async fn report_form(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ReportQuery>,
) -> Response {
    let target = query.target.unwrap_or_default();
    page(
        &html::render_report_form(&target, &state.config),
        &HeaderMap::new(),
    )
}

async fn report_submit(
    State(state): State<Arc<AppState>>,
    Form(form): Form<ReportForm>,
) -> Response {
    let target = form.target.trim();
    if target.is_empty() || target.len() > 512 {
        return (StatusCode::BAD_REQUEST, "invalid report target").into_response();
    }
    let reason = match form.reason.as_str() {
        reason @ ("spam" | "abuse" | "illegal" | "other") => reason,
        _ => "other",
    };
    let details = form
        .details
        .as_deref()
        .map(str::trim)
        .filter(|details| !details.is_empty());
    if details.is_some_and(|details| details.len() > 2000) {
        return (StatusCode::BAD_REQUEST, "details are too long").into_response();
    }
    let reporter = form
        .reporter
        .as_deref()
        .map(str::trim)
        .filter(|reporter| !reporter.is_empty());
    if reporter.is_some_and(|reporter| reporter.len() > 200) {
        return (StatusCode::BAD_REQUEST, "reporter is too long").into_response();
    }
    match state
        .moderation
        .create_report(target, reason, details, reporter)
    {
        Ok(report) => page(
            &html::render_report_receipt(&report, &state.config),
            &HeaderMap::new(),
        ),
        Err(error) => {
            tracing::error!("could not store report: {error}");
            (StatusCode::INTERNAL_SERVER_ERROR, "could not store report").into_response()
        }
    }
}

async fn report_status(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    state.moderation.report(&id).map_or_else(
        || not_found_page(&state),
        |report| {
            page(
                &html::render_report_status(&report, &state.config),
                &HeaderMap::new(),
            )
        },
    )
}

async fn admin_login_page(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if !admin_enabled(&state) {
        return not_found_page(&state);
    }
    if admin_authorized(&state, &headers) {
        return redirect_response("/admin");
    }
    page(
        &html::render_admin_login(&state.config, None),
        &HeaderMap::new(),
    )
}

async fn admin_login(State(state): State<Arc<AppState>>, Form(form): Form<LoginForm>) -> Response {
    if !admin_enabled(&state) {
        return not_found_page(&state);
    }
    let Some(expected) = state.config.admin_token.as_deref() else {
        return not_found_page(&state);
    };
    if !constant_time_eq(form.token.as_bytes(), expected.as_bytes()) {
        return (
            StatusCode::UNAUTHORIZED,
            page(
                &html::render_admin_login(&state.config, Some("Invalid admin token")),
                &HeaderMap::new(),
            ),
        )
            .into_response();
    }
    let cookie = admin_cookie(&state, &form.token);
    let mut response = redirect_response("/admin");
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().insert(header::SET_COOKIE, value);
    }
    response
}

async fn admin_logout() -> Response {
    let mut response = redirect_response("/admin/login");
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static("jabu_admin=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0"),
    );
    response
}

async fn admin_page(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if !admin_enabled(&state) {
        return not_found_page(&state);
    }
    if !admin_authorized(&state, &headers) {
        return redirect_response("/admin/login");
    }
    page(
        &html::render_admin(
            &state.moderation.reports(),
            &state.moderation.audit(),
            &state.config,
        ),
        &headers,
    )
}

async fn admin_action(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<ActionForm>,
) -> Response {
    if !admin_authorized(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    let note = form
        .note
        .as_deref()
        .map(str::trim)
        .filter(|note| !note.is_empty());
    match state.moderation.apply("admin", &id, &form.action, note) {
        Ok(report) => {
            if bearer_token(&headers).is_some() {
                Json(report).into_response()
            } else {
                redirect_response("/admin")
            }
        }
        Err(error) => {
            tracing::warn!("moderation action failed: {error}");
            (StatusCode::BAD_REQUEST, "could not apply the action").into_response()
        }
    }
}

async fn api_reports(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if !admin_authorized(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    Json(state.moderation.reports()).into_response()
}

async fn api_audit(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if !admin_authorized(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    Json(state.moderation.audit()).into_response()
}

fn admin_enabled(state: &AppState) -> bool {
    state
        .config
        .admin_token
        .as_deref()
        .is_some_and(|token| !token.is_empty())
}

fn admin_authorized(state: &AppState, headers: &HeaderMap) -> bool {
    let Some(expected) = state.config.admin_token.as_deref() else {
        return false;
    };
    if let Some(token) = bearer_token(headers) {
        return constant_time_eq(token.as_bytes(), expected.as_bytes());
    }
    admin_cookie_token(headers)
        .is_some_and(|token| constant_time_eq(token.as_bytes(), expected.as_bytes()))
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|token| !token.is_empty())
}

fn admin_cookie_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .split(';')
                .find_map(|part| part.trim().strip_prefix("jabu_admin=").map(str::to_string))
        })
        .filter(|token| !token.is_empty())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0_u8;
    for (left, right) in left.iter().zip(right) {
        difference |= left ^ right;
    }
    difference == 0
}

fn admin_cookie(state: &AppState, token: &str) -> String {
    let secure = state
        .config
        .public_url
        .as_deref()
        .is_some_and(|url| url.starts_with("https://"));
    format!(
        "jabu_admin={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=86400{}",
        if secure { "; Secure" } else { "" }
    )
}

fn redirect_response(location: &'static str) -> Response {
    let mut response = StatusCode::SEE_OTHER.into_response();
    response
        .headers_mut()
        .insert(header::LOCATION, HeaderValue::from_static(location));
    response
}

async fn not_found(State(state): State<Arc<AppState>>) -> Response {
    not_found_page(&state)
}

fn not_found_page(state: &AppState) -> Response {
    let mut response = page(&html::render_not_found(&state.config), &HeaderMap::new());
    *response.status_mut() = StatusCode::NOT_FOUND;
    response
}

fn content_error(error: ContentError, state: &AppState) -> Response {
    match error {
        ContentError::NotFound | ContentError::NotPublic => not_found_page(state),
        ContentError::Unavailable(message) => {
            tracing::warn!("upstream public content unavailable: {message}");
            (
                StatusCode::BAD_GATEWAY,
                [("content-type", "text/plain; charset=utf-8")],
                "Public content is temporarily unavailable.",
            )
                .into_response()
        }
    }
}

fn page(body: &str, headers: &HeaderMap) -> Response {
    text_page(body, "text/html; charset=utf-8", headers)
}

fn text_page(body: &str, content_type: &'static str, headers: &HeaderMap) -> Response {
    let tag = format!("\"{:x}\"", Sha1::digest(body.as_bytes()));
    let not_modified = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .map(str::trim)
                .any(|candidate| candidate.trim_start_matches("W/") == tag)
        });
    if not_modified {
        return StatusCode::NOT_MODIFIED.into_response();
    }
    let mut response = body.to_string().into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static(content_type),
    );
    if let Ok(value) = tag.parse() {
        response.headers_mut().insert(header::ETAG, value);
    }
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("public, max-age=60"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::Utc;
    use http_body_util::BodyExt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tower::ServiceExt;

    use crate::content::{
        BlogProfile, CommunityPage, ContactProfile, MediaLink, Post, SpacePage, SpaceRoomLink,
        Story, SubscriptionLink,
    };
    use crate::moderation::{ModeratedContent, ModerationStore};

    struct Fake {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl ContentSource for Fake {
        async fn blog(&self, jid: &Jid) -> Result<BlogProfile, ContentError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if jid.to_string() == "private@example.org" {
                return Err(ContentError::NotPublic);
            }
            Ok(BlogProfile {
                jid: jid.to_string(),
                name: Some("Alice".to_string()),
                description: None,
                posts: vec![Post {
                    id: "p1".to_string(),
                    author_jid: jid.to_string(),
                    title: Some("Hello".to_string()),
                    body: "<b>not html</b>".to_string(),
                    published: Utc::now(),
                    tags: vec!["rust".to_string()],
                    media: Vec::new(),
                }],
            })
        }

        async fn post(&self, jid: &Jid, item_id: &str) -> Result<Post, ContentError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if item_id != "p1" {
                return Err(ContentError::NotFound);
            }
            Ok(Post {
                id: item_id.to_string(),
                author_jid: jid.to_string(),
                title: Some("Hello".to_string()),
                body: "Body".to_string(),
                published: Utc::now(),
                tags: Vec::new(),
                media: Vec::new(),
            })
        }

        async fn community(
            &self,
            service: &Jid,
            node: &str,
        ) -> Result<CommunityPage, ContentError> {
            if node != "news" {
                return Err(ContentError::NotFound);
            }
            Ok(CommunityPage {
                service: service.to_string(),
                node: node.to_string(),
                title: Some("News".to_string()),
                description: Some("Public news".to_string()),
                posts: vec![Post {
                    id: "a1".to_string(),
                    author_jid: "bob@example.org".to_string(),
                    title: Some("Article".to_string()),
                    body: "Article body".to_string(),
                    published: Utc::now(),
                    tags: Vec::new(),
                    media: Vec::new(),
                }],
            })
        }

        async fn community_post(
            &self,
            service: &Jid,
            node: &str,
            item_id: &str,
        ) -> Result<Post, ContentError> {
            if node != "news" || item_id != "a1" {
                return Err(ContentError::NotFound);
            }
            Ok(Post {
                id: item_id.to_string(),
                author_jid: service.to_string(),
                title: Some("Article".to_string()),
                body: "Article body".to_string(),
                published: Utc::now(),
                tags: Vec::new(),
                media: Vec::new(),
            })
        }

        async fn space(&self, service: &Jid, node: &str) -> Result<SpacePage, ContentError> {
            if node != "dev" {
                return Err(ContentError::NotFound);
            }
            Ok(SpacePage {
                service: service.to_string(),
                node: node.to_string(),
                title: Some("Dev".to_string()),
                description: None,
                rooms: vec![SpaceRoomLink {
                    jid: "dev@conference.example.org".to_string(),
                    name: "Development".to_string(),
                    directory: Some("Engineering".to_string()),
                    voice: true,
                }],
            })
        }

        async fn story(&self, jid: &Jid, item_id: &str) -> Result<Story, ContentError> {
            if item_id != "s1" {
                return Err(ContentError::NotFound);
            }
            Ok(Story {
                id: item_id.to_string(),
                author_jid: jid.to_string(),
                caption: "Sunset".to_string(),
                media: Some(MediaLink {
                    url: "https://files.example/sunset.jpg".to_string(),
                    mime: Some("image/jpeg".to_string()),
                    name: None,
                }),
                published: Utc::now(),
                expires: None,
            })
        }

        async fn contact(&self, jid: &Jid) -> Result<ContactProfile, ContentError> {
            Ok(ContactProfile {
                jid: jid.to_string(),
                name: Some("Alice".to_string()),
                description: Some("Public profile".to_string()),
                subscriptions: vec![SubscriptionLink {
                    server: "spaces.example.org".to_string(),
                    node: "dev".to_string(),
                    title: Some("Dev".to_string()),
                }],
            })
        }
    }

    fn app() -> Router {
        app_with(HostConfig::default())
    }

    fn app_with(config: HostConfig) -> Router {
        let store = Arc::new(ModerationStore::in_memory().expect("moderation store"));
        app_with_store(config, store)
    }

    fn app_with_store(config: HostConfig, store: Arc<ModerationStore>) -> Router {
        let content = Arc::new(ModeratedContent::new(
            Arc::new(Fake {
                calls: AtomicUsize::new(0),
            }),
            store.clone(),
        ));
        router(content, config, store)
    }

    async fn body_text(response: Response) -> String {
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn serves_a_blog_page_with_og_tags() {
        let response = app()
            .oneshot(
                axum::http::Request::get("/blog/alice@example.org")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_text(response).await;
        assert!(body.contains("og:title"));
        assert!(body.contains("&lt;b&gt;not html&lt;/b&gt;"));
        assert!(!body.contains("<b>not html</b>"));
    }

    #[tokio::test]
    async fn unknown_content_is_a_404() {
        for path in [
            "/blog/private@example.org",
            "/post/alice@example.org/missing",
            "/nope",
        ] {
            let response = app()
                .oneshot(
                    axum::http::Request::get(path)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        }
    }

    #[tokio::test]
    async fn serves_an_etag_and_handles_if_none_match() {
        let app = app();
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::get("/post/alice@example.org/p1")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let etag = response
            .headers()
            .get(header::ETAG)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let response = app
            .oneshot(
                axum::http::Request::get("/post/alice@example.org/p1")
                    .header(header::IF_NONE_MATCH, etag)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
    }

    #[tokio::test]
    async fn health_endpoint_answers_ok() {
        let response = app()
            .oneshot(
                axum::http::Request::get("/healthz")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_text(response).await, "ok");
    }

    async fn get(app: Router, path: &str) -> Response {
        app.oneshot(
            axum::http::Request::get(path)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn serves_community_space_and_story_pages() {
        let response = get(app(), "/community/pubsub.example.org/news").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_text(response).await;
        assert!(body.contains("News"));
        assert!(body.contains("Article"));
        assert!(body.contains("atom.xml"));

        let response = get(app(), "/community/pubsub.example.org/news/a1").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(body_text(response).await.contains("Article body"));

        let response = get(app(), "/space/spaces.example.org/dev").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_text(response).await;
        assert!(body.contains("Development"));
        assert!(body.contains("voice"));

        let response = get(app(), "/story/alice@example.org/s1").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_text(response).await;
        assert!(body.contains("https://files.example/sunset.jpg"));
        assert!(body.contains("24 hours"));

        let response = get(app(), "/community/pubsub.example.org/missing").await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn serves_atom_and_rss_feeds() {
        let response = get(app(), "/blog/alice@example.org/atom.xml").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap(),
            "application/atom+xml; charset=utf-8"
        );
        let body = body_text(response).await;
        assert!(body.contains("<feed xmlns=\"http://www.w3.org/2005/Atom\">"));
        assert!(body.contains("<entry>"));

        let response = get(app(), "/blog/alice@example.org/rss.xml").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(body_text(response).await.contains("<rss version=\"2.0\">"));

        // The static feed path wins over the community item route.
        let response = get(app(), "/community/pubsub.example.org/news/atom.xml").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_text(response).await;
        assert!(body.contains("<feed"));
        assert!(body.contains("Article"));

        let response = get(app(), "/community/pubsub.example.org/news/rss.xml").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(body_text(response).await.contains("<rss version=\"2.0\">"));
    }

    #[tokio::test]
    async fn serves_sitemap_and_robots() {
        let app = app_with(HostConfig {
            public_url: Some("https://jabu.example".to_string()),
            featured_blogs: vec!["alice@example.org".to_string()],
            ..HostConfig::default()
        });
        let response = get(app.clone(), "/sitemap.xml").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_text(response).await;
        assert!(body.contains("<loc>https://jabu.example/</loc>"));
        assert!(body.contains("https://jabu.example/blog/alice%40example%2Eorg"));

        let response = get(app, "/robots.txt").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(body_text(response)
            .await
            .contains("Sitemap: https://jabu.example/sitemap.xml"));
    }

    #[tokio::test]
    async fn serves_contact_pages_and_deep_links() {
        let response = get(app(), "/contact/alice@example.org").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_text(response).await;
        assert!(body.contains("Public subscriptions"));
        assert!(body.contains("Dev"));
        assert!(body.contains("jabu://contact/alice%40example%2Eorg"));

        let response = get(app(), "/blog/alice@example.org").await;
        assert!(body_text(response)
            .await
            .contains("jabu://blog/alice%40example%2Eorg"));

        let response = get(app(), "/post/alice@example.org/p1").await;
        assert!(body_text(response)
            .await
            .contains("jabu://post/alice%40example%2Eorg/p1"));
    }

    #[tokio::test]
    async fn rate_limits_public_requests() {
        let app = app_with(HostConfig {
            rate_limit_per_minute: 2,
            ..HostConfig::default()
        });
        assert_eq!(get(app.clone(), "/about").await.status(), StatusCode::OK);
        assert_eq!(get(app.clone(), "/about").await.status(), StatusCode::OK);
        let response = get(app, "/about").await;
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            response
                .headers()
                .get(header::RETRY_AFTER)
                .unwrap()
                .to_str()
                .unwrap(),
            "60"
        );
    }

    fn form_request(path: &str, body: &str) -> axum::http::Request<axum::body::Body> {
        axum::http::Request::post(path)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(axum::body::Body::from(body.to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn report_form_stores_a_report_and_shows_its_status() {
        let store = Arc::new(ModerationStore::in_memory().unwrap());
        let app = app_with_store(HostConfig::default(), store.clone());

        let response = get(app.clone(), "/report?target=post/alice%40example.org/p1").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(body_text(response)
            .await
            .contains("post/alice@example.org/p1"));

        let response = app
            .clone()
            .oneshot(form_request(
                "/report",
                "target=post%2Falice%40example.org%2Fp1&reason=spam&details=bad&reporter=bob",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(body_text(response).await.contains("Report received"));
        let report = &store.reports()[0];
        assert_eq!(report.target, "post/alice@example.org/p1");
        assert_eq!(report.reason, "spam");

        let response = get(app, &format!("/report/{}", report.id)).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(body_text(response).await.contains("Awaiting review"));
    }

    #[tokio::test]
    async fn reports_reject_an_empty_target() {
        let app = app();
        let response = app
            .oneshot(form_request("/report", "target=&reason=spam"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn admin_login_and_a_hide_action_take_effect() {
        let store = Arc::new(ModerationStore::in_memory().unwrap());
        let config = HostConfig {
            admin_token: Some("secret".to_string()),
            ..HostConfig::default()
        };
        let app = app_with_store(config, store.clone());

        let response = get(app.clone(), "/admin").await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            response
                .headers()
                .get(header::LOCATION)
                .unwrap()
                .to_str()
                .unwrap(),
            "/admin/login"
        );

        let response = app
            .clone()
            .oneshot(form_request("/admin/login", "token=wrong"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = app
            .clone()
            .oneshot(form_request("/admin/login", "token=secret"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(cookie.starts_with("jabu_admin=secret"));

        let report = store
            .create_report("post/alice@example.org/p1", "spam", None, None)
            .unwrap();
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::get("/admin")
                    .header(header::COOKIE, &cookie)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_text(response).await;
        assert!(body.contains("post/alice@example.org/p1"));
        assert!(body.contains("Moderation queue"));

        let response = app
            .clone()
            .oneshot(
                axum::http::Request::post(format!("/admin/reports/{}/action", report.id))
                    .header(header::COOKIE, &cookie)
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(axum::body::Body::from("action=hide&note=confirmed"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);

        let response = get(app.clone(), "/post/alice@example.org/p1").await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = app
            .clone()
            .oneshot(
                axum::http::Request::get("/admin/api/reports")
                    .header(header::AUTHORIZATION, "Bearer secret")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_text(response).await;
        assert!(body.contains("\"status\":\"hidden\""));

        let response = get(app, "/admin/api/audit").await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
