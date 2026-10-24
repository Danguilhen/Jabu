//! Live Galener conference tests: several Rust clients in one MUC voice room.
//!
//! Two clients:
//!
//! ```text
//! JABU_TEST_XMPP=1 cargo test -p media --test conference_live -- --ignored --nocapture
//! ```
//!
//! A load run (accounts must already exist on the testbed):
//!
//! ```text
//! JABU_TEST_XMPP=1 JABU_TEST_USERS=load1,load2,load3,load4,load5,load6,load7,load8,load9,load10 \
//!   cargo test -p media --test conference_live -- --ignored --nocapture
//! ```
//!
//! It exercises MUC join, SFU discovery, propose, session-initiate/accept,
//! per-participant `content-add`, isolated receive transports, the
//! active-speaker probe and Galener's second session for screen sharing.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use media::gstreamer::GStreamerMediaPort;
use sha1::{Digest, Sha1};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::JoinHandle;
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
        resource: Some("jabu-conference".to_string()),
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

fn set_admin(client: &XmppClient, room: &str, hash: &str) {
    let query = Element::builder("query", "http://jabber.org/protocol/muc#admin")
        .append(
            Element::builder("item", "http://jabber.org/protocol/muc#admin")
                .attr("affiliation", "admin")
                .attr("jid", "sfu.example.org"),
        )
        .append(
            Element::builder("item", "http://jabber.org/protocol/muc#admin")
                .attr("affiliation", "admin")
                .attr("jid", hash),
        );
    let iq = Element::builder("iq", "jabber:client")
        .attr("type", "set")
        .attr("id", "admin-sfu")
        .attr("to", room)
        .append(query)
        .build();
    client.connection.send_stanza_now(iq).expect("admin iq");
}

fn invite_sfu(client: &XmppClient, room: &str) {
    let message = Element::builder("message", "jabber:client")
        .attr("to", room)
        .attr("id", "invite-sfu")
        .append(
            Element::builder("x", "http://jabber.org/protocol/muc#user").append(
                Element::builder("invite", "http://jabber.org/protocol/muc#user")
                    .attr("to", "sfu.example.org")
                    .append(Element::builder(
                        "reason",
                        "http://jabber.org/protocol/muc#user",
                    )),
            ),
        )
        .build();
    client
        .connection
        .send_stanza_now(message)
        .expect("invite sfu");
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

async fn start_peer(user: String) -> Peer {
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
        user,
        client,
        connection,
        task,
    }
}

fn remote_participants(peer: &Peer, room: &xmpp_core::Jid) -> HashSet<String> {
    let own = peer
        .client
        .connection
        .jid()
        .map(|jid| jid.to_bare().to_string())
        .unwrap_or_default();
    peer.client
        .conference
        .session(room)
        .map(|session| {
            session
                .contents
                .values()
                .filter_map(|content| content.participant.as_ref())
                .map(|participant| participant.to_bare().to_string())
                .filter(|participant| participant != &own)
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test(flavor = "current_thread")]
#[ignore]
async fn clients_share_a_galener_voice_room() {
    if std::env::var("JABU_TEST_XMPP").is_err() {
        return;
    }
    std::env::set_var("JABU_AUDIO_TEST_TONE", "1");
    std::env::set_var("JABU_AUDIO_TEST_FAKESINK", "1");
    std::env::set_var("JABU_SCREEN_SOURCE", "test");

    let users: Vec<String> = std::env::var("JABU_TEST_USERS")
        .unwrap_or_else(|_| "alice,bob".to_string())
        .split(',')
        .map(str::trim)
        .filter(|user| !user.is_empty())
        .map(str::to_string)
        .collect();
    assert!(users.len() >= 2, "need at least two users");
    let count = users.len();

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let mut peers = Vec::new();
            for user in users {
                peers.push(start_peer(user).await);
            }

            let room = format!(
                "voice-{}@conference.localhost",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let room_jid: xmpp_core::Jid = room.parse().unwrap();
            let hash = format!("{:x}@sfu.example.org", Sha1::digest(room.as_bytes()));

            join_room(&peers[0].client, &room, &peers[0].user);
            tokio::time::sleep(Duration::from_millis(1500)).await;
            configure_room(&peers[0].client, &room);
            tokio::time::sleep(Duration::from_millis(800)).await;
            for peer in peers.iter().skip(1) {
                join_room(&peer.client, &room, &peer.user);
            }
            tokio::time::sleep(Duration::from_millis(1000)).await;
            set_admin(&peers[0].client, &room, &hash);
            tokio::time::sleep(Duration::from_millis(800)).await;
            invite_sfu(&peers[0].client, &room);

            for peer in &peers {
                let discovered = wait_until(Duration::from_secs(30), || {
                    peer.client.conference.discovery(&room_jid).is_some()
                })
                .await;
                assert!(
                    discovered,
                    "{} never discovered the SFU (in the room: {:?})",
                    peer.user,
                    peer.client
                        .mucs
                        .get(&room_jid)
                        .map(|room| room.occupants().len())
                        .unwrap_or_default()
                );
            }

            for peer in &peers {
                let port = GStreamerMediaPort::new(Vec::new()).expect("media port");
                peer.client.set_call_media(port);
                peer.client.start_call_media_listener();
            }
            for peer in &peers {
                peer.client
                    .start_voice_conference(&room_jid)
                    .expect("start conference");
            }

            for peer in &peers {
                let active = wait_until(Duration::from_secs(30), || {
                    peer.client
                        .conference
                        .session(&room_jid)
                        .is_some_and(|session| session.state == ConferenceState::Active)
                })
                .await;
                assert!(active, "{} conference never became active", peer.user);
            }

            for peer in &peers {
                let complete = wait_until(Duration::from_secs(60), || {
                    remote_participants(peer, &room_jid).len() >= count - 1
                })
                .await;
                assert!(
                    complete,
                    "{} sees only {:?}",
                    peer.user,
                    remote_participants(peer, &room_jid)
                );
            }

            let first = &peers[0];
            let own = first
                .client
                .connection
                .jid()
                .map(|jid| jid.to_bare().to_string())
                .unwrap();
            let session = first
                .client
                .conference
                .session(&room_jid)
                .expect("first session");
            let audio_key = session
                .contents
                .values()
                .find(|content| {
                    content.media == "audio"
                        && content
                            .participant
                            .as_ref()
                            .is_some_and(|participant| participant.to_bare().to_string() != own)
                })
                .map(|content| content.stream_key())
                .expect("a remote audio content");
            let speaking = wait_until(Duration::from_secs(20), || {
                first
                    .client
                    .jingle
                    .content_speaking(&session.sid, &audio_key)
            })
            .await;
            assert!(speaking, "{} never heard another participant", first.user);

            first
                .client
                .start_voice_screen(&room_jid)
                .await
                .expect("start screen share");
            let observer = &peers[1];
            let slides = wait_until(Duration::from_secs(30), || {
                observer
                    .client
                    .conference
                    .session(&room_jid)
                    .is_some_and(|session| {
                        session.contents.values().any(|content| {
                            content.category == xmpp_core::conference::ConferenceCategory::Slides
                                && content.participant.as_ref().is_some_and(|participant| {
                                    participant.to_bare().to_string()
                                        == format!("{}@localhost", first.user)
                                })
                        })
                    })
            })
            .await;
            assert!(slides, "{} never received the screen share", observer.user);

            let observer_sid = observer
                .client
                .conference
                .session(&room_jid)
                .expect("observer session")
                .sid;
            let frame = wait_until(Duration::from_secs(30), || {
                observer.client.call_remote_screen(&observer_sid).is_some()
            })
            .await;
            assert!(frame, "{} never decoded a screen frame", observer.user);

            first
                .client
                .stop_voice_screen(&room_jid)
                .await
                .expect("stop screen share");
            for peer in &peers {
                peer.client
                    .leave_voice_conference(&room_jid)
                    .expect("leave conference");
            }
            for peer in &peers {
                peer.task.abort();
                peer.connection
                    .disconnect()
                    .await
                    .expect("disconnect failed");
            }
        })
        .await;
}
