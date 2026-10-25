//! The path the app takes for a voice channel, against the local testbed.
//!
//! The client has to prepare the room itself: a plain room that it joins and
//! invites the SFU into cannot carry a call until the SFU is an admin, so these
//! tests fail if the appointment is missing or happens in the wrong order.
//!
//! ```text
//! JABU_TEST_XMPP=1 cargo test -p media --test conference_client_flow -- --ignored --nocapture
//! ```
//!
//! Both scenarios run in one test on purpose: the testbed registers only
//! `alice`, `bob` and `admin`, and two tests in one binary run in parallel and
//! would kick each other off the same accounts.

use std::sync::Arc;
use std::time::Duration;

use media::gstreamer::GStreamerMediaPort;
use media::ConferenceVoicePort;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::JoinHandle;
use xmpp_core::voice::{VoiceChannel, VoiceJoin};
use xmpp_core::{
    ClientConfig, ConferenceManager, ConferenceState, Connection, ConnectionEvent, DiscoManager,
    EventBus, EventProcessor, IqRouter, JingleManager, MamManager, MucManager, OmemoManager,
    PubSubManager, PushManager, Roster, SpaceManager, StreamManagement, TlsConfig, VoiceManager,
    XmppClient,
};
use xmpp_parsers::minidom::Element;

fn fixture_config(jid: &str) -> ClientConfig {
    ClientConfig {
        jid: jid.to_string(),
        password: Some("jabu-test".to_string()),
        resource: Some("jabu-client-flow".to_string()),
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

fn build_client(
    jid: &str,
    events: Arc<EventBus>,
) -> (
    XmppClient,
    Arc<Connection>,
    UnboundedReceiver<ConnectionEvent>,
) {
    let (connection, event_rx) = Connection::new(fixture_config(jid).into());
    let connection = Arc::new(connection);
    let roster = Arc::new(Roster::new(events.clone()));
    let chats = Arc::new(xmpp_core::ChatManager::new(events.clone()));
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
        mam,
        jingle,
        conference: ConferenceManager::new(),
        voice: Arc::new(VoiceManager::new()),
    };
    (client, connection, event_rx)
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

fn join_room(client: &XmppClient, room: &str, nick: &str) {
    let presence = Element::builder("presence", "jabber:client")
        .attr("to", format!("{room}/{nick}"))
        .append(Element::builder("x", "http://jabber.org/protocol/muc"))
        .build();
    client
        .connection
        .send_stanza_now(presence)
        .expect("join presence");
}

fn configure_room(client: &XmppClient, room: &str) {
    let query = Element::builder("query", "http://jabber.org/protocol/muc#owner")
        .append(Element::builder("x", "jabber:x:data").attr("type", "submit"));
    let iq = Element::builder("iq", "jabber:client")
        .attr("type", "set")
        .attr("id", "config-room")
        .attr("to", room)
        .append(query)
        .build();
    client
        .connection
        .send_stanza_now(iq)
        .expect("room config iq");
}

async fn wait_until<F>(timeout: Duration, mut check: F) -> bool
where
    F: FnMut() -> bool,
{
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        if check() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

struct Peer {
    user: String,
    client: XmppClient,
    connection: Arc<Connection>,
    task: JoinHandle<()>,
}

async fn start_peer(user: &str) -> Peer {
    let (client, connection, event_rx) =
        build_client(&format!("{user}@localhost"), Arc::new(EventBus::new()));
    let processor = processor(&client);
    let task = tokio::task::spawn_local(async move {
        let _ = processor.start(event_rx).await;
    });
    tokio::time::timeout(Duration::from_secs(20), connection.connect())
        .await
        .expect("connect timed out")
        .expect("connect failed");
    Peer {
        user: user.to_string(),
        client,
        connection,
        task,
    }
}

fn install_voice(peer: &Peer, sfu: &xmpp_core::Jid) {
    peer.client
        .set_call_media(GStreamerMediaPort::new(Vec::new()).expect("media port"));
    peer.client.start_call_media_listener();
    peer.client
        .voice
        .set_port(Arc::new(ConferenceVoicePort::new(
            peer.client.clone(),
            Some(sfu.clone()),
        )));
}

fn voice_join(room: &str, nick: &str, name: &str) -> VoiceJoin {
    VoiceJoin {
        channel: VoiceChannel {
            space_server: "pubsub.localhost".to_string(),
            space_node: "voice".to_string(),
            room: room.to_string(),
            name: name.to_string(),
        },
        username: nick.to_string(),
        microphone: None,
        muted: false,
    }
}

fn room_is_joined(peer: &Peer, room: &xmpp_core::Jid) -> bool {
    peer.client
        .mucs
        .get_by_bare(&room.to_bare().to_string())
        .is_some_and(|muc| muc.is_joined())
}

fn call_is_up(peer: &Peer, room: &xmpp_core::Jid) -> bool {
    peer.client.conference.session(room).is_some_and(|session| {
        matches!(
            session.state,
            ConferenceState::Active | ConferenceState::Connecting
        )
    })
}

async fn disconnect(peer: Peer) {
    peer.task.abort();
    peer.connection
        .disconnect()
        .await
        .expect("disconnect failed");
}

#[tokio::test(flavor = "current_thread")]
#[ignore]
async fn a_room_the_client_prepares_carries_a_call() {
    if std::env::var("JABU_TEST_XMPP").is_err() {
        return;
    }
    std::env::set_var("JABU_AUDIO_TEST_TONE", "1");
    std::env::set_var("JABU_AUDIO_TEST_FAKESINK", "1");
    std::env::set_var("JABU_SCREEN_SOURCE", "test");

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let alice = start_peer("alice").await;
            let bob = start_peer("bob").await;
            let sfu: xmpp_core::Jid = "sfu.example.org".parse().expect("sfu jid");
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos();

            for (kind, prepare_first) in [("fresh", false), ("legacy", true)] {
                let room = format!("voice-{kind}-{stamp}@conference.localhost");
                let room_jid: xmpp_core::Jid = room.parse().expect("room jid");

                if prepare_first {
                    join_room(&alice.client, &room, &alice.user);
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                    configure_room(&alice.client, &room);
                    tokio::time::sleep(Duration::from_millis(800)).await;
                }

                install_voice(&alice, &sfu);
                install_voice(&bob, &sfu);

                alice
                    .client
                    .voice
                    .join(voice_join(&room, &alice.user, kind))
                    .await
                    .expect("owner join failed");
                assert!(
                    wait_until(Duration::from_secs(20), || room_is_joined(
                        &alice, &room_jid
                    ))
                    .await,
                    "the owner never joined the room"
                );

                assert!(
                    wait_until(Duration::from_secs(30), || {
                        alice.client.conference.discovery(&room_jid).is_some()
                    })
                    .await,
                    "the SFU never joined the room"
                );

                bob.client
                    .voice
                    .join(voice_join(&room, &bob.user, kind))
                    .await
                    .expect("guest join failed");
                assert!(
                    wait_until(Duration::from_secs(20), || room_is_joined(&bob, &room_jid)).await,
                    "the guest never joined the room"
                );

                for peer in [&alice, &bob] {
                    assert!(
                        wait_until(Duration::from_secs(40), || call_is_up(peer, &room_jid)).await,
                        "the SFU did not accept the call in {}",
                        peer.user
                    );
                }

                for peer in [&alice, &bob] {
                    peer.client.voice.leave().await.expect("leave voice");
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }

            disconnect(alice).await;
            disconnect(bob).await;
        })
        .await;
}
