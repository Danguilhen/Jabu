//! HTTP routes for the public Host.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use jid::Jid;
use sha1::{Digest, Sha1};

use crate::config::HostConfig;
use crate::content::{ContentError, ContentSource};
use crate::html;

#[derive(Clone)]
struct AppState {
    content: Arc<dyn ContentSource>,
    config: HostConfig,
}

/// Build the public router. The content source is injectable so tests do not
/// need an XMPP connection.
pub fn router(content: Arc<dyn ContentSource>, config: HostConfig) -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/", get(index))
        .route("/about", get(about))
        .route("/blog/{jid}", get(blog))
        .route("/post/{jid}/{item}", get(post))
        .fallback(not_found)
        .with_state(Arc::new(AppState { content, config }))
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

async fn post(
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
    let mut response = Html(body.to_string()).into_response();
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

    use crate::content::{BlogProfile, Post};

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
    }

    fn app() -> Router {
        router(
            Arc::new(Fake {
                calls: AtomicUsize::new(0),
            }),
            HostConfig::default(),
        )
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
}
