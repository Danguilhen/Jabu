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
        spawn(async move {
            if let Err(error) = processor.start(receiver).await {
                tracing::error!("event processor stopped: {error}");
            }
        });
    }

    let account = account.to_string();
    spawn(forward_stanzas(client.connection.clone(), events.clone()));
    spawn(persist(
        storage.clone(),
        client.clone(),
        events.clone(),
        account.clone(),
    ));
    spawn(watch_spaces(
        storage.clone(),
        client.clone(),
        events.clone(),
        account.clone(),
        state.clone(),
    ));
    spawn(notify(
        storage.clone(),
        platform.clone(),
        client.clone(),
        events,
        account.clone(),
        state.clone(),
    ));
    spawn(connect(
        storage.clone(),
        client,
        account,
        state.clone(),
        deep_link,
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

async fn connect(
    storage: Arc<dyn StorageBackend>,
    client: XmppClient,
    account: String,
    mut state: ClientState,
    deep_link: Option<String>,
) {
    let active = state.active_account.read().as_deref() == Some(account.as_str());
    if account.is_empty() {
        return;
    }
    if active {
        state.connection_state.set(ConnectionState::Connecting);
    }
    match client.connection.connect().await {
        Ok(()) => {
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
            if active {
                state.last_error.set(Some(message.clone()));
                state
                    .connection_state
                    .set(ConnectionState::Failed { reason: message });
            } else {
                tracing::warn!("background account {account} could not connect: {message}");
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

async fn forward_stanzas(connection: Arc<Connection>, events: Arc<EventBus>) {
    let mut rx = events.subscribe();
    while let Ok(event) = rx.recv().await {
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
) {
    let mut rx = events.subscribe();
    while let Ok(event) = rx.recv().await {
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
) {
    let mut rx = events.subscribe();
    while let Ok(event) = rx.recv().await {
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
) {
    let started_at = chrono::Utc::now();
    let mut rx = events.subscribe();
    while let Ok(event) = rx.recv().await {
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
                            "New story".to_string()
                        } else {
                            "New post".to_string()
                        }
                    });
                let author = client.roster.get(&item.publisher).map_or_else(
                    || item.publisher.to_bare().to_string(),
                    |item| item.display_name(),
                );
                let title = if is_story {
                    format!("📝 New Story from {author}")
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

fn show(platform: &PlatformContext, notification: Notification) {
    if let Err(error) = platform.notifications.show_notification(&notification) {
        tracing::warn!("Failed to show notification: {error}");
    }
}
