use std::time::Duration;

use media::GStreamerMediaPort;
use xmpp_core::jingle::{CallKind, MediaEvent, MediaPort};

async fn relay(
    port: std::sync::Arc<GStreamerMediaPort>,
    sid: &'static str,
    mut rx: tokio::sync::broadcast::Receiver<MediaEvent>,
) {
    loop {
        match rx.recv().await {
            Ok(MediaEvent::LocalCandidate { jingle, .. }) => {
                let _ = port.add_candidate(sid, &jingle).await;
            }
            Ok(_) => {}
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
            Err(_) => break,
        }
    }
}

#[tokio::test]
#[ignore]
async fn screen_share_reaches_the_other_port_audio() {
    std::env::set_var("JABU_SCREEN_SOURCE", "test");
    let a = GStreamerMediaPort::new(Vec::new()).unwrap();
    let b = GStreamerMediaPort::new(Vec::new()).unwrap();

    let a_events = a.subscribe();
    let b_events = b.subscribe();
    let b_for_a = b.clone();
    let a_for_b = a.clone();
    tokio::spawn(async move { relay(b_for_a, "b", a_events).await });
    tokio::spawn(async move { relay(a_for_b, "a", b_events).await });

    let offer = a.create_offer("a", CallKind::Audio).await.unwrap();
    let answer = b.accept_offer("b", CallKind::Audio, &offer).await.unwrap();
    a.apply_answer("a", &answer).await.unwrap();
    a.set_muted("a", true).await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    let content_add = a.add_screen_offer("a").await.unwrap();
    let content_accept = b.accept_screen_add("b", &content_add).await.unwrap();
    a.apply_screen_answer("a", &content_accept).await.unwrap();
    a.set_screen("a", true).await.unwrap();

    let mut saw_local = false;
    let mut saw_remote = false;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        saw_local |= a.screen_preview_frame("a").is_some();
        saw_remote |= b.remote_screen_frame("b").is_some();
        if saw_local && saw_remote {
            break;
        }
    }
    eprintln!("local preview: {saw_local}, remote screen: {saw_remote}");
    assert!(saw_local, "the sender never produced local screen frames");
    assert!(saw_remote, "the receiver never received screen frames");
}

#[tokio::test]
#[ignore]
async fn screen_share_reaches_the_other_port_video() {
    std::env::set_var("JABU_SCREEN_SOURCE", "test");
    let a = GStreamerMediaPort::new(Vec::new()).unwrap();
    let b = GStreamerMediaPort::new(Vec::new()).unwrap();

    let a_events = a.subscribe();
    let b_events = b.subscribe();
    let b_for_a = b.clone();
    let a_for_b = a.clone();
    tokio::spawn(async move { relay(b_for_a, "b", a_events).await });
    tokio::spawn(async move { relay(a_for_b, "a", b_events).await });

    let offer = a.create_offer("a", CallKind::Video).await.unwrap();
    let answer = b.accept_offer("b", CallKind::Video, &offer).await.unwrap();
    a.apply_answer("a", &answer).await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    let content_add = a.add_screen_offer("a").await.unwrap();
    let content_accept = b.accept_screen_add("b", &content_add).await.unwrap();
    a.apply_screen_answer("a", &content_accept).await.unwrap();
    a.set_screen("a", true).await.unwrap();

    let mut saw_local = false;
    let mut saw_remote = false;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        saw_local |= a.screen_preview_frame("a").is_some();
        saw_remote |= b.remote_screen_frame("b").is_some();
        if saw_local && saw_remote {
            break;
        }
    }
    eprintln!("video call: local preview: {saw_local}, remote screen: {saw_remote}");
    assert!(saw_local, "the sender never produced local screen frames");
    assert!(saw_remote, "the receiver never received screen frames");
}

#[tokio::test]
#[ignore]
async fn screen_share_captures_the_real_desktop() {
    std::env::remove_var("JABU_SCREEN_SOURCE");
    std::env::remove_var("JABU_SCREEN_NODE");
    let a = GStreamerMediaPort::new(Vec::new()).unwrap();
    let b = GStreamerMediaPort::new(Vec::new()).unwrap();

    let a_events = a.subscribe();
    let b_events = b.subscribe();
    let b_for_a = b.clone();
    let a_for_b = a.clone();
    tokio::spawn(async move { relay(b_for_a, "b", a_events).await });
    tokio::spawn(async move { relay(a_for_b, "a", b_events).await });

    let offer = a.create_offer("a", CallKind::Audio).await.unwrap();
    let answer = b.accept_offer("b", CallKind::Audio, &offer).await.unwrap();
    a.apply_answer("a", &answer).await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    eprintln!("waiting for a screen to be selected in the portal picker");
    let content_add = a.add_screen_offer("a").await.unwrap();
    let content_accept = b.accept_screen_add("b", &content_add).await.unwrap();
    a.apply_screen_answer("a", &content_accept).await.unwrap();
    a.set_screen("a", true).await.unwrap();

    let mut saw_local = false;
    let mut saw_remote = false;
    for _ in 0..150 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        saw_local |= a.screen_preview_frame("a").is_some();
        saw_remote |= b.remote_screen_frame("b").is_some();
        if saw_local && saw_remote {
            break;
        }
    }
    eprintln!("portal: local preview: {saw_local}, remote screen: {saw_remote}");
    assert!(saw_local, "the sender never produced local screen frames");
    assert!(saw_remote, "the receiver never received screen frames");
}
