//! UI components for Jabu

use chrono::{DateTime, Utc};
use dioxus::html::FileEngine;
use dioxus::prelude::*;
use std::sync::Arc;
use storage::{SettingsOmemoStore, StorageBackend, StoredCredentials};
use xmpp_core::{
    chat::ChatStateNotification,
    muc::{MucAffiliation, MucRole},
    omemo::TrustLevel,
    roster::PresenceShow,
    ChatMessage, ClientState, ConnectionState, MucMessage, RosterItem, XmppClient,
};

/// Main app component
#[component]
pub fn App() -> Element {
    let client_state = use_context::<ClientState>();
    let has_account = client_state
        .config
        .read()
        .as_ref()
        .map(|c| !c.jid.trim().is_empty())
        .unwrap_or(false);

    if !has_account {
        return rsx! { LoginScreen {} };
    }

    rsx! {
        div { class: "app h-screen w-screen flex flex-col bg-gray-900 text-white",
            ConnectionStatusBar {}
            div { class: "flex-1 flex overflow-hidden",
                Sidebar {}
                MainChatArea {}
            }
        }
    }
}

#[component]
fn LoginScreen() -> Element {
    let client = use_context::<XmppClient>();
    let client_state = use_context::<ClientState>();
    let storage = use_context::<Arc<dyn StorageBackend>>();
    let mut state = client_state.clone();

    let mut jid = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut resource = use_signal(|| "jabu".to_string());
    let mut remember = use_signal(|| true);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| Option::<String>::None);

    let submit = move |_| {
        let account = jid.read().trim().to_string();
        let pass = password.read().clone();
        if !account.contains('@') {
            error.set(Some("Enter a full JID, e.g. user@example.com".to_string()));
            return;
        }
        if pass.is_empty() {
            error.set(Some("Enter your password".to_string()));
            return;
        }

        let mut config = state.config.read().clone().unwrap_or_default();
        config.jid = account.clone();
        config.password = Some(pass.clone());
        let res = resource.read().trim().to_string();
        config.resource = if res.is_empty() { None } else { Some(res) };

        if *remember.read() {
            let credentials = StoredCredentials {
                jid: account.clone(),
                password: Some(pass),
                resource: config.resource.clone(),
            };
            if let Err(e) = storage::save_credentials(storage.as_ref(), &credentials) {
                tracing::warn!("Failed to save credentials: {}", e);
            }
        } else if let Err(e) = storage::clear_credentials(storage.as_ref()) {
            tracing::warn!("Failed to clear credentials: {}", e);
        }

        client.connection.set_config(config.clone().into());
        client.omemo.reset();
        client
            .omemo
            .set_store(Arc::new(SettingsOmemoStore::new(storage.clone(), &account)));
        if let Err(e) = client.omemo.load_persisted() {
            tracing::warn!("Failed to load OMEMO state: {}", e);
        }
        client.omemo.ensure_local_auto(100);

        state.config.set(Some(config));
        state.bump();
        busy.set(true);
        error.set(None);

        let connect_client = client.clone();
        spawn(async move {
            if let Err(e) = connect_client.connection.connect().await {
                error.set(Some(e.to_string()));
                busy.set(false);
            }
        });
    };

    rsx! {
        div { class: "h-screen w-screen flex items-center justify-center bg-gray-900 text-white",
            div { class: "w-96 rounded-xl bg-gray-800 p-6 space-y-4 shadow-xl",
                div { class: "text-center",
                    h1 { class: "text-xl font-semibold", "Jabu" }
                    p { class: "text-xs text-gray-400", "Sign in to your XMPP account" }
                }
                label { class: "block text-xs text-gray-400",
                    "JID"
                    input {
                        class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm focus:outline-none focus:ring-1 focus:ring-teal-500",
                        placeholder: "user@example.com",
                        autocomplete: "username",
                        value: "{jid}",
                        oninput: move |e| jid.set(e.value()),
                    }
                }
                label { class: "block text-xs text-gray-400",
                    "Password"
                    input {
                        class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm focus:outline-none focus:ring-1 focus:ring-teal-500",
                        r#type: "password",
                        autocomplete: "current-password",
                        value: "{password}",
                        oninput: move |e| password.set(e.value()),
                    }
                }
                label { class: "block text-xs text-gray-400",
                    "Resource"
                    input {
                        class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm focus:outline-none focus:ring-1 focus:ring-teal-500",
                        value: "{resource}",
                        oninput: move |e| resource.set(e.value()),
                    }
                }
                label { class: "flex items-center gap-2 text-xs text-gray-400",
                    input {
                        r#type: "checkbox",
                        checked: *remember.read(),
                        onchange: move |e| remember.set(e.checked()),
                    }
                    "Remember this account"
                }
                if let Some(err) = error.read().as_ref() {
                    p { class: "text-xs text-red-400", "{err}" }
                }
                button {
                    class: "w-full py-2 rounded bg-teal-600 hover:bg-teal-500 disabled:opacity-50",
                    disabled: *busy.read(),
                    onclick: submit,
                    if *busy.read() { "Connecting…" } else { "Sign in" }
                }
            }
        }
    }
}

/// Connection status bar
#[component]
fn ConnectionStatusBar() -> Element {
    let client_state = use_context::<ClientState>();
    let state = (*client_state.connection_state.read()).clone();
    let error = (*client_state.last_error.read()).clone();

    rsx! {
        div { class: "flex items-center justify-between px-4 py-2 bg-gray-800 border-b border-gray-700",
            div { class: "flex items-center gap-3",
                div { class: "flex items-center gap-2",
                    ConnectionIndicator { state: state.clone() }
                    span { class: "text-sm font-medium", "{format_connection_state(&state)}" }
                }
                if let Some(err) = error {
                    span { class: "text-xs text-red-400 truncate max-w-md", "{err}" }
                }
            }
            div { class: "flex items-center gap-4",
                div { class: "relative",
                    input {
                        class: "w-64 px-3 py-1.5 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-teal-500 focus:border-transparent",
                        placeholder: "Search...",
                        r#type: "text",
                    }
                }
                UserMenu {}
            }
        }
    }
}

/// Format connection state for display
fn format_connection_state(state: &ConnectionState) -> &'static str {
    match state {
        ConnectionState::Disconnected => "Disconnected",
        ConnectionState::Connecting => "Connecting",
        ConnectionState::Connected => "Connected",
        ConnectionState::Authenticated => "Authenticated",
        ConnectionState::Reconnecting { .. } => "Reconnecting",
        ConnectionState::Failed { .. } => "Failed",
    }
}

/// Connection indicator dot
#[component]
fn ConnectionIndicator(state: ConnectionState) -> Element {
    let (color, pulse) = match state {
        ConnectionState::Connected | ConnectionState::Authenticated => ("bg-green-500", true),
        ConnectionState::Connecting | ConnectionState::Reconnecting { .. } => {
            ("bg-yellow-500", true)
        }
        ConnectionState::Failed { .. } => ("bg-red-500", false),
        _ => ("bg-gray-500", false),
    };

    rsx! {
        div { class: "relative",
            div { class: "w-2.5 h-2.5 rounded-full {color}" }
            if pulse {
                div { class: "absolute inset-0 w-2.5 h-2.5 rounded-full {color} animate-ping opacity-75" }
            }
        }
    }
}

/// User menu dropdown
#[component]
fn UserMenu() -> Element {
    let client = use_context::<XmppClient>();
    let client_state = use_context::<ClientState>();
    let own_bare = client
        .connection
        .jid()
        .map(|j| j.to_bare().to_string())
        .unwrap_or_default();
    let own = client.roster.get_by_bare(&own_bare);
    let name = own
        .as_ref()
        .and_then(|i| i.name.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| own_bare.clone());
    let avatar_src = own.as_ref().and_then(avatar_data_uri);
    let storage = use_context::<Arc<dyn StorageBackend>>();
    let mut cs = client_state.clone();
    let mut config_signal = client_state.config;
    let disconnect_client = client.clone();
    let sign_out = move |_| {
        let client = disconnect_client.clone();
        let storage = storage.clone();
        let mut state = cs.clone();
        spawn(async move {
            let _ = storage::clear_credentials(storage.as_ref());
            config_signal.set(None);
            state.bump();
            let _ = client.connection.disconnect().await;
        });
    };

    rsx! {
        div { class: "relative group",
            button { class: "flex items-center gap-2 p-2 rounded-lg hover:bg-gray-700 transition-colors",
                div { class: "w-8 h-8 rounded-full bg-teal-500 flex items-center justify-center font-medium text-white overflow-hidden",
                    if let Some(src) = avatar_src {
                        img { class: "w-full h-full object-cover", src: "{src}" }
                    } else {
                        "U"
                    }
                }
                span { class: "hidden sm:block text-sm", "{name}" }
                svg { class: "w-4 h-4", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M19 9l-7 7-7-7" }
                }
            }
            div { class: "absolute right-0 mt-2 w-48 bg-gray-800 rounded-lg shadow-lg border border-gray-700 opacity-0 invisible group-hover:opacity-100 group-hover:visible transition-all",
                div { class: "py-1",
                    a { class: "block px-4 py-2 text-sm text-gray-300 hover:bg-gray-700", href: "#", "Profile" }
                    a { class: "block px-4 py-2 text-sm text-gray-300 hover:bg-gray-700", href: "#", "Settings" }
                    hr { class: "my-1 border-gray-700" }
                    button {
                        class: "block w-full text-left px-4 py-2 text-sm text-red-400 hover:bg-gray-700",
                        onclick: sign_out,
                        "Sign Out"
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum SidebarTab {
    Chats,
    Contacts,
    Rooms,
    Feed,
}

/// Sidebar with chats, contacts and rooms
#[component]
fn Sidebar() -> Element {
    let client_state = use_context::<ClientState>();
    let mut tab = use_signal(|| SidebarTab::Chats);
    let mut show_profile = use_signal(|| false);
    let mut show_feed = client_state.show_feed;

    rsx! {
        aside { class: "w-80 flex flex-col border-r border-gray-700 bg-gray-800",
            div { class: "flex border-b border-gray-700",
                TabButton {
                    label: "Chats",
                    active: *tab.read() == SidebarTab::Chats,
                    onclick: move |_| {
                        tab.set(SidebarTab::Chats);
                        show_feed.set(false);
                    },
                }
                TabButton {
                    label: "Contacts",
                    active: *tab.read() == SidebarTab::Contacts,
                    onclick: move |_| {
                        tab.set(SidebarTab::Contacts);
                        show_feed.set(false);
                    },
                }
                TabButton {
                    label: "Rooms",
                    active: *tab.read() == SidebarTab::Rooms,
                    onclick: move |_| {
                        tab.set(SidebarTab::Rooms);
                        show_feed.set(false);
                    },
                }
                TabButton {
                    label: "Feed",
                    active: *tab.read() == SidebarTab::Feed,
                    onclick: move |_| {
                        tab.set(SidebarTab::Feed);
                        show_feed.set(true);
                    },
                }
            }
            div { class: "flex-1 overflow-y-auto",
                match *tab.read() {
                    SidebarTab::Chats => rsx! { ChatList {} },
                    SidebarTab::Contacts => rsx! { ContactsList {} },
                    SidebarTab::Rooms => rsx! { RoomsList {} },
                    SidebarTab::Feed => rsx! {
                        div { class: "p-6 text-center text-sm text-gray-500",
                            "Your social feed is open in the main area."
                        }
                    },
                }
            }
            div { class: "p-2 border-t border-gray-700",
                button {
                    class: "w-full py-2 rounded bg-gray-700 hover:bg-gray-600 text-sm",
                    onclick: move |_| {
                        let next = !*show_profile.read();
                        show_profile.set(next);
                    },
                    "Profile"
                }
            }
            if *show_profile.read() {
                ProfileEditor { onclose: move |_| show_profile.set(false) }
            }
        }
    }
}

#[component]
fn ProfileEditor(onclose: EventHandler<()>) -> Element {
    let client = use_context::<XmppClient>();
    let client_state = use_context::<ClientState>();
    let own_bare = client
        .connection
        .jid()
        .map(|j| j.to_bare().to_string())
        .unwrap_or_default();
    let item = client.roster.get_by_bare(&own_bare);
    let current_avatar = item.as_ref().and_then(avatar_data_uri);

    let mut nickname = use_signal(|| {
        item.as_ref()
            .and_then(|r| r.name.clone())
            .unwrap_or_default()
    });
    let mut avatar = use_signal(|| Option::<(Vec<u8>, String)>::None);
    let mut error = use_signal(|| Option::<String>::None);
    let mut saving = use_signal(|| false);

    let mut preview = current_avatar.clone();
    if let Some((bytes, mime)) = avatar.read().as_ref() {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        preview = Some(format!("data:{};base64,{}", mime, STANDARD.encode(bytes)));
    }

    let pick_avatar = move |e: FormEvent| {
        let Some(engine) = e.files() else {
            return;
        };
        let Some(name) = engine.files().into_iter().next() else {
            return;
        };
        let mime = mime_type_for(&name).to_string();
        spawn(async move {
            if let Some(bytes) = engine.read_file(&name).await {
                avatar.set(Some((bytes, mime)));
            }
        });
    };

    let save = move |_| {
        let mut saving = saving;
        let mut error = error;
        let mut cs = client_state.clone();
        let client = client.clone();
        let nick = nickname.read().clone();
        let avatar = avatar.read().clone();
        saving.set(true);
        error.set(None);
        spawn(async move {
            let avatar_ref = avatar
                .as_ref()
                .map(|(bytes, mime)| (bytes.as_slice(), mime.as_str()));
            match client.set_vcard(Some(&nick), Some(&nick), avatar_ref) {
                Ok(()) => {
                    cs.bump();
                    onclose.call(());
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            saving.set(false);
        });
    };

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/60",
            onclick: move |_| onclose.call(()),
            div {
                class: "w-96 rounded-lg bg-gray-800 p-4 space-y-3",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center justify-between",
                    h3 { class: "font-medium", "Profile" }
                    button {
                        class: "text-gray-400 hover:text-white",
                        onclick: move |_| onclose.call(()),
                        "✕"
                    }
                }
                if let Some(src) = preview {
                    img { class: "w-16 h-16 rounded-full object-cover", src: "{src}" }
                }
                label { class: "block text-xs text-gray-400",
                    "Display name"
                    input {
                        class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-white focus:outline-none focus:ring-1 focus:ring-teal-500",
                        value: "{nickname}",
                        oninput: move |e| nickname.set(e.value()),
                    }
                }
                label { class: "block text-xs text-gray-400",
                    "Avatar"
                    input {
                        class: "mt-1 text-xs",
                        r#type: "file",
                        accept: "image/*",
                        onchange: pick_avatar,
                    }
                }
                if let Some(err) = error.read().as_ref() {
                    p { class: "text-xs text-red-400", "{err}" }
                }
                button {
                    class: "w-full py-2 rounded bg-teal-600 hover:bg-teal-500 disabled:opacity-50",
                    disabled: *saving.read(),
                    onclick: save,
                    if *saving.read() { "Saving…" } else { "Save" }
                }
            }
        }
    }
}

/// Tab button
#[component]
fn TabButton(label: &'static str, active: bool, onclick: EventHandler<MouseEvent>) -> Element {
    let class = if active {
        "flex-1 py-3 px-4 text-center text-sm font-medium text-teal-400 border-b-2 border-teal-400"
    } else {
        "flex-1 py-3 px-4 text-center text-sm font-medium text-gray-400 hover:text-white hover:bg-gray-700"
    };

    rsx! {
        button { class: "{class} transition-colors", onclick: move |e| onclick.call(e), "{label}" }
    }
}

/// Coarse timestamp formatting for the conversation list.
fn format_time(t: DateTime<Utc>) -> String {
    let now = Utc::now();
    let secs = (now - t).num_seconds();
    if secs < 60 {
        "now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else if secs < 7 * 86_400 {
        format!("{}d ago", secs / 86_400)
    } else {
        t.format("%Y-%m-%d").to_string()
    }
}

fn initial_of(name: &str) -> String {
    name.chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string())
}

/// A conversation entry in the chat list (1:1 chat or MUC room).
#[derive(Clone, PartialEq)]
struct ConvEntry {
    bare: String,
    name: String,
    last_text: String,
    last_time: Option<DateTime<Utc>>,
    unread: usize,
    online: bool,
    is_muc: bool,
    avatar_src: Option<String>,
}

/// Build a data URI for a contact's vCard avatar, if present.
fn avatar_data_uri(item: &RosterItem) -> Option<String> {
    item.avatar_data.as_ref().map(|d| {
        let mime = item
            .avatar_mime
            .clone()
            .unwrap_or_else(|| "image/png".to_string());
        format!("data:{};base64,{}", mime, d)
    })
}

/// Best-effort MIME type from a file name extension.
fn mime_type_for(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "mp4" => "video/mp4",
        "mp3" => "audio/mpeg",
        _ => "application/octet-stream",
    }
}

/// Chat list (conversations)
#[component]
fn ChatList() -> Element {
    let client_state = use_context::<ClientState>();
    let client = use_context::<XmppClient>();

    // Subscribe to data changes.
    let _rev = *client_state.revision.read();
    let selected = (*client_state.selected_chat.read()).clone();
    let mut query = use_signal(String::new);

    let mut entries: Vec<ConvEntry> = Vec::new();

    for chat in client.chats.list() {
        let bare = chat.bare_jid.clone();
        let roster = client.roster.get(&chat.jid);
        let name = roster
            .as_ref()
            .map(|r| r.display_name())
            .unwrap_or_else(|| bare.clone());
        let online = roster.as_ref().map(|r| r.is_online()).unwrap_or(false);
        let avatar_src = roster.as_ref().and_then(avatar_data_uri);
        let last = chat.last_message();
        let (text, time) = match last {
            Some(m) => (m.display_body(), Some(m.timestamp)),
            None => ("No messages yet".to_string(), None),
        };
        entries.push(ConvEntry {
            bare,
            name,
            last_text: text,
            last_time: time,
            unread: chat.unread_count(),
            online,
            is_muc: false,
            avatar_src,
        });
    }

    for room in client.mucs.list() {
        let bare = room.bare_jid.clone();
        let name = if room.name.is_empty() {
            bare.clone()
        } else {
            room.name.clone()
        };
        let last = room.last_message();
        let (text, time) = match last {
            Some(m) => (
                m.body.unwrap_or_else(|| "[attachment]".to_string()),
                Some(m.timestamp),
            ),
            None => ("No messages yet".to_string(), None),
        };
        entries.push(ConvEntry {
            bare,
            name,
            last_text: text,
            last_time: time,
            unread: room.unread_count(),
            online: true,
            is_muc: true,
            avatar_src: None,
        });
    }

    entries.sort_by(|a, b| b.last_time.cmp(&a.last_time));

    let needle = query.read().trim().to_lowercase();
    if !needle.is_empty() {
        entries.retain(|e| {
            e.name.to_lowercase().contains(&needle) || e.bare.to_lowercase().contains(&needle)
        });
    }

    rsx! {
        div { class: "p-2 space-y-1",
            div { class: "px-1 pb-2",
                input {
                    class: "w-full px-3 py-1.5 rounded bg-gray-800 text-sm text-white placeholder-gray-500 focus:outline-none focus:ring-1 focus:ring-teal-500",
                    r#type: "search",
                    placeholder: "Search conversations",
                    value: "{query}",
                    oninput: move |e| query.set(e.value()),
                }
            }
            if entries.is_empty() {
                div { class: "p-6 text-center text-sm text-gray-500", "No conversations yet" }
            }
            for entry in entries {
                {
                    let bare = entry.bare.clone();
                    let is_selected = selected.as_deref() == Some(entry.bare.as_str());
                    let mut cs = client_state.clone();
                    let client2 = client.clone();
                    let bare2 = bare.clone();
                    rsx! {
                        ChatListItem {
                            entry: entry,
                            selected: is_selected,
                            onselect: move |_| {
                                cs.select(Some(bare2.clone()));
                                if let Some(mut c) = client2.chats.get_by_bare(&bare2) {
                                    if let Some(last) = c.last_message() {
                                        if !last.is_own {
                                            let _ = client2.send_marker(&c.jid, "displayed", &last.id);
                                        }
                                    }
                                    c.mark_read();
                                    let _ = client2.query_mam_str(&bare2, 50);
                                }
                                if client2
                                    .roster
                                    .get_by_bare(&bare2)
                                    .and_then(|r| r.avatar_data)
                                    .is_none()
                                {
                                    let _ = client2.request_vcard_str(&bare2);
                                }
                                if let Some(mut r) = client2.mucs.get_by_bare(&bare2) {
                                    r.mark_read();
                                    let _ = client2.query_muc_mam_str(&bare2, 50);
                                }
                                cs.bump();
                            },
                        }
                    }
                }
            }
        }
    }
}

/// Chat list item
#[component]
fn ChatListItem(entry: ConvEntry, selected: bool, onselect: EventHandler<MouseEvent>) -> Element {
    let avatar = initial_of(&entry.name);
    let avatar_src = entry.avatar_src.clone();
    let name = entry.name.clone();
    let last_text = entry.last_text.clone();
    let time_str = entry.last_time.map(format_time).unwrap_or_default();
    let unread = entry.unread;
    let online = entry.online;
    let is_muc = entry.is_muc;

    let selected_class = if selected { "bg-gray-700" } else { "" };

    rsx! {
        button {
            class: "w-full flex items-center gap-3 p-3 rounded-lg hover:bg-gray-700 transition-colors text-left {selected_class}",
            onclick: move |e| onselect.call(e),
            div { class: "relative",
                div { class: "w-12 h-12 rounded-full bg-teal-500 flex items-center justify-center font-bold text-white text-lg overflow-hidden",
                    if let Some(src) = avatar_src {
                        img { class: "w-full h-full object-cover", src: "{src}", alt: "{avatar}" }
                    } else {
                        "{avatar}"
                    }
                }
                if is_muc {
                    div { class: "absolute -bottom-1 -right-1 w-5 h-5 bg-gray-600 border-2 border-gray-800 rounded-full flex items-center justify-center text-[10px] font-bold",
                        "#"
                    }
                } else if online {
                    div { class: "absolute bottom-0 right-0 w-3 h-3 bg-green-500 border-2 border-gray-800 rounded-full" }
                }
            }
            div { class: "flex-1 min-w-0",
                div { class: "flex items-center justify-between",
                    h4 { class: "font-medium truncate", "{name}" }
                    span { class: "text-xs text-gray-400 whitespace-nowrap", "{time_str}" }
                }
                p { class: "text-sm text-gray-400 truncate", "{last_text}" }
            }
            if unread > 0 {
                span { class: "flex items-center justify-center min-w-5 h-5 px-1 bg-teal-500 text-white text-xs rounded-full",
                    "{unread}"
                }
            }
        }
    }
}

/// Contacts list (roster)
#[component]
fn ContactsList() -> Element {
    let client_state = use_context::<ClientState>();
    let client = use_context::<XmppClient>();

    let _rev = *client_state.revision.read();
    let mut new_jid = use_signal(String::new);
    let mut cs = client_state.clone();
    let client_add = client.clone();

    let add = move |_| {
        let j = new_jid.read().trim().to_string();
        if j.is_empty() {
            return;
        }
        match client_add.add_contact_str(&j, None) {
            Ok(_) => {
                new_jid.set(String::new());
                cs.bump();
            }
            Err(e) => tracing::warn!("Failed to add contact: {}", e),
        }
    };

    let mut items = client.roster.list();
    items.sort_by(|a, b| {
        a.display_name()
            .to_lowercase()
            .cmp(&b.display_name().to_lowercase())
    });

    rsx! {
        div { class: "p-2 space-y-1",
            div { class: "flex gap-1 px-1 pb-1",
                input {
                    class: "flex-1 px-3 py-1.5 bg-gray-700 border border-gray-600 rounded-lg text-sm text-white placeholder-gray-400 focus:outline-none focus:ring-1 focus:ring-teal-500",
                    placeholder: "user@example.com",
                    value: "{new_jid}",
                    oninput: move |e| new_jid.set(e.value()),
                }
                button {
                    class: "px-3 py-1.5 bg-teal-500 hover:bg-teal-400 rounded-lg text-sm text-white",
                    onclick: add,
                    "Add"
                }
            }
            if items.is_empty() {
                div { class: "p-6 text-center text-sm text-gray-500", "No contacts" }
            }
            for item in items {
                ContactRow { item: item }
            }
        }
    }
}

/// A single roster contact row; clicking opens (or creates) a chat.
#[component]
fn ContactRow(item: RosterItem) -> Element {
    let client_state = use_context::<ClientState>();
    let client = use_context::<XmppClient>();

    let name = item.display_name();
    let avatar = initial_of(&name);
    let avatar_src = avatar_data_uri(&item);
    let online = item.is_online();
    let status = presence_text(item.best_show());
    let has_avatar = item.avatar_data.is_some();
    let jid = item.jid.clone();
    let dot = if online {
        "bg-green-500"
    } else {
        "bg-gray-600"
    };

    let mut cs = client_state.clone();
    let client2 = client.clone();

    let mut cs_block = client_state.clone();
    let block_client = client.clone();
    let block_jid = item.jid.clone();
    let blocked = item.is_blocked;
    let toggle_block = move |_| {
        let result = if blocked {
            block_client.roster.unblock(&block_jid)
        } else {
            block_client.roster.block(&block_jid)
        };
        if let Err(e) = result {
            tracing::warn!("Failed to change block state: {}", e);
        }
        cs_block.bump();
    };

    let row_class = if blocked {
        "flex items-center gap-3 p-3 rounded-lg hover:bg-gray-700 transition-colors opacity-60"
    } else {
        "flex items-center gap-3 p-3 rounded-lg hover:bg-gray-700 transition-colors"
    };

    rsx! {
        div { class: "group {row_class}",
            button {
                class: "flex flex-1 items-center gap-3 text-left min-w-0",
                onclick: move |_| {
                    let bare = jid.to_bare().to_string();
                    let _chat = client2.chats.get_or_create(&jid);
                    if !has_avatar {
                        let _ = client2.request_vcard_str(&bare);
                    }
                    cs.select(Some(bare));
                    cs.bump();
                },
                div { class: "relative",
                    div { class: "w-10 h-10 rounded-full bg-teal-500 flex items-center justify-center font-bold text-white overflow-hidden",
                        if let Some(src) = avatar_src {
                            img { class: "w-full h-full object-cover", src: "{src}", alt: "{avatar}" }
                        } else {
                            "{avatar}"
                        }
                    }
                    div { class: "absolute bottom-0 right-0 w-3 h-3 {dot} border-2 border-gray-800 rounded-full" }
                }
                div { class: "flex-1 min-w-0",
                    h4 { class: "font-medium truncate", "{name}" }
                    p { class: "text-xs text-gray-400 truncate", "{status}" }
                }
            }
            button {
                class: "text-xs text-gray-400 hover:text-red-400 opacity-0 group-hover:opacity-100",
                onclick: toggle_block,
                if blocked { "Unblock" } else { "Block" }
            }
        }
    }
}

fn presence_text(show: PresenceShow) -> &'static str {
    match show {
        PresenceShow::Online => "Online",
        PresenceShow::Away => "Away",
        PresenceShow::Dnd => "Do not disturb",
        PresenceShow::Xa => "Extended away",
        PresenceShow::Unavailable => "Offline",
    }
}

/// MUC rooms list
#[component]
fn RoomsList() -> Element {
    let client_state = use_context::<ClientState>();
    let client = use_context::<XmppClient>();

    let _rev = *client_state.revision.read();
    let mut new_room = use_signal(String::new);
    let mut nick_input = use_signal(String::new);
    let mut cs = client_state.clone();
    let client_join = client.clone();

    let join = move |_| {
        let room = new_room.read().trim().to_string();
        if room.is_empty() {
            return;
        }
        let rid = client_join.connection.jid();
        let nick = {
            let n = nick_input.read().trim().to_string();
            if !n.is_empty() {
                n
            } else if let Some(j) = rid {
                j.to_string().split('@').next().unwrap_or("me").to_string()
            } else {
                "me".to_string()
            }
        };
        match client_join.join_room_str(&room, &nick) {
            Ok(_) => {
                new_room.set(String::new());
                cs.bump();
            }
            Err(e) => tracing::warn!("Failed to join room: {}", e),
        }
    };

    let client_create = client.clone();
    let mut cs_create = client_state.clone();
    let create = move |_| {
        let room = new_room.read().trim().to_string();
        if room.is_empty() {
            return;
        }
        let nick = {
            let n = nick_input.read().trim().to_string();
            if !n.is_empty() {
                n
            } else if let Some(j) = client_create.connection.jid() {
                j.to_string().split('@').next().unwrap_or("me").to_string()
            } else {
                "me".to_string()
            }
        };
        match client_create.create_room_str(&room, &nick) {
            Ok(_) => {
                new_room.set(String::new());
                cs_create.bump();
            }
            Err(e) => tracing::warn!("Failed to create room: {}", e),
        }
    };

    let mut rooms = client.mucs.list();
    rooms.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    rsx! {
        div { class: "p-2 space-y-1",
            div { class: "flex flex-col gap-1 px-1 pb-1",
                input {
                    class: "w-full px-3 py-1.5 bg-gray-700 border border-gray-600 rounded-lg text-sm text-white placeholder-gray-400 focus:outline-none focus:ring-1 focus:ring-teal-500",
                    placeholder: "room@conference.example.com",
                    value: "{new_room}",
                    oninput: move |e| new_room.set(e.value()),
                }
                div { class: "flex gap-1",
                    input {
                        class: "flex-1 px-3 py-1.5 bg-gray-700 border border-gray-600 rounded-lg text-sm text-white placeholder-gray-400 focus:outline-none focus:ring-1 focus:ring-teal-500",
                        placeholder: "nickname (optional)",
                        value: "{nick_input}",
                        oninput: move |e| nick_input.set(e.value()),
                    }
                    button {
                        class: "px-3 py-1.5 bg-teal-500 hover:bg-teal-400 rounded-lg text-sm text-white",
                        onclick: join,
                        "Join"
                    }
                    button {
                        class: "px-3 py-1.5 bg-gray-700 hover:bg-gray-600 rounded-lg text-sm text-white",
                        onclick: create,
                        "Create"
                    }
                }
            }
            if rooms.is_empty() {
                div { class: "p-6 text-center text-sm text-gray-500", "No rooms joined" }
            }
            for room in rooms {
                {
                    let bare = room.bare_jid.clone();
                    let name = if room.name.is_empty() { bare.clone() } else { room.name.clone() };
                    let members = room.occupant_count();
                    let avatar = initial_of(&name);
                    let mut cs = client_state.clone();
                    let bare2 = bare.clone();
                    let client2 = client.clone();
                    rsx! {
                        button {
                            class: "w-full flex items-center gap-3 p-3 rounded-lg hover:bg-gray-700 transition-colors text-left",
                            onclick: move |_| {
                                cs.select(Some(bare2.clone()));
                                if let Some(mut r) = client2.mucs.get_by_bare(&bare2) {
                                    r.mark_read();
                                    let _ = client2.query_muc_mam_str(&bare2, 50);
                                }
                                cs.bump();
                            },
                            div { class: "w-10 h-10 rounded-full bg-gray-600 flex items-center justify-center font-bold text-white",
                                "{avatar}"
                            }
                            div { class: "flex-1 min-w-0",
                                h4 { class: "font-medium truncate", "{name}" }
                                p { class: "text-xs text-gray-400", "{members} members" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A normalized message for rendering.
#[derive(Clone, PartialEq)]
struct DisplayMessage {
    id: String,
    is_own: bool,
    sender: String,
    body: String,
    time: String,
    status: String,
    reactions: Vec<(String, u32)>,
    edited: bool,
    encrypted: bool,
}

fn group_reactions<I, F>(reactions: I, emoji_of: F) -> Vec<(String, u32)>
where
    I: IntoIterator,
    F: Fn(&I::Item) -> String,
{
    let mut counts: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
    for r in reactions {
        *counts.entry(emoji_of(&r)).or_insert(0) += 1;
    }
    counts.into_iter().collect()
}

/// Social feed (XEP-0277 microblog) with a composer.
#[component]
fn FeedView() -> Element {
    let client = use_context::<XmppClient>();
    let client_state = use_context::<ClientState>();

    let _rev = *client_state.revision.read();
    let mut draft = use_signal(String::new);
    let mut error = use_signal(|| Option::<String>::None);

    let client_fetch = client.clone();
    use_future(move || {
        let client = client_fetch.clone();
        async move {
            if let Some(own) = client.connection.jid() {
                let _ = client.fetch_microblog(&own.to_bare(), 30);
            }
        }
    });

    let client_post = client.clone();
    let mut cs = client_state.clone();
    let post = move |_| {
        let text = draft.read().trim().to_string();
        if text.is_empty() {
            return;
        }
        match client_post.publish_microblog(&text, None) {
            Ok(_) => {
                draft.set(String::new());
                error.set(None);
                cs.bump();
            }
            Err(e) => error.set(Some(e.to_string())),
        }
    };

    let items = client.pubsub.microblog_items();

    rsx! {
        main { class: "flex-1 flex flex-col bg-gray-900 overflow-y-auto",
            div { class: "p-6 max-w-2xl w-full mx-auto space-y-4",
                div { class: "bg-gray-800 rounded-xl p-4 space-y-3",
                    textarea {
                        class: "w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-teal-500 resize-none min-h-[72px]",
                        placeholder: "Share something with your contacts...",
                        value: "{draft}",
                        oninput: move |e| draft.set(e.value()),
                        rows: 3,
                    }
                    div { class: "flex items-center justify-end",
                        button {
                            class: "px-4 py-2 bg-teal-500 hover:bg-teal-400 rounded-lg text-sm text-white disabled:opacity-50 disabled:cursor-not-allowed",
                            disabled: draft.read().trim().is_empty(),
                            onclick: post,
                            "Post"
                        }
                    }
                    if let Some(err) = error.read().as_ref() {
                        p { class: "text-xs text-red-400", "{err}" }
                    }
                }

                if items.is_empty() {
                    div { class: "text-center text-gray-500 py-10 text-sm", "No posts yet" }
                }
                for item in items {
                    {
                        let author_name = client
                            .roster
                            .get(&item.publisher)
                            .map(|r| r.display_name())
                            .unwrap_or_else(|| item.publisher.to_string());
                        let initial = initial_of(&author_name);
                        let time_str = format_time(item.published_at);
                        let body = item.body.clone().unwrap_or_default();
                        let title = item.title.clone();
                        let images: Vec<String> = item
                            .media
                            .iter()
                            .filter(|m| m.media_type == xmpp_core::MediaType::Image)
                            .map(|m| m.url.clone())
                            .collect();
                        rsx! {
                            div { key: "{item.id}", class: "bg-gray-800 rounded-xl p-4 space-y-2",
                                div { class: "flex items-center gap-3",
                                    div { class: "w-9 h-9 rounded-full bg-teal-500 flex items-center justify-center font-bold text-white text-sm",
                                        "{initial}"
                                    }
                                    div { class: "min-w-0",
                                        div { class: "font-medium truncate", "{author_name}" }
                                        span { class: "text-xs text-gray-400", "{time_str}" }
                                    }
                                }
                                if let Some(title) = title {
                                    h3 { class: "font-semibold", "{title}" }
                                }
                                p { class: "text-sm whitespace-pre-wrap break-words", "{body}" }
                                for url in images {
                                    img { class: "rounded-lg max-h-80 object-cover", src: "{url}", alt: "Attachment" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Main chat area
#[component]
fn MainChatArea() -> Element {
    let client_state = use_context::<ClientState>();
    let client = use_context::<XmppClient>();

    if *client_state.show_feed.read() {
        return rsx! { FeedView {} };
    }

    let _rev = *client_state.revision.read();
    let selected = (*client_state.selected_chat.read()).clone();
    let mut reply_target = use_signal(|| Option::<(String, String)>::None);

    let Some(bare) = selected else {
        return rsx! {
            main { class: "flex-1 flex flex-col items-center justify-center bg-gray-900 text-gray-500",
                div { class: "text-center",
                    div { class: "text-5xl mb-4", "💬" }
                    p { "Select a conversation to start chatting" }
                }
            }
        };
    };

    let is_muc = client.mucs.get_by_bare(&bare).is_some();
    let mut show_devices = use_signal(|| false);
    let mut show_members = use_signal(|| false);
    let mut editing_subject = use_signal(|| false);
    let mut subject_input = use_signal(String::new);

    let (title, subtitle, subtitle_class) = if let Some(room) = client.mucs.get_by_bare(&bare) {
        let name = if room.name.is_empty() {
            bare.clone()
        } else {
            room.name.clone()
        };
        let subtitle = room
            .subject
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("{} members", room.occupant_count()));
        (name, subtitle, "text-xs text-gray-400")
    } else if let Some(chat) = client.chats.get_by_bare(&bare) {
        let roster = client.roster.get(&chat.jid);
        let name = roster
            .as_ref()
            .map(|r| r.display_name())
            .unwrap_or_else(|| bare.clone());
        let (mut text, mut class) = match roster.as_ref().map(|r| r.best_show()) {
            Some(PresenceShow::Online) => ("Online", "text-xs text-green-400"),
            Some(PresenceShow::Away) => ("Away", "text-xs text-yellow-400"),
            Some(PresenceShow::Dnd) => ("Do not disturb", "text-xs text-red-400"),
            Some(PresenceShow::Xa) => ("Extended away", "text-xs text-yellow-400"),
            _ => ("Offline", "text-xs text-gray-400"),
        };
        if matches!(chat.peer_state(), ChatStateNotification::Composing) {
            text = "typing…";
            class = "text-xs text-teal-400";
        }
        (name, text.to_string(), class)
    } else {
        (bare.clone(), String::new(), "text-xs text-gray-400")
    };

    let avatar = initial_of(&title);

    // Build normalized messages.
    let mut messages: Vec<DisplayMessage> = Vec::new();
    if let Some(room) = client.mucs.get_by_bare(&bare) {
        for m in room.messages() {
            messages.push(muc_to_display(&m));
        }
    } else if let Some(chat) = client.chats.get_by_bare(&bare) {
        let other_name = client
            .roster
            .get(&chat.jid)
            .map(|r| r.display_name())
            .unwrap_or_else(|| bare.clone());
        for m in chat.messages() {
            messages.push(chat_to_display(&m, &other_name));
        }
    }

    rsx! {
        main { class: "flex-1 flex flex-col bg-gray-900 min-w-0",
            header { class: "flex items-center justify-between px-4 py-3 bg-gray-800 border-b border-gray-700",
                div { class: "flex items-center gap-3",
                    div { class: "w-10 h-10 rounded-full bg-teal-500 flex items-center justify-center font-bold text-white",
                        "{avatar}"
                    }
                    div {
                        h3 { class: "font-medium", "{title}" }
                        p { class: "{subtitle_class}", "{subtitle}" }
                    }
                }
                div { class: "flex items-center gap-2",
                    if is_muc {
                        {
                            let count = client
                                .mucs
                                .get_by_bare(&bare)
                                .map(|r| r.occupant_count())
                                .unwrap_or(0);
                            rsx! {
                                button {
                                    class: "text-xs px-2 py-1 rounded bg-gray-700 hover:bg-gray-600",
                                    onclick: move |_| {
                                        let next = !*show_members.read();
                                        show_members.set(next);
                                    },
                                    "👥 Members ({count})"
                                }
                                button {
                                    class: "text-xs px-2 py-1 rounded bg-gray-700 hover:bg-gray-600",
                                    title: "Change subject",
                                    onclick: {
                                        let subject_client = client.clone();
                                        let subject_target = bare.clone();
                                        move |_| {
                                            if let Some(room) = subject_client
                                                .mucs
                                                .get_by_bare(&subject_target)
                                            {
                                                subject_input.set(
                                                    room.subject.clone().unwrap_or_default(),
                                                );
                                            }
                                            editing_subject.set(true);
                                        }
                                    },
                                    "✎ Subject"
                                }
                            }
                        }
                    } else {
                        {
                            let c = client.clone();
                            let j = bare.clone();
                            rsx! {
                                button {
                                    class: "text-xs px-2 py-1 rounded bg-gray-700 hover:bg-gray-600",
                                    onclick: move |_| {
                                        let next = !*show_devices.read();
                                        show_devices.set(next);
                                        if let Some(chat) = c.chats.get_by_bare(&j) {
                                            let _ = c.fetch_omemo(&chat.jid);
                                        }
                                    },
                                    "🔐 Devices"
                                }
                            }
                        }
                    }
                }
            }

            if *show_devices.read() && !is_muc {
                OmemoDevices { jid: bare.clone() }
            }
            if *show_members.read() && is_muc {
                MucOccupants { room_jid: bare.clone() }
            }
            if *editing_subject.read() && is_muc {
                {
                    let save_client = client.clone();
                    let save_target = bare.clone();
                    let mut save_state = client_state.clone();
                    let mut input = subject_input;
                    rsx! {
                        div { class: "flex items-center gap-2 px-4 py-2 bg-gray-800 border-b border-gray-700",
                            input {
                                class: "flex-1 px-3 py-1.5 rounded bg-gray-900 text-sm text-white focus:outline-none focus:ring-1 focus:ring-teal-500",
                                placeholder: "Room subject",
                                value: "{subject_input}",
                                oninput: move |e| input.set(e.value()),
                            }
                            button {
                                class: "px-3 py-1.5 rounded bg-teal-600 hover:bg-teal-500 text-sm",
                                onclick: move |_| {
                                    if let Some(room) = save_client.mucs.get_by_bare(&save_target) {
                                        let text = input.read().trim().to_string();
                                        let _ = save_client.send_muc_subject(&room.jid, &text);
                                    }
                                    editing_subject.set(false);
                                    save_state.bump();
                                },
                                "Save"
                            }
                            button {
                                class: "px-3 py-1.5 rounded bg-gray-700 hover:bg-gray-600 text-sm",
                                onclick: move |_| editing_subject.set(false),
                                "Cancel"
                            }
                        }
                    }
                }
            }

            div { class: "flex-1 overflow-y-auto p-4 space-y-4",
                if messages.is_empty() {
                    div { class: "h-full flex items-center justify-center text-sm text-gray-500",
                        "No messages yet"
                    }
                } else {
                    {
                        let older_client = client.clone();
                        let older_target = bare.clone();
                        let older_is_muc = is_muc;
                        rsx! {
                            button {
                                class: "mx-auto block text-xs text-gray-400 hover:text-white bg-gray-800 rounded px-3 py-1",
                                onclick: move |_| {
                                    if older_is_muc {
                                        if let Some(room) = older_client.mucs.get_by_bare(&older_target) {
                                            if let Some(before) = room.mam_cursor() {
                                                let _ = older_client.query_muc_mam_before_str(
                                                    &older_target,
                                                    50,
                                                    &before,
                                                );
                                            } else {
                                                let _ = older_client.query_muc_mam_str(&older_target, 50);
                                            }
                                        }
                                    } else if older_client.chats.get_by_bare(&older_target).is_some() {
                                        if let Some(before) = older_client
                                            .chats
                                            .get_by_bare(&older_target)
                                            .and_then(|c| c.mam_cursor())
                                        {
                                            let _ = older_client.query_mam_before_str(
                                                &older_target,
                                                50,
                                                &before,
                                            );
                                        } else {
                                            let _ = older_client.query_mam_str(&older_target, 50);
                                        }
                                    }
                                },
                                "Load older messages"
                            }
                        }
                    }
                }
                for msg in messages {
                    {
                        let mut reply_target = reply_target;
                        let target = bare.clone();
                        rsx! {
                            MessageBubble {
                                msg: msg,
                                target: target,
                                is_muc: is_muc,
                                onreply: move |m: DisplayMessage| {
                                    reply_target.set(Some((m.id.clone(), m.body.clone())));
                                },
                            }
                        }
                    }
                }
            }

            if let Some((_, preview)) = reply_target.read().clone() {
                div { class: "flex items-center justify-between px-4 py-2 bg-gray-800 border-t border-gray-700",
                    span { class: "text-xs text-gray-400 truncate", "Replying to: {preview}" }
                    button {
                        class: "text-gray-400 hover:text-white text-xs px-2",
                        onclick: move |_| reply_target.set(None),
                        "✕"
                    }
                }
            }

            MessageInput {
                target: bare.clone(),
                is_muc: is_muc,
                reply_to: reply_target.read().clone().map(|(id, _)| id),
                on_sent: move |_| reply_target.set(None),
            }
        }
    }
}

#[component]
fn MucOccupants(room_jid: String) -> Element {
    let client = use_context::<XmppClient>();
    let client_state = use_context::<ClientState>();
    let _ = *client_state.revision.read();

    let Some(room) = client.mucs.get_by_bare(&room_jid) else {
        return rsx! {};
    };
    let mut occupants = room.occupants();
    occupants.sort_by(|a, b| {
        b.role
            .hierarchy()
            .cmp(&a.role.hierarchy())
            .then_with(|| a.nick.to_lowercase().cmp(&b.nick.to_lowercase()))
    });
    let self_nick = room.self_nick().unwrap_or_default();
    let can_moderate = room.can_moderate();
    let can_admin = room.can_admin();

    rsx! {
        div { class: "px-4 py-3 bg-gray-800 border-b border-gray-700 max-h-64 overflow-y-auto",
            if occupants.is_empty() {
                p { class: "text-xs text-gray-500", "No occupants visible yet." }
            } else {
                div { class: "space-y-1",
                    for occupant in occupants {
                        {
                            let is_self = occupant.nick == self_nick;
                            let affiliation = match occupant.affiliation {
                                MucAffiliation::Owner => Some(("owner", "text-amber-400")),
                                MucAffiliation::Admin => Some(("admin", "text-teal-400")),
                                MucAffiliation::Member => Some(("member", "text-gray-400")),
                                MucAffiliation::Outcast => Some(("banned", "text-red-400")),
                                MucAffiliation::None => None,
                            };
                            let role = match occupant.role {
                                MucRole::Moderator => Some(("moderator", "text-teal-400")),
                                MucRole::Visitor => Some(("visitor", "text-gray-500")),
                                _ => None,
                            };
                            let kick_client = client.clone();
                            let kick_room = room.jid.clone();
                            let kick_nick = occupant.nick.clone();
                            let ban_client = client.clone();
                            let ban_room = room.jid.clone();
                            let ban_jid = occupant.jid.clone();
                            rsx! {
                                div { class: "flex items-center gap-2 text-xs",
                                    span { class: "text-gray-200", "{occupant.nick}" }
                                    if is_self {
                                        span { class: "text-gray-500", "(you)" }
                                    }
                                    if let Some((label, class)) = affiliation {
                                        span { class: "{class}", "{label}" }
                                    }
                                    if let Some((label, class)) = role {
                                        span { class: "{class}", "{label}" }
                                    }
                                    if can_moderate && !is_self && occupant.role != MucRole::None {
                                        button {
                                            class: "ml-auto px-2 py-0.5 rounded bg-red-700 hover:bg-red-600",
                                            onclick: move |_| {
                                                let _ = kick_client.set_muc_role(
                                                    &kick_room,
                                                    &kick_nick,
                                                    "none",
                                                    Some("Kicked"),
                                                );
                                            },
                                            "Kick"
                                        }
                                    }
                                    if can_admin {
                                        if let Some(jid) = ban_jid {
                                            button {
                                                class: "px-2 py-0.5 rounded bg-red-800 hover:bg-red-700",
                                                onclick: move |_| {
                                                    let _ = ban_client.set_muc_affiliation(
                                                        &ban_room,
                                                        &jid,
                                                        "outcast",
                                                        Some("Banned"),
                                                    );
                                                },
                                                "Ban"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn OmemoDevices(jid: String) -> Element {
    let client = use_context::<XmppClient>();
    let client_state = use_context::<ClientState>();
    let _ = *client_state.revision.read();
    let devices = client.omemo.device_list(&jid);

    rsx! {
        div { class: "px-4 py-3 bg-gray-800 border-b border-gray-700",
            if devices.is_empty() {
                p { class: "text-xs text-gray-500",
                    "No OMEMO devices known yet. Enable encryption in the composer to fetch them."
                }
            } else {
                div { class: "space-y-2",
                    for id in devices {
                        {
                            let bundle = client.omemo.bundle(&jid, id);
                            let fingerprint = bundle
                                .as_ref()
                                .map(|b| b.fingerprint())
                                .unwrap_or_else(|| "bundle not downloaded".to_string());
                            let verified = bundle.as_ref().map(|b| b.verify()).unwrap_or(false);
                            let level = client.omemo.trust(&jid, id);
                            let (label, class) = match level {
                                TrustLevel::Trusted => ("Trusted", "text-xs text-green-400"),
                                TrustLevel::Untrusted => ("Not trusted", "text-xs text-red-400"),
                                TrustLevel::Undecided => ("Not verified", "text-xs text-gray-400"),
                            };
                            let target_trust = jid.clone();
                            let target_untrust = jid.clone();
                            let trust_client = client.clone();
                            let untrust_client = client.clone();
                            let mut trust_state = client_state.clone();
                            let mut untrust_state = client_state.clone();
                            rsx! {
                                div { class: "flex flex-wrap items-center gap-2",
                                    span { class: "text-xs text-gray-300", "Device {id}" }
                                    span { class: "font-mono text-[10px] text-gray-400", "{fingerprint}" }
                                    if !verified {
                                        span { class: "text-xs text-yellow-400", "signature invalid" }
                                    }
                                    span { class: "{class}", "{label}" }
                                    button {
                                        class: "text-xs px-2 py-0.5 rounded bg-emerald-700 hover:bg-emerald-600",
                                        onclick: move |_| {
                                            trust_client
                                                .omemo
                                                .set_trust(&target_trust, id, TrustLevel::Trusted);
                                            trust_state.bump();
                                        },
                                        "Trust"
                                    }
                                    button {
                                        class: "text-xs px-2 py-0.5 rounded bg-red-700 hover:bg-red-600",
                                        onclick: move |_| {
                                            untrust_client.omemo.set_trust(
                                                &target_untrust,
                                                id,
                                                TrustLevel::Untrusted,
                                            );
                                            untrust_state.bump();
                                        },
                                        "Distrust"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
enum TextSegment {
    Plain(String),
    Bold(String),
    Italic(String),
    Strike(String),
    Mono(String),
    Block(String),
    Link(String),
}

#[component]
fn FormattedText(text: String) -> Element {
    let segments = styled_segments(&text);
    rsx! {
        div { class: "whitespace-pre-wrap break-words",
            for segment in segments {
                { render_segment(&segment) }
            }
        }
    }
}

fn render_segment(segment: &TextSegment) -> Element {
    match segment {
        TextSegment::Plain(text) => rsx! { span { "{text}" } },
        TextSegment::Bold(text) => rsx! { strong { "{text}" } },
        TextSegment::Italic(text) => rsx! { em { "{text}" } },
        TextSegment::Strike(text) => rsx! { del { "{text}" } },
        TextSegment::Mono(text) => rsx! {
            code { class: "px-1 rounded bg-black/30 font-mono text-[0.95em]", "{text}" }
        },
        TextSegment::Block(text) => rsx! {
            pre { class: "mt-1 mb-1 p-2 rounded bg-black/30 font-mono text-xs overflow-x-auto", "{text}" }
        },
        TextSegment::Link(url) => rsx! {
            a {
                class: "underline",
                href: "{url}",
                target: "_blank",
                rel: "noopener noreferrer",
                "{url}"
            }
        },
    }
}

fn styled_segments(text: &str) -> Vec<TextSegment> {
    let chars: Vec<char> = text.chars().collect();
    let mut segments = Vec::new();
    let mut plain = String::new();
    let mut i = 0;

    while i < chars.len() {
        if starts_with(&chars, i, "```") {
            if let Some(end) = find_sequence(&chars, i + 3, "```") {
                flush_plain(&mut plain, &mut segments);
                let inner: String = chars[i + 3..end].iter().collect();
                segments.push(TextSegment::Block(inner.trim_matches('\n').to_string()));
                i = end + 3;
                continue;
            }
        }

        if starts_with(&chars, i, "https://") || starts_with(&chars, i, "http://") {
            let mut end = i;
            while end < chars.len()
                && !chars[end].is_whitespace()
                && !matches!(chars[end], '<' | '>' | '"')
            {
                end += 1;
            }
            let mut url_end = end;
            while url_end > i
                && matches!(
                    chars[url_end - 1],
                    '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}'
                )
            {
                url_end -= 1;
            }
            if url_end > i + 7 {
                flush_plain(&mut plain, &mut segments);
                segments.push(TextSegment::Link(chars[i..url_end].iter().collect()));
                i = url_end;
                continue;
            }
        }

        let marker = chars[i];
        if matches!(marker, '*' | '_' | '~' | '`') && valid_marker_open(&chars, i) {
            if let Some(end) = find_marker_close(&chars, i + 1, marker) {
                let inner: String = chars[i + 1..end].iter().collect();
                if !inner.is_empty() {
                    flush_plain(&mut plain, &mut segments);
                    segments.push(match marker {
                        '*' => TextSegment::Bold(inner),
                        '_' => TextSegment::Italic(inner),
                        '~' => TextSegment::Strike(inner),
                        _ => TextSegment::Mono(inner),
                    });
                    i = end + 1;
                    continue;
                }
            }
        }

        plain.push(marker);
        i += 1;
    }
    flush_plain(&mut plain, &mut segments);
    segments
}

fn flush_plain(plain: &mut String, segments: &mut Vec<TextSegment>) {
    if !plain.is_empty() {
        segments.push(TextSegment::Plain(std::mem::take(plain)));
    }
}

fn starts_with(chars: &[char], at: usize, needle: &str) -> bool {
    let mut idx = at;
    for ch in needle.chars() {
        if chars.get(idx) != Some(&ch) {
            return false;
        }
        idx += 1;
    }
    true
}

fn find_sequence(chars: &[char], from: usize, needle: &str) -> Option<usize> {
    (from..chars.len()).find(|i| starts_with(chars, *i, needle))
}

fn boundary(ch: Option<&char>) -> bool {
    match ch {
        None => true,
        Some(c) => c.is_whitespace() || c.is_ascii_punctuation(),
    }
}

fn valid_marker_open(chars: &[char], i: usize) -> bool {
    boundary(i.checked_sub(1).and_then(|p| chars.get(p)))
        && chars
            .get(i + 1)
            .map(|c| !c.is_whitespace())
            .unwrap_or(false)
}

fn find_marker_close(chars: &[char], from: usize, marker: char) -> Option<usize> {
    let mut i = from;
    while i < chars.len() {
        if chars[i] == marker {
            let prev_ok = !chars[i - 1].is_whitespace();
            let next_ok = boundary(chars.get(i + 1));
            if prev_ok && next_ok {
                return Some(i);
            }
        }
        if chars[i] == '\n' && marker != '`' {
            return None;
        }
        i += 1;
    }
    None
}

fn chat_to_display(m: &ChatMessage, other_name: &str) -> DisplayMessage {
    let status = match m.receipt {
        xmpp_core::MessageReceipt::None => "",
        xmpp_core::MessageReceipt::Sent => "sent",
        xmpp_core::MessageReceipt::Delivered => "delivered",
        xmpp_core::MessageReceipt::Displayed => "read",
        xmpp_core::MessageReceipt::Acknowledged => "delivered",
    };
    DisplayMessage {
        id: m.id.clone(),
        is_own: m.is_own,
        sender: if m.is_own {
            "You".to_string()
        } else {
            other_name.to_string()
        },
        body: m.display_body(),
        time: m.timestamp.format("%H:%M").to_string(),
        status: status.to_string(),
        reactions: group_reactions(m.reactions.iter(), |r| r.emoji.clone()),
        edited: m.is_edited,
        encrypted: m.encryption.is_some(),
    }
}

fn muc_to_display(m: &MucMessage) -> DisplayMessage {
    DisplayMessage {
        id: m.id.clone(),
        is_own: m.is_own,
        sender: m.from_nick.clone(),
        body: m.body.clone().unwrap_or_else(|| "[attachment]".to_string()),
        time: m.timestamp.format("%H:%M").to_string(),
        status: if m.is_own {
            "sent".to_string()
        } else {
            String::new()
        },
        reactions: group_reactions(m.reactions.iter(), |r| r.emoji.clone()),
        edited: m.is_edited,
        encrypted: m.encryption.is_some(),
    }
}

/// Message bubble
#[component]
fn MessageBubble(
    msg: DisplayMessage,
    target: String,
    #[props(default = false)] is_muc: bool,
    onreply: EventHandler<DisplayMessage>,
) -> Element {
    let client = use_context::<XmppClient>();
    let client_state = use_context::<ClientState>();
    let cs = client_state.clone();

    let is_own = msg.is_own;
    let sender = msg.sender.clone();
    let body = msg.body.clone();
    let time = msg.time.clone();
    let status = msg.status.clone();
    let edited = msg.edited;
    let encrypted = msg.encrypted;
    let reactions = msg.reactions.clone();
    let avatar = initial_of(&sender);
    let msg_id = msg.id.clone();
    let reply_msg = msg.clone();

    let bubble_class = if is_own {
        "bg-teal-600 text-white rounded-2xl rounded-br-md"
    } else {
        "bg-gray-800 text-white rounded-2xl rounded-bl-md"
    };
    let align_class = if is_own {
        "flex-row-reverse"
    } else {
        "flex-row"
    };
    let action_class = "opacity-0 group-hover:opacity-100 transition-opacity hover:text-white";

    rsx! {
        div { class: "flex items-end gap-2 {align_class} group",
            if !is_own {
                div { class: "w-8 h-8 rounded-full bg-teal-500 flex items-center justify-center font-bold text-white text-sm shrink-0",
                    "{avatar}"
                }
            }
            div { class: "max-w-xs lg:max-w-md min-w-0",
                div { class: "{bubble_class} px-4 py-2",
                    if !is_own {
                        span { class: "text-xs text-gray-400 mb-1 block", "{sender}" }
                    }
                    FormattedText { text: body.clone() }
                    if edited {
                        span { class: "text-xs text-gray-300/70", " (edited)" }
                    }
                }
                if !reactions.is_empty() {
                    div { class: "flex flex-wrap gap-1 mt-1",
                        for (emoji, count) in reactions {
                            span { class: "px-1.5 py-0.5 bg-gray-700 rounded-full text-xs", "{emoji} {count}" }
                        }
                    }
                }
                div { class: "flex items-center gap-1 mt-1 text-xs text-gray-500",
                    if encrypted {
                        span { title: "OMEMO encrypted", "🔒" }
                    }
                    span { "{time}" }
                    if is_own {
                        MessageStatus { status: status }
                    }
                    button {
                        class: "{action_class} ml-1",
                        title: "Reply",
                        onclick: move |_| onreply.call(reply_msg.clone()),
                        "↩"
                    }
                    if !is_muc {
                        {
                            let client = client.clone();
                            let target = target.clone();
                            let mid = msg_id.clone();
                            let mut cs = cs.clone();
                            rsx! {
                                button {
                                    class: "{action_class}",
                                    title: "React 👍",
                                    onclick: move |_| {
                                        do_react(&client, &target, &mid, "👍");
                                        cs.bump();
                                    },
                                    "👍"
                                }
                            }
                        }
                        {
                            let client = client.clone();
                            let target = target.clone();
                            let mid = msg_id.clone();
                            let mut cs = cs.clone();
                            rsx! {
                                button {
                                    class: "{action_class}",
                                    title: "React ❤️",
                                    onclick: move |_| {
                                        do_react(&client, &target, &mid, "❤️");
                                        cs.bump();
                                    },
                                    "❤️"
                                }
                            }
                        }
                        {
                            let client = client.clone();
                            let target = target.clone();
                            let mid = msg_id.clone();
                            let mut cs = cs.clone();
                            rsx! {
                                button {
                                    class: "{action_class}",
                                    title: "React 😂",
                                    onclick: move |_| {
                                        do_react(&client, &target, &mid, "😂");
                                        cs.bump();
                                    },
                                    "😂"
                                }
                            }
                        }
                    }
                    if is_own && !is_muc {
                        {
                            let client = client.clone();
                            let target = target.clone();
                            let mid = msg_id.clone();
                            let mut cs = cs.clone();
                            rsx! {
                                button {
                                    class: "{action_class} hover:text-red-400",
                                    title: "Delete",
                                    onclick: move |_| {
                                        do_retract(&client, &target, &mid);
                                        cs.bump();
                                    },
                                    "🗑"
                                }
                            }
                        }
                    }
                }
            }
            if is_own {
                div { class: "w-8 h-8 rounded-full bg-gray-700 flex items-center justify-center font-bold text-white text-sm shrink-0",
                    "U"
                }
            }
        }
    }
}

fn do_react(client: &XmppClient, target: &str, msg_id: &str, emoji: &str) {
    if let Some(chat) = client.chats.get_by_bare(target) {
        let _ = client.send_reaction(&chat.jid, msg_id, &[emoji]);
    }
}

fn do_retract(client: &XmppClient, target: &str, msg_id: &str) {
    if let Some(chat) = client.chats.get_by_bare(target) {
        let _ = client.send_retract(&chat.jid, msg_id);
    }
}

/// Message status indicator
#[component]
fn MessageStatus(status: String) -> Element {
    let (icon, color) = match status.as_str() {
        "sending" => ("⏳", "text-yellow-400"),
        "sent" => ("✓", "text-gray-400"),
        "delivered" => ("✓✓", "text-gray-400"),
        "read" => ("✓✓", "text-teal-400"),
        "failed" => ("✗", "text-red-400"),
        _ => ("", "text-gray-400"),
    };

    rsx! {
        span { class: "{color}", "{icon}" }
    }
}

/// Message input area
#[component]
fn MessageInput(
    target: String,
    is_muc: bool,
    reply_to: Option<String>,
    on_sent: EventHandler<()>,
) -> Element {
    let client_state = use_context::<ClientState>();
    let client = use_context::<XmppClient>();
    let mut message = use_signal(String::new);
    let mut composing = use_signal(|| false);
    let mut uploading = use_signal(|| false);
    let mut upload_error = use_signal(|| Option::<String>::None);
    let mut omemo = use_signal(|| false);
    let mut omemo_error = use_signal(|| Option::<String>::None);

    let mut cs = client_state.clone();
    let client_send = client.clone();
    let client_typing = client.clone();
    let target_typing = target.clone();
    let target_send = target.clone();

    let send = move |_| {
        let text = message.read().trim().to_string();
        if text.is_empty() {
            return;
        }
        let bare = target_send.clone();
        if is_muc {
            if let Some(room) = client_send.mucs.get_by_bare(&bare) {
                let nick = room.self_nick().unwrap_or_else(|| "me".to_string());
                match client_send.send_muc_message(&room.jid, &nick, &text) {
                    Ok(_) => {
                        message.set(String::new());
                        on_sent.call(());
                        cs.bump();
                    }
                    Err(e) => tracing::warn!("Failed to send MUC message: {}", e),
                }
            }
        } else if let Some(chat) = client_send.chats.get_by_bare(&bare) {
            let res = if *omemo.read() {
                client_send.send_omemo_message(&chat.jid, &text).map(|_| ())
            } else if let Some(rid) = reply_to.as_ref() {
                client_send.send_reply(&chat.jid, &text, rid).map(|_| ())
            } else {
                client_send.send_message(&chat.jid, &text).map(|_| ())
            };
            match res {
                Ok(_) => {
                    message.set(String::new());
                    composing.set(false);
                    on_sent.call(());
                    cs.bump();
                }
                Err(e) => {
                    tracing::warn!("Failed to send message: {}", e);
                    omemo_error.set(Some(e.to_string()));
                }
            }
        }
    };

    let on_input = move |e: FormEvent| {
        let val = e.value();
        let was_composing = *composing.read();
        let now_composing = !val.trim().is_empty();
        if !is_muc && now_composing != was_composing {
            if let Some(chat) = client_typing.chats.get_by_bare(&target_typing) {
                let state = if now_composing { "composing" } else { "paused" };
                let _ = client_typing.send_chat_state(&chat.jid, state);
            }
            composing.set(now_composing);
        }
        message.set(val);
    };

    let client_omemo = client.clone();
    let target_omemo = target.clone();
    let toggle_omemo = move |_| {
        let next = !*omemo.read();
        omemo.set(next);
        omemo_error.set(None);
        if next {
            if let Some(chat) = client_omemo.chats.get_by_bare(&target_omemo) {
                if let Err(e) = client_omemo.fetch_omemo(&chat.jid) {
                    omemo_error.set(Some(e.to_string()));
                }
            }
        }
    };

    let client_file = client.clone();
    let target_file = target.clone();
    let on_file = move |e: FormEvent| {
        let Some(engine) = e.files() else {
            return;
        };
        let Some(name) = engine.files().into_iter().next() else {
            return;
        };
        uploading.set(true);
        upload_error.set(None);
        let client = client_file.clone();
        let bare = target_file.clone();
        spawn(async move {
            let Some(bytes) = engine.read_file(&name).await else {
                upload_error.set(Some("Could not read the selected file".to_string()));
                uploading.set(false);
                return;
            };
            let mime = mime_type_for(&name);
            match client.upload_file(&name, bytes, mime).await {
                Ok(slot) => {
                    let text = format!("{} ({})", slot.get_url, name);
                    if is_muc {
                        if let Some(room) = client.mucs.get_by_bare(&bare) {
                            let nick = room.self_nick().unwrap_or_else(|| "me".to_string());
                            let _ = client.send_muc_message(&room.jid, &nick, &text);
                        }
                    } else if let Some(chat) = client.chats.get_by_bare(&bare) {
                        let _ = client.send_message(&chat.jid, &text);
                    }
                    on_sent.call(());
                }
                Err(err) => upload_error.set(Some(err.to_string())),
            }
            uploading.set(false);
        });
    };

    let omemo_class = if *omemo.read() {
        "p-2 rounded-lg bg-gray-700 text-teal-400"
    } else {
        "p-2 rounded-lg hover:bg-gray-700 text-gray-400 hover:text-white"
    };

    rsx! {
        div { class: "p-4 bg-gray-800 border-t border-gray-700",
            div { class: "flex items-end gap-2",
                if !is_muc {
                    button {
                        class: "{omemo_class}",
                        title: "Toggle OMEMO encryption",
                        onclick: toggle_omemo,
                        svg { class: "w-5 h-5", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                            path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" }
                        }
                    }
                }
                label {
                    class: "p-2 rounded-lg hover:bg-gray-700 transition-colors text-gray-400 hover:text-white cursor-pointer",
                    title: "Attach a file",
                    input {
                        r#type: "file",
                        class: "hidden",
                        disabled: *uploading.read(),
                        onchange: on_file,
                    }
                    if *uploading.read() {
                        svg { class: "w-5 h-5 animate-spin", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                            path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" }
                        }
                    } else {
                        svg { class: "w-5 h-5", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                            path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M15.172 7l-6.586 6.586a2 2 0 102.828 2.828l6.414-6.586a4 4 0 00-5.656-5.656l-6.415 6.585a6 6 0 108.486 8.486L20.5 13" }
                        }
                    }
                }
                div { class: "flex-1 relative",
                    textarea {
                        class: "w-full px-4 py-3 bg-gray-700 border border-gray-600 rounded-xl text-white placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-teal-500 focus:border-transparent resize-none min-h-[48px] max-h-48 pr-12",
                        placeholder: "Type a message...",
                        value: "{message}",
                        oninput: on_input,
                        rows: 1,
                    }
                }
                button {
                    class: "p-2 rounded-lg bg-teal-500 hover:bg-teal-400 transition-colors text-white disabled:opacity-50 disabled:cursor-not-allowed",
                    disabled: message.read().trim().is_empty(),
                    onclick: send,
                    svg { class: "w-5 h-5", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M12 19l9 2-9-18-9 18 9-2zm0 0v-8" }
                    }
                }
            }
            if let Some(err) = upload_error.read().as_ref() {
                p { class: "mt-2 text-xs text-red-400", "{err}" }
            }
            if let Some(err) = omemo_error.read().as_ref() {
                p { class: "mt-2 text-xs text-red-400", "{err}" }
            }
        }
    }
}
