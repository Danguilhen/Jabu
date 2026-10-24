//! Live Galener conference test: two Rust clients, one MUC voice room.
//!
//! Run the Movim testbed, then:
//!
//! ```text
//! JABU_TEST_XMPP=1 cargo test -p media --test conference_live -- --ignored --nocapture
//! ```
//!
//! It exercises the whole path: MUC join, SFU discovery, propose,
//! session-initiate/accept, Galener content-add, isolated receive transports
//! and the active-speaker probe.

use std::sync::Arc;
use std::time::Duration;

use media::gstreamer::GStreamerMediaPort;
use sha1::{Digest, Sha1};
use tokio::sync::mpsc::UnboundedReceiver;
use xmpp_core::{
    ClientConfig, ConferenceManager, Connection, ConnectionEvent, DiscoManager, EventBus,
    EventProcessor, IqRouter, JingleManager, MamManager, MucManager, OmemoManager, PubSubManager,
    PushManager, Roster, SpaceManager, StreamManagement, TlsConfig, VoiceManager, XmppClient,
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

#[tokio::test(flavor = "current_thread")]
#[ignore]
async fn two_clients_share_a_galener_voice_room() {
    if std::env::var("JABU_TEST_XMPP").is_err() {
        return;
    }
    std::env::set_var("JABU_AUDIO_TEST_TONE", "1");

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let alice = build_client("alice@localhost", Arc::new(EventBus::new()));
            let bob = build_client("bob@localhost", Arc::new(EventBus::new()));

            let alice_processor = processor(&alice.0);
            let bob_processor = processor(&bob.0);

            let alice_task =
                tokio::task::spawn_local(async move { alice_processor.start(alice.2).await });
            let bob_task =
                tokio::task::spawn_local(async move { bob_processor.start(bob.2).await });

            tokio::time::timeout(Duration::from_secs(20), alice.1.connect())
                .await
                .expect("alice connect timed out")
                .expect("alice connect");
            tokio::time::timeout(Duration::from_secs(20), bob.1.connect())
                .await
                .expect("bob connect timed out")
                .expect("bob connect");

            let room = format!(
                "voice-{}@conference.localhost",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let room_jid: xmpp_core::Jid = room.parse().unwrap();
            let hash = format!("{:x}@sfu.example.org", Sha1::digest(room.as_bytes()));

            join_room(&alice.0, &room, "alice");
            tokio::time::sleep(Duration::from_millis(1500)).await;
            configure_room(&alice.0, &room);
            tokio::time::sleep(Duration::from_millis(800)).await;
            join_room(&bob.0, &room, "bob");
            tokio::time::sleep(Duration::from_millis(800)).await;
            set_admin(&alice.0, &room, &hash);
            tokio::time::sleep(Duration::from_millis(800)).await;
            invite_sfu(&alice.0, &room);

            let discovered = wait_until(Duration::from_secs(20), || {
                alice.0.conference.discovery(&room_jid).is_some()
            })
            .await;
            if !discovered {
                let occupants = alice
                    .0
                    .mucs
                    .get(&room_jid)
                    .map(|room| {
                        room.occupants()
                            .into_iter()
                            .map(|occupant| {
                                (occupant.nick, occupant.jid.map(|jid| jid.to_string()))
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                eprintln!("alice occupants: {occupants:?}");
            }
            assert!(discovered, "alice never discovered the SFU");
            assert!(
                wait_until(Duration::from_secs(20), || bob
                    .0
                    .conference
                    .discovery(&room_jid)
                    .is_some())
                .await,
                "bob never discovered the SFU"
            );

            let alice_port = GStreamerMediaPort::new(Vec::new()).expect("alice media");
            alice.0.set_call_media(alice_port);
            alice.0.start_call_media_listener();
            let bob_port = GStreamerMediaPort::new(Vec::new()).expect("bob media");
            bob.0.set_call_media(bob_port);
            bob.0.start_call_media_listener();

            alice
                .0
                .start_voice_conference(&room_jid)
                .expect("alice conference");
            assert!(
                wait_until(Duration::from_secs(20), || alice
                    .0
                    .conference
                    .session(&room_jid)
                    .is_some_and(
                        |session| session.state == xmpp_core::conference::ConferenceState::Active
                    ))
                .await,
                "alice conference never became active"
            );

            bob.0
                .start_voice_conference(&room_jid)
                .expect("bob conference");

            let alice_sees_bob = wait_until(Duration::from_secs(20), || {
                alice
                    .0
                    .conference
                    .session(&room_jid)
                    .is_some_and(|session| {
                        session.contents.values().any(|content| {
                            content.participant.as_ref().is_some_and(|participant| {
                                participant.to_bare().to_string() == "bob@localhost"
                            })
                        })
                    })
            })
            .await;
            assert!(alice_sees_bob, "alice never received bob's content");

            let bob_sees_alice = wait_until(Duration::from_secs(20), || {
                bob.0.conference.session(&room_jid).is_some_and(|session| {
                    session.contents.values().any(|content| {
                        content.participant.as_ref().is_some_and(|participant| {
                            participant.to_bare().to_string() == "alice@localhost"
                        })
                    })
                })
            })
            .await;
            assert!(bob_sees_alice, "bob never received alice's content");

            let sid = alice
                .0
                .conference
                .session(&room_jid)
                .expect("alice session")
                .sid;
            let key = alice
                .0
                .conference
                .session(&room_jid)
                .expect("alice session")
                .contents
                .values()
                .find(|content| {
                    content.media == "audio"
                        && content.participant.as_ref().is_some_and(|participant| {
                            participant.to_bare().to_string() == "bob@localhost"
                        })
                })
                .map(|content| content.stream_key())
                .expect("bob audio content");
            let speaking = wait_until(Duration::from_secs(15), || {
                alice.0.jingle.content_speaking(&sid, &key)
            })
            .await;
            assert!(speaking, "alice never heard bob's tone");

            alice
                .0
                .start_voice_screen(&room_jid)
                .await
                .expect("start screen share");
            let bob_sid = bob
                .0
                .conference
                .session(&room_jid)
                .expect("bob session")
                .sid;
            let slides = wait_until(Duration::from_secs(20), || {
                bob.0.conference.session(&room_jid).is_some_and(|session| {
                    session.contents.values().any(|content| {
                        content.category == xmpp_core::conference::ConferenceCategory::Slides
                            && content.participant.as_ref().is_some_and(|participant| {
                                participant.to_bare().to_string() == "alice@localhost"
                            })
                    })
                })
            })
            .await;
            assert!(slides, "bob never received alice's screen share");

            let frame = wait_until(Duration::from_secs(20), || {
                bob.0.call_remote_screen(&bob_sid).is_some()
            })
            .await;
            assert!(frame, "bob never decoded a screen frame");

            alice
                .0
                .stop_voice_screen(&room_jid)
                .await
                .expect("stop screen");
            alice
                .0
                .leave_voice_conference(&room_jid)
                .expect("alice leave");
            bob.0.leave_voice_conference(&room_jid).expect("bob leave");
            alice_task.abort();
            bob_task.abort();
            alice.1.disconnect().await.expect("alice disconnect");
            bob.1.disconnect().await.expect("bob disconnect");
        })
        .await;
}
