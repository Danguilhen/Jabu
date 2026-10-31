//! Live OMEMO interop against slixmpp-omemo.
//!
//! The test publishes a modern OMEMO 2 bundle, exchanges a key exchange, sends
//! an SCE-encrypted `ping`, and waits for the Python peer's encrypted `pong`.
//! slixmpp-omemo 2.2.0 does not implement the XEP-0420 envelope itself, so
//! `bob.py` decrypts at the session-manager level and unwraps the envelope in
//! the harness. The ratchet and payload layers are the real Python
//! implementations.
//!
//! Run `prosody/run.sh`, install `dev/omemo-interop/requirements.txt` into a
//! virtualenv, then:
//!
//! ```text
//! JABU_TEST_XMPP=1 \
//! JABU_TEST_OMEMO_PYTHON=/tmp/omemo-venv/bin/python \
//! cargo test -p xmpp-core --test omemo_interop -- --ignored --nocapture
//! ```

use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use xmpp_core::{
    ChatManager, ClientConfig, Connection, DiscoManager, EventBus, EventProcessor, IqRouter,
    JingleManager, MamManager, MucManager, OmemoManager, PubSubManager, PushManager, Roster,
    SpaceManager, StreamManagement, TlsConfig, XmppClient,
};

fn fixture_config(jid: &str) -> ClientConfig {
    ClientConfig {
        jid: jid.to_string(),
        password: Some("jabu-test".to_string()),
        resource: Some("jabu-omemo-interop".to_string()),
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

fn fixture_enabled() -> bool {
    std::env::var("JABU_TEST_XMPP").is_ok()
}

fn spawn_peer(python: &str) -> Peer {
    let _ = std::fs::remove_file("/tmp/omemo-interop-bob.json");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("repo root");
    let child = Command::new(python)
        .arg(root.join("dev/omemo-interop/bob.py"))
        .arg("--seconds")
        .arg("120")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("could not start slixmpp-omemo peer");
    Peer(child)
}

/// Kills the Python peer even when the test panics inside the async block.
struct Peer(Child);

impl Drop for Peer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[allow(clippy::large_futures)]
#[tokio::test(flavor = "current_thread")]
#[ignore = "requires a local Prosody and a slixmpp-omemo virtualenv"]
async fn interop_with_slixmpp_omemo() {
    if !fixture_enabled() {
        return;
    }
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
    let python = std::env::var("JABU_TEST_OMEMO_PYTHON")
        .unwrap_or_else(|_| "/tmp/omemo-venv/bin/python".to_string());
    let _peer = spawn_peer(&python);
    tokio::time::sleep(Duration::from_secs(4)).await;

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let events = Arc::new(EventBus::new());
            let (connection, event_rx) = Connection::new(fixture_config("alice@localhost").into());
            let connection = Arc::new(connection);
            let roster = Arc::new(Roster::new(events.clone()));
            let chats = Arc::new(ChatManager::new(events.clone()));
            let mucs = Arc::new(MucManager::new(events.clone()));
            let pubsub = Arc::new(PubSubManager::new(events.clone()));
            let disco = Arc::new(DiscoManager::new());
            let push = Arc::new(PushManager::new());
            let sm = Arc::new(StreamManagement::new());
            let omemo = Arc::new(OmemoManager::new());
            let iq = Arc::new(IqRouter::new(Duration::from_secs(10)));
            let mam = Arc::new(MamManager::new());

            connection.set_stream_management(sm.clone());
            let jingle = JingleManager::new(connection.clone(), iq.clone(), events.clone());
            let spaces = Arc::new(SpaceManager::new(events.clone()));
            let client = XmppClient {
                connection: connection.clone(),
                roster,
                chats,
                mucs,
                pubsub,
                spaces,
                disco,
                push,
                sm,
                omemo,
                events,
                iq,
                mam: mam.clone(),
                jingle,
                conference: xmpp_core::ConferenceManager::new(),
                voice: Arc::new(xmpp_core::VoiceManager::new()),
            };

            let processor = EventProcessor::new(
                connection.clone(),
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
                mam,
            );
            let processor_task =
                tokio::task::spawn_local(async move { processor.start(event_rx).await });

            tokio::time::timeout(Duration::from_secs(20), connection.connect())
                .await
                .expect("connect timed out")
                .expect("connect failed");

            let device = client.omemo.ensure_local_auto(100);
            client
                .publish_omemo_device_list(&[device.device_id])
                .expect("device list publish");
            client
                .publish_omemo_bundle(&device.bundle())
                .expect("bundle publish");

            let bob: xmpp_core::Jid = "bob@localhost".parse().unwrap();
            let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
            while client.omemo_bundles(&bob).is_empty() {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "slixmpp-omemo peer never published a bundle"
                );
                let _ = client.fetch_omemo(&bob);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            for bundle in client.omemo_bundles(&bob) {
                println!(
                    "BUNDLE device={} verify={} ik={} spk={} sig={} prekeys={}",
                    bundle.device_id,
                    bundle.verify(),
                    bundle.identity_key.len(),
                    bundle.signed_prekey.len(),
                    bundle.signed_prekey_signature.len(),
                    bundle.prekeys.len()
                );
            }

            client
                .send_omemo_message(&bob, "ping from rust")
                .expect("send failed");

            let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
            loop {
                if client
                    .chats
                    .get(&bob)
                    .map(|chat| {
                        chat.messages().iter().any(|message| {
                            message.body.as_deref() == Some("pong from slixmpp-omemo")
                        })
                    })
                    .unwrap_or(false)
                {
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "no decrypted reply from slixmpp-omemo"
                );
                tokio::time::sleep(Duration::from_millis(200)).await;
            }

            processor_task.abort();
            connection.disconnect().await.expect("disconnect failed");
        })
        .await;
}
