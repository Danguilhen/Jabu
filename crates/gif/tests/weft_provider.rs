// SPDX-License-Identifier: MIT

use std::sync::Arc;

use jabu_gif::{GifError, GifProvider, GifQuery, WeftProvider};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use weft_client::Client;

const HASH: &str = "3a7bd3e2360a3a7bd3e2360a3a7bd3e2360a3a7bd3e2360a3a7bd3e2360a3a7b";

const SUMMARY: &str = r#"{
  "hash": "3a7bd3e2360a3a7bd3e2360a3a7bd3e2360a3a7bd3e2360a3a7bd3e2360a3a7b",
  "contentId": "urn:weft:sha256:3a7bd3e2360a3a7bd3e2360a3a7bd3e2360a3a7bd3e2360a3a7bd3e2360a3a7b",
  "mime": "image/gif",
  "width": 480,
  "height": 270,
  "frames": 42,
  "durationMs": 3200,
  "title": "Surprised Cat!",
  "tags": ["cat", "reaction"],
  "licence": { "spdx": "CC0-1.0", "status": "verified_evidence", "url": "https://example.org/licence" },
  "mediaUrl": "https://gifs.example/media/3a7b",
  "thumbnailUrl": "https://gifs.example/media/3a7b/thumbnail",
  "manifestUrl": "https://gifs.example/api/v1/media/3a7b",
  "source": "https://example.org/original.gif",
  "createdAt": "2026-11-14T00:00:00Z",
  "status": "active"
}"#;

async fn serve_once(
    status: &str,
    content_type: &str,
    extra_headers: &str,
    body: String,
) -> (String, oneshot::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = oneshot::channel();
    let status = status.to_string();
    let content_type = content_type.to_string();
    let extra_headers = extra_headers.to_string();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buffer = vec![0_u8; 8192];
        let read = socket.read(&mut buffer).await.unwrap();
        let request = String::from_utf8_lossy(&buffer[..read]).to_string();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(response.as_bytes()).await.unwrap();
        let _ = tx.send(request);
    });
    (format!("http://{address}"), rx)
}

fn provider(base: &str) -> WeftProvider {
    WeftProvider::from_client(
        Client::new(base).unwrap(),
        "https://gifs.example",
        "Weft test node",
    )
}

#[tokio::test]
async fn search_maps_weft_items_with_attribution() {
    let body = format!(r#"{{ "items": [{SUMMARY}], "nextCursor": "next" }}"#);
    let (base, request) = serve_once("200 OK", "application/json", "", body).await;
    let page = provider(&base)
        .search(GifQuery::new("cat").with_limit(8))
        .await
        .unwrap();

    let request = request.await.unwrap();
    assert!(request.starts_with("GET /api/v1/search?"), "{request}");
    assert!(request.contains("q=cat"));
    assert_eq!(page.items.len(), 1);
    let item = &page.items[0];
    assert_eq!(item.id, HASH);
    assert_eq!(item.title, "Surprised Cat!");
    assert_eq!(item.media_url, "https://gifs.example/media/3a7b");
    assert_eq!(
        item.preview_url,
        "https://gifs.example/media/3a7b/thumbnail"
    );
    assert_eq!(item.licence.status, "verified_evidence");
    assert_eq!(
        item.source.as_deref(),
        Some("https://example.org/original.gif")
    );
    assert_eq!(item.node, "https://gifs.example");
    assert_eq!(page.next_cursor.as_deref(), Some("next"));
}

#[tokio::test]
async fn fetch_downloads_the_original_and_builds_a_safe_name() {
    let (base, request) = serve_once("200 OK", "image/gif", "", "GIF89a-demo".to_string()).await;
    let provider = provider(&base);
    let body = format!(r#"{{ "items": [{SUMMARY}], "nextCursor": null }}"#);
    drop(body);

    let item = jabu_gif::GifItem {
        id: HASH.to_string(),
        title: "Surprised Cat!".to_string(),
        media_url: "https://gifs.example/media/3a7b".to_string(),
        preview_url: "https://gifs.example/media/3a7b".to_string(),
        mime: "image/gif".to_string(),
        width: Some(480),
        height: Some(270),
        duration_ms: Some(3200),
        tags: vec!["cat".to_string()],
        source: Some("https://example.org/original.gif".to_string()),
        licence: jabu_gif::GifLicence::default(),
        node: "https://gifs.example".to_string(),
    };
    let asset = provider.fetch(&item).await.unwrap();
    let request = request.await.unwrap();
    assert!(
        request.starts_with(&format!("GET /media/{HASH}")),
        "{request}"
    );
    assert_eq!(asset.bytes, b"GIF89a-demo");
    assert_eq!(asset.mime, "image/gif");
    assert_eq!(asset.filename, "surprised-cat.gif");
    assert_eq!(asset.width, Some(480));
}

#[tokio::test]
async fn rate_limits_are_preserved_for_the_ui() {
    let (base, _) = serve_once(
        "429 Too Many Requests",
        "application/json",
        "Retry-After: 3\r\n",
        r#"{"error":{"code":"rate_limited","message":"Too many requests"}}"#.to_string(),
    )
    .await;
    let error = provider(&base)
        .search(GifQuery::default())
        .await
        .unwrap_err();
    match error {
        GifError::RateLimited { retry_after } => assert_eq!(retry_after, Some(3)),
        other => panic!("unexpected error: {other}"),
    }
}

#[tokio::test]
async fn tags_are_mapped() {
    let (base, _) = serve_once(
        "200 OK",
        "application/json",
        "",
        r#"{"items":[{"tag":"cat","count":12}],"nextCursor":null}"#.to_string(),
    )
    .await;
    let tags = provider(&base).tags().await.unwrap();
    assert_eq!(tags[0].tag, "cat");
    assert_eq!(tags[0].count, 12);
}

#[tokio::test]
async fn the_trait_is_object_safe() {
    let providers: Vec<Arc<dyn GifProvider>> = Vec::new();
    assert!(providers.is_empty());
}
