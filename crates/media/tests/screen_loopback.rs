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
    if let Some(p) = a.screen_preview_frame("a") {
        eprintln!("local preview size {}x{}", p.width, p.height);
    }
    if let Some(p) = b.remote_screen_frame("b") {
        eprintln!("remote screen size {}x{}", p.width, p.height);
    }
    eprintln!("local preview: {saw_local}, remote screen: {saw_remote}");
    assert!(saw_local, "the sender never produced local screen frames");
    assert!(saw_remote, "the receiver never received screen frames");
    let first_local = a.screen_preview_frame("a").unwrap().sequence;
    let first_remote = b.remote_screen_frame("b").unwrap().sequence;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let last_local = a.screen_preview_frame("a").unwrap().sequence;
    let last_remote = b.remote_screen_frame("b").unwrap().sequence;
    eprintln!("local {first_local}->{last_local}, remote {first_remote}->{last_remote}");
    assert!(last_local > first_local, "local preview froze");
    assert!(last_remote > first_remote, "remote screen froze");
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
    if let Some(p) = a.screen_preview_frame("a") {
        eprintln!("video call local size {}x{}", p.width, p.height);
    }
    if let Some(p) = b.remote_screen_frame("b") {
        eprintln!("video call remote size {}x{}", p.width, p.height);
    }
    eprintln!("video call: local preview: {saw_local}, remote screen: {saw_remote}");
    assert!(saw_local, "the sender never produced local screen frames");
    assert!(saw_remote, "the receiver never received screen frames");
    let first_local = a.screen_preview_frame("a").unwrap().sequence;
    let first_remote = b.remote_screen_frame("b").unwrap().sequence;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let last_local = a.screen_preview_frame("a").unwrap().sequence;
    let last_remote = b.remote_screen_frame("b").unwrap().sequence;
    eprintln!(
        "video call: local {first_local}->{last_local}, remote {first_remote}->{last_remote}"
    );
    assert!(last_local > first_local, "local preview froze");
    assert!(last_remote > first_remote, "remote screen froze");
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
    if let Some(p) = a.screen_preview_frame("a") {
        eprintln!("portal local size {}x{}", p.width, p.height);
    }
    if let Some(p) = b.remote_screen_frame("b") {
        eprintln!("portal remote size {}x{}", p.width, p.height);
    }
    eprintln!("portal: local preview: {saw_local}, remote screen: {saw_remote}");
    assert!(saw_local, "the sender never produced local screen frames");
    assert!(saw_remote, "the receiver never received screen frames");
    let first_local = a.screen_preview_frame("a").unwrap().sequence;
    let first_remote = b.remote_screen_frame("b").unwrap().sequence;
    tokio::time::sleep(Duration::from_secs(3)).await;
    let last_local = a.screen_preview_frame("a").unwrap().sequence;
    let last_remote = b.remote_screen_frame("b").unwrap().sequence;
    eprintln!("portal: local {first_local}->{last_local}, remote {first_remote}->{last_remote}");
    assert!(last_local > first_local, "local preview froze");
    assert!(last_remote > first_remote, "remote screen froze");
}

#[tokio::test]
#[ignore]
async fn audio_device_can_be_selected() {
    let a = GStreamerMediaPort::new(Vec::new()).unwrap();
    let devices = a.audio_devices();
    eprintln!("audio devices: {devices:?}");
    assert!(!devices.is_empty(), "no audio input devices found");
    a.set_preferred_audio(Some(&devices[0].id));

    let offer = a.create_offer("a", CallKind::Audio).await.unwrap();
    assert!(!offer.is_empty());
    tokio::time::sleep(Duration::from_secs(1)).await;
    a.stop("a").await.unwrap();
}
