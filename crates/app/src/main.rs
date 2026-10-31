//! Jabu - Cross-platform XMPP Client
//!
//! A modern, cross-platform XMPP client built with Rust and Dioxus.

use anyhow::Result;
use clap::{Parser, Subcommand};
use dioxus::prelude::*;
use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::{fmt, EnvFilter};

use jid::Jid;
use platform::{init_platform, PlatformContext};
use storage::{
    load_session, SettingsOmemoStore, StorageBackend, StorageFactory, StoredChat, StoredContact,
    StoredMucRoom, StoredPubSubNode,
};
use ui::{App, ClientState, Theme};
use xmpp_core::{
    CallKind, CallState, ChatManager, ClientConfig, Connection, ConnectionEvent, ConnectionState,
    DiscoManager, EventBus, EventProcessor, IqRouter, JingleManager, MamManager, MucManager,
    OmemoManager, ProcessorNotifier, PubSubManager, PushManager, Roster, SpaceManager,
    StreamManagement, XmppClient, XmppEvent,
};

// Thread-local context for non-Send types
thread_local! {
    static APP_CONTEXT: std::cell::RefCell<Option<AppContext>> = const { std::cell::RefCell::new(None) };
    static EVENT_RX: std::cell::RefCell<Option<tokio::sync::mpsc::UnboundedReceiver<ConnectionEvent>>> = const { std::cell::RefCell::new(None) };
}

struct UiNotifier {
    revision: Signal<u64>,
    connection_state: Signal<ConnectionState>,
}

impl ProcessorNotifier for UiNotifier {
    fn bump(&self) {
        let mut revision = self.revision;
        let next = *revision.read() + 1;
        revision.set(next);
    }

    fn set_connection_state(&self, state: ConnectionState) {
        let mut connection_state = self.connection_state;
        connection_state.set(state);
    }
}

#[derive(Clone)]
struct AppContext {
    platform: PlatformContext,
    storage: Arc<dyn StorageBackend>,
    events: Arc<EventBus>,
    xmpp_client: XmppClient,
    initial_config: ClientConfig,
    initial_theme: Theme,
}

fn app_root() -> Element {
    // Extract all needed data while the borrow is held
    let (platform, storage, events, xmpp_client, initial_config, initial_theme) =
        APP_CONTEXT.with(|cell| {
            let binding = cell.borrow();
            let ctx = binding.as_ref().expect("App context not initialized");
            (
                ctx.platform.clone(),
                ctx.storage.clone(),
                ctx.events.clone(),
                ctx.xmpp_client.clone(),
                ctx.initial_config.clone(),
                ctx.initial_theme,
            )
        });

    // Create ClientState inside the component (non-Send)
    let should_connect = !initial_config.jid.trim().is_empty();
    let account_bare = initial_config
        .jid
        .split('/')
        .next()
        .unwrap_or_default()
        .to_string();

    let mut client_state = ClientState::new();
    let mut account_list: Vec<String> = storage
        .list_accounts()
        .unwrap_or_default()
        .into_iter()
        .map(|account| account.jid)
        .collect();
    if !account_bare.is_empty() && !account_list.contains(&account_bare) {
        account_list.push(account_bare.clone());
    }
    account_list.sort();
    client_state.accounts.set(account_list);
    client_state
        .active_account
        .set((!account_bare.is_empty()).then(|| account_bare.clone()));
    client_state.config.set(Some(initial_config));
    client_state.client.set(Some(xmpp_client.clone()));
    if let Some(enabled) = storage
        .get_setting("ui.notifications")
        .ok()
        .flatten()
        .and_then(|v| v.as_bool())
    {
        client_state.notifications_enabled.set(enabled);
    }
    if let Some(show) = storage
        .get_setting("ui.timestamps")
        .ok()
        .flatten()
        .and_then(|v| v.as_bool())
    {
        client_state.show_timestamps.set(show);
    }
    if let Some(enabled) = storage
        .get_setting("ui.link_previews")
        .ok()
        .flatten()
        .and_then(|v| v.as_bool())
    {
        client_state.link_previews.set(enabled);
    }
    if let Some(size) = storage
        .get_setting("ui.font_size")
        .ok()
        .flatten()
        .and_then(|v| v.as_u64())
    {
        client_state.font_size.set(size.clamp(12, 24) as u32);
    }
    if let Some(compact) = storage
        .get_setting("ui.compact")
        .ok()
        .flatten()
        .and_then(|v| v.as_bool())
    {
        client_state.compact.set(compact);
    }
    let theme = storage
        .get_setting("ui.theme")
        .ok()
        .flatten()
        .and_then(|v| v.as_str().map(Theme::from_slug))
        .unwrap_or(initial_theme);
    client_state.theme.set(theme);

    let initial_spaces: Vec<ui::Space> = xmpp_client
        .spaces
        .list()
        .into_iter()
        .map(Into::into)
        .collect();
    if !initial_spaces.is_empty() {
        client_state.spaces.set(initial_spaces);
    }

    let spaces_signal = client_state.spaces;
    let save_storage = storage.clone();
    let save_client = xmpp_client.clone();
    let save_account = account_bare.clone();
    let save_events = events.clone();
    use_future(move || {
        let storage = save_storage.clone();
        let client = save_client.clone();
        let account = save_account.clone();
        let events = save_events.clone();
        let mut spaces_signal = spaces_signal;
        async move {
            let mut rx = events.subscribe();
            while let Ok(event) = rx.recv().await {
                if matches!(event, XmppEvent::SpacesChanged) {
                    let spaces = client.spaces.list();
                    for space in &spaces {
                        let _ = storage.save_space(&account, space);
                    }
                    let ui_spaces: Vec<ui::Space> = spaces.into_iter().map(Into::into).collect();
                    if !ui_spaces.is_empty() {
                        spaces_signal.set(ui_spaces);
                    }
                }
            }
        }
    });

    // Copy the signals the event processor needs before moving ClientState into context.
    let revision_signal = client_state.revision;
    let connection_state_signal = client_state.connection_state;
    let last_error_signal = client_state.last_error;
    let selected_chat_signal = client_state.selected_chat;
    let notifications_signal = client_state.notifications_enabled;
    let notifier = Arc::new(UiNotifier {
        revision: revision_signal,
        connection_state: connection_state_signal,
    });
    let media_client = xmpp_client.clone();
    use_future(move || {
        let client = media_client.clone();
        async move {
            client.start_call_media_listener();
        }
    });

    #[cfg(debug_assertions)]
    {
        let auto_client = xmpp_client.clone();
        use_future(move || {
            let client = auto_client.clone();
            async move {
                if let Ok(target) = std::env::var("JABU_AUTO_CALL") {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    if let Ok(jid) = target.parse::<Jid>() {
                        let kind = match std::env::var("JABU_AUTO_CALL_KIND").as_deref() {
                            Ok("video") => CallKind::Video,
                            _ => CallKind::Audio,
                        };
                        let _ = client.start_call(&jid, kind).await;
                    }
                }
                if std::env::var("JABU_AUTO_ANSWER").is_ok() {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        for session in client.call_sessions() {
                            if session.state == CallState::Incoming {
                                let _ = client.accept_call(&session.sid).await;
                            }
                        }
                    }
                }
                if let Ok(secs) = std::env::var("JABU_AUTO_HANGUP") {
                    let secs = secs.parse::<u64>().unwrap_or(5);
                    tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
                    for session in client.call_sessions() {
                        if !session.state.is_final() {
                            let _ = client.end_call(&session.sid, Some("success")).await;
                        }
                    }
                }
            }
        });
    }

    let call_client = xmpp_client.clone();
    let call_events = events.clone();
    let call_notifier = notifier.clone();
    use_future(move || {
        let client = call_client.clone();
        let mut rx = call_events.subscribe();
        let notifier = call_notifier.clone();
        async move {
            while let Ok(event) = rx.recv().await {
                if let XmppEvent::CallEnded { sid, .. } = event {
                    let Some(session) = client.call_session(&sid) else {
                        continue;
                    };
                    let peer = session.peer.to_bare().to_string();
                    let is_room =
                        client.mucs.get_by_bare(&peer).is_some()
                            || client.spaces.list().iter().any(|space| {
                                space.rooms.iter().any(|room| room.conference == peer)
                            });
                    if is_room {
                        continue;
                    }
                    let ended = session.ended_at.unwrap_or_else(chrono::Utc::now);
                    let started = session.connected_at.unwrap_or(session.created_at);
                    let call = xmpp_core::CallEventInfo {
                        direction: match session.direction {
                            xmpp_core::CallDirection::Incoming => "incoming".to_string(),
                            xmpp_core::CallDirection::Outgoing => "outgoing".to_string(),
                        },
                        kind: session.kind.as_str().to_string(),
                        state: session.state.as_str().to_string(),
                        duration_secs: (ended - started).num_seconds().max(0),
                        reason: session.reason.clone(),
                    };
                    if let Err(error) = client.add_call_event(&session.peer, call) {
                        tracing::warn!("Failed to record call event: {error}");
                    } else {
                        notifier.bump();
                    }
                }
            }
        }
    });

    let notif_platform = platform.clone();
    let notif_events = events.clone();
    let notif_roster = xmpp_client.roster.clone();
    let notif_client = xmpp_client.clone();

    use_context_provider(|| client_state);
    use_context_provider(|| platform);
    use_context_provider(|| storage.clone());
    use_context_provider(|| events.clone());
    use_context_provider(|| xmpp_client.clone());

    // Connect after the Dioxus/Tokio runtime exists.
    let connect_client = xmpp_client.clone();
    use_future(move || {
        let client = connect_client.clone();
        let mut connection_state = connection_state_signal;
        let mut last_error = last_error_signal;
        async move {
            if should_connect {
                connection_state.set(ConnectionState::Connecting);
                if let Err(e) = client.connection.connect().await {
                    let message = e.to_string();
                    tracing::error!("Failed to connect: {}", message);
                    last_error.set(Some(message.clone()));
                    connection_state.set(ConnectionState::Failed { reason: message });
                } else {
                    // The rooms of a space live in a pubsub node and only
                    // changes made while online are announced, so a client that
                    // was away when a room was added would never see it.
                    client.refresh_space_rooms().await;
                    // The roster is usually loaded right after authentication;
                    // fetching before it arrives would miss every contact.
                    for _ in 0..100 {
                        if !client.roster.list().is_empty() {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                    client.refresh_stories().await;
                }
            }
        }
    });

    use_future(move || {
        let platform = notif_platform.clone();
        let events = notif_events.clone();
        let roster = notif_roster.clone();
        let client = notif_client.clone();
        let selected = selected_chat_signal;
        let notifications = notifications_signal;
        async move {
            let started_at = chrono::Utc::now();
            let mut rx = events.subscribe();
            while let Ok(event) = rx.recv().await {
                match event {
                    XmppEvent::MessageReceived { jid, message } => {
                        if !*notifications.read() {
                            continue;
                        }
                        let Some(body) = message.body.clone().filter(|b| !b.is_empty()) else {
                            continue;
                        };
                        if message.is_own || *selected.read() == Some(jid.clone()) {
                            continue;
                        }
                        if client
                            .chats
                            .get_by_bare(&jid)
                            .is_some_and(|chat| !chat.preferences().notifications_enabled)
                        {
                            continue;
                        }
                        let title = roster
                            .get_by_bare(&jid)
                            .map(|r| r.display_name())
                            .unwrap_or_else(|| jid.clone());
                        let notification = platform::Notification {
                            id: message.id.clone(),
                            title,
                            body,
                            icon: None,
                            tag: Some(jid),
                            data: None,
                            actions: Vec::new(),
                            require_interaction: false,
                            silent: false,
                            timestamp: chrono::Utc::now(),
                        };
                        if let Err(e) = platform.notifications.show_notification(&notification) {
                            tracing::warn!("Failed to show notification: {}", e);
                        }
                    }
                    XmppEvent::PubSubItemPublished { node_id, item, .. } => {
                        let is_story = node_id == xmpp_core::STORIES_NODE;
                        let now = chrono::Utc::now();
                        if !*notifications.read()
                            || (node_id != xmpp_core::MICROBLOG_NODE && !is_story)
                            || item.title.is_none() && item.body.is_none()
                            || item.published_at < started_at
                            || item.published_at < now - chrono::Duration::minutes(10)
                            || item.expires_at.is_some_and(|expires| expires <= now)
                        {
                            continue;
                        }
                        let own = client.connection.jid().map(|jid| jid.to_bare().to_string());
                        if own.as_deref() == Some(item.publisher.to_bare().to_string().as_str()) {
                            continue;
                        }
                        let body = item
                            .title
                            .clone()
                            .or_else(|| item.body.clone())
                            .unwrap_or_else(|| {
                                if is_story {
                                    "New story".to_string()
                                } else {
                                    "New post".to_string()
                                }
                            });
                        let author = roster
                            .get(&item.publisher)
                            .map(|r| r.display_name())
                            .unwrap_or_else(|| item.publisher.to_bare().to_string());
                        let title = if is_story {
                            format!("📝 New Story from {author}")
                        } else {
                            author
                        };
                        let notification = platform::Notification {
                            id: item.id.clone(),
                            title,
                            body,
                            icon: None,
                            tag: Some(item.publisher.to_bare().to_string()),
                            data: None,
                            actions: Vec::new(),
                            require_interaction: false,
                            silent: false,
                            timestamp: chrono::Utc::now(),
                        };
                        if let Err(e) = platform.notifications.show_notification(&notification) {
                            tracing::warn!("Failed to show notification: {}", e);
                        }
                    }
                    _ => {}
                }
            }
        }
    });

    // Persist incoming changes so restarts keep history and state.
    let persist_storage = storage.clone();
    let persist_events = events.clone();
    let persist_client = xmpp_client.clone();
    use_future(move || {
        let storage = persist_storage.clone();
        let events = persist_events.clone();
        let client = persist_client.clone();
        async move {
            let mut rx = events.subscribe();
            while let Ok(event) = rx.recv().await {
                let account = client
                    .connection
                    .jid()
                    .map(|jid| jid.to_bare().to_string())
                    .unwrap_or_default();
                match event {
                    XmppEvent::MessageReceived { jid, message }
                    | XmppEvent::MessageSent { jid, message } => {
                        let _ = storage.save_message_for(&account, &message);
                        if let Some(chat) = client.chats.get_by_bare(&jid) {
                            let last = chat.last_message();
                            let previous = storage.get_chat(&account, &jid).ok().flatten();
                            let stored = StoredChat {
                                account_jid: account.clone(),
                                peer_jid: jid.clone(),
                                unread_count: chat.unread_count(),
                                last_message_id: last.as_ref().map(|m| m.id.clone()),
                                last_activity: last
                                    .map(|m| m.timestamp)
                                    .unwrap_or_else(chrono::Utc::now),
                                is_archived: previous.as_ref().is_some_and(|chat| chat.is_archived),
                                is_pinned: previous.as_ref().is_some_and(|chat| chat.is_pinned),
                                notifications_enabled: previous
                                    .as_ref()
                                    .is_none_or(|chat| chat.notifications_enabled),
                            };
                            let _ = storage.save_chat(&stored);
                        }
                    }
                    XmppEvent::MucMessageReceived { jid, message } => {
                        let _ = storage.save_muc_message_for(&account, &message);
                        if let Some(room) = client.mucs.get_by_bare(&jid) {
                            let previous = storage
                                .get_muc_rooms(&account)
                                .ok()
                                .and_then(|rooms| rooms.into_iter().find(|r| r.jid == jid));
                            let stored = StoredMucRoom {
                                account_jid: account.clone(),
                                jid: room.bare_jid.clone(),
                                name: room.name.clone(),
                                nick: room.self_nick().unwrap_or_default(),
                                is_joined: room.is_joined(),
                                notifications_enabled: previous
                                    .as_ref()
                                    .is_none_or(|room| room.notifications_enabled),
                                last_activity: message.timestamp,
                                is_favorite: room.is_bookmarked(),
                                autojoin: room.is_autojoin(),
                                pinned: room.is_pinned(),
                                ..StoredMucRoom::default()
                            };
                            let _ = storage.save_muc_room(&stored);
                        }
                    }
                    XmppEvent::ChatChanged { jid } => {
                        if let Some(chat) = client.chats.get_by_bare(&jid) {
                            let last = chat.last_message();
                            let prefs = chat.preferences();
                            let stored = StoredChat {
                                account_jid: account.clone(),
                                peer_jid: jid.clone(),
                                unread_count: chat.unread_count(),
                                last_message_id: last.as_ref().map(|m| m.id.clone()),
                                last_activity: last
                                    .map(|m| m.timestamp)
                                    .unwrap_or_else(chrono::Utc::now),
                                is_archived: prefs.is_archived,
                                is_pinned: prefs.is_pinned,
                                notifications_enabled: prefs.notifications_enabled,
                            };
                            let _ = storage.save_chat(&stored);
                        }
                    }
                    XmppEvent::RosterChanged => {
                        for item in client.roster.list() {
                            let contact = StoredContact {
                                account_jid: account.clone(),
                                jid: item.jid.to_bare().to_string(),
                                name: item.name.clone(),
                                groups: item.groups.clone(),
                                subscription: item.subscription,
                                avatar_hash: item.avatar_hash.clone(),
                                is_blocked: item.is_blocked,
                                updated_at: chrono::Utc::now(),
                            };
                            let _ = storage.save_contact(&contact);
                        }
                    }
                    XmppEvent::MucChanged { jid } => {
                        if let Some(room) = client.mucs.get_by_bare(&jid) {
                            let previous = storage
                                .get_muc_rooms(&account)
                                .ok()
                                .and_then(|rooms| rooms.into_iter().find(|r| r.jid == jid));
                            let stored = StoredMucRoom {
                                account_jid: account.clone(),
                                jid: room.bare_jid.clone(),
                                name: room.name.clone(),
                                nick: room.self_nick().unwrap_or_default(),
                                is_joined: room.is_joined(),
                                notifications_enabled: previous
                                    .as_ref()
                                    .is_none_or(|room| room.notifications_enabled),
                                last_activity: chrono::Utc::now(),
                                is_favorite: room.is_bookmarked(),
                                autojoin: room.is_autojoin(),
                                pinned: room.is_pinned(),
                                ..StoredMucRoom::default()
                            };
                            let _ = storage.save_muc_room(&stored);
                        }
                    }
                    XmppEvent::PubSubNodeChanged {
                        service_jid,
                        node_id,
                    } => {
                        if let Ok(service) = service_jid.parse::<Jid>() {
                            if let Some(svc) = client.pubsub.get(&service) {
                                if let Some(node) = svc.get_node(&node_id) {
                                    let stored = StoredPubSubNode {
                                        service_jid: service_jid.clone(),
                                        id: node_id.clone(),
                                        config: node.config.clone(),
                                        last_sync: Some(chrono::Utc::now()),
                                    };
                                    let _ = storage.save_pubsub_node(&stored);
                                    for item in node.items() {
                                        let _ = storage.save_pubsub_item_for(&account, &item);
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    });

    // Forward roster/subscription actions to the connection.
    let outbound_events = events.clone();
    let outbound_connection = xmpp_client.connection.clone();
    use_future(move || {
        let events = outbound_events.clone();
        let connection = outbound_connection.clone();
        async move {
            let mut rx = events.subscribe();
            while let Ok(event) = rx.recv().await {
                if let XmppEvent::SendStanza(xml) = event {
                    if let Err(e) = connection.send_stanza_str(&xml) {
                        tracing::warn!("Failed to send queued stanza: {}", e);
                    }
                }
            }
        }
    });

    // Start event processor on main thread using use_future
    let connection = xmpp_client.connection.clone();
    let roster = xmpp_client.roster.clone();
    let chats = xmpp_client.chats.clone();
    let mucs = xmpp_client.mucs.clone();
    let pubsub = xmpp_client.pubsub.clone();
    let spaces = xmpp_client.spaces.clone();
    let disco = xmpp_client.disco.clone();
    let push = xmpp_client.push.clone();
    let sm = xmpp_client.sm.clone();
    let omemo = xmpp_client.omemo.clone();
    let iq = xmpp_client.iq.clone();
    let mam = xmpp_client.mam.clone();
    let jingle = xmpp_client.jingle.clone();
    let conference = xmpp_client.conference.clone();

    // Use a flag to ensure processor only starts once
    let processor_started = use_signal(|| false);

    let notifier_for_processor = notifier.clone();
    use_future(move || {
        let notifier = notifier_for_processor.clone();
        let connection = connection.clone();
        let roster = roster.clone();
        let chats = chats.clone();
        let mucs = mucs.clone();
        let pubsub = pubsub.clone();
        let spaces = spaces.clone();
        let disco = disco.clone();
        let push = push.clone();
        let sm = sm.clone();
        let omemo = omemo.clone();
        let iq = iq.clone();
        let mam = mam.clone();
        let jingle = jingle.clone();
        let conference = conference.clone();
        let mut processor_started = processor_started;
        async move {
            // Only start processor once
            if *processor_started.read() {
                return;
            }
            processor_started.set(true);

            // Get event_rx from thread-local
            let event_rx = EVENT_RX.with(|cell| cell.borrow_mut().take());

            if let Some(event_rx) = event_rx {
                let processor = EventProcessor::new_with_jingle_and_conference(
                    connection, roster, chats, mucs, pubsub, spaces, disco, push, sm, omemo, iq,
                    mam, jingle, conference,
                )
                .with_notifier(notifier);
                if let Err(e) = processor.start(event_rx).await {
                    tracing::error!("Event processor error: {}", e);
                }
            }
        }
    });

    rsx! {
        ui::AppStyles {}
        App {}
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn install_call_media(
    client: &XmppClient,
    disco: Arc<DiscoManager>,
    preferred_camera: Option<String>,
    preferred_microphone: Option<String>,
    preferred_sfu: Option<String>,
) {
    client
        .voice
        .set_port(Arc::new(media::ConferenceVoicePort::new(
            client.clone(),
            preferred_sfu
                .as_deref()
                .and_then(|value| value.parse().ok()),
        )));
    let port = match media::GStreamerMediaPort::new(Vec::new()) {
        Ok(port) => port,
        Err(error) => {
            tracing::warn!("Call media backend unavailable: {error}");
            return;
        }
    };
    client.set_call_media(port.clone());
    client.set_call_camera_preference(preferred_camera.as_deref());
    client.set_call_audio_preference(preferred_microphone.as_deref());
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(10));
        let servers = disco
            .stun_turn_servers()
            .into_iter()
            .map(|service| {
                let scheme = if service.kind == "turn" {
                    if service.transport.as_deref() == Some("tcp") {
                        "turns"
                    } else {
                        "turn"
                    }
                } else {
                    "stun"
                };
                media::IceServer {
                    urls: vec![format!("{scheme}:{}:{}", service.host, service.port)],
                    username: service.username,
                    credential: service.password,
                }
            })
            .collect::<Vec<_>>();
        port.set_ice_servers(servers);
    });
}

/// Application configuration
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct AppConfig {
    pub xmpp: ClientConfig,
    pub ui: UiConfig,
    pub storage: StorageConfig,
    pub notifications: NotificationConfig,
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct UiConfig {
    pub theme: String,
    pub language: String,
    pub font_size: u32,
    pub compact_mode: bool,
    pub show_avatars: bool,
    pub show_timestamps: bool,
    pub animate: bool,
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct StorageConfig {
    pub backend: String, // "redb", "sqlite", "indexeddb"
    pub path: Option<String>,
    pub max_message_history: usize,
    pub auto_cleanup: bool,
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct NotificationConfig {
    pub enabled: bool,
    pub sound: bool,
    pub desktop: bool,
    pub push: bool,
    pub per_chat: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            xmpp: ClientConfig::default(),
            ui: UiConfig {
                theme: "dark".to_string(),
                language: "en".to_string(),
                font_size: 14,
                compact_mode: false,
                show_avatars: true,
                show_timestamps: true,
                animate: true,
            },
            storage: StorageConfig {
                backend: "redb".to_string(),
                path: None,
                max_message_history: 10000,
                auto_cleanup: true,
            },
            notifications: NotificationConfig {
                enabled: true,
                sound: true,
                desktop: true,
                push: true,
                per_chat: true,
            },
        }
    }
}

/// CLI arguments
#[derive(Parser)]
#[command(name = "jabu", version, about = "Cross-platform XMPP Client")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Configuration file path
    #[arg(short, long)]
    config: Option<String>,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,

    /// JID to connect to
    #[arg(short, long)]
    jid: Option<String>,

    /// Password (will prompt if not provided)
    #[arg(short, long)]
    password: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the application (default)
    Run,
    /// Show version information
    Version,
    /// Manage accounts
    Account {
        #[command(subcommand)]
        action: AccountAction,
    },
    /// Import/export data
    Data {
        #[command(subcommand)]
        action: DataAction,
    },
}

#[derive(Subcommand)]
enum AccountAction {
    /// Add a new account
    Add {
        jid: String,
        /// Password (prompted when omitted)
        #[arg(long)]
        password: Option<String>,
        /// Resource to bind
        #[arg(long)]
        resource: Option<String>,
    },
    /// Remove an account
    Remove { jid: String },
    /// List accounts
    List,
}

#[derive(Subcommand)]
enum DataAction {
    /// Export all data
    Export { path: String },
    /// Import data
    Import { path: String },
}

/// Main entry point
fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    init_logging(&cli.log_level)?;

    // Load configuration
    let config = load_config(cli.config.as_deref())?;

    // Override with CLI arguments
    let mut config = config;
    if let Some(jid) = cli.jid {
        config.xmpp.jid = jid;
    }
    if let Some(password) = cli.password {
        config.xmpp.password = Some(password);
    }

    // Handle commands
    if let Some(cmd) = cli.command {
        return handle_command(cmd, config);
    }

    // Run the application
    run_app(config)
}

/// Initialize logging
fn init_logging(level: &str) -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    Ok(())
}

/// Load configuration from file
fn load_config(config_path: Option<&str>) -> Result<AppConfig> {
    let mut figment = Figment::new();
    if std::path::Path::new("config.toml").exists() {
        figment = figment.merge(Toml::file("config.toml"));
    }

    if let Some(path) = config_path {
        figment = figment.merge(Toml::file(path));
    }

    figment = figment.merge(Env::prefixed("JABU_"));

    Ok(figment.extract()?)
}

/// Handle CLI commands
fn handle_command(cmd: Command, config: AppConfig) -> Result<()> {
    match cmd {
        Command::Run => run_app(config),
        Command::Version => {
            println!("jabu {}", env!("CARGO_PKG_VERSION"));
            println!(
                "Built with Rust {}",
                option_env!("RUSTC_VERSION").unwrap_or("unknown")
            );
            Ok(())
        }
        Command::Account { action } => handle_account_action(action, config),
        Command::Data { action } => handle_data_action(action, config),
    }
}

fn handle_account_action(action: AccountAction, config: AppConfig) -> Result<()> {
    let storage = open_storage(&config)?;
    match action {
        AccountAction::Add {
            jid,
            password,
            resource,
        } => {
            let bare = jid.split('/').next().unwrap_or(&jid).to_string();
            let password = match password {
                Some(password) => password,
                None => {
                    print!("Password for {bare}: ");
                    std::io::Write::flush(&mut std::io::stdout())?;
                    let mut line = String::new();
                    std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut line)?;
                    line.trim_end_matches(['\r', '\n']).to_string()
                }
            };
            let resource = resource.unwrap_or_else(|| "jabu".to_string());
            let account = storage::StoredAccount {
                jid: bare.clone(),
                password_hash: None,
                resource: resource.clone(),
                priority: 0,
                auto_reconnect: true,
                created_at: chrono::Utc::now(),
                last_login: None,
            };
            storage.save_account(&account)?;
            let credentials = storage::StoredCredentials {
                jid: bare.clone(),
                password: Some(password),
                resource: Some(resource),
            };
            storage::save_account_credentials(storage.as_ref(), &credentials)?;
            storage::save_credentials(storage.as_ref(), &credentials)?;
            storage.set_setting("ui.active_account", &serde_json::json!(bare))?;
            println!("Added {bare}");
        }
        AccountAction::Remove { jid } => {
            let bare = jid.split('/').next().unwrap_or(&jid).to_string();
            storage.delete_account(&bare)?;
            storage::delete_account_credentials(storage.as_ref(), &bare)?;
            if storage
                .get_setting("ui.active_account")?
                .and_then(|value| value.as_str().map(str::to_string))
                .as_deref()
                == Some(bare.as_str())
            {
                let _ = storage::clear_credentials(storage.as_ref());
                let _ = storage.set_setting("ui.active_account", &serde_json::Value::Null);
            }
            println!("Removed {bare}");
        }
        AccountAction::List => {
            let accounts = storage.list_accounts()?;
            if accounts.is_empty() {
                println!("No accounts configured.");
            } else {
                for account in accounts {
                    println!("{}\t{}", account.jid, account.resource);
                }
            }
        }
    }
    Ok(())
}

fn open_storage(config: &AppConfig) -> Result<Arc<dyn StorageBackend>> {
    let platform = init_platform(Arc::new(EventBus::new()))?;
    let path = config
        .storage
        .path
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(|| platform.storage.get_data_dir().unwrap());
    Ok(Arc::new(StorageFactory::create(path)?))
}

fn handle_data_action(action: DataAction, _config: AppConfig) -> Result<()> {
    match action {
        DataAction::Export { path } => {
            println!("Exporting data to: {}", path);
        }
        DataAction::Import { path } => {
            println!("Importing data from: {}", path);
        }
    }
    Ok(())
}

/// Run the Dioxus application
fn run_app(config: AppConfig) -> Result<()> {
    let mut config = config;
    info!("Starting Jabu v{}", env!("CARGO_PKG_VERSION"));

    let _instance = match platform::single_instance::acquire("jabu")? {
        Some(guard) => guard,
        None => {
            info!("Another Jabu instance is already running");
            return Ok(());
        }
    };

    // Create event bus
    let events = Arc::new(EventBus::new());

    // Initialize platform
    let platform = init_platform(events.clone())?;
    info!("Platform initialized: {:?}", platform.platform);

    // Initialize storage
    let storage_path = config
        .storage
        .path
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(|| platform.storage.get_data_dir().unwrap());

    let storage = Arc::new(StorageFactory::create(storage_path)?);
    info!("Storage initialized");

    if config.xmpp.jid.trim().is_empty() {
        let preferred = storage
            .get_setting("ui.active_account")
            .ok()
            .flatten()
            .and_then(|value| value.as_str().map(str::to_string));
        let credentials = preferred
            .as_deref()
            .and_then(|jid| storage::load_account_credentials(storage.as_ref(), jid))
            .or_else(|| {
                storage
                    .list_accounts()
                    .ok()
                    .and_then(|accounts| accounts.first().cloned())
                    .and_then(|account| {
                        storage::load_account_credentials(storage.as_ref(), &account.jid)
                    })
            })
            .or_else(|| storage::load_credentials(storage.as_ref()));
        if let Some(credentials) = credentials {
            config.xmpp.jid = credentials.jid;
            if config.xmpp.password.is_none() {
                config.xmpp.password = credentials.password;
            }
            if config.xmpp.resource.is_none() {
                config.xmpp.resource = credentials.resource;
            }
        }
    }
    let has_account = !config.xmpp.jid.trim().is_empty();
    if has_account {
        let bare = config
            .xmpp
            .jid
            .split('/')
            .next()
            .unwrap_or(&config.xmpp.jid)
            .to_string();
        let credentials = storage::StoredCredentials {
            jid: bare.clone(),
            password: config.xmpp.password.clone(),
            resource: config.xmpp.resource.clone(),
        };
        let account = storage::StoredAccount {
            jid: bare.clone(),
            password_hash: None,
            resource: config.xmpp.resource.clone().unwrap_or_default(),
            priority: 0,
            auto_reconnect: config.xmpp.auto_reconnect,
            created_at: chrono::Utc::now(),
            last_login: Some(chrono::Utc::now()),
        };
        let _ = storage.save_account(&account);
        if credentials.password.is_some() {
            let _ = storage::save_account_credentials(storage.as_ref(), &credentials);
        }
        let _ = storage.set_setting("ui.active_account", &serde_json::json!(bare));
    }

    // Create connection and get event receiver
    let (connection_for_processor, event_rx) = Connection::new(config.xmpp.clone().into());
    let connection = Arc::new(connection_for_processor);

    let roster = Arc::new(Roster::new(events.clone()));
    let chats = Arc::new(ChatManager::new(events.clone()));
    let mucs = Arc::new(MucManager::new(events.clone()));
    let pubsub = Arc::new(PubSubManager::new(events.clone()));
    let spaces = Arc::new(SpaceManager::new(events.clone()));
    let disco = Arc::new(DiscoManager::new());
    let push = Arc::new(PushManager::new());
    let sm = Arc::new(StreamManagement::new());
    let omemo = Arc::new(OmemoManager::new());
    let iq = Arc::new(IqRouter::new(std::time::Duration::from_secs(30)));
    let mam = Arc::new(MamManager::new());
    let jingle = JingleManager::new(connection.clone(), iq.clone(), events.clone());
    let conference = xmpp_core::ConferenceManager::new();
    if has_account {
        omemo.set_store(Arc::new(SettingsOmemoStore::new(
            storage.clone(),
            &config.xmpp.jid,
        )));
        if let Err(e) = omemo.load_persisted() {
            tracing::warn!("Failed to load OMEMO state: {}", e);
        }
        omemo.ensure_local_auto(100);
        omemo.set_own_jid(
            config
                .xmpp
                .jid
                .split('/')
                .next()
                .unwrap_or(&config.xmpp.jid),
        );
    }
    connection.set_stream_management(sm.clone());

    // Create XMPP client
    let xmpp_client = XmppClient {
        connection: connection.clone(),
        roster: roster.clone(),
        chats: chats.clone(),
        mucs: mucs.clone(),
        pubsub: pubsub.clone(),
        spaces: spaces.clone(),
        disco: disco.clone(),
        push: push.clone(),
        sm: sm.clone(),
        omemo: omemo.clone(),
        events: events.clone(),
        iq: iq.clone(),
        mam: mam.clone(),
        jingle: jingle.clone(),
        conference: conference.clone(),
        voice: Arc::new(xmpp_core::VoiceManager::new()),
    };

    #[cfg(not(target_arch = "wasm32"))]
    let preferred_camera = storage
        .get_setting("calls.camera")
        .ok()
        .flatten()
        .and_then(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        });
    #[cfg(not(target_arch = "wasm32"))]
    let preferred_microphone = storage
        .get_setting("calls.microphone")
        .ok()
        .flatten()
        .and_then(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        });
    // The SFU component cannot be discovered from the server (Prosody does not
    // list it under disco#items), so it is configured, as it is in Movim.
    #[cfg(not(target_arch = "wasm32"))]
    let preferred_sfu = storage
        .get_setting("calls.sfu")
        .ok()
        .flatten()
        .and_then(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        });
    #[cfg(not(target_arch = "wasm32"))]
    let preferred_spaces_service = storage
        .get_setting("spaces.service")
        .ok()
        .flatten()
        .and_then(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        });
    xmpp_client.set_spaces_service(preferred_spaces_service.as_deref());
    install_call_media(
        &xmpp_client,
        disco.clone(),
        preferred_camera,
        preferred_microphone,
        preferred_sfu,
    );

    load_session(storage.as_ref(), &xmpp_client, &config.xmpp.jid);
    let account_jid = config
        .xmpp
        .jid
        .split('/')
        .next()
        .unwrap_or_default()
        .to_string();
    if let Ok(spaces) = storage.get_spaces(&account_jid) {
        for space in spaces {
            xmpp_client.spaces.upsert(space);
        }
    }

    // Launch Dioxus app
    let app_ctx = AppContext {
        platform: platform.clone(),
        storage: storage.clone(),
        events: events.clone(),
        xmpp_client: xmpp_client.clone(),
        initial_config: config.xmpp.clone(),
        initial_theme: Theme::from_slug(&config.ui.theme),
    };
    APP_CONTEXT.with(|cell| {
        *cell.borrow_mut() = Some(app_ctx);
    });
    EVENT_RX.with(|cell| {
        *cell.borrow_mut() = Some(event_rx);
    });

    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        use dioxus::desktop::{Config, LogicalSize, WindowBuilder};

        dioxus::LaunchBuilder::new()
            .with_cfg(
                Config::new().with_disable_context_menu(true).with_window(
                    WindowBuilder::new()
                        .with_title("Jabu")
                        .with_decorations(false)
                        .with_inner_size(LogicalSize::new(1280.0, 800.0))
                        .with_min_inner_size(LogicalSize::new(860.0, 560.0)),
                ),
            )
            .launch(app_root);
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    dioxus::launch(app_root);

    Ok(())
}
