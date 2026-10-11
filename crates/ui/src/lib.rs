//! UI components for Jabu

pub mod state;
pub use state::ClientState;

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use storage::{
    load_account_credentials, load_draft, load_session, save_account_credentials, save_credentials,
    save_draft, SettingsOmemoStore, StorageBackend, StoredAccount, StoredCredentials,
};
use xmpp_core::{
    chat::ChatStateNotification,
    muc::{MucAffiliation, MucRole},
    omemo::TrustLevel,
    roster::PresenceShow,
    vcard4::VCard4,
    ChatMessage, ConnectionState, Gateway, GatewayRegistration, MucMessage, RosterItem, XmppClient,
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
            let bare = account.split('/').next().unwrap_or(&account).to_string();
            let stored = StoredAccount {
                jid: bare.clone(),
                password_hash: None,
                resource: config.resource.clone().unwrap_or_default(),
                priority: 0,
                auto_reconnect: config.auto_reconnect,
                created_at: chrono::Utc::now(),
                last_login: Some(chrono::Utc::now()),
            };
            if let Err(e) = storage.save_account(&stored) {
                tracing::warn!("Failed to save account: {}", e);
            }
            if let Err(e) = save_account_credentials(storage.as_ref(), &credentials) {
                tracing::warn!("Failed to save account credentials: {}", e);
            }
            let _ = storage.set_setting("ui.active_account", &serde_json::json!(bare));
            let mut accounts = state.accounts.read().clone();
            if !accounts.contains(&bare) {
                accounts.push(bare);
                accounts.sort();
                state.accounts.set(accounts);
            }
            state.active_account.set(Some(
                account.split('/').next().unwrap_or(&account).to_string(),
            ));
        } else if let Err(e) = storage::clear_credentials(storage.as_ref()) {
            tracing::warn!("Failed to clear credentials: {}", e);
        }

        client.connection.set_config(config.clone().into());
        client.roster.clear();
        client.chats.clear();
        client.mucs.clear();
        client.pubsub.clear();
        client.disco.clear();
        client.push.clear();
        client.sm.reset();
        load_session(storage.as_ref(), &client, &account);
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
    let mut show_settings = use_signal(|| false);
    let mut show_accounts = use_signal(|| false);
    let mut show_presence = use_signal(|| false);
    let cs = client_state.clone();
    let mut config_signal = client_state.config;
    let disconnect_client = client.clone();
    let sign_out = move |_| {
        let client = disconnect_client.clone();
        let storage = storage.clone();
        let mut state = cs.clone();
        spawn(async move {
            let _ = storage::clear_credentials(storage.as_ref());
            config_signal.set(None);
            state.active_account.set(None);
            state.bump();
            let _ = client.connection.disconnect().await;
            client.roster.clear();
            client.chats.clear();
            client.mucs.clear();
            client.pubsub.clear();
            client.disco.clear();
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
            if *show_settings.read() {
                SettingsDialog { onclose: move |_| show_settings.set(false) }
            }
            if *show_accounts.read() {
                AccountsDialog { onclose: move |_| show_accounts.set(false) }
            }
            if *show_presence.read() {
                PresenceDialog { onclose: move |_| show_presence.set(false) }
            }
            div { class: "absolute right-0 mt-2 w-48 bg-gray-800 rounded-lg shadow-lg border border-gray-700 opacity-0 invisible group-hover:opacity-100 group-hover:visible transition-all",
                div { class: "py-1",
                    button {
                        class: "block w-full text-left px-4 py-2 text-sm text-gray-300 hover:bg-gray-700",
                        onclick: move |_| show_accounts.set(true),
                        "Accounts"
                    }
                    button {
                        class: "block w-full text-left px-4 py-2 text-sm text-gray-300 hover:bg-gray-700",
                        onclick: move |_| show_presence.set(true),
                        "Presence"
                    }
                    button {
                        class: "block w-full text-left px-4 py-2 text-sm text-gray-300 hover:bg-gray-700",
                        onclick: move |_| show_settings.set(true),
                        "Settings"
                    }
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

#[component]
fn PresenceDialog(onclose: EventHandler<()>) -> Element {
    let client = use_context::<XmppClient>();
    let client_state = use_context::<ClientState>();
    let config = client.connection.config();
    let mut show = use_signal(|| config.presence_show.clone().unwrap_or_default());
    let mut status = use_signal(|| config.presence_status.clone().unwrap_or_default());
    let mut priority = use_signal(|| config.priority);
    let mut error = use_signal(|| Option::<String>::None);

    let save_client = client.clone();
    let mut cs = client_state.clone();
    let save = move |_| {
        let selected = show.read().clone();
        let show_value = (!selected.is_empty()).then_some(selected.as_str());
        let status_value = {
            let text = status.read().trim().to_string();
            (!text.is_empty()).then_some(text)
        };
        let priority_value = *priority.read();
        match save_client.set_presence(show_value, status_value.as_deref(), Some(priority_value)) {
            Ok(()) => {
                let current = cs.config.read().clone();
                if let Some(mut config) = current {
                    config.presence_show = show_value.map(str::to_string);
                    config.presence_status = status_value.clone();
                    config.priority = priority_value;
                    cs.config.set(Some(config));
                }
                cs.bump();
                onclose.call(());
            }
            Err(e) => error.set(Some(e.to_string())),
        }
    };

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/60",
            onclick: move |_| onclose.call(()),
            div {
                class: "w-80 rounded-lg bg-gray-800 p-4 space-y-3",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center justify-between",
                    h3 { class: "font-medium", "Presence" }
                    button {
                        class: "text-gray-400 hover:text-white",
                        onclick: move |_| onclose.call(()),
                        "✕"
                    }
                }
                label { class: "block text-xs text-gray-400",
                    "Status"
                    select {
                        class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-white",
                        value: "{show}",
                        onchange: move |e| show.set(e.value()),
                        option { value: "", "Online" }
                        option { value: "away", "Away" }
                        option { value: "dnd", "Do not disturb" }
                        option { value: "xa", "Extended away" }
                    }
                }
                label { class: "block text-xs text-gray-400",
                    "Custom message"
                    input {
                        class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-white",
                        placeholder: "What are you up to?",
                        value: "{status}",
                        oninput: move |e| status.set(e.value()),
                    }
                }
                label { class: "block text-xs text-gray-400",
                    "Priority"
                    input {
                        class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-white",
                        r#type: "number",
                        min: "0",
                        max: "10",
                        value: "{priority}",
                        oninput: move |e| {
                            if let Ok(value) = e.value().parse::<i32>() {
                                priority.set(value.clamp(0, 10));
                            }
                        },
                    }
                }
                if let Some(err) = error.read().as_ref() {
                    p { class: "text-xs text-red-400", "{err}" }
                }
                div { class: "flex justify-end gap-2",
                    button {
                        class: "px-3 py-1.5 rounded bg-gray-700 hover:bg-gray-600 text-sm",
                        onclick: move |_| onclose.call(()),
                        "Cancel"
                    }
                    button {
                        class: "px-3 py-1.5 rounded bg-teal-600 hover:bg-teal-500 text-sm",
                        onclick: save,
                        "Save"
                    }
                }
            }
        }
    }
}

#[component]
fn AccountsDialog(onclose: EventHandler<()>) -> Element {
    let client = use_context::<XmppClient>();
    let client_state = use_context::<ClientState>();
    let storage = use_context::<Arc<dyn StorageBackend>>();
    let mut adding = use_signal(|| false);
    let mut jid = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut resource = use_signal(|| "jabu".to_string());
    let mut error = use_signal(|| Option::<String>::None);
    let mut busy = use_signal(|| false);

    let switch = {
        let storage = storage.clone();
        let client = client.clone();
        let client_state = client_state.clone();
        move |target: String| {
            let storage = storage.clone();
            let client = client.clone();
            let mut client_state = client_state.clone();
            let mut error = error;
            let mut busy = busy;
            busy.set(true);
            error.set(None);
            spawn(async move {
                let bare = target.split('/').next().unwrap_or(&target).to_string();
                let Some(credentials) = load_account_credentials(storage.as_ref(), &bare) else {
                    error.set(Some("No saved password for this account.".into()));
                    busy.set(false);
                    return;
                };
                let _ = client.connection.disconnect().await;
                client.roster.clear();
                client.chats.clear();
                client.mucs.clear();
                client.pubsub.clear();
                client.disco.clear();
                client.push.clear();
                client.sm.reset();

                let mut config = client_state.config.read().clone().unwrap_or_default();
                config.jid = credentials.jid.clone();
                config.password = credentials.password.clone();
                config.resource = credentials.resource.clone();
                client.connection.set_config(config.clone().into());

                client.omemo.reset();
                client
                    .omemo
                    .set_store(Arc::new(SettingsOmemoStore::new(storage.clone(), &bare)));
                if let Err(e) = client.omemo.load_persisted() {
                    tracing::warn!("Failed to load OMEMO state: {}", e);
                }
                client.omemo.ensure_local_auto(100);
                load_session(storage.as_ref(), &client, &bare);
                let _ = storage.set_setting("ui.active_account", &serde_json::json!(bare));
                client_state.set_active_account(bare, config, client.clone());

                if let Err(e) = client.connection.connect().await {
                    error.set(Some(e.to_string()));
                }
                busy.set(false);
            });
        }
    };

    let remove = {
        let storage = storage.clone();
        let client = client.clone();
        let client_state = client_state.clone();
        move |target: String| {
            let storage = storage.clone();
            let client = client.clone();
            let mut client_state = client_state.clone();
            let mut error = error;
            spawn(async move {
                let bare = target.split('/').next().unwrap_or(&target).to_string();
                if let Err(e) = storage.delete_account(&bare) {
                    error.set(Some(e.to_string()));
                    return;
                }
                let _ = storage::delete_account_credentials(storage.as_ref(), &bare);
                let mut accounts = client_state.accounts.read().clone();
                accounts.retain(|account| account != &bare);
                client_state.accounts.set(accounts);
                if client_state.active_account.read().as_deref() == Some(bare.as_str()) {
                    let _ = client.connection.disconnect().await;
                    client.roster.clear();
                    client.chats.clear();
                    client.mucs.clear();
                    client.pubsub.clear();
                    client.disco.clear();
                    client_state.config.set(None);
                    client_state.active_account.set(None);
                    let _ = storage::clear_credentials(storage.as_ref());
                }
                client_state.bump();
            });
        }
    };

    let add = {
        let storage = storage.clone();
        let client_state = client_state.clone();
        let switch = switch.clone();
        move |_| {
            let storage = storage.clone();
            let mut client_state = client_state.clone();
            let mut error = error;
            let mut adding = adding;
            let mut busy = busy;
            let switch = switch.clone();
            let account = jid.read().trim().to_string();
            let pass = password.read().clone();
            if !account.contains('@') {
                error.set(Some("Enter a full JID, e.g. user@example.com".into()));
                return;
            }
            if pass.is_empty() {
                error.set(Some("Enter the account password.".into()));
                return;
            }
            let res = resource.read().trim().to_string();
            let resource_value = if res.is_empty() { None } else { Some(res) };
            let bare = account.split('/').next().unwrap_or(&account).to_string();
            busy.set(true);
            spawn(async move {
                let stored = StoredAccount {
                    jid: bare.clone(),
                    password_hash: None,
                    resource: resource_value.clone().unwrap_or_default(),
                    priority: 0,
                    auto_reconnect: true,
                    created_at: chrono::Utc::now(),
                    last_login: None,
                };
                let credentials = StoredCredentials {
                    jid: bare.clone(),
                    password: Some(pass),
                    resource: resource_value,
                };
                if let Err(e) = storage.save_account(&stored) {
                    error.set(Some(e.to_string()));
                    busy.set(false);
                    return;
                }
                if let Err(e) = save_account_credentials(storage.as_ref(), &credentials) {
                    error.set(Some(e.to_string()));
                    busy.set(false);
                    return;
                }
                let _ = save_credentials(storage.as_ref(), &credentials);
                let mut accounts = client_state.accounts.read().clone();
                if !accounts.contains(&bare) {
                    accounts.push(bare.clone());
                    accounts.sort();
                    client_state.accounts.set(accounts);
                }
                adding.set(false);
                jid.set(String::new());
                password.set(String::new());
                busy.set(false);
                switch(bare);
            });
        }
    };

    let accounts = client_state.accounts.read().clone();
    let active = client_state.active_account.read().clone();
    let mut switch_buttons = Vec::new();
    for account in &accounts {
        switch_buttons.push((account.clone(), switch.clone(), remove.clone()));
    }

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/60",
            onclick: move |_| onclose.call(()),
            div {
                class: "w-[28rem] rounded-lg bg-gray-800 p-4 space-y-3",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center justify-between",
                    h3 { class: "font-medium", "Accounts" }
                    button {
                        class: "text-gray-400 hover:text-white",
                        onclick: move |_| onclose.call(()),
                        "✕"
                    }
                }
                if let Some(err) = error.read().as_ref() {
                    p { class: "text-xs text-red-400", "{err}" }
                }
                for (account, switch, remove) in switch_buttons {
                    {
                        let is_active = active.as_deref() == Some(account.as_str());
                        let account_for_switch = account.clone();
                        let account_for_remove = account.clone();
                        let mut switch = switch.clone();
                        let mut remove = remove.clone();
                        rsx! {
                            div { class: "flex items-center justify-between gap-2 px-3 py-2 rounded bg-gray-700",
                                div { class: "min-w-0",
                                    p { class: "text-sm truncate", "{account}" }
                                    if is_active {
                                        p { class: "text-xs text-green-400", "Active" }
                                    }
                                }
                                div { class: "flex gap-1",
                                    button {
                                        class: "px-2 py-1 rounded bg-teal-600 hover:bg-teal-500 text-xs",
                                        disabled: is_active,
                                        onclick: move |_| switch(account_for_switch.clone()),
                                        "Switch"
                                    }
                                    button {
                                        class: "px-2 py-1 rounded bg-gray-600 hover:bg-red-600 text-xs",
                                        onclick: move |_| remove(account_for_remove.clone()),
                                        "Remove"
                                    }
                                }
                            }
                        }
                    }
                }
                if *adding.read() {
                    div { class: "space-y-2 border-t border-gray-700 pt-3",
                        input {
                            class: "w-full px-3 py-2 rounded bg-gray-900 text-sm",
                            placeholder: "user@example.com",
                            value: "{jid}",
                            oninput: move |e| jid.set(e.value()),
                        }
                        input {
                            class: "w-full px-3 py-2 rounded bg-gray-900 text-sm",
                            r#type: "password",
                            placeholder: "Password",
                            value: "{password}",
                            oninput: move |e| password.set(e.value()),
                        }
                        input {
                            class: "w-full px-3 py-2 rounded bg-gray-900 text-sm",
                            placeholder: "Resource",
                            value: "{resource}",
                            oninput: move |e| resource.set(e.value()),
                        }
                        button {
                            class: "w-full py-2 rounded bg-teal-600 hover:bg-teal-500 text-sm disabled:opacity-50",
                            disabled: *busy.read(),
                            onclick: add,
                            "Add and connect"
                        }
                    }
                } else {
                    button {
                        class: "w-full py-2 rounded bg-gray-700 hover:bg-gray-600 text-sm",
                        onclick: move |_| adding.set(true),
                        "Add account"
                    }
                }
            }
        }
    }
}

#[component]
fn SettingsDialog(onclose: EventHandler<()>) -> Element {
    let client_state = use_context::<ClientState>();
    let storage = use_context::<Arc<dyn StorageBackend>>();
    let mut notifications = client_state.notifications_enabled;
    let mut timestamps = client_state.show_timestamps;

    let notif_storage = storage.clone();
    let mut toggle_notifications = move |checked: bool| {
        notifications.set(checked);
        let _ = notif_storage.set_setting("ui.notifications", &serde_json::Value::Bool(checked));
    };
    let time_storage = storage.clone();
    let mut toggle_timestamps = move |checked: bool| {
        timestamps.set(checked);
        let _ = time_storage.set_setting("ui.timestamps", &serde_json::Value::Bool(checked));
    };

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/60",
            onclick: move |_| onclose.call(()),
            div {
                class: "w-96 rounded-lg bg-gray-800 p-4 space-y-3",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center justify-between",
                    h3 { class: "font-medium", "Settings" }
                    button {
                        class: "text-gray-400 hover:text-white",
                        onclick: move |_| onclose.call(()),
                        "✕"
                    }
                }
                label { class: "flex items-center justify-between text-sm text-gray-300",
                    "Desktop notifications"
                    input {
                        r#type: "checkbox",
                        checked: *notifications.read(),
                        onchange: move |e| toggle_notifications(e.checked()),
                    }
                }
                label { class: "flex items-center justify-between text-sm text-gray-300",
                    "Show timestamps"
                    input {
                        r#type: "checkbox",
                        checked: *timestamps.read(),
                        onchange: move |e| toggle_timestamps(e.checked()),
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
    let mut show_gateways = use_signal(|| false);
    let mut show_search = use_signal(|| false);
    let mut show_feed = client_state.show_feed;
    let profile_jid = client_state.selected_profile.read().clone();

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
            div { class: "p-2 border-t border-gray-700 flex gap-2",
                button {
                    class: "flex-1 py-2 rounded bg-gray-700 hover:bg-gray-600 text-sm",
                    onclick: move |_| {
                        let next = !*show_profile.read();
                        show_profile.set(next);
                    },
                    "Profile"
                }
                button {
                    class: "flex-1 py-2 rounded bg-gray-700 hover:bg-gray-600 text-sm",
                    onclick: move |_| show_search.set(true),
                    "Search"
                }
                button {
                    class: "flex-1 py-2 rounded bg-gray-700 hover:bg-gray-600 text-sm",
                    onclick: move |_| show_gateways.set(true),
                    "Gateways"
                }
            }
            if *show_profile.read() {
                ProfileEditor { onclose: move |_| show_profile.set(false) }
            }
            if *show_gateways.read() {
                GatewaySetup { onclose: move |_| show_gateways.set(false) }
            }
            if *show_search.read() {
                SearchDialog { onclose: move |_| show_search.set(false) }
            }
            if let Some(profile_jid) = profile_jid.clone() {
                ContactProfile {
                    jid: profile_jid,
                    onclose: move |_| {
                        let mut state = client_state.clone();
                        state.show_profile(None);
                        state.bump();
                    },
                }
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
    let vcard4 = item.as_ref().and_then(RosterItem::vcard4);
    let field = |name: &str| {
        vcard4
            .as_ref()
            .and_then(|vcard| vcard.field(name))
            .map(str::to_string)
            .or_else(|| item.as_ref().and_then(|r| r.vcard_field(name)))
            .unwrap_or_default()
    };

    let mut full_name = use_signal(|| field("FN"));
    let mut nickname = use_signal(|| field("NICKNAME"));
    let mut organization = use_signal(|| field("ORG"));
    let mut title = use_signal(|| field("TITLE"));
    let mut email = use_signal(|| field("EMAIL"));
    let mut phone = use_signal(|| field("TEL"));
    let mut url = use_signal(|| field("URL"));
    let mut note = use_signal(|| field("NOTE"));
    let mut avatar = use_signal(|| Option::<(Vec<u8>, String)>::None);
    let error = use_signal(|| Option::<String>::None);
    let saving = use_signal(|| false);

    let mut preview = current_avatar.clone();
    if let Some((bytes, mime)) = avatar.read().as_ref() {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        preview = Some(format!("data:{};base64,{}", mime, STANDARD.encode(bytes)));
    }

    let pick_avatar = move |e: FormEvent| {
        let Some(file) = e.files().into_iter().next() else {
            return;
        };
        let name = file.name();
        let mime = file
            .content_type()
            .unwrap_or_else(|| mime_type_for(&name).to_string());
        spawn(async move {
            if let Ok(bytes) = file.read_bytes().await {
                avatar.set(Some((bytes.to_vec(), mime)));
            }
        });
    };

    let save = move |_| {
        let mut saving = saving;
        let mut error = error;
        let mut cs = client_state.clone();
        let client = client.clone();
        let clean = |value: &str| {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        };
        let vcard = VCard4 {
            full_name: clean(&full_name.read()),
            nickname: clean(&nickname.read()),
            organization: clean(&organization.read()),
            title: clean(&title.read()),
            email: clean(&email.read()),
            phone: clean(&phone.read()),
            url: clean(&url.read()),
            note: clean(&note.read()),
            ..VCard4::default()
        };
        let avatar = avatar.read().clone();
        saving.set(true);
        error.set(None);
        spawn(async move {
            let avatar_ref = avatar
                .as_ref()
                .map(|(bytes, mime)| (bytes.as_slice(), mime.as_str()));
            if avatar_ref.is_some() {
                let nick = vcard.nickname.as_deref().or(vcard.full_name.as_deref());
                if let Err(e) = client.set_vcard(nick, vcard.full_name.as_deref(), avatar_ref) {
                    error.set(Some(e.to_string()));
                    saving.set(false);
                    return;
                }
            }
            match client.update_vcard(vcard.to_vcard_temp()) {
                Ok(()) => {
                    if let Err(e) = client.update_vcard4(vcard.to_element()) {
                        tracing::warn!("Failed to publish vCard4: {}", e);
                    }
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
                class: "w-[30rem] max-h-[90vh] overflow-y-auto rounded-lg bg-gray-800 p-4 space-y-3",
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
                    "Full name"
                    input {
                        class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-white focus:outline-none focus:ring-1 focus:ring-teal-500",
                        value: "{full_name}",
                        oninput: move |e| full_name.set(e.value()),
                    }
                }
                label { class: "block text-xs text-gray-400",
                    "Nickname"
                    input {
                        class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-white focus:outline-none focus:ring-1 focus:ring-teal-500",
                        value: "{nickname}",
                        oninput: move |e| nickname.set(e.value()),
                    }
                }
                label { class: "block text-xs text-gray-400",
                    "Organisation"
                    input {
                        class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-white focus:outline-none focus:ring-1 focus:ring-teal-500",
                        value: "{organization}",
                        oninput: move |e| organization.set(e.value()),
                    }
                }
                label { class: "block text-xs text-gray-400",
                    "Title"
                    input {
                        class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-white focus:outline-none focus:ring-1 focus:ring-teal-500",
                        value: "{title}",
                        oninput: move |e| title.set(e.value()),
                    }
                }
                div { class: "grid grid-cols-2 gap-2",
                    label { class: "block text-xs text-gray-400",
                        "Email"
                        input {
                            class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-white focus:outline-none focus:ring-1 focus:ring-teal-500",
                            value: "{email}",
                            oninput: move |e| email.set(e.value()),
                        }
                    }
                    label { class: "block text-xs text-gray-400",
                        "Phone"
                        input {
                            class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-white focus:outline-none focus:ring-1 focus:ring-teal-500",
                            value: "{phone}",
                            oninput: move |e| phone.set(e.value()),
                        }
                    }
                }
                label { class: "block text-xs text-gray-400",
                    "Website"
                    input {
                        class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-white focus:outline-none focus:ring-1 focus:ring-teal-500",
                        value: "{url}",
                        oninput: move |e| url.set(e.value()),
                    }
                }
                label { class: "block text-xs text-gray-400",
                    "Note"
                    input {
                        class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-white focus:outline-none focus:ring-1 focus:ring-teal-500",
                        value: "{note}",
                        oninput: move |e| note.set(e.value()),
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
fn format_action(text: &str, nick: &str) -> String {
    if let Some(action) = text.strip_prefix("/me ") {
        format!("* {nick} {}", action.trim_start())
    } else {
        text.to_string()
    }
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

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
    is_pinned: bool,
    is_archived: bool,
    notifications_enabled: bool,
    draft: Option<String>,
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
    let storage = use_context::<Arc<dyn StorageBackend>>();
    let account = client
        .connection
        .jid()
        .map(|jid| jid.to_bare().to_string())
        .unwrap_or_default();
    let mut query = use_signal(String::new);
    let mut show_archived = use_signal(|| false);
    let mut unread_only = use_signal(|| false);

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
        let prefs = chat.preferences();
        entries.push(ConvEntry {
            draft: load_draft(storage.as_ref(), &account, &bare),
            bare,
            name,
            last_text: text,
            last_time: time,
            unread: chat.unread_count(),
            online,
            is_muc: false,
            avatar_src,
            is_pinned: prefs.is_pinned,
            is_archived: prefs.is_archived,
            notifications_enabled: prefs.notifications_enabled,
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
            draft: load_draft(storage.as_ref(), &account, &bare),
            bare,
            name,
            last_text: text,
            last_time: time,
            unread: room.unread_count(),
            online: true,
            is_muc: true,
            avatar_src: None,
            is_pinned: false,
            is_archived: false,
            notifications_enabled: true,
        });
    }

    let archived_count = entries.iter().filter(|entry| entry.is_archived).count();
    let showing_archived = *show_archived.read();
    entries.retain(|entry| entry.is_archived == showing_archived);
    if *unread_only.read() {
        entries.retain(|entry| entry.unread > 0);
    }
    entries.sort_by(|a, b| {
        b.is_pinned
            .cmp(&a.is_pinned)
            .then_with(|| b.last_time.cmp(&a.last_time))
    });

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
            div { class: "flex items-center gap-1 mb-1",
                button {
                    class: if *unread_only.read() {
                        "px-2 py-1 rounded text-xs bg-teal-600 text-white"
                    } else {
                        "px-2 py-1 rounded text-xs text-gray-400 hover:text-white hover:bg-gray-700"
                    },
                    onclick: move |_| {
                        let next = !*unread_only.read();
                        unread_only.set(next);
                    },
                    "Unread"
                }
                if archived_count > 0 || showing_archived {
                    button {
                        class: "px-2 py-1 rounded text-xs text-gray-400 hover:text-white hover:bg-gray-700",
                        onclick: move |_| {
                            let next = !*show_archived.read();
                            show_archived.set(next);
                        },
                        if showing_archived {
                            "← Back to chats"
                        } else {
                            "📦 Archived ({archived_count})"
                        }
                    }
                }
            }
            if entries.is_empty() {
                div { class: "p-6 text-center text-sm text-gray-500",
                    if showing_archived {
                        "No archived conversations"
                    } else {
                        "No conversations yet"
                    }
                }
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
                                    let _ = client2.fetch_avatar_str(&bare2);
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
    let client = use_context::<XmppClient>();
    let client_state = use_context::<ClientState>();
    let avatar = initial_of(&entry.name);
    let avatar_src = entry.avatar_src.clone();
    let name = entry.name.clone();
    let last_text = entry
        .draft
        .as_ref()
        .map(|draft| format!("Draft: {draft}"))
        .unwrap_or_else(|| entry.last_text.clone());
    let time_str = entry.last_time.map(format_time).unwrap_or_default();
    let unread = entry.unread;
    let online = entry.online;
    let is_muc = entry.is_muc;
    let is_pinned = entry.is_pinned;
    let is_archived = entry.is_archived;
    let muted = !entry.notifications_enabled;

    let selected_class = if selected { "bg-gray-700" } else { "" };

    let bare = entry.bare.clone();
    let mut cs = client_state.clone();
    let client_pin = client.clone();
    let pin_bare = bare.clone();
    let pin = move |_| {
        if let Some(mut chat) = client_pin.chats.get_by_bare(&pin_bare) {
            chat.set_pinned(!is_pinned);
        }
        cs.bump();
    };
    let mut cs_mute = client_state.clone();
    let client_mute = client.clone();
    let mute_bare = bare.clone();
    let mute = move |_| {
        if let Some(mut chat) = client_mute.chats.get_by_bare(&mute_bare) {
            chat.set_notifications_enabled(!muted);
        }
        cs_mute.bump();
    };
    let mut cs_archive = client_state.clone();
    let client_archive = client.clone();
    let archive_bare = bare.clone();
    let archive = move |_| {
        if let Some(mut chat) = client_archive.chats.get_by_bare(&archive_bare) {
            chat.set_archived(!is_archived);
        }
        if !is_archived && cs_archive.selected_chat.read().as_deref() == Some(archive_bare.as_str())
        {
            cs_archive.select(None);
        }
        cs_archive.bump();
    };

    rsx! {
        div { class: "group flex items-center gap-1 rounded-lg hover:bg-gray-700 transition-colors {selected_class}",
            button {
                class: "flex flex-1 items-center gap-3 p-3 text-left min-w-0",
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
                        div { class: "flex items-center gap-1 min-w-0",
                            h4 { class: "font-medium truncate", "{name}" }
                            if is_pinned {
                                span { class: "text-xs", title: "Pinned", "📌" }
                            }
                            if muted {
                                span { class: "text-xs", title: "Muted", "🔕" }
                            }
                        }
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
            if !is_muc {
                div { class: "flex flex-col pr-1 opacity-50 group-hover:opacity-100 transition-opacity",
                    button {
                        class: "px-1 text-xs text-gray-400 hover:text-white",
                        title: if is_pinned { "Unpin" } else { "Pin" },
                        onclick: pin,
                        "📌"
                    }
                    button {
                        class: "px-1 text-xs text-gray-400 hover:text-white",
                        title: if muted { "Unmute" } else { "Mute" },
                        onclick: mute,
                        "🔕"
                    }
                    button {
                        class: "px-1 text-xs text-gray-400 hover:text-white",
                        title: if is_archived { "Unarchive" } else { "Archive" },
                        onclick: archive,
                        "📦"
                    }
                }
            }
        }
    }
}

#[component]
fn RoomConfigDialog(room: String, onclose: EventHandler<()>) -> Element {
    let client = use_context::<XmppClient>();
    let client_state = use_context::<ClientState>();
    let mut form = use_signal(|| Option::<xmpp_core::DataForm>::None);
    let mut values = use_signal(HashMap::<String, String>::new);
    let mut error = use_signal(|| Option::<String>::None);
    let mut status = use_signal(|| Option::<String>::None);
    let mut saving = use_signal(|| false);
    let mut confirm_destroy = use_signal(|| false);

    use_future({
        let client = client.clone();
        let room = room.clone();
        move || {
            let client = client.clone();
            let room = room.clone();
            async move {
                let Ok(jid) = room.parse::<xmpp_core::Jid>() else {
                    error.set(Some("Invalid room JID".into()));
                    return;
                };
                match client.room_config(&jid).await {
                    Ok(parsed) => {
                        let mut initial = HashMap::new();
                        for field in &parsed.fields {
                            if !field.value.is_empty() {
                                initial.insert(field.var.clone(), field.value.clone());
                            }
                        }
                        values.set(initial);
                        form.set(Some(parsed));
                    }
                    Err(err) => error.set(Some(err.to_string())),
                }
            }
        }
    });

    let save_client = client.clone();
    let save_room = room.clone();
    let save = move |_| {
        let client = save_client.clone();
        let room = save_room.clone();
        let mut saving = saving;
        let mut error = error;
        let mut status = status;
        let Some(parsed) = form.read().clone() else {
            return;
        };
        let pairs: Vec<(String, String)> = parsed
            .fields
            .iter()
            .map(|field| {
                let value = values
                    .read()
                    .get(&field.var)
                    .cloned()
                    .unwrap_or_else(|| field.value.clone());
                (field.var.clone(), value)
            })
            .collect();
        saving.set(true);
        error.set(None);
        status.set(None);
        spawn(async move {
            let Ok(jid) = room.parse::<xmpp_core::Jid>() else {
                error.set(Some("Invalid room JID".into()));
                saving.set(false);
                return;
            };
            match client
                .submit_room_config(&jid, parsed.form.as_ref(), &pairs)
                .await
            {
                Ok(()) => status.set(Some("Room configuration saved.".into())),
                Err(err) => error.set(Some(err.to_string())),
            }
            saving.set(false);
        });
    };

    let destroy_client = client.clone();
    let destroy_room = room.clone();
    let mut cs = client_state.clone();
    let destroy = move |_| {
        if !*confirm_destroy.read() {
            confirm_destroy.set(true);
            return;
        }
        let client = destroy_client.clone();
        let room = destroy_room.clone();
        let mut error = error;
        let mut status = status;
        let mut state = cs.clone();
        spawn(async move {
            let Ok(jid) = room.parse::<xmpp_core::Jid>() else {
                error.set(Some("Invalid room JID".into()));
                return;
            };
            match client.destroy_room(&jid, None).await {
                Ok(()) => {
                    status.set(Some("Room destroyed.".into()));
                    state.select(None);
                    state.bump();
                    onclose.call(());
                }
                Err(err) => error.set(Some(err.to_string())),
            }
        });
    };

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/60",
            onclick: move |_| onclose.call(()),
            div {
                class: "w-[30rem] max-h-[90vh] overflow-y-auto rounded-lg bg-gray-800 p-4 space-y-3",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center justify-between",
                    h3 { class: "font-medium", "Room configuration" }
                    button {
                        class: "text-gray-400 hover:text-white",
                        onclick: move |_| onclose.call(()),
                        "✕"
                    }
                }
                if let Some(err) = error.read().as_ref() {
                    p { class: "text-xs text-red-400", "{err}" }
                }
                if let Some(parsed) = form.read().as_ref() {
                    if let Some(title) = parsed.title.clone() {
                        p { class: "text-sm font-medium", "{title}" }
                    }
                    if let Some(instructions) = parsed.instructions.clone() {
                        p { class: "text-xs text-gray-400", "{instructions}" }
                    }
                    for field in parsed.fields.iter().filter(|field| !field.is_hidden()) {
                        {
                            let var = field.var.clone();
                            let label = field.label.clone();
                            let secret = field.is_secret();
                            let boolean = field.is_boolean();
                            let options = field.options.clone();
                            let value = values
                                .read()
                                .get(&var)
                                .cloned()
                                .unwrap_or_else(|| field.value.clone());
                            rsx! {
                                label { class: "block text-xs text-gray-400",
                                    "{label}"
                                    if boolean {
                                        select {
                                            class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-white",
                                            value: "{value}",
                                            onchange: {
                                                let var = var.clone();
                                                move |e| {
                                                    values.write().insert(var.clone(), e.value());
                                                }
                                            },
                                            option { value: "1", "Yes" }
                                            option { value: "0", "No" }
                                        }
                                    } else if !options.is_empty() {
                                        select {
                                            class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-white",
                                            value: "{value}",
                                            onchange: {
                                                let var = var.clone();
                                                move |e| {
                                                    values.write().insert(var.clone(), e.value());
                                                }
                                            },
                                            for (option_label, option_value) in options {
                                                option { value: "{option_value}", "{option_label}" }
                                            }
                                        }
                                    } else {
                                        input {
                                            class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-white focus:outline-none focus:ring-1 focus:ring-teal-500",
                                            r#type: if secret { "password" } else { "text" },
                                            value: "{value}",
                                            oninput: {
                                                let var = var.clone();
                                                move |e| {
                                                    values.write().insert(var.clone(), e.value());
                                                }
                                            },
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "flex gap-2 pt-1",
                        button {
                            class: "flex-1 py-2 rounded bg-teal-600 hover:bg-teal-500 text-sm disabled:opacity-50",
                            disabled: *saving.read(),
                            onclick: save,
                            "Save"
                        }
                        button {
                            class: "px-3 py-2 rounded bg-red-700 hover:bg-red-600 text-sm",
                            onclick: destroy,
                            if *confirm_destroy.read() {
                                "Confirm destroy"
                            } else {
                                "Destroy"
                            }
                        }
                    }
                } else if error.read().is_none() {
                    p { class: "text-xs text-gray-400", "Loading configuration…" }
                }
                if let Some(message) = status.read().as_ref() {
                    p { class: "text-xs text-teal-400", "{message}" }
                }
            }
        }
    }
}

#[component]
fn InviteDialog(room: String, onclose: EventHandler<()>) -> Element {
    let client = use_context::<XmppClient>();
    let mut jid = use_signal(String::new);
    let mut reason = use_signal(String::new);
    let mut error = use_signal(|| Option::<String>::None);

    let invite_client = client.clone();
    let invite_room = room.clone();
    let invite = move |_| {
        let Ok(room_jid) = invite_room.parse::<xmpp_core::Jid>() else {
            error.set(Some("Invalid room JID".into()));
            return;
        };
        let Ok(invitee) = jid.read().trim().parse::<xmpp_core::Jid>() else {
            error.set(Some("Enter a valid JID to invite".into()));
            return;
        };
        let reason = {
            let reason = reason.read().trim().to_string();
            (!reason.is_empty()).then_some(reason)
        };
        match invite_client.invite_to_muc(&room_jid, &invitee, reason.as_deref()) {
            Ok(()) => onclose.call(()),
            Err(err) => error.set(Some(err.to_string())),
        }
    };

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/60",
            onclick: move |_| onclose.call(()),
            div {
                class: "w-80 rounded-lg bg-gray-800 p-4 space-y-3",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center justify-between",
                    h3 { class: "font-medium", "Invite to room" }
                    button {
                        class: "text-gray-400 hover:text-white",
                        onclick: move |_| onclose.call(()),
                        "✕"
                    }
                }
                input {
                    class: "w-full px-3 py-2 rounded bg-gray-900 text-sm text-white placeholder-gray-500",
                    placeholder: "user@example.org",
                    value: "{jid}",
                    oninput: move |e| jid.set(e.value()),
                }
                input {
                    class: "w-full px-3 py-2 rounded bg-gray-900 text-sm text-white placeholder-gray-500",
                    placeholder: "Reason (optional)",
                    value: "{reason}",
                    oninput: move |e| reason.set(e.value()),
                }
                if let Some(err) = error.read().as_ref() {
                    p { class: "text-xs text-red-400", "{err}" }
                }
                div { class: "flex justify-end gap-2",
                    button {
                        class: "px-3 py-1.5 rounded bg-gray-700 hover:bg-gray-600 text-sm",
                        onclick: move |_| onclose.call(()),
                        "Cancel"
                    }
                    button {
                        class: "px-3 py-1.5 rounded bg-teal-600 hover:bg-teal-500 text-sm",
                        onclick: invite,
                        "Invite"
                    }
                }
            }
        }
    }
}

#[component]
fn ForwardDialog(source: String, message_id: String, onclose: EventHandler<()>) -> Element {
    let client = use_context::<XmppClient>();
    let client_state = use_context::<ClientState>();
    let mut error = use_signal(|| Option::<String>::None);
    let mut cs = client_state.clone();

    let mut chats = client.chats.list();
    chats.retain(|chat| chat.bare_jid != source);
    chats.sort_by(|a, b| a.bare_jid.cmp(&b.bare_jid));
    let mut rooms = client.mucs.list();
    rooms.sort_by(|a, b| a.bare_jid.cmp(&b.bare_jid));

    let forward_client = client.clone();
    let forward_source = source.clone();
    let forward_message = message_id.clone();
    let forward = move |target: String, is_room: bool| {
        let client = forward_client.clone();
        let source = forward_source.clone();
        let message_id = forward_message.clone();
        let mut error = error;
        let mut cs = cs.clone();
        let result = (|| {
            let chat = client.chats.get_by_bare(&source)?;
            let original = chat.find_message(&message_id)?;
            if is_room {
                let room = client.mucs.get_by_bare(&target)?;
                let nick = room.self_nick().unwrap_or_else(|| "me".to_string());
                client.send_muc_forward(&room.jid, &nick, &original).ok()?;
            } else {
                let jid = target.parse::<xmpp_core::Jid>().ok()?;
                client.send_forward(&jid, &original).ok()?;
            }
            Some(())
        })();
        match result {
            Some(()) => {
                cs.bump();
                onclose.call(());
            }
            None => error.set(Some("Could not forward this message.".into())),
        }
    };

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/60",
            onclick: move |_| onclose.call(()),
            div {
                class: "w-96 max-h-[70vh] overflow-y-auto rounded-lg bg-gray-800 p-4 space-y-2",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center justify-between",
                    h3 { class: "font-medium", "Forward message" }
                    button {
                        class: "text-gray-400 hover:text-white",
                        onclick: move |_| onclose.call(()),
                        "✕"
                    }
                }
                if chats.is_empty() && rooms.is_empty() {
                    p { class: "text-sm text-gray-500", "No other conversations to forward to." }
                }
                for chat in chats {
                    {
                        let target = chat.bare_jid.clone();
                        let name = client
                            .roster
                            .get_by_bare(&target)
                            .map(|item| item.display_name())
                            .unwrap_or_else(|| target.clone());
                        let forward = forward.clone();
                        rsx! {
                            button {
                                class: "w-full text-left px-3 py-2 rounded hover:bg-gray-700",
                                onclick: move |_| forward(target.clone(), false),
                                span { class: "text-sm", "{name}" }
                                span { class: "ml-2 text-xs text-gray-500", "{target}" }
                            }
                        }
                    }
                }
                for room in rooms {
                    {
                        let target = room.bare_jid.clone();
                        let name = if room.name.is_empty() { target.clone() } else { room.name.clone() };
                        let forward = forward.clone();
                        rsx! {
                            button {
                                class: "w-full text-left px-3 py-2 rounded hover:bg-gray-700",
                                onclick: move |_| forward(target.clone(), true),
                                span { class: "text-sm", "# {name}" }
                                span { class: "ml-2 text-xs text-gray-500", "{target}" }
                            }
                        }
                    }
                }
                if let Some(err) = error.read().as_ref() {
                    p { class: "text-xs text-red-400", "{err}" }
                }
            }
        }
    }
}

#[component]
fn ClearHistoryDialog(target: String, is_muc: bool, onclose: EventHandler<()>) -> Element {
    let client = use_context::<XmppClient>();
    let client_state = use_context::<ClientState>();
    let storage = use_context::<Arc<dyn StorageBackend>>();
    let account = client
        .connection
        .jid()
        .map(|jid| jid.to_bare().to_string())
        .unwrap_or_default();
    let mut cs = client_state.clone();
    let confirm_client = client.clone();
    let confirm_storage = storage.clone();
    let confirm_target = target.clone();
    let confirm = move |_| {
        if is_muc {
            if let Err(error) = confirm_storage.clear_muc_messages_for(&account, &confirm_target) {
                tracing::warn!("Failed to clear room history: {}", error);
            }
            if let Some(mut room) = confirm_client.mucs.get_by_bare(&confirm_target) {
                room.clear_messages();
            }
        } else {
            if let Err(error) = confirm_storage.clear_messages_for(&account, &confirm_target) {
                tracing::warn!("Failed to clear chat history: {}", error);
            }
            if let Some(mut chat) = confirm_client.chats.get_by_bare(&confirm_target) {
                chat.clear_messages();
            }
        }
        cs.bump();
        onclose.call(());
    };

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/60",
            onclick: move |_| onclose.call(()),
            div {
                class: "w-80 rounded-lg bg-gray-800 p-4 space-y-3",
                onclick: move |e| e.stop_propagation(),
                h3 { class: "font-medium", "Clear history" }
                p { class: "text-sm text-gray-400",
                    "This removes the messages from this device and local storage. Messages on the server are not deleted."
                }
                div { class: "flex justify-end gap-2",
                    button {
                        class: "px-3 py-1.5 rounded bg-gray-700 hover:bg-gray-600 text-sm",
                        onclick: move |_| onclose.call(()),
                        "Cancel"
                    }
                    button {
                        class: "px-3 py-1.5 rounded bg-red-600 hover:bg-red-500 text-sm",
                        onclick: confirm,
                        "Clear"
                    }
                }
            }
        }
    }
}

#[component]
fn SearchDialog(onclose: EventHandler<()>) -> Element {
    let client = use_context::<XmppClient>();
    let client_state = use_context::<ClientState>();
    let storage = use_context::<Arc<dyn StorageBackend>>();
    let mut query = use_signal(String::new);
    let account = client
        .connection
        .jid()
        .map(|jid| jid.to_bare().to_string())
        .unwrap_or_default();
    let needle = query.read().trim().to_string();
    let active = needle.len() >= 2;
    let messages = if active {
        storage
            .search_messages_for(&account, &needle, 20)
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let muc_messages = if active {
        storage
            .search_muc_messages_for(&account, &needle, 20)
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let contacts: Vec<RosterItem> = if active {
        client
            .roster
            .list()
            .into_iter()
            .filter(|item| {
                item.display_name()
                    .to_lowercase()
                    .contains(&needle.to_lowercase())
                    || item
                        .jid
                        .to_bare()
                        .to_string()
                        .to_lowercase()
                        .contains(&needle.to_lowercase())
            })
            .take(10)
            .collect()
    } else {
        Vec::new()
    };
    let rooms: Vec<_> = if active {
        client
            .mucs
            .list()
            .into_iter()
            .filter(|room| {
                room.name.to_lowercase().contains(&needle.to_lowercase())
                    || room
                        .bare_jid
                        .to_lowercase()
                        .contains(&needle.to_lowercase())
            })
            .take(10)
            .collect()
    } else {
        Vec::new()
    };
    let empty =
        messages.is_empty() && muc_messages.is_empty() && contacts.is_empty() && rooms.is_empty();

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-start justify-center bg-black/60 pt-24",
            onclick: move |_| onclose.call(()),
            div {
                class: "w-[36rem] max-h-[70vh] overflow-y-auto rounded-lg bg-gray-800 p-4 space-y-3",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center justify-between",
                    h3 { class: "font-medium", "Search" }
                    button {
                        class: "text-gray-400 hover:text-white",
                        onclick: move |_| onclose.call(()),
                        "✕"
                    }
                }
                input {
                    class: "w-full px-3 py-2 rounded bg-gray-900 text-sm text-white placeholder-gray-500 focus:outline-none focus:ring-1 focus:ring-teal-500",
                    r#type: "search",
                    placeholder: "Search messages, contacts and rooms",
                    value: "{query}",
                    oninput: move |e| query.set(e.value()),
                }
                if !active {
                    p { class: "text-xs text-gray-500", "Type at least two characters." }
                } else if empty {
                    p { class: "text-xs text-gray-500", "No results." }
                } else {
                    if !messages.is_empty() {
                        div {
                            p { class: "text-xs uppercase text-gray-500 mb-1", "Messages" }
                            for message in messages {
                                {
                                    let peer = message.bare_jid.clone();
                                    let title = client
                                        .roster
                                        .get_by_bare(&peer)
                                        .map(|item| item.display_name())
                                        .unwrap_or_else(|| peer.clone());
                                    let body = message.display_body();
                                    let time = format_time(message.timestamp);
                                    let mut cs = client_state.clone();
                                    let client = client.clone();
                                    rsx! {
                                        button {
                                            class: "w-full text-left px-3 py-2 rounded hover:bg-gray-700",
                                            onclick: move |_| {
                                                cs.select(Some(peer.clone()));
                                                if let Ok(jid) = peer.parse::<xmpp_core::Jid>() {
                                                    let _ = client.chats.get_or_create(&jid);
                                                }
                                                cs.bump();
                                                onclose.call(());
                                            },
                                            div { class: "flex items-center justify-between gap-2",
                                                span { class: "text-sm font-medium truncate", "{title}" }
                                                span { class: "text-xs text-gray-500 whitespace-nowrap", "{time}" }
                                            }
                                            p { class: "text-xs text-gray-400 truncate", "{body}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if !muc_messages.is_empty() {
                        div {
                            p { class: "text-xs uppercase text-gray-500 mb-1", "Room messages" }
                            for message in muc_messages {
                                {
                                    let room = message.room_jid.clone();
                                    let title = client
                                        .mucs
                                        .get_by_bare(&room)
                                        .map(|room| room.name)
                                        .filter(|name| !name.is_empty())
                                        .unwrap_or_else(|| room.clone());
                                    let body = message
                                        .body
                                        .clone()
                                        .unwrap_or_else(|| "[attachment]".to_string());
                                    let time = format_time(message.timestamp);
                                    let mut cs = client_state.clone();
                                    let client = client.clone();
                                    rsx! {
                                        button {
                                            class: "w-full text-left px-3 py-2 rounded hover:bg-gray-700",
                                            onclick: move |_| {
                                                cs.select(Some(room.clone()));
                                                if let Ok(jid) = room.parse::<xmpp_core::Jid>() {
                                                    let _ = client.mucs.get_or_create(&jid);
                                                }
                                                cs.bump();
                                                onclose.call(());
                                            },
                                            div { class: "flex items-center justify-between gap-2",
                                                span { class: "text-sm font-medium truncate", "{title}" }
                                                span { class: "text-xs text-gray-500 whitespace-nowrap", "{time}" }
                                            }
                                            p { class: "text-xs text-gray-400 truncate", "{body}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if !contacts.is_empty() {
                        div {
                            p { class: "text-xs uppercase text-gray-500 mb-1", "Contacts" }
                            for item in contacts {
                                {
                                    let jid = item.jid.clone();
                                    let name = item.display_name();
                                    let bare = jid.to_bare().to_string();
                                    let mut cs = client_state.clone();
                                    let client = client.clone();
                                    rsx! {
                                        button {
                                            class: "w-full text-left px-3 py-2 rounded hover:bg-gray-700",
                                            onclick: move |_| {
                                                let _ = client.chats.get_or_create(&jid);
                                                cs.select(Some(bare.clone()));
                                                cs.bump();
                                                onclose.call(());
                                            },
                                            span { class: "text-sm", "{name}" }
                                            span { class: "ml-2 text-xs text-gray-500", "{bare}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if !rooms.is_empty() {
                        div {
                            p { class: "text-xs uppercase text-gray-500 mb-1", "Rooms" }
                            for room in rooms {
                                {
                                    let bare = room.bare_jid.clone();
                                    let name = if room.name.is_empty() { bare.clone() } else { room.name.clone() };
                                    let mut cs = client_state.clone();
                                    rsx! {
                                        button {
                                            class: "w-full text-left px-3 py-2 rounded hover:bg-gray-700",
                                            onclick: move |_| {
                                                cs.select(Some(bare.clone()));
                                                cs.bump();
                                                onclose.call(());
                                            },
                                            span { class: "text-sm", "{name}" }
                                            span { class: "ml-2 text-xs text-gray-500", "{bare}" }
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

/// Contact profile drawer: presence, vCard, activity, groups and actions.
#[component]
fn ContactProfile(jid: String, onclose: EventHandler<()>) -> Element {
    let client_state = use_context::<ClientState>();
    let client = use_context::<XmppClient>();
    let _rev = *client_state.revision.read();

    let item = client.roster.get_by_bare(&jid);
    let mut groups = use_signal(|| {
        item.as_ref()
            .map(|item| item.groups.join(", "))
            .unwrap_or_default()
    });
    let mut display_name = use_signal(|| {
        item.as_ref()
            .and_then(|item| item.name.clone())
            .unwrap_or_default()
    });

    let Some(item) = item else {
        return rsx! {
            div { class: "fixed inset-0 z-50 flex items-center justify-center bg-black/60",
                div { class: "bg-gray-800 rounded-xl p-6 w-96 border border-gray-700",
                    p { class: "text-gray-300 mb-4", "Contact not found" }
                    button {
                        class: "w-full py-2 rounded bg-gray-700 hover:bg-gray-600",
                        onclick: move |_| onclose.call(()),
                        "Close"
                    }
                }
            }
        };
    };

    let name = item.display_name();
    let avatar = initial_of(&name);
    let avatar_src = avatar_data_uri(&item);
    let online = item.is_online();
    let show = presence_text(item.best_show());
    let full_name = item.vcard_field("FN");
    let org = item.vcard_field("ORG");
    let title = item.vcard_field("TITLE");
    let role = item.vcard_field("ROLE");
    let nickname = item.vcard_field("NICKNAME");
    let email = item.vcard_field("EMAIL");
    let phone = item.vcard_field("TEL");
    let website = item.vcard_field("URL");
    let note = item.vcard_field("NOTE");
    let tune = item.tune_summary();
    let mood = item.mood_value();
    let activity = item.activity_value();
    let location = item.pep_field(item.location.as_deref(), "text");
    let caps_node = item.caps.as_ref().map(|caps| caps.node.clone());
    let resources: Vec<_> = item.presence.values().cloned().collect();
    let blocked = item.is_blocked;
    let online_class = if online {
        "text-green-400"
    } else {
        "text-gray-500"
    };
    let item_jid = item.jid.clone();
    let bare = item.jid.to_bare().to_string();

    let mut cs_save = client_state.clone();
    let client_save = client.clone();
    let jid_save = item.jid.clone();
    let save_groups = move |_| {
        let groups: Vec<String> = groups
            .read()
            .split(',')
            .map(str::trim)
            .filter(|group| !group.is_empty())
            .map(str::to_string)
            .collect();
        match client_save.set_contact_groups(&jid_save, &groups) {
            Ok(()) => cs_save.bump(),
            Err(error) => tracing::warn!("Failed to save groups: {}", error),
        }
    };

    let mut cs_name = client_state.clone();
    let client_name = client.clone();
    let jid_name = item.jid.clone();
    let save_name = move |_| {
        let name = display_name.read().trim().to_string();
        let name = (!name.is_empty()).then_some(name);
        match client_name.set_contact_name(&jid_name, name.as_deref()) {
            Ok(()) => cs_name.bump(),
            Err(error) => tracing::warn!("Failed to rename contact: {}", error),
        }
    };

    let mut cs_chat = client_state.clone();
    let client_chat = client.clone();
    let jid_chat = item.jid.clone();
    let bare_chat = bare.clone();
    let open_chat = move |_| {
        let _ = client_chat.chats.get_or_create(&jid_chat);
        cs_chat.select(Some(bare_chat.clone()));
        cs_chat.show_profile(None);
        cs_chat.bump();
    };

    let mut cs_block = client_state.clone();
    let client_block = client.clone();
    let jid_block = item.jid.clone();
    let toggle_block = move |_| {
        let result = if blocked {
            client_block.roster.unblock(&jid_block)
        } else {
            client_block.roster.block(&jid_block)
        };
        if let Err(error) = result {
            tracing::warn!("Failed to change block state: {}", error);
        }
        cs_block.bump();
    };

    let mut cs_refresh = client_state.clone();
    let client_refresh = client.clone();
    let jid_refresh = item_jid.clone();
    let bare_refresh = bare.clone();
    let refresh = move |_| {
        let _ = client_refresh.request_vcard(&jid_refresh);
        let _ = client_refresh.fetch_avatar_str(&bare_refresh);
        cs_refresh.bump();
    };

    rsx! {
        div { class: "fixed inset-0 z-50 flex items-center justify-center bg-black/60",
            div { class: "bg-gray-800 rounded-xl p-6 w-[26rem] max-h-[90vh] overflow-y-auto border border-gray-700 space-y-4",
                div { class: "flex items-start gap-4",
                    div { class: "w-16 h-16 rounded-full bg-teal-500 flex items-center justify-center text-2xl font-bold text-white overflow-hidden",
                        if let Some(src) = avatar_src {
                            img { class: "w-full h-full object-cover", src: "{src}", alt: "{avatar}" }
                        } else {
                            "{avatar}"
                        }
                    }
                    div { class: "flex-1 min-w-0",
                        h3 { class: "text-lg font-semibold truncate", "{name}" }
                        p { class: "text-xs text-gray-400 truncate", "{bare}" }
                        p { class: "text-xs mt-1 {online_class}", "{show}" }
                    }
                    button {
                        class: "text-gray-400 hover:text-white",
                        onclick: move |_| onclose.call(()),
                        "✕"
                    }
                }

                div { class: "flex gap-2",
                    button {
                        class: "flex-1 py-2 rounded bg-teal-500 hover:bg-teal-400 text-sm text-white",
                        onclick: open_chat,
                        "Open chat"
                    }
                    button {
                        class: "px-3 py-2 rounded bg-gray-700 hover:bg-gray-600 text-sm",
                        onclick: toggle_block,
                        if blocked { "Unblock" } else { "Block" }
                    }
                    button {
                        class: "px-3 py-2 rounded bg-gray-700 hover:bg-gray-600 text-sm",
                        onclick: refresh,
                        "Refresh"
                    }
                }

                div { class: "space-y-2 text-sm",
                    if let Some(full_name) = full_name {
                        ProfileField { label: "Name", value: full_name }
                    }
                    if let Some(nickname) = nickname {
                        ProfileField { label: "Nickname", value: nickname }
                    }
                    if let Some(org) = org {
                        ProfileField { label: "Organisation", value: org }
                    }
                    if let Some(title) = title {
                        ProfileField { label: "Title", value: title }
                    }
                    if let Some(role) = role {
                        ProfileField { label: "Role", value: role }
                    }
                    if let Some(email) = email {
                        ProfileField { label: "Email", value: email }
                    }
                    if let Some(phone) = phone {
                        ProfileField { label: "Phone", value: phone }
                    }
                    if let Some(website) = website {
                        ProfileField { label: "Website", value: website }
                    }
                    if let Some(note) = note {
                        ProfileField { label: "Note", value: note }
                    }
                    if let Some(tune) = tune {
                        ProfileField { label: "Listening", value: tune }
                    }
                    if let Some(mood) = mood {
                        ProfileField { label: "Mood", value: mood }
                    }
                    if let Some(activity) = activity {
                        ProfileField { label: "Activity", value: activity }
                    }
                    if let Some(location) = location {
                        ProfileField { label: "Location", value: location }
                    }
                    if let Some(caps_node) = caps_node {
                        ProfileField { label: "Capabilities", value: caps_node }
                    }
                }

                div {
                    h4 { class: "text-xs uppercase tracking-wide text-gray-500 mb-2", "Resources" }
                    if resources.is_empty() {
                        p { class: "text-xs text-gray-500", "No presence information" }
                    }
                    for presence in resources {
                        {
                            let resource = presence
                                .resource
                                .clone()
                                .unwrap_or_else(|| "default".to_string());
                            rsx! {
                                div { class: "flex items-center justify-between text-xs text-gray-300 py-1 border-b border-gray-700/50",
                                    span { "{resource}" }
                                    span { "{presence_text(presence.show)}" }
                                }
                            }
                        }
                    }
                }

                div { class: "space-y-2",
                    label { class: "text-xs uppercase tracking-wide text-gray-500", "Name" }
                    div { class: "flex gap-2",
                        input {
                            class: "flex-1 px-3 py-1.5 bg-gray-700 border border-gray-600 rounded-lg text-sm text-white",
                            value: "{display_name}",
                            oninput: move |event| display_name.set(event.value()),
                        }
                        button {
                            class: "px-3 py-1.5 bg-teal-500 hover:bg-teal-400 rounded-lg text-sm text-white",
                            onclick: save_name,
                            "Save"
                        }
                    }
                }

                div { class: "space-y-2",
                    label { class: "text-xs uppercase tracking-wide text-gray-500", "Groups (comma separated)" }
                    div { class: "flex gap-2",
                        input {
                            class: "flex-1 px-3 py-1.5 bg-gray-700 border border-gray-600 rounded-lg text-sm text-white",
                            value: "{groups}",
                            oninput: move |event| groups.set(event.value()),
                        }
                        button {
                            class: "px-3 py-1.5 bg-teal-500 hover:bg-teal-400 rounded-lg text-sm text-white",
                            onclick: save_groups,
                            "Save"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ProfileField(label: &'static str, value: String) -> Element {
    rsx! {
        div { class: "flex justify-between gap-4",
            span { class: "text-gray-500", "{label}" }
            span { class: "text-gray-200 text-right break-all", "{value}" }
        }
    }
}

/// Gateway discovery and registration (XEP-0100).
#[component]
fn GatewaySetup(onclose: EventHandler<()>) -> Element {
    let client = use_context::<XmppClient>();
    let mut gateways = use_signal(Vec::<Gateway>::new);
    let mut registration = use_signal(|| Option::<GatewayRegistration>::None);
    let mut selected = use_signal(|| Option::<Gateway>::None);
    let mut values = use_signal(HashMap::<String, String>::new);
    let mut status = use_signal(|| Option::<String>::None);
    let mut error = use_signal(|| Option::<String>::None);
    let mut loading = use_signal(|| true);

    let refresh = {
        let client = client.clone();
        move || {
            let client = client.clone();
            let mut loading = loading;
            let mut error = error;
            let mut gateways = gateways;
            loading.set(true);
            error.set(None);
            spawn(async move {
                match client.list_gateways().await {
                    Ok(found) => {
                        if found.is_empty() {
                            error.set(Some("This server advertises no gateways.".into()));
                        }
                        gateways.set(found);
                    }
                    Err(e) => error.set(Some(e.to_string())),
                }
                loading.set(false);
            });
        }
    };
    use_future({
        let client = client.clone();
        move || {
            let client = client.clone();
            async move {
                match client.list_gateways().await {
                    Ok(found) => {
                        if found.is_empty() {
                            error.set(Some("This server advertises no gateways.".into()));
                        }
                        gateways.set(found);
                    }
                    Err(e) => error.set(Some(e.to_string())),
                }
                loading.set(false);
            }
        }
    });

    let choose = {
        let client = client.clone();
        move |gateway: Gateway| {
            let client = client.clone();
            let mut selected = selected;
            let mut registration = registration;
            let mut values = values;
            let mut status = status;
            let mut error = error;
            selected.set(Some(gateway.clone()));
            registration.set(None);
            values.set(HashMap::new());
            status.set(None);
            error.set(None);
            spawn(async move {
                let jid = match gateway.jid.parse::<xmpp_core::Jid>() {
                    Ok(jid) => jid,
                    Err(e) => {
                        error.set(Some(e.to_string()));
                        return;
                    }
                };
                match client.gateway_registration(&jid).await {
                    Ok(form) => {
                        let mut initial = HashMap::new();
                        for field in &form.fields {
                            if !field.value.is_empty() {
                                initial.insert(field.var.clone(), field.value.clone());
                            }
                        }
                        values.set(initial);
                        registration.set(Some(form));
                    }
                    Err(e) => error.set(Some(e.to_string())),
                }
            });
        }
    };

    let submit = {
        let client = client.clone();
        move |remove: bool| {
            let client = client.clone();
            let mut status = status;
            let mut error = error;
            let Some(gateway) = selected.read().clone() else {
                return;
            };
            let form = registration.read().as_ref().and_then(|r| r.form.clone());
            let pairs: Vec<(String, String)> = registration
                .read()
                .as_ref()
                .map(|r| {
                    r.fields
                        .iter()
                        .map(|field| {
                            let value = values
                                .read()
                                .get(&field.var)
                                .cloned()
                                .unwrap_or_else(|| field.value.clone());
                            (field.var.clone(), value)
                        })
                        .collect()
                })
                .unwrap_or_default();
            status.set(Some(if remove {
                "Removing…".into()
            } else {
                "Saving…".into()
            }));
            spawn(async move {
                let jid = match gateway.jid.parse::<xmpp_core::Jid>() {
                    Ok(jid) => jid,
                    Err(e) => {
                        error.set(Some(e.to_string()));
                        return;
                    }
                };
                match client
                    .submit_gateway_registration(&jid, form.as_ref(), &pairs, remove)
                    .await
                {
                    Ok(()) => status.set(Some(if remove {
                        "Registration removed.".into()
                    } else {
                        "Registration submitted.".into()
                    })),
                    Err(e) => {
                        error.set(Some(e.to_string()));
                        status.set(None);
                    }
                }
            });
        }
    };

    let current = selected.read().clone();
    let refresh_button = refresh.clone();
    let submit_register = submit.clone();
    let submit_remove = submit.clone();
    let choose_item = choose.clone();
    rsx! {
        div { class: "fixed inset-0 z-50 flex items-center justify-center bg-black/60",
            div { class: "w-[30rem] max-h-[90vh] overflow-y-auto rounded-lg bg-gray-800 p-4 space-y-3",
                div { class: "flex items-center justify-between",
                    h3 { class: "font-medium", "Gateways" }
                    div { class: "flex gap-2",
                        button {
                            class: "text-xs text-gray-400 hover:text-white",
                            onclick: move |_| refresh_button(),
                            "Refresh"
                        }
                        button {
                            class: "text-gray-400 hover:text-white",
                            onclick: move |_| onclose.call(()),
                            "✕"
                        }
                    }
                }
                if let Some(err) = error.read().as_ref() {
                    p { class: "text-xs text-red-400", "{err}" }
                }
                if *loading.read() {
                    p { class: "text-xs text-gray-400", "Looking for gateways…" }
                }
                if let Some(gateway) = current {
                    div { class: "space-y-2",
                        div { class: "flex items-center justify-between",
                            div {
                                p { class: "text-sm font-medium", "{gateway.name}" }
                                p { class: "text-xs text-gray-400", "{gateway.jid} · {gateway.kind}" }
                            }
                            button {
                                class: "text-xs text-gray-400 hover:text-white",
                                onclick: move |_| {
                                    selected.set(None);
                                    registration.set(None);
                                    values.set(HashMap::new());
                                    status.set(None);
                                },
                                "Back"
                            }
                        }
                        if let Some(reg) = registration.read().as_ref() {
                            if let Some(instructions) = reg.instructions.clone() {
                                p { class: "text-xs text-gray-400", "{instructions}" }
                            }
                            if reg.registered {
                                p { class: "text-xs text-green-400", "This account is registered." }
                            }
                            for field in reg.fields.iter().filter(|field| !field.is_hidden()) {
                                {
                                    let var = field.var.clone();
                                    let label = field.label.clone();
                                    let kind = field.kind.clone();
                                    let secret = field.is_secret();
                                    let options = field.options.clone();
                                    let value = values
                                        .read()
                                        .get(&var)
                                        .cloned()
                                        .unwrap_or_else(|| field.value.clone());
                                    let required = field.required;
                                    rsx! {
                                        label { class: "block text-xs text-gray-400",
                                            "{label}"
                                            if !options.is_empty() {
                                                select {
                                                    class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-white",
                                                    value: "{value}",
                                                    onchange: {
                                                        let var = var.clone();
                                                        move |e| {
                                                            values.write().insert(var.clone(), e.value());
                                                        }
                                                    },
                                                    for (option_label, option_value) in options {
                                                        option { value: "{option_value}", "{option_label}" }
                                                    }
                                                }
                                            } else {
                                                input {
                                                    class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-white focus:outline-none focus:ring-1 focus:ring-teal-500",
                                                    r#type: if secret { "password" } else if kind == "text-multi" { "text" } else { "text" },
                                                    required,
                                                    value: "{value}",
                                                    oninput: {
                                                        let var = var.clone();
                                                        move |e| {
                                                            values.write().insert(var.clone(), e.value());
                                                        }
                                                    },
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            div { class: "flex gap-2 pt-1",
                                button {
                                    class: "flex-1 py-2 rounded bg-teal-600 hover:bg-teal-500 text-sm",
                                    onclick: move |_| submit_register(false),
                                    "Register"
                                }
                                button {
                                    class: "px-3 py-2 rounded bg-gray-700 hover:bg-gray-600 text-sm",
                                    onclick: move |_| submit_remove(true),
                                    "Remove"
                                }
                            }
                        } else {
                            p { class: "text-xs text-gray-400", "Loading form…" }
                        }
                        if let Some(message) = status.read().as_ref() {
                            p { class: "text-xs text-teal-400", "{message}" }
                        }
                    }
                } else {
                    for gateway in gateways.read().iter() {
                        {
                            let gateway = gateway.clone();
                            let name = gateway.name.clone();
                            let jid = gateway.jid.clone();
                            let kind = gateway.kind.clone();
                            let choose_item = choose_item.clone();
                            rsx! {
                                button {
                                    class: "w-full text-left px-3 py-2 rounded bg-gray-700 hover:bg-gray-600",
                                    onclick: move |_| choose_item(gateway.clone()),
                                    p { class: "text-sm", "{name}" }
                                    p { class: "text-xs text-gray-400", "{jid} · {kind}" }
                                }
                            }
                        }
                    }
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
    let mut show_blocked = use_signal(|| false);
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
    let blocked_count = items.iter().filter(|item| item.is_blocked).count();
    if *show_blocked.read() {
        items.retain(|item| item.is_blocked);
    }
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
            if blocked_count > 0 || *show_blocked.read() {
                button {
                    class: if *show_blocked.read() {
                        "px-2 py-1 rounded text-xs bg-red-600 text-white"
                    } else {
                        "px-2 py-1 rounded text-xs text-gray-400 hover:text-white hover:bg-gray-700"
                    },
                    onclick: move |_| {
                        let next = !*show_blocked.read();
                        show_blocked.set(next);
                    },
                    if *show_blocked.read() {
                        "← All contacts"
                    } else {
                        "Blocked ({blocked_count})"
                    }
                }
            }
            if items.is_empty() {
                div { class: "p-6 text-center text-sm text-gray-500",
                    if *show_blocked.read() {
                        "No blocked contacts"
                    } else {
                        "No contacts"
                    }
                }
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
                    let _ = client2.fetch_avatar_str(&bare);
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
                class: "text-xs text-gray-400 hover:text-teal-400 opacity-0 group-hover:opacity-100",
                onclick: {
                    let mut cs_info = client_state.clone();
                    let info_jid = item.jid.to_bare().to_string();
                    move |_| {
                        cs_info.show_profile(Some(info_jid.clone()));
                        cs_info.bump();
                    }
                },
                "Info"
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
                    let mut leave_state = client_state.clone();
                    let leave_client = client.clone();
                    let leave_room = room.jid.clone();
                    let leave_bare = bare.clone();
                    rsx! {
                        div { class: "group flex items-center gap-3 p-3 rounded-lg hover:bg-gray-700 transition-colors",
                            button {
                                class: "flex flex-1 items-center gap-3 text-left min-w-0",
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
                            button {
                                class: "text-xs text-gray-400 hover:text-red-400 opacity-0 group-hover:opacity-100",
                                onclick: move |_| {
                                    let _ = leave_client.leave_room(&leave_room);
                                    if leave_state.selected_chat.read().as_deref()
                                        == Some(leave_bare.as_str())
                                    {
                                        leave_state.select(None);
                                    }
                                    leave_state.bump();
                                },
                                "Leave"
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
    media: Vec<xmpp_core::MediaAttachment>,
    forwarded_from: Option<String>,
    mentions: Vec<String>,
    attention: bool,
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
    let can_admin = client
        .mucs
        .get_by_bare(&bare)
        .is_some_and(|room| room.can_admin());
    let mut show_devices = use_signal(|| false);
    let mut show_members = use_signal(|| false);
    let mut editing_subject = use_signal(|| false);
    let mut subject_input = use_signal(String::new);
    let mut search = use_signal(String::new);
    let mut show_clear = use_signal(|| false);
    let mut show_invite = use_signal(|| false);
    let mut show_config = use_signal(|| false);
    let mut forward_id = use_signal(|| Option::<String>::None);

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

    let needle = search.read().trim().to_lowercase();
    if !needle.is_empty() {
        messages.retain(|m| m.body.to_lowercase().contains(&needle));
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
                    input {
                        class: "w-40 px-2 py-1 rounded bg-gray-900 text-xs text-white placeholder-gray-500 focus:outline-none focus:ring-1 focus:ring-teal-500",
                        r#type: "search",
                        placeholder: "Search messages",
                        value: "{search}",
                        oninput: move |e| search.set(e.value()),
                    }
                    if !messages.is_empty() {
                        button {
                            class: "text-xs px-2 py-1 rounded bg-gray-700 hover:bg-red-600",
                            title: "Clear this conversation's history",
                            onclick: move |_| show_clear.set(true),
                            "Clear"
                        }
                    }
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
                                button {
                                    class: "text-xs px-2 py-1 rounded bg-gray-700 hover:bg-gray-600",
                                    onclick: move |_| show_invite.set(true),
                                    "Invite"
                                }
                                if can_admin {
                                    button {
                                        class: "text-xs px-2 py-1 rounded bg-gray-700 hover:bg-gray-600",
                                        onclick: move |_| show_config.set(true),
                                        "Configure"
                                    }
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
                                    title: "Send an attention request",
                                    onclick: {
                                        let attention_client = client.clone();
                                        let attention_target = bare.clone();
                                        move |_| {
                                            if let Some(chat) = attention_client.chats.get_by_bare(&attention_target) {
                                                let _ = attention_client.send_attention(&chat.jid);
                                            }
                                        }
                                    },
                                    "Nudge"
                                }
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

            if *show_clear.read() {
                ClearHistoryDialog {
                    target: bare.clone(),
                    is_muc,
                    onclose: move |_| show_clear.set(false),
                }
            }
            if *show_invite.read() {
                InviteDialog {
                    room: bare.clone(),
                    onclose: move |_| show_invite.set(false),
                }
            }
            if *show_config.read() {
                RoomConfigDialog {
                    room: bare.clone(),
                    onclose: move |_| show_config.set(false),
                }
            }
            if let Some(message_id) = forward_id.read().clone() {
                ForwardDialog {
                    source: bare.clone(),
                    message_id,
                    onclose: move |_| forward_id.set(None),
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
                        let mut forward_target = forward_id;
                        let target = bare.clone();
                        rsx! {
                            MessageBubble {
                                msg: msg,
                                target: target,
                                is_muc: is_muc,
                                onreply: move |m: DisplayMessage| {
                                    reply_target.set(Some((m.id.clone(), m.body.clone())));
                                },
                                onforward: move |id| forward_target.set(Some(id)),
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
                key: "{bare}",
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
        media: m.media.clone(),
        forwarded_from: m
            .forward_info
            .as_ref()
            .map(|forward| forward.original_jid.to_string()),
        mentions: m.mentions.clone(),
        attention: m.attention,
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
        media: m.media.clone(),
        forwarded_from: m
            .forward_info
            .as_ref()
            .map(|forward| forward.original_jid.to_string()),
        mentions: m.mentions.clone(),
        attention: m.attention,
    }
}

/// Message bubble
#[component]
fn MessageBubble(
    msg: DisplayMessage,
    target: String,
    #[props(default = false)] is_muc: bool,
    onreply: EventHandler<DisplayMessage>,
    onforward: EventHandler<String>,
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
    let media = msg.media.clone();
    let forwarded_from = msg.forwarded_from.clone();
    let mentions = msg.mentions.clone();
    let attention = msg.attention;
    let show_body = !body.is_empty() && !media.iter().any(|attachment| attachment.url == body);
    let own_bare = client
        .connection
        .jid()
        .map(|jid| jid.to_bare().to_string())
        .unwrap_or_default();
    let self_nick = if is_muc {
        client
            .mucs
            .get_by_bare(&target)
            .and_then(|room| room.self_nick())
    } else {
        None
    };
    let mentions_me = mentions.iter().any(|uri| {
        if uri == &format!("xmpp:{own_bare}") {
            return true;
        }
        self_nick.as_ref().is_some_and(|nick| {
            uri == &format!("xmpp:{target}/{nick}")
                || (uri.starts_with(&format!("xmpp:{target}/"))
                    && uri.ends_with(&format!("/{nick}")))
        })
    });
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
                    if let Some(from) = forwarded_from {
                        p { class: "text-[11px] text-gray-300/70 mb-1", "Forwarded from {from}" }
                    }
                    if mentions_me {
                        p { class: "text-[11px] text-yellow-300 mb-1", "mentioned you" }
                    }
                    if attention {
                        p { class: "text-[11px] text-orange-300 mb-1", "⚠️ Attention" }
                    }
                    if show_body {
                        FormattedText { text: body.clone() }
                    }
                    for attachment in media {
                        {
                            let url = attachment.url.clone();
                            let name = attachment
                                .name
                                .clone()
                                .unwrap_or_else(|| url.clone());
                            let size = attachment.size.map(format_size).unwrap_or_default();
                            match attachment.media_type {
                                xmpp_core::MediaType::Image => rsx! {
                                    a {
                                        key: "{attachment.id}",
                                        class: "block mt-2",
                                        href: "{url}",
                                        target: "_blank",
                                        rel: "noopener noreferrer",
                                        img {
                                            class: "rounded-lg max-h-80 max-w-full object-cover",
                                            src: "{url}",
                                            alt: "{name}",
                                            loading: "lazy",
                                        }
                                    }
                                },
                                xmpp_core::MediaType::Video => rsx! {
                                    video {
                                        key: "{attachment.id}",
                                        class: "mt-2 rounded-lg max-h-80 max-w-full",
                                        controls: "true",
                                        preload: "metadata",
                                        src: "{url}",
                                    }
                                },
                                xmpp_core::MediaType::Audio => rsx! {
                                    audio {
                                        key: "{attachment.id}",
                                        class: "mt-2 w-full",
                                        controls: "true",
                                        preload: "metadata",
                                        src: "{url}",
                                    }
                                },
                                _ => rsx! {
                                    a {
                                        key: "{attachment.id}",
                                        class: "mt-2 flex items-center gap-2 rounded-lg bg-black/20 px-3 py-2 text-sm hover:bg-black/30",
                                        href: "{url}",
                                        target: "_blank",
                                        rel: "noopener noreferrer",
                                        download: "{name}",
                                        span { "📎" }
                                        span { class: "truncate", "{name}" }
                                        if !size.is_empty() {
                                            span { class: "ml-auto text-xs opacity-70", "{size}" }
                                        }
                                    }
                                },
                            }
                        }
                    }
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
                    if *client_state.show_timestamps.read() {
                        span { "{time}" }
                    }
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
                            let mut onforward = onforward;
                            let forward_id = msg_id.clone();
                            rsx! {
                                button {
                                    class: "{action_class}",
                                    title: "Forward",
                                    onclick: move |_| onforward.call(forward_id.clone()),
                                    "↪"
                                }
                            }
                        }
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
    let storage = use_context::<Arc<dyn StorageBackend>>();
    let account = client
        .connection
        .jid()
        .map(|jid| jid.to_bare().to_string())
        .unwrap_or_default();
    let mut message = {
        let storage = storage.clone();
        let account = account.clone();
        let target = target.clone();
        use_signal(move || load_draft(storage.as_ref(), &account, &target).unwrap_or_default())
    };
    let mut composing = use_signal(|| false);
    let mut uploading = use_signal(|| false);
    let mut upload_progress = use_signal(|| Option::<String>::None);
    let mut upload_cancel = use_signal(|| false);
    let mut upload_error = use_signal(|| Option::<String>::None);
    let mut omemo = use_signal(|| false);
    let mut omemo_error = use_signal(|| Option::<String>::None);

    let mut cs = client_state.clone();
    let client_send = client.clone();
    let client_typing = client.clone();
    let target_typing = target.clone();
    let target_send = target.clone();
    let send_storage = storage.clone();
    let send_account = account.clone();

    let send = move |_| {
        let raw = message.read().trim().to_string();
        if raw.is_empty() {
            return;
        }
        let bare = target_send.clone();
        let action_nick = if is_muc {
            client_send
                .mucs
                .get_by_bare(&bare)
                .and_then(|room| room.self_nick())
        } else {
            client_send.connection.jid().map(|jid| {
                jid.to_string()
                    .split('@')
                    .next()
                    .unwrap_or("me")
                    .to_string()
            })
        };
        let text = format_action(&raw, action_nick.as_deref().unwrap_or("me"));
        let draft_storage = send_storage.clone();
        let draft_account = send_account.clone();
        let draft_target = target_send.clone();
        if is_muc {
            if let Some(room) = client_send.mucs.get_by_bare(&bare) {
                let nick = room.self_nick().unwrap_or_else(|| "me".to_string());
                match client_send.send_muc_message(&room.jid, &nick, &text) {
                    Ok(_) => {
                        message.set(String::new());
                        let _ =
                            save_draft(draft_storage.as_ref(), &draft_account, &draft_target, "");
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
                    let _ = save_draft(draft_storage.as_ref(), &draft_account, &draft_target, "");
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
        let _ = save_draft(storage.as_ref(), &account, &target_typing, &val);
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
        let files = e.files();
        if files.is_empty() {
            return;
        }
        uploading.set(true);
        upload_cancel.set(false);
        upload_progress.set(None);
        upload_error.set(None);
        let client = client_file.clone();
        let bare = target_file.clone();
        spawn(async move {
            let total = files.len();
            let mut sent = false;
            let mut cancelled = false;
            for (index, file) in files.into_iter().enumerate() {
                if *upload_cancel.read() {
                    cancelled = true;
                    break;
                }
                let name = file.name();
                upload_progress.set(Some(format!("Uploading {}/{}: {name}", index + 1, total)));
                let bytes = match file.read_bytes().await {
                    Ok(bytes) => bytes.to_vec(),
                    Err(_) => {
                        upload_error.set(Some(format!("Could not read {name}")));
                        break;
                    }
                };
                let mime = mime_type_for(&name);
                let size = bytes.len() as u64;
                match client.upload_file(&name, bytes, &mime).await {
                    Ok(slot) => {
                        if is_muc {
                            if let Some(room) = client.mucs.get_by_bare(&bare) {
                                let nick = room.self_nick().unwrap_or_else(|| "me".to_string());
                                let _ = client.send_muc_file_message(
                                    &room.jid,
                                    &nick,
                                    &slot.get_url,
                                    &name,
                                    &mime,
                                    size,
                                );
                            }
                        } else if let Some(chat) = client.chats.get_by_bare(&bare) {
                            let _ = client.send_file_message(
                                &chat.jid,
                                &slot.get_url,
                                &name,
                                &mime,
                                size,
                            );
                        }
                        sent = true;
                    }
                    Err(err) => {
                        upload_error.set(Some(format!("{name}: {err}")));
                        break;
                    }
                }
            }
            if cancelled && !sent {
                upload_error.set(Some("Upload cancelled".to_string()));
            }
            if sent {
                on_sent.call(());
            }
            upload_progress.set(None);
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
                        multiple: true,
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
                if *uploading.read() {
                    button {
                        class: "p-2 text-xs rounded-lg text-red-400 hover:text-red-300",
                        title: "Stop uploading the remaining files",
                        onclick: move |_| upload_cancel.set(true),
                        "Cancel"
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
            if let Some(progress) = upload_progress.read().as_ref() {
                p { class: "mt-2 text-xs text-gray-400", "{progress}" }
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
