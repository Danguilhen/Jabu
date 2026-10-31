//! Live smoke test against the local Prosody fixture.
//!
//! Run `prosody/run.sh`, then:
//! `JABU_TEST_XMPP=1 cargo test -p xmpp-core --test prosody_smoke`
//!
//! Without `JABU_TEST_XMPP` the test is skipped so `cargo test` stays hermetic.

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::UnboundedReceiver;
use xmpp_core::{
    ChatManager, ClientConfig, Connection, ConnectionEvent, DiscoManager, EventBus, EventProcessor,
    IqRouter, JingleManager, MamManager, MucManager, OmemoManager, ProxyConfig, ProxyType,
    PubSubManager, PushManager, Roster, RosterItem, SpaceManager, StreamManagement, TlsConfig,
    VCard4, XmppClient, XmppEvent,
};
use xmpp_parsers::minidom::Element;

fn fixture_config(jid: &str) -> ClientConfig {
    ClientConfig {
        jid: jid.to_string(),
        password: Some("jabu-test".to_string()),
        resource: Some("jabu-smoke".to_string()),
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

#[tokio::test]
async fn logs_in_to_prosody() {
    if !fixture_enabled() {
        return;
    }

    let (connection, _events) = Connection::new(fixture_config("alice@localhost").into());

    tokio::time::timeout(Duration::from_secs(20), connection.connect())
        .await
        .expect("connect timed out")
        .expect("connect failed");

    assert!(connection.current_state().is_connected());
    assert_eq!(
        connection
            .jid()
            .map(|jid| jid.to_bare().to_string())
            .as_deref(),
        Some("alice@localhost")
    );

    connection.disconnect().await.expect("disconnect failed");
}

#[tokio::test]
async fn rejects_wrong_password() {
    if !fixture_enabled() {
        return;
    }

    let mut config = fixture_config("alice@localhost");
    config.password = Some("not-the-password".to_string());
    let (connection, _events) = Connection::new(config.into());

    let result = tokio::time::timeout(Duration::from_secs(20), connection.connect()).await;

    assert!(
        !matches!(result, Ok(Ok(()))),
        "authentication unexpectedly succeeded"
    );
}

#[tokio::test]
async fn logs_in_over_direct_tls() {
    if !fixture_enabled() {
        return;
    }

    let mut config = fixture_config("alice@localhost");
    config.direct_tls = true;

    let (connection, _events) = Connection::new(config.into());
    tokio::time::timeout(Duration::from_secs(20), connection.connect())
        .await
        .expect("connect timed out")
        .expect("direct-TLS connect failed");
    assert!(connection.jid().is_some());
    connection.disconnect().await.expect("disconnect failed");
}

async fn start_socks5_proxy() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        loop {
            let Ok((mut client, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut greeting = [0u8; 2];
                client.read_exact(&mut greeting).await.unwrap();
                let mut methods = vec![0u8; greeting[1] as usize];
                client.read_exact(&mut methods).await.unwrap();
                client.write_all(&[0x05, 0x00]).await.unwrap();

                let mut header = [0u8; 5];
                client.read_exact(&mut header).await.unwrap();
                let mut host = vec![0u8; header[4] as usize];
                client.read_exact(&mut host).await.unwrap();
                let mut port_bytes = [0u8; 2];
                client.read_exact(&mut port_bytes).await.unwrap();
                let host = String::from_utf8(host).unwrap();
                let port = u16::from_be_bytes(port_bytes);

                let mut upstream = TcpStream::connect((host.as_str(), port)).await.unwrap();
                client
                    .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                    .await
                    .unwrap();
                let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
            });
        }
    });

    port
}

#[tokio::test]
async fn logs_in_through_socks5_proxy() {
    if !fixture_enabled() {
        return;
    }

    let proxy_port = start_socks5_proxy().await;
    let mut config = fixture_config("alice@localhost");
    config.proxy_config = Some(ProxyConfig {
        proxy_type: ProxyType::Socks5,
        host: "127.0.0.1".to_string(),
        port: proxy_port,
        username: None,
        password: None,
    });

    let (connection, _events) = Connection::new(config.into());
    tokio::time::timeout(Duration::from_secs(20), connection.connect())
        .await
        .expect("connect timed out")
        .expect("proxied connect failed");
    assert!(connection.jid().is_some());
    connection.disconnect().await.expect("disconnect failed");
}

#[tokio::test]
async fn iq_router_receives_disco_reply() {
    if !fixture_enabled() {
        return;
    }

    let (connection, mut events) = Connection::new(fixture_config("alice@localhost").into());
    tokio::time::timeout(Duration::from_secs(20), connection.connect())
        .await
        .expect("connect timed out")
        .expect("connect failed");

    let router = Arc::new(IqRouter::new(Duration::from_secs(10)));
    let router_for_events = router.clone();
    let pump = tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            let ConnectionEvent::StanzaReceived(xml) = event else {
                continue;
            };
            let Ok(elem) = Element::from_str(&xml) else {
                continue;
            };
            if elem.name() == "iq" && elem.attr("type") == Some("result") {
                if let Some(id) = elem.attr("id") {
                    router_for_events.resolve(id, Ok(elem.clone()));
                }
            }
        }
    });

    let iq = Element::builder("iq", "jabber:client")
        .attr("type", "get")
        .attr("to", "localhost")
        .attr("id", "disco-router-test")
        .append(Element::builder(
            "query",
            "http://jabber.org/protocol/disco#info",
        ))
        .build();

    let reply = router
        .request(|stanza| connection.send_stanza_now(stanza.clone()), iq)
        .await
        .expect("no disco reply");
    assert!(reply
        .get_child("query", "http://jabber.org/protocol/disco#info")
        .is_some());

    pump.abort();
    connection.disconnect().await.expect("disconnect failed");
}

#[allow(clippy::large_futures)]
#[tokio::test(flavor = "current_thread")]
async fn archives_a_message_and_completes_the_page() {
    if !fixture_enabled() {
        return;
    }

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
                mam.clone(),
            );
            let processor_task =
                tokio::task::spawn_local(async move { processor.start(event_rx).await });

            tokio::time::timeout(Duration::from_secs(20), connection.connect())
                .await
                .expect("connect timed out")
                .expect("connect failed");

            let bob: jid::Jid = "bob@localhost".parse().unwrap();
            client
                .send_message(&bob, "archive me")
                .expect("send failed");

            tokio::time::sleep(Duration::from_secs(1)).await;
            client.query_mam(&bob, 1000).expect("MAM query failed");

            let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
            while !mam.is_complete("bob@localhost") {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "MAM page never completed"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            let chat = client.chats.get(&bob).expect("chat missing");
            assert!(chat
                .messages()
                .iter()
                .any(|message| message.body.as_deref() == Some("archive me")));

            processor_task.abort();
            connection.disconnect().await.expect("disconnect failed");
        })
        .await;
}

#[allow(clippy::large_futures)]
#[tokio::test(flavor = "current_thread")]
async fn blocklist_roundtrip() {
    if !fixture_enabled() {
        return;
    }

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
                roster: roster.clone(),
                chats,
                mucs,
                pubsub,
                spaces,
                disco,
                push,
                sm,
                omemo,
                events: events.clone(),
                iq,
                mam,
                jingle,
        conference: xmpp_core::ConferenceManager::new(),
                voice: Arc::new(xmpp_core::VoiceManager::new()),
            };

            let processor = EventProcessor::new(
                connection.clone(),
                roster.clone(),
                client.chats.clone(),
                client.mucs.clone(),
                client.pubsub.clone(),
                client.spaces.clone(),
                client.disco.clone(),
                client.push.clone(),
                client.sm.clone(),
                client.omemo.clone(),
                client.iq.clone(),
                Arc::new(MamManager::new()),
            );
            let processor_task = tokio::task::spawn_local(async move { processor.start(event_rx).await });

            let outbound_connection = connection.clone();
            let mut outbound = events.subscribe();
            let forwarder = tokio::task::spawn_local(async move {
                while let Ok(event) = outbound.recv().await {
                    if let XmppEvent::SendStanza(xml) = event {
                        let _ = outbound_connection.send_stanza_str(&xml);
                    }
                }
            });

            tokio::time::timeout(Duration::from_secs(20), connection.connect())
                .await
                .expect("connect timed out")
                .expect("connect failed");

            let bob: jid::Jid = "bob@localhost".parse().unwrap();
            roster.upsert(RosterItem::new(bob.clone()));

            connection
                .send_stanza_str(
                    "<iq type='set' id='block-direct' xmlns='jabber:client'><block xmlns='urn:xmpp:blocking'><item jid='bob@localhost'/></block></iq>",
                )
                .expect("block failed");

            roster.request_blocklist();
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            while roster.get(&bob).is_none_or(|item| !item.is_blocked) {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "blocklist did not report the block"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            connection
                .send_stanza_str(
                    "<iq type='set' id='unblock-direct' xmlns='jabber:client'><unblock xmlns='urn:xmpp:blocking'><item jid='bob@localhost'/></unblock></iq>",
                )
                .expect("unblock failed");
            roster.request_blocklist();
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            while roster.get(&bob).is_none_or(|item| item.is_blocked) {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "blocklist did not report the unblock"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            forwarder.abort();
            processor_task.abort();
            connection.disconnect().await.expect("disconnect failed");
        })
        .await;
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
        mam,
        jingle,
        conference: xmpp_core::ConferenceManager::new(),
        voice: Arc::new(xmpp_core::VoiceManager::new()),
    };
    (client, connection, event_rx)
}

#[allow(clippy::large_futures)]
#[tokio::test(flavor = "current_thread")]
async fn two_clients_exchange_a_message_without_duplicates() {
    if !fixture_enabled() {
        return;
    }

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let alice_events = Arc::new(EventBus::new());
            let bob_events = Arc::new(EventBus::new());
            let (alice, alice_connection, alice_rx) = build_client("alice@localhost", alice_events);
            let (bob, bob_connection, bob_rx) = build_client("bob@localhost", bob_events);

            let alice_processor = EventProcessor::new(
                alice_connection.clone(),
                alice.roster.clone(),
                alice.chats.clone(),
                alice.mucs.clone(),
                alice.pubsub.clone(),
                alice.spaces.clone(),
                alice.disco.clone(),
                alice.push.clone(),
                alice.sm.clone(),
                alice.omemo.clone(),
                alice.iq.clone(),
                alice.mam.clone(),
            );
            let bob_processor = EventProcessor::new(
                bob_connection.clone(),
                bob.roster.clone(),
                bob.chats.clone(),
                bob.mucs.clone(),
                bob.pubsub.clone(),
                bob.spaces.clone(),
                bob.disco.clone(),
                bob.push.clone(),
                bob.sm.clone(),
                bob.omemo.clone(),
                bob.iq.clone(),
                bob.mam.clone(),
            );
            let alice_task =
                tokio::task::spawn_local(async move { alice_processor.start(alice_rx).await });
            let bob_task =
                tokio::task::spawn_local(async move { bob_processor.start(bob_rx).await });

            tokio::time::timeout(Duration::from_secs(20), alice_connection.connect())
                .await
                .expect("alice connect timed out")
                .expect("alice connect failed");
            tokio::time::timeout(Duration::from_secs(20), bob_connection.connect())
                .await
                .expect("bob connect timed out")
                .expect("bob connect failed");

            let alice_jid: jid::Jid = "alice@localhost".parse().unwrap();
            let bob_jid: jid::Jid = "bob@localhost".parse().unwrap();
            let body = format!(
                "hello {}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            bob.send_message(&alice_jid, &body).expect("send failed");

            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            loop {
                let received = alice
                    .chats
                    .get_by_bare("bob@localhost")
                    .map_or(0, |chat| chat.message_count());
                if received > 0 {
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "alice never received the message"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            alice.query_mam(&bob_jid, 1000).expect("MAM query failed");
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            while !alice.mam.is_complete("bob@localhost") {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "alice MAM page never completed"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            let chat = alice.chats.get_by_bare("bob@localhost").unwrap();
            let matching = chat
                .messages()
                .iter()
                .filter(|message| message.body.as_deref() == Some(body.as_str()))
                .count();
            assert_eq!(matching, 1, "live and archived copies must deduplicate");

            let forward_body = format!(
                "forward {}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let original = bob
                .send_message(&alice_jid, &forward_body)
                .expect("send failed");
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            while alice
                .chats
                .get_by_bare("bob@localhost")
                .is_none_or(|chat| chat.find_message(&original.id).is_none())
            {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "alice never received the message to forward"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            alice
                .send_forward(&bob_jid, &original)
                .expect("forward failed");
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            loop {
                let forwarded = bob.chats.get_by_bare("alice@localhost").and_then(|chat| {
                    chat.messages().into_iter().find(|message| {
                        message.body.as_deref() == Some(forward_body.as_str())
                            && message.forward_info.is_some()
                    })
                });
                if let Some(message) = forwarded {
                    assert_eq!(
                        message
                            .forward_info
                            .unwrap()
                            .original_jid
                            .to_bare()
                            .to_string(),
                        "bob@localhost"
                    );
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "bob never received the forwarded message"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            alice.send_attention(&bob_jid).expect("attention failed");
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            loop {
                let attention = bob
                    .chats
                    .get_by_bare("alice@localhost")
                    .is_some_and(|chat| chat.messages().iter().any(|message| message.attention));
                if attention {
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "bob never received the attention request"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            let sticker_bytes = b"sticker-test-bytes";
            let sticker_cid = xmpp_core::bob::cid_for_bytes(sticker_bytes);
            alice
                .send_sticker(&bob_jid, sticker_bytes, "image/png", "Test sticker")
                .expect("sticker send failed");
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            loop {
                let received = bob
                    .chats
                    .get_by_bare("alice@localhost")
                    .is_some_and(|chat| {
                        chat.messages().iter().any(|message| {
                            message.sticker_cid.as_deref() == Some(sticker_cid.as_str())
                        })
                    });
                if received {
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "bob never received the sticker"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            assert!(xmpp_core::bob::data_uri(&sticker_cid).is_some());

            alice_task.abort();
            bob_task.abort();
            alice_connection
                .disconnect()
                .await
                .expect("alice disconnect");
            bob_connection.disconnect().await.expect("bob disconnect");
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn configures_and_destroys_a_room() {
    if !fixture_enabled() {
        return;
    }

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let (alice, connection, rx) =
                build_client("alice@localhost", Arc::new(EventBus::new()));
            let processor = EventProcessor::new(
                connection.clone(),
                alice.roster.clone(),
                alice.chats.clone(),
                alice.mucs.clone(),
                alice.pubsub.clone(),
                alice.spaces.clone(),
                alice.disco.clone(),
                alice.push.clone(),
                alice.sm.clone(),
                alice.omemo.clone(),
                alice.iq.clone(),
                alice.mam.clone(),
            );
            let task = tokio::task::spawn_local(async move { processor.start(rx).await });

            tokio::time::timeout(Duration::from_secs(20), connection.connect())
                .await
                .expect("connect timed out")
                .expect("connect failed");

            let room: jid::Jid = format!(
                "config-{}@conference.localhost",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )
            .parse()
            .unwrap();
            let room_bare = room.to_bare().to_string();
            alice.create_room(&room, "alice").expect("create failed");

            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            while !alice
                .mucs
                .get_by_bare(&room_bare)
                .is_some_and(|room| room.is_joined())
            {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "alice never joined the created room"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            let form = alice
                .room_config(&room)
                .await
                .expect("room config fetch failed");
            assert!(!form.fields.is_empty(), "room config form was empty");
            let values: Vec<(String, String)> = form
                .fields
                .iter()
                .map(|field| (field.var.clone(), field.value.clone()))
                .collect();
            alice
                .submit_room_config(&room, form.form.as_ref(), &values)
                .await
                .expect("room config submit failed");

            alice
                .destroy_room(&room, Some("test finished"))
                .await
                .expect("room destroy failed");
            assert!(alice.mucs.get_by_bare(&room_bare).is_none());

            task.abort();
            connection.disconnect().await.expect("disconnect failed");
        })
        .await;
}

#[allow(clippy::large_futures, clippy::too_many_lines)]
#[tokio::test(flavor = "current_thread")]
async fn joins_a_room_and_exchanges_a_group_message() {
    if !fixture_enabled() {
        return;
    }

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let (alice, alice_connection, alice_rx) =
                build_client("alice@localhost", Arc::new(EventBus::new()));
            let (bob, bob_connection, bob_rx) =
                build_client("bob@localhost", Arc::new(EventBus::new()));

            let alice_processor = EventProcessor::new(
                alice_connection.clone(),
                alice.roster.clone(),
                alice.chats.clone(),
                alice.mucs.clone(),
                alice.pubsub.clone(),
                alice.spaces.clone(),
                alice.disco.clone(),
                alice.push.clone(),
                alice.sm.clone(),
                alice.omemo.clone(),
                alice.iq.clone(),
                alice.mam.clone(),
            );
            let bob_processor = EventProcessor::new(
                bob_connection.clone(),
                bob.roster.clone(),
                bob.chats.clone(),
                bob.mucs.clone(),
                bob.pubsub.clone(),
                bob.spaces.clone(),
                bob.disco.clone(),
                bob.push.clone(),
                bob.sm.clone(),
                bob.omemo.clone(),
                bob.iq.clone(),
                bob.mam.clone(),
            );
            let alice_task =
                tokio::task::spawn_local(async move { alice_processor.start(alice_rx).await });
            let bob_task =
                tokio::task::spawn_local(async move { bob_processor.start(bob_rx).await });

            tokio::time::timeout(Duration::from_secs(20), alice_connection.connect())
                .await
                .expect("alice connect timed out")
                .expect("alice connect failed");
            tokio::time::timeout(Duration::from_secs(20), bob_connection.connect())
                .await
                .expect("bob connect timed out")
                .expect("bob connect failed");

            let room: jid::Jid = format!(
                "room-{}@conference.localhost",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )
            .parse()
            .unwrap();
            let room_bare = room.to_bare().to_string();

            alice
                .join_room(&room, "alice", None)
                .expect("alice join failed");
            bob.join_room(&room, "bob", None).expect("bob join failed");

            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            loop {
                let joined = alice
                    .mucs
                    .get_by_bare(&room_bare)
                    .is_some_and(|room| room.is_joined());
                if joined {
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "alice never joined the room"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            alice
                .send_muc_message(&room, "alice", "hello room")
                .expect("MUC send failed");

            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            loop {
                let received = bob.mucs.get_by_bare(&room_bare).is_some_and(|room| {
                    room.messages()
                        .iter()
                        .any(|message| message.body.as_deref() == Some("hello room"))
                });
                if received {
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "bob never received the room message"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            let alice_room = alice.mucs.get_by_bare(&room_bare).unwrap();
            assert!(alice_room
                .messages()
                .iter()
                .any(|message| message.is_own && message.body.as_deref() == Some("hello room")));
            assert_eq!(alice_room.unread_count(), 0);

            alice
                .send_muc_message(&room, "alice", "hi @bob")
                .expect("MUC mention send failed");
            let mention_uri = format!("xmpp:{room_bare}/bob");
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            loop {
                let mentioned = bob.mucs.get_by_bare(&room_bare).is_some_and(|room| {
                    room.messages().iter().any(|message| {
                        message.body.as_deref() == Some("hi @bob")
                            && message.mentions.contains(&mention_uri)
                    })
                });
                if mentioned {
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "bob never received the room mention"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            let reaction_target = bob
                .send_muc_message(&room, "bob", "react to this")
                .expect("reaction target send failed");
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            while alice
                .mucs
                .get_by_bare(&room_bare)
                .is_none_or(|room| room.find_message(&reaction_target.id).is_none())
            {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "alice never received the reaction target"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            alice
                .send_muc_reaction(&room, &reaction_target.id, &["👍"])
                .expect("room reaction failed");
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            loop {
                let reacted = bob.mucs.get_by_bare(&room_bare).is_some_and(|room| {
                    room.find_message(&reaction_target.id)
                        .is_some_and(|message| {
                            message.reactions.iter().any(|reaction| {
                                reaction.emoji == "👍" && reaction.from_nick == "alice"
                            })
                        })
                });
                if reacted {
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "bob never received the room reaction"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            alice
                .send_muc_reaction(&room, &reaction_target.id, &[])
                .expect("room reaction removal failed");
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            loop {
                let removed = bob.mucs.get_by_bare(&room_bare).is_some_and(|room| {
                    room.find_message(&reaction_target.id)
                        .is_some_and(|message| message.reactions.is_empty())
                });
                if removed {
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "bob never received the reaction removal"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            alice_task.abort();
            bob_task.abort();
            alice_connection
                .disconnect()
                .await
                .expect("alice disconnect");
            bob_connection.disconnect().await.expect("bob disconnect");
        })
        .await;
}

#[allow(clippy::large_futures)]
#[tokio::test(flavor = "current_thread")]
async fn a_restored_room_is_rejoined_and_receives_messages() {
    if !fixture_enabled() {
        return;
    }

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let (alice, alice_connection, alice_rx) =
                build_client("alice@localhost", Arc::new(EventBus::new()));
            let (bob, bob_connection, bob_rx) =
                build_client("bob@localhost", Arc::new(EventBus::new()));

            let alice_processor = EventProcessor::new(
                alice_connection.clone(),
                alice.roster.clone(),
                alice.chats.clone(),
                alice.mucs.clone(),
                alice.pubsub.clone(),
                alice.spaces.clone(),
                alice.disco.clone(),
                alice.push.clone(),
                alice.sm.clone(),
                alice.omemo.clone(),
                alice.iq.clone(),
                alice.mam.clone(),
            );
            let bob_processor = EventProcessor::new(
                bob_connection.clone(),
                bob.roster.clone(),
                bob.chats.clone(),
                bob.mucs.clone(),
                bob.pubsub.clone(),
                bob.spaces.clone(),
                bob.disco.clone(),
                bob.push.clone(),
                bob.sm.clone(),
                bob.omemo.clone(),
                bob.iq.clone(),
                bob.mam.clone(),
            );
            let alice_task =
                tokio::task::spawn_local(async move { alice_processor.start(alice_rx).await });
            let bob_task =
                tokio::task::spawn_local(async move { bob_processor.start(bob_rx).await });

            tokio::time::timeout(Duration::from_secs(20), alice_connection.connect())
                .await
                .expect("alice connect timed out")
                .expect("alice connect failed");
            tokio::time::timeout(Duration::from_secs(20), bob_connection.connect())
                .await
                .expect("bob connect timed out")
                .expect("bob connect failed");

            let room: jid::Jid = format!(
                "room-{}@conference.localhost",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )
            .parse()
            .unwrap();
            let room_bare = room.to_bare().to_string();

            alice
                .join_room(&room, "alice", None)
                .expect("alice join failed");
            bob.join_room(&room, "bob", None).expect("bob join failed");
            for client in [&alice, &bob] {
                let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
                while !client
                    .mucs
                    .get_by_bare(&room_bare)
                    .is_some_and(|room| room.is_joined())
                {
                    assert!(
                        tokio::time::Instant::now() < deadline,
                        "a client never joined the room"
                    );
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }

            // The room works while both clients are in it.
            alice
                .send_muc_message(&room, "alice", "before restart")
                .expect("MUC send failed");
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            while bob.mucs.get_by_bare(&room_bare).is_none_or(|room| {
                !room
                    .messages()
                    .iter()
                    .any(|message| message.body.as_deref() == Some("before restart"))
            }) {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "bob never received the first room message"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            // What the UI sees after a restart: the room is restored from
            // storage (metadata and nick, no presence). The old code only sent
            // presence when the room object was missing entirely, so opening a
            // restored room showed local echoes and received nothing.
            bob.mucs
                .get_by_bare(&room_bare)
                .expect("bob's room")
                .leave(None)
                .expect("leave failed");
            let restored = bob.mucs.get_by_bare(&room_bare).expect("bob's room");
            assert!(!restored.is_joined());
            assert!(restored.needs_join(), "a restored room has to rejoin");

            assert!(
                bob.ensure_room_joined_str(&room_bare, "bob")
                    .expect("rejoin failed"),
                "opening a restored room did not send presence"
            );
            // A second open while the join is in flight must not send again.
            assert!(
                !bob.ensure_room_joined_str(&room_bare, "bob")
                    .expect("second rejoin failed"),
                "a room that is joining was joined twice"
            );
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            while !bob
                .mucs
                .get_by_bare(&room_bare)
                .is_some_and(|room| room.is_joined())
            {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "bob never rejoined the restored room"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            alice
                .send_muc_message(&room, "alice", "after restart")
                .expect("MUC send failed");
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            while bob.mucs.get_by_bare(&room_bare).is_none_or(|room| {
                !room
                    .messages()
                    .iter()
                    .any(|message| message.body.as_deref() == Some("after restart"))
            }) {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "bob never received a message after rejoining"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            alice_task.abort();
            bob_task.abort();
            alice_connection
                .disconnect()
                .await
                .expect("alice disconnect");
            bob_connection.disconnect().await.expect("bob disconnect");
        })
        .await;
}

#[allow(clippy::large_futures)]
#[tokio::test(flavor = "current_thread")]
async fn uploads_a_file_through_the_discovered_service() {
    if !fixture_enabled() {
        return;
    }

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let events = Arc::new(EventBus::new());
            let (client, connection, event_rx) = build_client("alice@localhost", events);
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
                client.mam.clone(),
            );
            let processor_task =
                tokio::task::spawn_local(async move { processor.start(event_rx).await });

            tokio::time::timeout(Duration::from_secs(20), connection.connect())
                .await
                .expect("connect timed out")
                .expect("connect failed");

            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            while client.disco.upload_service().is_none() {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "upload service was never discovered"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            let slot = client
                .upload_file("hello.txt", b"hello from the test".to_vec(), "text/plain")
                .await
                .expect("upload failed");
            assert!(slot.get_url.starts_with("http"));

            processor_task.abort();
            connection.disconnect().await.expect("disconnect failed");
        })
        .await;
}

#[tokio::test]
async fn registers_an_account_and_logs_in_with_it() {
    if !fixture_enabled() {
        return;
    }

    let username = format!(
        "reg{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let password = "registered-secret";

    let mut config = fixture_config("registration@localhost");
    config.jid = "registration@localhost".into();
    let (registration, _events) = Connection::new(config.into());
    registration
        .register_account(&username, password)
        .await
        .expect("registration failed");

    let jid = format!("{username}@localhost");
    let mut login = fixture_config(&jid);
    login.password = Some(password.to_string());
    let (connection, _events) = Connection::new(login.into());
    tokio::time::timeout(Duration::from_secs(20), connection.connect())
        .await
        .expect("connect timed out")
        .expect("registered account could not log in");
    assert_eq!(
        connection.jid().map(|jid| jid.to_string()),
        Some(format!("{jid}/jabu-smoke"))
    );
    connection.disconnect().await.expect("disconnect failed");
}

#[allow(clippy::large_futures)]
#[tokio::test(flavor = "current_thread")]
async fn vcard_updates_merge_on_the_server() {
    if !fixture_enabled() {
        return;
    }

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let events = Arc::new(EventBus::new());
            let (client, connection, event_rx) = build_client("alice@localhost", events);
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
                client.mam.clone(),
            );
            let processor_task =
                tokio::task::spawn_local(async move { processor.start(event_rx).await });

            tokio::time::timeout(Duration::from_secs(20), connection.connect())
                .await
                .expect("connect timed out")
                .expect("connect failed");

            let own: jid::Jid = "alice@localhost".parse().unwrap();
            client
                .update_vcard(
                    Element::from_str(
                        r#"<vCard xmlns="vcard-temp"><FN>Alice Test</FN><ORG>Jabu</ORG></vCard>"#,
                    )
                    .unwrap(),
                )
                .expect("vCard update failed");
            client.request_own_vcard().expect("vCard fetch failed");

            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            loop {
                let stored = client
                    .roster
                    .get(&own)
                    .and_then(|item| item.vcard)
                    .unwrap_or_default();
                if stored.contains("Jabu") {
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "server never stored the vCard"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            client
                .update_vcard(
                    Element::from_str(
                        r#"<vCard xmlns="vcard-temp"><FN>Alice Renamed</FN></vCard>"#,
                    )
                    .unwrap(),
                )
                .expect("second vCard update failed");
            client.request_own_vcard().expect("vCard fetch failed");

            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            loop {
                let stored = client
                    .roster
                    .get(&own)
                    .and_then(|item| item.vcard)
                    .unwrap_or_default();
                if stored.contains("Alice Renamed") && stored.contains("Jabu") {
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "vCard merge dropped a field"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            processor_task.abort();
            connection.disconnect().await.expect("disconnect failed");
        })
        .await;
}

#[allow(clippy::large_futures)]
#[tokio::test(flavor = "current_thread")]
async fn queries_last_activity_for_an_offline_contact() {
    if !fixture_enabled() {
        return;
    }

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let events = Arc::new(EventBus::new());
            let (client, connection, event_rx) = build_client("alice@localhost", events);
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
                client.mam.clone(),
            );
            let processor_task =
                tokio::task::spawn_local(async move { processor.start(event_rx).await });

            tokio::time::timeout(Duration::from_secs(20), connection.connect())
                .await
                .expect("connect timed out")
                .expect("connect failed");

            let server: jid::Jid = "localhost".parse().unwrap();
            client
                .request_last_activity(&server)
                .expect("last activity request failed");

            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            while client
                .roster
                .get(&server)
                .and_then(|item| item.last_seen)
                .is_none()
            {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "last activity was never recorded"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            processor_task.abort();
            connection.disconnect().await.expect("disconnect failed");
        })
        .await;
}

#[allow(clippy::large_futures)]
#[tokio::test(flavor = "current_thread")]
async fn publishes_and_fetches_public_subscriptions() {
    if !fixture_enabled() {
        return;
    }

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let (alice, alice_connection, alice_rx) =
                build_client("alice@localhost", Arc::new(EventBus::new()));
            let processor = EventProcessor::new(
                alice_connection.clone(),
                alice.roster.clone(),
                alice.chats.clone(),
                alice.mucs.clone(),
                alice.pubsub.clone(),
                alice.spaces.clone(),
                alice.disco.clone(),
                alice.push.clone(),
                alice.sm.clone(),
                alice.omemo.clone(),
                alice.iq.clone(),
                alice.mam.clone(),
            );
            let task = tokio::task::spawn_local(async move { processor.start(alice_rx).await });

            tokio::time::timeout(Duration::from_secs(20), alice_connection.connect())
                .await
                .expect("connect timed out")
                .expect("connect failed");

            alice
                .publish_subscription(
                    "pubsub.localhost",
                    "party",
                    Some("Party at the Capulets"),
                    Some("mentions"),
                    true,
                )
                .await
                .expect("publish failed");

            let own: jid::Jid = "alice@localhost".parse().expect("jid");
            let entries = alice.fetch_subscriptions(&own).await.expect("fetch failed");
            let entry = entries
                .iter()
                .find(|entry| entry.node == "party")
                .expect("published entry");
            assert_eq!(entry.title.as_deref(), Some("Party at the Capulets"));
            assert_eq!(entry.notify.as_deref(), Some("mentions"));
            assert!(entry.pinned);

            alice
                .retract_subscription("pubsub.localhost", "party")
                .await
                .expect("retract failed");
            let entries = alice
                .fetch_subscriptions(&own)
                .await
                .expect("fetch after retract failed");
            assert!(entries.iter().all(|entry| entry.node != "party"));

            task.abort();
            alice_connection
                .disconnect()
                .await
                .expect("disconnect failed");
        })
        .await;
}

#[allow(clippy::large_futures)]
#[tokio::test(flavor = "current_thread")]
async fn publishes_and_fetches_a_story() {
    if !fixture_enabled() {
        return;
    }

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let (alice, alice_connection, alice_rx) =
                build_client("alice@localhost", Arc::new(EventBus::new()));
            let processor = EventProcessor::new(
                alice_connection.clone(),
                alice.roster.clone(),
                alice.chats.clone(),
                alice.mucs.clone(),
                alice.pubsub.clone(),
                alice.spaces.clone(),
                alice.disco.clone(),
                alice.push.clone(),
                alice.sm.clone(),
                alice.omemo.clone(),
                alice.iq.clone(),
                alice.mam.clone(),
            );
            let task = tokio::task::spawn_local(async move { processor.start(alice_rx).await });

            tokio::time::timeout(Duration::from_secs(20), alice_connection.connect())
                .await
                .expect("connect timed out")
                .expect("connect failed");

            let media = xmpp_core::MediaAttachment {
                id: "story-media".to_string(),
                url: "https://files.example/sunset.jpg".to_string(),
                media_type: xmpp_core::MediaType::Image,
                mime: Some("image/jpeg".to_string()),
                name: Some("sunset.jpg".to_string()),
                size: Some(1024),
                width: None,
                height: None,
                duration: None,
                thumbnail_url: None,
                blur_hash: None,
            };
            let story = alice
                .publish_story("sunset #sky", &media)
                .expect("story publish failed");

            let own: jid::Jid = "alice@localhost".parse().expect("jid");
            alice.fetch_stories(&own).expect("story fetch failed");

            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            loop {
                let cached = alice
                    .pubsub
                    .get(&own)
                    .and_then(|service| service.get_node(xmpp_core::STORIES_NODE))
                    .and_then(|node| node.get_item(&story.id));
                if let Some(item) = cached {
                    assert_eq!(item.body.as_deref(), Some("sunset #sky"));
                    assert_eq!(item.tags, vec!["sky".to_string()]);
                    assert_eq!(item.media.len(), 1);
                    assert_eq!(item.media[0].media_type, xmpp_core::MediaType::Image);
                    assert!(item.expires_at.is_some());
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "story was not fetched"
                );
                tokio::time::sleep(Duration::from_millis(200)).await;
            }

            alice
                .retract_story(&story.id)
                .expect("story retract failed");
            task.abort();
            alice_connection
                .disconnect()
                .await
                .expect("disconnect failed");
        })
        .await;
}

#[allow(clippy::large_futures)]
#[tokio::test(flavor = "current_thread")]
async fn publishes_and_fetches_pep_activity() {
    if !fixture_enabled() {
        return;
    }

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let (alice, alice_connection, alice_rx) =
                build_client("alice@localhost", Arc::new(EventBus::new()));
            let (bob, bob_connection, bob_rx) =
                build_client("bob@localhost", Arc::new(EventBus::new()));
            let alice_processor = EventProcessor::new(
                alice_connection.clone(),
                alice.roster.clone(),
                alice.chats.clone(),
                alice.mucs.clone(),
                alice.pubsub.clone(),
                alice.spaces.clone(),
                alice.disco.clone(),
                alice.push.clone(),
                alice.sm.clone(),
                alice.omemo.clone(),
                alice.iq.clone(),
                alice.mam.clone(),
            );
            let bob_processor = EventProcessor::new(
                bob_connection.clone(),
                bob.roster.clone(),
                bob.chats.clone(),
                bob.mucs.clone(),
                bob.pubsub.clone(),
                bob.spaces.clone(),
                bob.disco.clone(),
                bob.push.clone(),
                bob.sm.clone(),
                bob.omemo.clone(),
                bob.iq.clone(),
                bob.mam.clone(),
            );
            let alice_task =
                tokio::task::spawn_local(async move { alice_processor.start(alice_rx).await });
            let bob_task =
                tokio::task::spawn_local(async move { bob_processor.start(bob_rx).await });

            tokio::time::timeout(Duration::from_secs(20), alice_connection.connect())
                .await
                .expect("alice connect timed out")
                .expect("alice connect failed");
            tokio::time::timeout(Duration::from_secs(20), bob_connection.connect())
                .await
                .expect("bob connect timed out")
                .expect("bob connect failed");

            alice
                .publish_tune(Some("Artist"), Some("Title"), None, Some(180))
                .expect("tune publish failed");
            alice
                .update_vcard4(
                    VCard4 {
                        full_name: Some("Alice Four".into()),
                        organization: Some("Jabu".into()),
                        email: Some("alice@example.org".into()),
                        ..VCard4::default()
                    }
                    .to_element(),
                )
                .expect("vCard4 publish failed");

            let alice_jid: jid::Jid = "alice@localhost".parse().unwrap();
            tokio::time::sleep(Duration::from_millis(500)).await;
            bob.fetch_user_activity(&alice_jid)
                .expect("activity fetch failed");

            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            loop {
                let item = bob.roster.get(&alice_jid);
                let tune = item
                    .as_ref()
                    .and_then(|item| item.tune.as_deref())
                    .unwrap_or_default();
                let vcard4 = item
                    .as_ref()
                    .and_then(|item| item.vcard4.as_deref())
                    .unwrap_or_default();
                let email = item.as_ref().and_then(|item| item.vcard4_field("EMAIL"));
                if tune.contains("Artist")
                    && vcard4.contains("Alice Four")
                    && email.as_deref() == Some("alice@example.org")
                {
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "PEP activity was never received"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            alice_task.abort();
            bob_task.abort();
            alice_connection
                .disconnect()
                .await
                .expect("alice disconnect");
            bob_connection.disconnect().await.expect("bob disconnect");
        })
        .await;
}

#[allow(clippy::large_futures)]
#[tokio::test(flavor = "current_thread")]
async fn lists_admin_adhoc_commands() {
    if !fixture_enabled() {
        return;
    }

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let events = Arc::new(EventBus::new());
            let (client, connection, event_rx) = build_client("admin@localhost", events);
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
                client.mam.clone(),
            );
            let processor_task =
                tokio::task::spawn_local(async move { processor.start(event_rx).await });

            tokio::time::timeout(Duration::from_secs(20), connection.connect())
                .await
                .expect("connect timed out")
                .expect("connect failed");

            let server: jid::Jid = "localhost".parse().unwrap();
            let commands = client
                .list_commands(&server)
                .await
                .expect("command list failed");
            assert!(
                !commands.is_empty(),
                "Prosody admin_adhoc should advertise commands"
            );
            let command = commands
                .iter()
                .find(|command| command.node.ends_with("#get-online-users-list"))
                .expect("get-online-users-list command");
            let response = client
                .execute_command(&server, &command.node, None)
                .await
                .expect("command execution failed");
            let response = if response.status == xmpp_core::AdHocStatus::Executing {
                let payload = response.form.as_ref().map(|form| {
                    let pairs: Vec<(String, String)> = form
                        .fields
                        .iter()
                        .map(|field| (field.var.clone(), field.value.clone()))
                        .collect();
                    xmpp_core::data_form::submit(form.form.as_ref(), &pairs)
                });
                client
                    .run_command(
                        &server,
                        &command.node,
                        response.sessionid.as_deref(),
                        response.default_action(),
                        payload,
                    )
                    .await
                    .expect("command continuation failed")
            } else {
                response
            };
            assert_eq!(response.status, xmpp_core::AdHocStatus::Completed);

            processor_task.abort();
            connection.disconnect().await.expect("disconnect failed");
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn discovers_gateways_without_error() {
    if !fixture_enabled() {
        return;
    }

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let (client, connection, event_rx) =
                build_client("alice@localhost", Arc::new(EventBus::new()));
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
                client.mam.clone(),
            );
            let processor_task =
                tokio::task::spawn_local(async move { processor.start(event_rx).await });

            tokio::time::timeout(Duration::from_secs(20), connection.connect())
                .await
                .expect("connect timed out")
                .expect("connect failed");

            let gateways = client
                .list_gateways()
                .await
                .expect("gateway discovery failed");
            assert!(gateways.iter().all(|gateway| gateway.kind != "unknown"));

            processor_task.abort();
            connection.disconnect().await.expect("disconnect failed");
        })
        .await;
}

#[tokio::test]
async fn stream_management_offers_resumption() {
    if !fixture_enabled() {
        return;
    }

    let sm = Arc::new(StreamManagement::new());
    let (connection, mut events) = Connection::new(fixture_config("alice@localhost").into());
    connection.set_stream_management(sm.clone());

    tokio::time::timeout(Duration::from_secs(20), connection.connect())
        .await
        .expect("connect timed out")
        .expect("connect failed");

    connection
        .send_stanza(
            Element::builder("enable", "urn:xmpp:sm:3")
                .attr("resume", "true")
                .build(),
        )
        .await
        .expect("failed to request stream management");

    let enabled = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(event) = events.recv().await {
            let ConnectionEvent::StanzaReceived(xml) = event else {
                continue;
            };
            let Ok(elem) = Element::from_str(&xml) else {
                continue;
            };
            if elem.is("enabled", "urn:xmpp:sm:3") {
                return Some(elem);
            }
        }
        None
    })
    .await
    .expect("no stream-management response")
    .expect("connection ended");

    let id = enabled.attr("id").map(str::to_string);
    let resume = enabled.attr("resume") == Some("true");
    let max = enabled
        .attr("max")
        .and_then(|value| value.parse::<u32>().ok());
    sm.enable(id, resume, max);

    assert!(sm.details().and_then(|details| details.id).is_some());
    assert!(sm.resume_request().is_some());

    connection.disconnect().await.expect("disconnect failed");
}

#[allow(clippy::large_futures)]
#[tokio::test(flavor = "current_thread")]
async fn creates_a_space_on_the_discovered_pubsub_service() {
    if !fixture_enabled() {
        return;
    }

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let events = Arc::new(EventBus::new());
            let (client, connection, rx) = build_client("alice@localhost", events);
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
                client.mam.clone(),
            );
            let processor_task = tokio::task::spawn_local(async move { processor.start(rx).await });

            tokio::time::timeout(Duration::from_secs(20), connection.connect())
                .await
                .expect("connect timed out")
                .expect("connect failed");

            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            while client.spaces_service().is_none() {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "no spaces service was discovered"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            let service = client
                .resolve_spaces_service()
                .await
                .expect("spaces service resolution failed");
            assert_eq!(service.to_string(), "pubsub.localhost");

            let title = format!(
                "Space {}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let id = client
                .create_space(&service, &title, false)
                .await
                .expect("space creation failed");
            client
                .publish_space_avatar(
                    &id,
                    "https://files.example/avatar.jpg",
                    "image/jpeg",
                    b"avatar",
                    Some(64),
                    Some(64),
                )
                .await
                .expect("avatar publish failed");
            client
                .publish_space_banner(
                    &id,
                    "https://files.example/banner.jpg",
                    "image/jpeg",
                    b"banner",
                    None,
                    None,
                )
                .await
                .expect("banner publish failed");
            let directory = xmpp_core::spaces::SpaceDirectory {
                id: "general".to_string(),
                name: "General".to_string(),
                order: 0,
            };
            client
                .save_space_directories(&id, &[directory])
                .await
                .expect("directory save failed");
            let room = xmpp_core::spaces::SpaceRoom {
                conference: "general@conference.localhost".to_string(),
                name: "General".to_string(),
                directory_id: Some("general".to_string()),
                autojoin: true,
                ..xmpp_core::spaces::SpaceRoom::default()
            };
            client
                .save_space_room(&id, &room)
                .await
                .expect("room save failed");
            client.fetch_space_info(&id).await.expect("info failed");
            client.fetch_space_items(&id).await.expect("items failed");

            let stored = client.spaces.get(&id).expect("space missing");
            assert_eq!(stored.name, title);
            assert_eq!(
                stored.avatar.as_deref(),
                Some("https://files.example/avatar.jpg")
            );
            assert_eq!(
                stored.banner.as_deref(),
                Some("https://files.example/banner.jpg")
            );
            assert!(stored.directories.iter().any(|entry| entry.id == "general"));
            assert!(stored.rooms.iter().any(|entry| {
                entry.conference == "general@conference.localhost"
                    && entry.directory_id.as_deref() == Some("general")
            }));
            client
                .delete_space_room(&id, "general@conference.localhost")
                .await
                .expect("room delete failed");
            assert!(client
                .spaces
                .get(&id)
                .expect("space missing")
                .rooms
                .is_empty());

            client.destroy_space(&id).await.expect("destroy failed");
            assert!(client.spaces.get(&id).is_none());

            processor_task.abort();
            connection.disconnect().await.expect("disconnect failed");
        })
        .await;
}

#[allow(clippy::large_futures)]
#[tokio::test(flavor = "current_thread")]
async fn refetches_the_rooms_of_a_known_space_on_connect() {
    if !fixture_enabled() {
        return;
    }

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();

            // A first session publishes the rooms, as the owner of the space.
            let events = Arc::new(EventBus::new());
            let (owner, connection, rx) = build_client("alice@localhost", events);
            let processor = EventProcessor::new(
                connection.clone(),
                owner.roster.clone(),
                owner.chats.clone(),
                owner.mucs.clone(),
                owner.pubsub.clone(),
                owner.spaces.clone(),
                owner.disco.clone(),
                owner.push.clone(),
                owner.sm.clone(),
                owner.omemo.clone(),
                owner.iq.clone(),
                owner.mam.clone(),
            );
            let processor_task = tokio::task::spawn_local(async move { processor.start(rx).await });
            tokio::time::timeout(Duration::from_secs(20), connection.connect())
                .await
                .expect("connect timed out")
                .expect("connect failed");
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            while owner.spaces_service().is_none() {
                assert!(tokio::time::Instant::now() < deadline, "no spaces service");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            let service = owner
                .resolve_spaces_service()
                .await
                .expect("spaces service resolution failed");
            let id = owner
                .create_space(&service, &format!("Rooms {stamp}"), false)
                .await
                .expect("space creation failed");
            for (kind, conference) in [
                (
                    xmpp_core::spaces::SpaceRoomKind::Text,
                    format!("text-{stamp}@conference.localhost"),
                ),
                (
                    xmpp_core::spaces::SpaceRoomKind::Voice,
                    format!("voice-{stamp}@conference.localhost"),
                ),
            ] {
                let room = xmpp_core::spaces::SpaceRoom {
                    conference: conference.clone(),
                    name: conference,
                    kind,
                    ..xmpp_core::spaces::SpaceRoom::default()
                };
                owner
                    .save_space_room(&id, &room)
                    .await
                    .expect("room publish failed");
            }
            processor_task.abort();
            connection.disconnect().await.expect("disconnect failed");

            // A later session only knows the node, as it does from storage. The
            // rooms have to come back without anyone publishing again.
            let events = Arc::new(EventBus::new());
            let (client, connection, rx) = build_client("alice@localhost", events);
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
                client.mam.clone(),
            );
            let processor_task = tokio::task::spawn_local(async move { processor.start(rx).await });
            client.spaces.upsert(xmpp_core::spaces::Space {
                id: id.clone(),
                name: format!("Rooms {stamp}"),
                ..xmpp_core::spaces::Space::default()
            });
            tokio::time::timeout(Duration::from_secs(20), connection.connect())
                .await
                .expect("connect timed out")
                .expect("connect failed");
            client.refresh_space_rooms().await;

            let stored = client.spaces.get(&id).expect("space missing");
            assert_eq!(
                stored.rooms.len(),
                2,
                "rooms not refetched: {:?}",
                stored.rooms
            );
            assert!(stored
                .rooms
                .iter()
                .any(|room| room.conference == format!("voice-{stamp}@conference.localhost")));

            client.destroy_space(&id).await.expect("destroy failed");
            processor_task.abort();
            connection.disconnect().await.expect("disconnect failed");
        })
        .await;
}

#[allow(clippy::large_futures)]
#[tokio::test(flavor = "current_thread")]
async fn a_room_notification_keeps_the_other_rooms() {
    if !fixture_enabled() {
        return;
    }

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();

            let events = Arc::new(EventBus::new());
            let (client, connection, rx) = build_client("alice@localhost", events);
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
                client.mam.clone(),
            );
            let processor_task = tokio::task::spawn_local(async move { processor.start(rx).await });
            tokio::time::timeout(Duration::from_secs(20), connection.connect())
                .await
                .expect("connect timed out")
                .expect("connect failed");
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            while client.spaces_service().is_none() {
                assert!(tokio::time::Instant::now() < deadline, "no spaces service");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            let service = client
                .resolve_spaces_service()
                .await
                .expect("spaces service resolution failed");
            let id = client
                .create_space(&service, &format!("Notify {stamp}"), false)
                .await
                .expect("space creation failed");

            let first = format!("text-a-{stamp}@conference.localhost");
            let room = xmpp_core::spaces::SpaceRoom {
                conference: first.clone(),
                name: "A".to_string(),
                ..xmpp_core::spaces::SpaceRoom::default()
            };
            client
                .save_space_room(&id, &room)
                .await
                .expect("first room publish failed");

            // Publish the second room directly, so the room list can only come
            // from the notification: `save_space_room` would re-read the node.
            let second = format!("text-b-{stamp}@conference.localhost");
            let stanza = format!(
                "<iq type='set' to='{}' id='publish-b' xmlns='jabber:client'><pubsub xmlns='http://jabber.org/protocol/pubsub'>\
                 <publish node='{}'><item id='{second}'><conference xmlns='urn:xmpp:bookmarks:1' name='B'>\
                 <extensions><notify xmlns='urn:xmpp:notification-settings:0'><always/></notify></extensions>\
                 </conference></item></publish></pubsub></iq>",
                id.server, id.node
            );
            client
                .connection
                .send_stanza_str(&stanza)
                .expect("second room publish failed");

            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            loop {
                let rooms = client.spaces.get(&id).map(|space| space.rooms).unwrap_or_default();
                if rooms.iter().any(|room| room.conference == second) {
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "the second room never arrived: {rooms:?}"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            let rooms = client.spaces.get(&id).expect("space missing").rooms;
            assert!(
                rooms.iter().any(|room| room.conference == first),
                "the notification dropped the first room: {rooms:?}"
            );

            client.destroy_space(&id).await.expect("destroy failed");
            processor_task.abort();
            connection.disconnect().await.expect("disconnect failed");
        })
        .await;
}
