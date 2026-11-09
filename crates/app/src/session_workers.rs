//! Per-account workers: processors, persistence, notifications and connects.
//!
//! Every [`xmpp_core::AccountSession`] gets its own event processor and event
//! loops so accounts stay online in the background. The UI still renders the
//! active session only; switching swaps the client in [`ClientState`] without
//! reconnecting.
//!
//! The workers run on the Dioxus local runtime and hold UI signals, so their
//! futures are deliberately not `Send`.

#![allow(
    clippy::arc_with_non_send_sync,
    clippy::future_not_send,
    clippy::needless_pass_by_value,
    clippy::too_many_lines
)]

use std::sync::Arc;
use std::time::Duration;

use dioxus::prelude::{spawn, ReadableExt, Signal, WritableExt};
use platform::{Notification, PlatformContext};
use storage::{StorageBackend, StoredChat, StoredContact, StoredMucRoom, StoredPubSubNode};
use tokio::sync::watch;
use ui::{ClientState, PresenceChoice};
use xmpp_core::{
    Connection, ConnectionState, EventBus, EventProcessor, Jid, ProcessorNotifier, SessionRegistry,
    XmppClient, XmppEvent,
};

/// Start the processor, persistence, notification and connect workers for all
/// registered sessions.
pub fn start(
    registry: &Arc<SessionRegistry>,
    storage: &Arc<dyn StorageBackend>,
    platform: &PlatformContext,
    state: &ClientState,
    deep_link: Option<String>,
) {
    for account in registry.accounts() {
        start_one(
            registry,
            storage,
            platform,
            state,
            deep_link.clone(),
            &account,
        );
    }
}

/// Start one session. Existing sessions keep their workers.
pub fn start_one(
    registry: &Arc<SessionRegistry>,
    storage: &Arc<dyn StorageBackend>,
    platform: &PlatformContext,
    state: &ClientState,
    deep_link: Option<String>,
    account: &str,
) {
    let Some(client) = registry.client(account) else {
        return;
    };
    let Some(events) = registry.events(account) else {
        return;
    };
    let Some(shutdown) = registry.shutdown_receiver(account) else {
        return;
    };
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        let camera = setting_string(storage, "calls.camera");
        let microphone = setting_string(storage, "calls.microphone");
        let sfu = setting_string(storage, "calls.sfu");
        crate::install_call_media(&client, client.disco.clone(), camera, microphone, sfu);
        let media_client = client.clone();
        spawn(async move {
            media_client.start_call_media_listener();
        });
    }

    if let Some(receiver) = registry.take_receiver(account) {
        let notifier = Arc::new(SessionNotifier {
            account: account.to_string(),
            active: state.active_account,
            revision: state.revision,
            connection_state: state.connection_state,
        });
        let processor = EventProcessor::new_with_jingle_and_conference(
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
        .with_notifier(notifier);
        let mut shutdown = shutdown.clone();
        spawn(async move {
            tokio::select! {
                () = shutdown_requested(&mut shutdown) => {
                    tracing::debug!("event processor stopped");
                }
                result = processor.start(receiver) => {
                    if let Err(error) = result {
                        tracing::error!("event processor stopped: {error}");
                    }
                }
            }
        });
    }

    let account = account.to_string();
    spawn(forward_stanzas(
        client.connection.clone(),
        events.clone(),
        shutdown.clone(),
    ));
    spawn(persist(
        storage.clone(),
        client.clone(),
        events.clone(),
        account.clone(),
        shutdown.clone(),
    ));
    spawn(watch_spaces(
        storage.clone(),
        client.clone(),
        events.clone(),
        account.clone(),
        state.clone(),
        shutdown.clone(),
    ));
    spawn(notify(
        storage.clone(),
        platform.clone(),
        client.clone(),
        events.clone(),
        account.clone(),
        state.clone(),
        shutdown.clone(),
    ));
    spawn(record_calls(
        client.clone(),
        events,
        account.clone(),
        state.clone(),
        shutdown.clone(),
    ));
    spawn(connect(
        storage.clone(),
        client,
        account,
        state.clone(),
        deep_link,
        shutdown,
    ));
}

struct SessionNotifier {
    account: String,
    active: Signal<Option<String>>,
    revision: Signal<u64>,
    connection_state: Signal<ConnectionState>,
}

impl SessionNotifier {
    fn is_active(&self) -> bool {
        self.active.read().as_deref() == Some(self.account.as_str())
    }
}

impl ProcessorNotifier for SessionNotifier {
    fn bump(&self) {
        if self.is_active() {
            let mut revision = self.revision;
            let next = *revision.read() + 1;
            revision.set(next);
        }
    }

    fn set_connection_state(&self, state: ConnectionState) {
        let mut revision = self.revision;
        let next = *revision.read() + 1;
        revision.set(next);
        if self.is_active() {
            let mut connection_state = self.connection_state;
            connection_state.set(state);
        }
    }
}

/// Wait until the session asks its workers to stop.
async fn shutdown_requested(shutdown: &mut watch::Receiver<bool>) {
    if *shutdown.borrow() {
        return;
    }
    while shutdown.changed().await.is_ok() {
        if *shutdown.borrow() {
            return;
        }
    }
}

async fn connect(
    storage: Arc<dyn StorageBackend>,
    client: XmppClient,
    account: String,
    mut state: ClientState,
    deep_link: Option<String>,
    mut shutdown: watch::Receiver<bool>,
) {
    let active = state.active_account.read().as_deref() == Some(account.as_str());
    if account.is_empty() {
        return;
    }
    if active {
        state.connection_state.set(ConnectionState::Connecting);
    }
    let result = tokio::select! {
        () = shutdown_requested(&mut shutdown) => return,
        result = client.connection.connect() => result,
    };
    match result {
        Ok(()) => {
            tracing::info!("account {account} connected");
            client.refresh_space_rooms().await;
            // The roster usually arrives just after authentication; fetching
            // spaces before it would miss every contact in the UI.
            for _ in 0..100 {
                if !client.roster.list().is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            client.refresh_stories().await;
            if active {
                state.own_presence.set(PresenceChoice::from_show(
                    client.connection.config().presence_show.as_deref(),
                ));
                state.bump();
                apply_deep_link(&storage, &client, &account, &mut state, deep_link).await;
            }
        }
        Err(error) => {
            let message = error.to_string();
            tracing::warn!("account {account} could not connect: {message}");
            if active {
                state.last_error.set(Some(message.clone()));
                state
                    .connection_state
                    .set(ConnectionState::Failed { reason: message });
            }
        }
    }
}

async fn apply_deep_link(
    storage: &Arc<dyn StorageBackend>,
    client: &XmppClient,
    account: &str,
    state: &mut ClientState,
    deep_link: Option<String>,
) {
    let Some(link) = deep_link.as_deref().and_then(ui::deeplink::DeepLink::parse) else {
        return;
    };
    match link {
        ui::deeplink::DeepLink::Blog { jid } | ui::deeplink::DeepLink::Post { jid, .. } => {
            if let Ok(jid) = jid.parse::<Jid>() {
                let _ = client.fetch_microblog(&jid, 30);
                state.bump();
            }
        }
        ui::deeplink::DeepLink::Story { jid, item } => {
            if let Ok(jid) = jid.parse::<Jid>() {
                let _ = client.fetch_stories_now(&jid).await;
                let feed = ui::StoryFeed::load(client, account, storage.as_ref());
                if let Some(index) = feed.index_of(&jid, &item) {
                    state.story_view.set(feed.view_at(index));
                }
                state.pending_story.set(None);
                state.bump();
            }
        }
        _ => {}
    }
}

async fn forward_stanzas(
    connection: Arc<Connection>,
    events: Arc<EventBus>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut rx = events.subscribe();
    loop {
        let event = tokio::select! {
            () = shutdown_requested(&mut shutdown) => break,
            event = rx.recv() => match event {
                Ok(event) => event,
                Err(_) => break,
            },
        };
        if let XmppEvent::SendStanza(xml) = event {
            if let Err(error) = connection.send_stanza_str(&xml) {
                tracing::warn!("Failed to send queued stanza: {error}");
            }
        }
    }
}

async fn watch_spaces(
    storage: Arc<dyn StorageBackend>,
    client: XmppClient,
    events: Arc<EventBus>,
    account: String,
    mut state: ClientState,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut rx = events.subscribe();
    loop {
        let event = tokio::select! {
            () = shutdown_requested(&mut shutdown) => break,
            event = rx.recv() => match event {
                Ok(event) => event,
                Err(_) => break,
            },
        };
        if !matches!(event, XmppEvent::SpacesChanged) {
            continue;
        }
        let spaces = client.spaces.list();
        for space in &spaces {
            let _ = storage.save_space(&account, space);
        }
        if state.active_account.read().as_deref() == Some(account.as_str()) {
            state
                .spaces
                .set(spaces.into_iter().map(Into::into).collect());
        }
    }
}

async fn persist(
    storage: Arc<dyn StorageBackend>,
    client: XmppClient,
    events: Arc<EventBus>,
    account: String,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut rx = events.subscribe();
    loop {
        let event = tokio::select! {
            () = shutdown_requested(&mut shutdown) => break,
            event = rx.recv() => match event {
                Ok(event) => event,
                Err(_) => break,
            },
        };
        match event {
            XmppEvent::MessageReceived { jid, message }
            | XmppEvent::MessageSent { jid, message } => {
                if message.is_edited || message.is_retracted {
                    let _ = storage.delete_translations_for_message(&account, &jid, &message.id);
                }
                let _ = storage.save_message_for(&account, &message);
                if let Some(chat) = client.chats.get_by_bare(&jid) {
                    let last = chat.last_message();
                    let previous = storage.get_chat(&account, &jid).ok().flatten();
                    let stored = StoredChat {
                        account_jid: account.clone(),
                        peer_jid: jid.clone(),
                        unread_count: chat.unread_count(),
                        last_message_id: last.as_ref().map(|m| m.id.clone()),
                        last_activity: last.map_or_else(chrono::Utc::now, |m| m.timestamp),
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
                if message.is_edited || message.is_retracted {
                    let _ = storage.delete_translations_for_message(&account, &jid, &message.id);
                }
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
            XmppEvent::MessageUpdated { jid, message_id }
            | XmppEvent::MessageDeleted { jid, message_id } => {
                let _ = storage.delete_translations_for_message(&account, &jid, &message_id);
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
                        last_activity: last.map_or_else(chrono::Utc::now, |m| m.timestamp),
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

async fn notify(
    storage: Arc<dyn StorageBackend>,
    platform: PlatformContext,
    client: XmppClient,
    events: Arc<EventBus>,
    account: String,
    state: ClientState,
    mut shutdown: watch::Receiver<bool>,
) {
    let started_at = chrono::Utc::now();
    let mut rx = events.subscribe();
    loop {
        let event = tokio::select! {
            () = shutdown_requested(&mut shutdown) => break,
            event = rx.recv() => match event {
                Ok(event) => event,
                Err(_) => break,
            },
        };
        let active = state.active_account.read().as_deref() == Some(account.as_str());
        let global_enabled = storage
            .get_setting("ui.notifications")
            .ok()
            .flatten()
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        let enabled = global_enabled
            && if active {
                *state.notifications_enabled.read()
            } else {
                storage::load_account_preferences(storage.as_ref(), &account)
                    .is_none_or(|preferences| preferences.notifications)
            };
        if !enabled {
            continue;
        }
        let language = state.language.read().clone();
        let i18n = jabu_i18n::Translator::for_tag(&language);
        match event {
            XmppEvent::MessageReceived { jid, message } => {
                let Some(body) = message.body.clone().filter(|body| !body.is_empty()) else {
                    continue;
                };
                if message.is_own || (active && *state.selected_chat.read() == Some(jid.clone())) {
                    continue;
                }
                if client
                    .chats
                    .get_by_bare(&jid)
                    .is_some_and(|chat| !chat.preferences().notifications_enabled)
                {
                    continue;
                }
                let title = client
                    .roster
                    .get_by_bare(&jid)
                    .map_or_else(|| jid.clone(), |item| item.display_name());
                show(
                    &platform,
                    Notification {
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
                    },
                );
            }
            XmppEvent::MucMessageReceived { jid, message } => {
                if message.is_own || (active && *state.selected_chat.read() == Some(jid.clone())) {
                    continue;
                }
                let now = chrono::Utc::now();
                if message.timestamp < started_at
                    || message.timestamp < now - chrono::Duration::minutes(2)
                {
                    continue;
                }
                let Some(body) = message.body.clone().filter(|body| !body.is_empty()) else {
                    continue;
                };
                let Some(space) = client
                    .spaces
                    .list()
                    .into_iter()
                    .find(|space| space.rooms.iter().any(|room| room.conference == jid))
                else {
                    continue;
                };
                let own_bare = client
                    .connection
                    .jid()
                    .map(|jid| jid.to_bare().to_string())
                    .unwrap_or_default();
                let self_nick = client
                    .mucs
                    .get_by_bare(&jid)
                    .and_then(|room| room.self_nick());
                let is_mention = message.mentions_user(&own_bare, self_nick.as_deref());
                if !space.allows_room_notification(&jid, is_mention, now) {
                    continue;
                }
                let fallback = jid.split('@').next().unwrap_or(jid.as_str()).to_string();
                let room_name = client
                    .mucs
                    .get_by_bare(&jid)
                    .map(|room| {
                        if room.name.is_empty() {
                            fallback.clone()
                        } else {
                            room.name
                        }
                    })
                    .unwrap_or(fallback);
                show(
                    &platform,
                    Notification {
                        id: message.id.clone(),
                        title: format!("{} · #{}", space.name, room_name),
                        body,
                        icon: None,
                        tag: Some(jid),
                        data: None,
                        actions: Vec::new(),
                        require_interaction: false,
                        silent: false,
                        timestamp: chrono::Utc::now(),
                    },
                );
            }
            XmppEvent::IncomingCall { from, kind, .. } => {
                tracing::info!("incoming call for {account} from {from}");
                // The active session shows its call dialog; background calls
                // need a notification so the user can switch accounts.
                if active {
                    continue;
                }
                let title = client
                    .roster
                    .get_by_bare(&from)
                    .map_or_else(|| from.clone(), |item| item.display_name());
                show(
                    &platform,
                    Notification {
                        id: format!("call-{from}"),
                        title,
                        body: if kind == "video" {
                            i18n.t("chat-incoming-video")
                        } else if kind == "audio" {
                            i18n.t("chat-incoming-audio")
                        } else {
                            i18n.t("chat-incoming-call")
                        },
                        icon: None,
                        tag: Some(from),
                        data: None,
                        actions: Vec::new(),
                        require_interaction: true,
                        silent: false,
                        timestamp: chrono::Utc::now(),
                    },
                );
            }
            XmppEvent::PubSubItemPublished { node_id, item, .. } => {
                let is_story = node_id == xmpp_core::STORIES_NODE;
                let now = chrono::Utc::now();
                if (node_id != xmpp_core::MICROBLOG_NODE && !is_story)
                    || (item.title.is_none() && item.body.is_none())
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
                            i18n.t("notify-new-story")
                        } else {
                            i18n.t("notify-new-post")
                        }
                    });
                let author = client.roster.get(&item.publisher).map_or_else(
                    || item.publisher.to_bare().to_string(),
                    |item| item.display_name(),
                );
                let title = if is_story {
                    let mut args = jabu_i18n::Args::new();
                    args.set("author", author);
                    i18n.t_args("notify-story-title", &args)
                } else {
                    author
                };
                show(
                    &platform,
                    Notification {
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
                    },
                );
            }
            _ => {}
        }
    }
}

fn setting_string(storage: &Arc<dyn StorageBackend>, key: &str) -> Option<String> {
    storage.get_setting(key).ok().flatten().and_then(|value| {
        value
            .as_str()
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

async fn record_calls(
    client: XmppClient,
    events: Arc<EventBus>,
    account: String,
    mut state: ClientState,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut rx = events.subscribe();
    loop {
        let event = tokio::select! {
            () = shutdown_requested(&mut shutdown) => break,
            event = rx.recv() => match event {
                Ok(event) => event,
                Err(_) => break,
            },
        };
        let XmppEvent::CallEnded { sid, .. } = event else {
            continue;
        };
        let Some(session) = client.call_session(&sid) else {
            continue;
        };
        let peer = session.peer.to_bare().to_string();
        let is_room = client.mucs.get_by_bare(&peer).is_some()
            || client
                .spaces
                .list()
                .iter()
                .any(|space| space.rooms.iter().any(|room| room.conference == peer));
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
            tracing::warn!("Failed to record call event for {account}: {error}");
        } else if state.active_account.read().as_deref() == Some(account.as_str()) {
            state.bump();
        }
    }
}

fn show(platform: &PlatformContext, notification: Notification) {
    if let Err(error) = platform.notifications.show_notification(&notification) {
        tracing::warn!("Failed to show notification: {error}");
    }
}
