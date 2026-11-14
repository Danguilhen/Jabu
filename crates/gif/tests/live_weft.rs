// SPDX-License-Identifier: MIT

//! Optional smoke test against a running node with at least one active GIF.
//!
//! ```text
//! JABU_WEFT_NODE=http://127.0.0.1:8096 cargo test -p jabu-gif --test live_weft -- --ignored
//! ```

use jabu_gif::{GifProvider, GifQuery, WeftProvider};

#[tokio::test]
#[ignore = "requires JABU_WEFT_NODE pointing at a node with active content"]
async fn search_and_fetch_from_a_live_node() {
    let node = std::env::var("JABU_WEFT_NODE").expect("set JABU_WEFT_NODE");
    let provider = WeftProvider::connect(node).await.unwrap();
    let page = provider
        .trending(GifQuery::default().with_limit(1))
        .await
        .unwrap();
    let item = page.items.first().expect("the node needs at least one GIF");
    assert_eq!(
        item.node,
        provider
            .name()
            .is_empty()
            .then_some("")
            .unwrap_or(&item.node)
    );
    let asset = provider.fetch(item).await.unwrap();
    assert!(!asset.bytes.is_empty());
    assert!(asset.mime.starts_with("image/"));
}
