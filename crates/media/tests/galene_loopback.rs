use std::time::Duration;

use media::GaleneVoicePort;
use xmpp_core::voice::{VoiceChannel, VoiceEvent, VoiceJoin, VoicePort};

fn request(username: &str) -> VoiceJoin {
    VoiceJoin {
        channel: VoiceChannel {
            space_server: "pubsub.localhost".to_string(),
            space_node: "jabu-test".to_string(),
            room: "voice-test@conference.localhost".to_string(),
            name: "Test".to_string(),
        },
        server: "http://127.0.0.1:8443/group".to_string(),
        group: "public/movim-test/voice-lobby".to_string(),
        username: username.to_string(),
        password: None,
        microphone: None,
        muted: false,
    }
}

#[tokio::test]
#[ignore]
async fn two_clients_exchange_voice_in_a_galene_channel() {
    std::env::set_var("JABU_VOICE_TEST_TONE", "1");
    let alice = GaleneVoicePort::new();
    let bob = GaleneVoicePort::new();
    let mut alice_events = alice.subscribe();
    let mut bob_events = bob.subscribe();

    tokio::time::timeout(Duration::from_secs(20), alice.join(request("alice")))
        .await
        .expect("alice join timed out")
        .expect("alice join failed");
    tokio::time::timeout(Duration::from_secs(20), bob.join(request("bob")))
        .await
        .expect("bob join timed out")
        .expect("bob join failed");

    let joined = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match alice_events.recv().await {
                Ok(VoiceEvent::Joined) => return true,
                Ok(_) => {}
                Err(_) => return false,
            }
        }
    })
    .await
    .unwrap_or(false);
    assert!(joined, "alice never emitted Joined");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let alice_names: Vec<String> = alice
            .participants()
            .into_iter()
            .map(|participant| participant.name)
            .collect();
        let bob_names: Vec<String> = bob
            .participants()
            .into_iter()
            .map(|participant| participant.name)
            .collect();
        if alice_names.iter().any(|name| name == "bob")
            && bob_names.iter().any(|name| name == "alice")
        {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "peers never appeared: alice={alice_names:?} bob={bob_names:?}"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let (alice_streams, alice_answered) = alice.stream_stats();
        let (bob_streams, bob_answered) = bob.stream_stats();
        if alice_streams >= 2 && bob_streams >= 2 && alice_answered >= 1 && bob_answered >= 1 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "media never connected: alice=({alice_streams},{alice_answered}) bob=({bob_streams},{bob_answered})"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    let mut errors = Vec::new();
    while let Ok(event) = alice_events.try_recv() {
        if let VoiceEvent::Error(error) = event {
            errors.push(error);
        }
    }
    while let Ok(event) = bob_events.try_recv() {
        if let VoiceEvent::Error(error) = event {
            errors.push(error);
        }
    }
    assert!(errors.is_empty(), "Galène reported errors: {errors:?}");

    assert!(alice.is_joined());
    assert!(bob.is_joined());

    alice.set_muted(true).await.expect("alice mute failed");
    bob.set_muted(true).await.expect("bob mute failed");

    while bob_events.try_recv().is_ok() {}

    alice.leave().await.expect("alice leave failed");
    bob.leave().await.expect("bob leave failed");
    assert!(!alice.is_joined());
    assert!(!bob.is_joined());
}
