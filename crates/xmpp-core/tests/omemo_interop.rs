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

/// The harness uses one bob account and one state file, so tests must not run
/// concurrently.
static INTEROP_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn spawn_peer(python: &str, room: Option<&str>, legacy_only: bool) -> Peer {
    let _ = std::fs::remove_file("/tmp/omemo-interop-bob.json");
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("repo root");
    let mut command = Command::new(python);
    command
        .arg(repo_root.join("dev/omemo-interop/bob.py"))
        .arg("--seconds")
        .arg("120")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if let Some(room) = room {
        command.arg("--room").arg(room);
    }
    if legacy_only {
        command.arg("--legacy-only");
    }
    let child = command.spawn().expect("could not start slixmpp-omemo peer");
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

/// Build, connect and bootstrap a fixture client with OMEMO keys published.
async fn fixture_client() -> (XmppClient, tokio::task::JoinHandle<()>) {
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
    let processor_task = tokio::task::spawn_local(async move {
        let _ = processor.start(event_rx).await;
    });

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

    (client, processor_task)
}

#[allow(clippy::large_futures)]
#[tokio::test(flavor = "current_thread")]
#[ignore = "requires a local Prosody and a slixmpp-omemo virtualenv"]
async fn interop_muc_with_slixmpp_omemo() {
    if !fixture_enabled() {
        return;
    }
    let _guard = INTEROP_LOCK.lock().await;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
    let python = std::env::var("JABU_TEST_OMEMO_PYTHON")
        .unwrap_or_else(|_| "/tmp/omemo-venv/bin/python".to_string());
    let room: xmpp_core::Jid = format!(
        "omemo-{}@conference.localhost",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos()
    )
    .parse()
    .expect("valid room JID");
    let _peer = spawn_peer(&python, Some(&room.to_string()), false);
    tokio::time::sleep(Duration::from_secs(4)).await;

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let (client, processor_task) = fixture_client().await;
            let nick = "alice";
            client.join_room(&room, nick, None).expect("join room");

            let bob: xmpp_core::Jid = "bob@localhost".parse().unwrap();
            let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
            loop {
                if client.omemo_bundles(&bob).is_empty() {
                    let _ = client.fetch_omemo(&bob);
                }
                let bob_present = client.mucs.get(&room).is_some_and(|room| {
                    room.occupants().iter().any(|occupant| {
                        occupant.nick != nick
                            && occupant
                                .jid
                                .as_ref()
                                .is_some_and(|jid| jid.to_bare() == bob)
                    })
                });
                if bob_present && !client.omemo_bundles(&bob).is_empty() {
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "slixmpp-omemo peer never joined the room with a real JID"
                );
                tokio::time::sleep(Duration::from_millis(250)).await;
            }

            client
                .send_muc_omemo_message(&room, nick, "ping from rust")
                .expect("group send failed");

            let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
            loop {
                if client.mucs.get(&room).is_some_and(|room| {
                    room.messages()
                        .iter()
                        .any(|message| message.body.as_deref() == Some("pong from slixmpp-omemo"))
                }) {
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "no decrypted group reply from slixmpp-omemo"
                );
                tokio::time::sleep(Duration::from_millis(200)).await;
            }

            processor_task.abort();
            client
                .connection
                .disconnect()
                .await
                .expect("disconnect failed");
        })
        .await;
}

#[allow(clippy::large_futures)]
#[tokio::test(flavor = "current_thread")]
#[ignore = "requires a local Prosody and a slixmpp-omemo virtualenv"]
async fn interop_legacy_with_slixmpp_omemo() {
    if !fixture_enabled() {
        return;
    }
    let _guard = INTEROP_LOCK.lock().await;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
    let python = std::env::var("JABU_TEST_OMEMO_PYTHON")
        .unwrap_or_else(|_| "/tmp/omemo-venv/bin/python".to_string());
    let _peer = spawn_peer(&python, None, true);
    tokio::time::sleep(Duration::from_secs(4)).await;

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let (client, processor_task) = fixture_client().await;
            let device = client.omemo.local().expect("local device");
            client
                .publish_legacy_omemo_device_list(&[device.device_id])
                .expect("legacy device list publish");
            client
                .publish_legacy_omemo_bundle(&device.legacy_bundle())
                .expect("legacy bundle publish");

            let bob: xmpp_core::Jid = "bob@localhost".parse().unwrap();
            let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
            while client.omemo_legacy_bundles(&bob).is_empty() {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "slixmpp-omemo peer never published a legacy bundle"
                );
                let _ = client.fetch_legacy_omemo(&bob);
                let _ = client.fetch_omemo(&bob);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            let _ = client.fetch_omemo(&bob);
            tokio::time::sleep(Duration::from_secs(1)).await;
            assert!(
                client.omemo_bundles(&bob).is_empty(),
                "legacy-only peer unexpectedly published OMEMO 2 devices"
            );

            client
                .send_omemo_message(&bob, "ping from rust")
                .expect("legacy fallback send failed");

            let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
            loop {
                if client.chats.get(&bob).is_some_and(|chat| {
                    chat.messages()
                        .iter()
                        .any(|message| message.body.as_deref() == Some("pong from slixmpp-omemo"))
                }) {
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "no decrypted legacy reply from slixmpp-omemo"
                );
                tokio::time::sleep(Duration::from_millis(200)).await;
            }

            processor_task.abort();
            client
                .connection
                .disconnect()
                .await
                .expect("disconnect failed");
        })
        .await;
}

#[allow(clippy::large_futures)]
#[tokio::test(flavor = "current_thread")]
#[ignore = "requires a local Prosody and a slixmpp-omemo virtualenv"]
async fn interop_with_slixmpp_omemo() {
    if !fixture_enabled() {
        return;
    }
    let _guard = INTEROP_LOCK.lock().await;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
    let python = std::env::var("JABU_TEST_OMEMO_PYTHON")
        .unwrap_or_else(|_| "/tmp/omemo-venv/bin/python".to_string());
    let _peer = spawn_peer(&python, None, false);
    tokio::time::sleep(Duration::from_secs(4)).await;

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let (client, processor_task) = fixture_client().await;

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
                if client.chats.get(&bob).is_some_and(|chat| {
                    chat.messages()
                        .iter()
                        .any(|message| message.body.as_deref() == Some("pong from slixmpp-omemo"))
                }) {
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "no decrypted reply from slixmpp-omemo"
                );
                tokio::time::sleep(Duration::from_millis(200)).await;
            }

            processor_task.abort();
            client
                .connection
                .disconnect()
                .await
                .expect("disconnect failed");
        })
        .await;
}
