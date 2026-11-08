//! Two simultaneous account sessions against the local Prosody fixture.
//!
//! Run `prosody/run.sh`, then:
//! `JABU_TEST_XMPP=1 cargo test -p xmpp-core --test multi_account_smoke`
//!
//! Without `JABU_TEST_XMPP` the test is skipped so the suite stays hermetic.

use std::sync::Arc;
use std::time::Duration;

use tokio::task::LocalSet;

use xmpp_core::{AccountSession, ClientConfig, EventProcessor, Jid, TlsConfig, XmppClient};

fn fixture_config(jid: &str) -> ClientConfig {
    ClientConfig {
        jid: jid.to_string(),
        password: Some("jabu-test".to_string()),
        resource: Some("jabu-multi".to_string()),
        auto_reconnect: false,
        keepalive_interval: 0,
        max_reconnect_attempts: 0,
        tls_config: TlsConfig {
            verify_cert: false,
            verify_hostname: false,
            ca_file: None,
            cert_file: None,
            key_file: None,
        },
        ..ClientConfig::default()
    }
}

fn processor(client: &XmppClient) -> EventProcessor {
    EventProcessor::new_with_jingle_and_conference(
        client.connection.clone(),
        client.roster.clone(),
        client.chats.clone(),
        client.mucs.clone(),
        client.pubsub.clone(),
        client.spaces.clone(),
        client.disco.clone(),
        client.push.clone(),
        client.sm.clone(),
        client.omemo.clone(),
        client.iq.clone(),
        client.mam.clone(),
        client.jingle.clone(),
        client.conference.clone(),
    )
}

#[tokio::test]
async fn two_sessions_connect_and_receive_independently() {
    if std::env::var("JABU_TEST_XMPP").is_err() {
        return;
    }
    let local = LocalSet::new();
    local.run_until(run_two_sessions()).await;
}

async fn run_two_sessions() {
    let mut alice = AccountSession::new(fixture_config("alice@localhost"));
    let mut bob = AccountSession::new(fixture_config("bob@localhost"));
    let alice_client = alice.client.clone();
    let bob_client = bob.client.clone();

    spawn_processor(&mut alice, &alice_client);
    spawn_processor(&mut bob, &bob_client);

    let (alice_result, bob_result) = tokio::join!(
        tokio::time::timeout(Duration::from_secs(20), alice.connect()),
        tokio::time::timeout(Duration::from_secs(20), bob.connect()),
    );
    alice_result
        .expect("alice connect timed out")
        .expect("alice connect failed");
    bob_result
        .expect("bob connect timed out")
        .expect("bob connect failed");
    assert!(alice.is_connected());
    assert!(bob.is_connected());
    assert!(!Arc::ptr_eq(&alice.events, &bob.events));

    // Let both processors send their initial presence before the first direct
    // message; otherwise the server may store it offline.
    tokio::time::sleep(Duration::from_millis(1500)).await;

    let bob_jid: Jid = "bob@localhost".parse().unwrap();
    alice_client
        .send_message(&bob_jid, "hello from alice")
        .expect("send");

    let mut received = false;
    for _ in 0..100 {
        if bob_client
            .chats
            .get_by_bare("alice@localhost")
            .is_some_and(|chat| {
                chat.messages()
                    .iter()
                    .any(|message| message.body.as_deref() == Some("hello from alice"))
            })
        {
            received = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(received, "bob did not receive alice's message");

    // The two managers stayed independent: alice has the outgoing chat with
    // bob, bob has the incoming chat with alice, and neither saw the other's
    // conversation under the wrong key.
    assert!(alice_client.chats.get_by_bare("bob@localhost").is_some());
    assert!(bob_client.chats.get_by_bare("alice@localhost").is_some());
    assert!(alice_client.chats.get_by_bare("alice@localhost").is_none());
    assert!(bob_client.chats.get_by_bare("bob@localhost").is_none());
}

fn spawn_processor(session: &mut AccountSession, client: &XmppClient) {
    let receiver = session.take_receiver().expect("receiver");
    let processor = processor(client);
    tokio::task::spawn_local(async move {
        let _ = processor.start(receiver).await;
    });
}
