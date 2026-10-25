//! UI components for Jabu

pub mod emoji;
pub mod emoji_packs;
pub mod emoji_picker;
pub mod icons;
pub mod spaces;
pub mod state;
pub mod sticker_packs;
pub mod sticker_picker;
pub use spaces::{Space, SpaceDirectory, SpaceId, SpaceNotify, SpaceRoom, SpaceRoomKind};
pub use state::{AppSection, ClientState, Theme};

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use emoji_picker::EmojiPicker;
use icons::{Icon, IconName};
use std::collections::HashMap;
use std::sync::Arc;
use sticker_picker::StickerPicker;
use storage::{
    load_account_credentials, load_draft, load_session, save_account_credentials, save_credentials,
    save_draft, SettingsOmemoStore, StorageBackend, StoredAccount, StoredCredentials,
};
/// Compiled Tailwind stylesheet, embedded so the desktop webview never loses it
/// when the document head is reconciled.
const APP_CSS: &str = include_str!("../assets/app.css");

use xmpp_core::{
    chat::ChatStateNotification,
    muc::{MucAffiliation, MucRole},
    omemo::TrustLevel,
    roster::{PresenceShow, SubscriptionState},
    vcard4::VCard4,
    CallKind, CallState, ChatMessage, ConnectionState, Gateway, GatewayRegistration, MucMessage,
    RosterItem, XmppClient,
};

/// Fonts and the compiled Tailwind stylesheet.
///
/// Rendered by the stable application root rather than by [`App`], so the
/// document head is not dropped when the login screen is replaced by the
/// signed-in shell.
#[component]
pub fn AppStyles() -> Element {
    let font_css = format!(
        r#"
@font-face {{
  font-family: "IBM Plex Sans";
  src: url("{}") format("woff2");
  font-weight: 400;
  font-style: normal;
  font-display: swap;
}}
@font-face {{
  font-family: "IBM Plex Sans";
  src: url("{}") format("woff2");
  font-weight: 500;
  font-style: normal;
  font-display: swap;
}}
@font-face {{
  font-family: "IBM Plex Sans";
  src: url("{}") format("woff2");
  font-weight: 600;
  font-style: normal;
  font-display: swap;
}}
@font-face {{
  font-family: "IBM Plex Mono";
  src: url("{}") format("woff2");
  font-weight: 400;
  font-style: normal;
  font-display: swap;
}}
"#,
        asset!("/assets/fonts/ibm-plex-sans-400.woff2"),
        asset!("/assets/fonts/ibm-plex-sans-500.woff2"),
        asset!("/assets/fonts/ibm-plex-sans-600.woff2"),
        asset!("/assets/fonts/ibm-plex-mono-400.woff2"),
    );
    // Dioxus rebuilds the desktop document head when the signed-in shell
    // replaces the login screen, which drops `document::Stylesheet`. Keep the
    // stylesheet as a body-level <style> instead and re-assert it periodically.
    use_future(move || {
        let font_css = font_css.clone();
        async move {
            let css = format!("{font_css}\n{APP_CSS}");
            let Ok(css) = serde_json::to_string(&css) else {
                return;
            };
            let script = format!(
                r#"(function() {{
                let style = document.getElementById('jabu-app-css');
                if (!style) {{
                    style = document.createElement('style');
                    style.id = 'jabu-app-css';
                    document.body.appendChild(style);
                }}
                style.textContent = {css};
            }})();"#
            );
            loop {
                dioxus::document::eval(&script);
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
    });
    rsx! {}
}

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

    let client = use_context::<XmppClient>();
    let mut revision = client_state.revision;
    let ticker_client = client.clone();
    use_future(move || {
        let ticker_client = ticker_client.clone();
        async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                if ticker_client
                    .call_sessions()
                    .iter()
                    .any(|session| !session.state.is_final())
                    || ticker_client.voice.is_joined()
                {
                    let next = *revision.read() + 1;
                    revision.set(next);
                }
            }
        }
    });

    let mut search_state = client_state.clone();
    let theme = *client_state.theme.read();
    let theme_class = format!(
        "theme-{} min-h-screen bg-gray-950 text-gray-100",
        theme.slug()
    );
    rsx! {
        div { class: "{theme_class}",
            if has_account {
                div { class: "app-shell",
                    DesktopTitlebar {}
                    ConnectionBanner {}
                    div { class: "flex min-h-0 flex-1 overflow-hidden",
                        SpaceRail {}
                        ContextSidebar {}
                        MainChatArea {}
                    }
                }
                IncomingCallDialog {}
                ActiveCallBar {}
                if *client_state.search_open.read() {
                    SearchDialog {
                        onclose: move |_| {
                            search_state.search_open.set(false);
                            search_state.bump();
                        }
                    }
                }
            } else {
                div { class: "app-shell",
                    DesktopTitlebar {}
                    ConnectionBanner {}
                    div { class: "flex min-h-0 flex-1 overflow-hidden",
                        LoginScreen {}
                    }
                }
            }
        }
    }
}

#[component]
fn IncomingCallDialog() -> Element {
    let client = use_context::<XmppClient>();
    let client_state = use_context::<ClientState>();
    let _ = *client_state.revision.read();
    let Some(session) = client
        .call_sessions()
        .into_iter()
        .find(|session| session.state == CallState::Incoming)
    else {
        return rsx! {};
    };
    let name = client
        .roster
        .get(&session.peer)
        .map(|item| item.display_name())
        .unwrap_or_else(|| session.peer.to_string());
    let kind = session.kind.as_str();
    let sid_accept = session.sid.clone();
    let sid_reject = session.sid.clone();
    let accept_client = client.clone();
    let reject_client = client.clone();
    let accept_state = client_state.clone();
    let reject_state = client_state.clone();
    let icon = if session.kind == CallKind::Video {
        IconName::Video
    } else {
        IconName::PhoneIncoming
    };

    rsx! {
        div { class: "fixed inset-0 z-[70] flex items-center justify-center bg-black/70 p-4",
            div { class: "w-80 rounded-2xl border border-gray-700 bg-gray-900 p-5 text-center shadow-panel",
                div { class: "mx-auto mb-3 flex h-16 w-16 items-center justify-center rounded-full bg-teal-500 text-white",
                    Icon { name: icon, class: "h-8 w-8" }
                }
                h3 { class: "text-lg font-semibold", "{name}" }
                p { class: "mt-1 text-xs text-gray-400", "Incoming {kind} call" }
                div { class: "mt-5 flex gap-2",
                    button {
                        class: "inline-flex flex-1 items-center justify-center gap-1 rounded-xl bg-red-600 px-3 py-2 text-sm font-medium text-white hover:bg-red-500",
                        onclick: move |_| {
                            let client = reject_client.clone();
                            let sid = sid_reject.clone();
                            let mut cs = reject_state.clone();
                            spawn(async move {
                                let _ = client.reject_call(&sid, Some("decline")).await;
                                cs.bump();
                            });
                        },
                        Icon { name: IconName::PhoneOff, class: "h-4 w-4" }
                        "Decline"
                    }
                    button {
                        class: "inline-flex flex-1 items-center justify-center gap-1 rounded-xl bg-teal-500 px-3 py-2 text-sm font-semibold text-white hover:bg-teal-400",
                        onclick: move |_| {
                            let client = accept_client.clone();
                            let sid = sid_accept.clone();
                            let mut cs = accept_state.clone();
                            spawn(async move {
                                let _ = client.accept_call(&sid).await;
                                cs.bump();
                            });
                        },
                        Icon { name: IconName::Phone, class: "h-4 w-4" }
                        "Accept"
                    }
                }
            }
        }
    }
}

#[component]
fn ActiveCallBar() -> Element {
    let client = use_context::<XmppClient>();
    let client_state = use_context::<ClientState>();
    let _ = *client_state.revision.read();
    let Some(session) = client
        .call_sessions()
        .into_iter()
        .find(|session| !session.state.is_final() && session.state != CallState::Incoming)
    else {
        return rsx! {};
    };
    let name = client
        .roster
        .get(&session.peer)
        .map(|item| item.display_name())
        .unwrap_or_else(|| session.peer.to_string());
    let elapsed = session
        .connected_at
        .map(|started| (chrono::Utc::now() - started).num_seconds().max(0))
        .unwrap_or(0);
    let timer = format!("{:02}:{:02}", elapsed / 60, elapsed % 60);
    let state = match session.state {
        CallState::Proposing => "Calling…",
        CallState::Ringing => "Ringing…",
        CallState::Accepted => "Connecting…",
        CallState::Connecting => "Connecting…",
        CallState::Active => "In call",
        CallState::Ended => "Ended",
        CallState::Rejected => "Declined",
        CallState::Missed => "Missed",
        CallState::Failed => "Failed",
        CallState::Incoming => "Incoming",
    };
    let icon = if session.kind == CallKind::Video {
        IconName::Video
    } else {
        IconName::Phone
    };
    let sid = session.sid.clone();
    let sid_mute = sid.clone();
    let sid_speaker = sid.clone();
    let sid_camera = sid.clone();
    let sid_switch = sid.clone();
    let sid_screen = sid.clone();
    let sid_end = sid.clone();
    let end_client = client.clone();
    let mute_client = client.clone();
    let speaker_client = client.clone();
    let camera_client = client.clone();
    let switch_client = client.clone();
    let screen_client = client.clone();
    let mut muted = use_signal(|| false);
    let mut speaker_muted = use_signal(|| false);
    let mut audio_error = use_signal(|| Option::<String>::None);
    let camera_devices = use_signal(|| client.call_camera_devices());
    let camera_enabled = session.camera_enabled;
    let camera_pending = session.camera_pending;
    let camera_error = session.camera_error.clone();
    let camera_kind = session.kind;
    let camera_available = camera_kind == CallKind::Video || session.state == CallState::Active;
    let screen_sharing = session.screen_sharing;
    let screen_expected = session.remote_screen;
    let screen_pending = session.screen_pending;
    let screen_error = session.screen_error.clone();
    let screen_available = session.state == CallState::Active;
    let camera_state = client_state.clone();
    let switch_state = client_state.clone();
    let screen_state = client_state.clone();
    let call_state = client_state.clone();

    rsx! {
        if camera_kind == CallKind::Video {
            div { class: "pointer-events-none fixed inset-x-0 top-16 bottom-24 z-[55] flex items-center justify-center p-4",
                RemoteCallVideo { sid: sid.clone(), screen_expected }
            }
        }
        if camera_kind == CallKind::Video && camera_enabled {
            div { class: "fixed bottom-24 right-4 z-[64] overflow-hidden rounded-xl border border-gray-700 bg-gray-950 shadow-panel",
                CallPreview { sid: sid.clone() }
            }
        }
        if screen_sharing {
            div { class: "fixed bottom-24 left-4 z-[64] overflow-hidden rounded-xl border border-gray-700 bg-gray-950 shadow-panel",
                CallPreview { sid: sid.clone(), screen: true }
            }
        }
        div { class: "fixed bottom-4 left-1/2 z-[65] flex -translate-x-1/2 items-center gap-3 rounded-2xl border border-gray-700 bg-gray-900/95 px-4 py-3 shadow-panel backdrop-blur",
            div { class: "flex h-10 w-10 items-center justify-center rounded-full bg-teal-500 text-white",
                Icon { name: icon, class: "h-5 w-5" }
            }
            div { class: "min-w-32",
                p { class: "text-sm font-semibold", "{name}" }
                p { class: "text-xs text-gray-400", "{state} · {timer}" }
                if camera_pending {
                    p { class: "text-[10px] text-teal-400", "Turning on camera…" }
                }
                if screen_pending {
                    p { class: "text-[10px] text-teal-400", "Starting screen share…" }
                }
                if let Some(err) = camera_error.as_ref() {
                    p { class: "text-[10px] text-red-400", "{err}" }
                }
                if let Some(err) = screen_error.as_ref() {
                    p { class: "text-[10px] text-red-400", "{err}" }
                }
                if let Some(err) = audio_error.read().as_ref() {
                    p { class: "text-[10px] text-red-400", "{err}" }
                }
            }
            button {
                class: if *muted.read() { "inline-flex h-10 w-10 items-center justify-center rounded-full bg-red-600 text-white" } else { "inline-flex h-10 w-10 items-center justify-center rounded-full bg-gray-700 text-gray-100 hover:bg-gray-600" },
                title: if *muted.read() { "Unmute" } else { "Mute" },
                onclick: move |_| {
                    let next = !*muted.read();
                    muted.set(next);
                    audio_error.set(None);
                    let client = mute_client.clone();
                    let sid = sid_mute.clone();
                    let mut revert = muted;
                    let mut error_slot = audio_error;
                    spawn(async move {
                        if let Err(error) = client.set_call_muted(&sid, next).await {
                            revert.set(!next);
                            error_slot.set(Some(error.to_string()));
                        }
                    });
                },
                Icon { name: if *muted.read() { IconName::MicOff } else { IconName::Mic }, class: "h-5 w-5" }
            }
            button {
                class: if *speaker_muted.read() { "inline-flex h-10 w-10 items-center justify-center rounded-full bg-red-600 text-white" } else { "inline-flex h-10 w-10 items-center justify-center rounded-full bg-gray-700 text-gray-100 hover:bg-gray-600" },
                title: if *speaker_muted.read() { "Unmute speaker" } else { "Mute speaker" },
                onclick: move |_| {
                    let next = !*speaker_muted.read();
                    speaker_muted.set(next);
                    audio_error.set(None);
                    let client = speaker_client.clone();
                    let sid = sid_speaker.clone();
                    let mut revert = speaker_muted;
                    let mut error_slot = audio_error;
                    spawn(async move {
                        if let Err(error) = client.set_call_speaker_muted(&sid, next).await {
                            revert.set(!next);
                            error_slot.set(Some(error.to_string()));
                        }
                    });
                },
                Icon { name: if *speaker_muted.read() { IconName::VolumeOff } else { IconName::Volume }, class: "h-5 w-5" }
            }
            if camera_available {
                button {
                    class: if camera_pending { "inline-flex h-10 w-10 items-center justify-center rounded-full bg-gray-700 text-gray-500" } else if camera_enabled { "inline-flex h-10 w-10 items-center justify-center rounded-full bg-teal-600 text-white hover:bg-teal-500" } else { "inline-flex h-10 w-10 items-center justify-center rounded-full bg-gray-700 text-gray-100 hover:bg-gray-600" },
                    title: if camera_pending { "Turning on camera…" } else if camera_enabled { "Turn camera off" } else { "Turn camera on" },
                    disabled: camera_pending,
                    onclick: move |_| {
                        let next = !camera_enabled;
                        let client = camera_client.clone();
                        let sid = sid_camera.clone();
                        let mut cs = camera_state.clone();
                        spawn(async move {
                            let _ = client.set_call_camera(&sid, next).await;
                            cs.bump();
                        });
                    },
                    Icon { name: if camera_enabled { IconName::Video } else { IconName::VideoOff }, class: "h-5 w-5" }
                }
            }
            if camera_available && camera_enabled && camera_devices.read().len() > 1 {
                button {
                    class: "inline-flex h-10 w-10 items-center justify-center rounded-full bg-gray-700 text-gray-100 hover:bg-gray-600",
                    title: "Switch camera",
                    onclick: move |_| {
                        let client = switch_client.clone();
                        let sid = sid_switch.clone();
                        let mut cs = switch_state.clone();
                        spawn(async move {
                            let _ = client.switch_call_camera(&sid).await;
                            cs.bump();
                        });
                    },
                    Icon { name: IconName::SwitchCamera, class: "h-5 w-5" }
                }
            }
            if screen_available {
                button {
                    class: if screen_pending { "inline-flex h-10 w-10 items-center justify-center rounded-full bg-gray-700 text-gray-500" } else if screen_sharing { "inline-flex h-10 w-10 items-center justify-center rounded-full bg-teal-600 text-white hover:bg-teal-500" } else { "inline-flex h-10 w-10 items-center justify-center rounded-full bg-gray-700 text-gray-100 hover:bg-gray-600" },
                    title: if screen_pending { "Starting screen share…" } else if screen_sharing { "Stop sharing screen" } else { "Share screen" },
                    disabled: screen_pending,
                    onclick: move |_| {
                        let client = screen_client.clone();
                        let sid = sid_screen.clone();
                        let mut cs = screen_state.clone();
                        let sharing = screen_sharing;
                        spawn(async move {
                            if sharing {
                                let _ = client.stop_call_screen_share(&sid).await;
                            } else {
                                let _ = client.start_call_screen_share(&sid).await;
                            }
                            cs.bump();
                        });
                    },
                    Icon { name: IconName::Monitor, class: "h-5 w-5" }
                }
            }
            button {
                class: "inline-flex h-10 w-10 items-center justify-center rounded-full bg-red-600 text-white hover:bg-red-500",
                title: "Hang up",
                onclick: move |_| {
                    let client = end_client.clone();
                    let sid = sid_end.clone();
                    let mut cs = call_state.clone();
                    spawn(async move {
                        let _ = client.end_call(&sid, Some("success")).await;
                        cs.bump();
                    });
                },
                Icon { name: IconName::PhoneOff, class: "h-5 w-5" }
            }
        }
    }
}

#[component]
fn ConferenceScreen(sid: String) -> Element {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let client = use_context::<XmppClient>();
    let mut frame = use_signal(|| Option::<String>::None);
    let mut at = use_signal(std::time::Instant::now);

    use_future({
        let client = client.clone();
        let sid = sid.clone();
        move || {
            let client = client.clone();
            let sid = sid.clone();
            async move {
                let mut last = u64::MAX;
                loop {
                    if let Some(preview) = client.call_remote_screen(&sid) {
                        if preview.sequence != last {
                            last = preview.sequence;
                            frame.set(Some(format!(
                                "data:image/jpeg;base64,{}",
                                STANDARD.encode(preview.jpeg.as_slice())
                            )));
                            at.set(std::time::Instant::now());
                        }
                    }
                    // Nothing will update the image once the share ends, and a
                    // first call to `call_remote_screen` may keep handing back
                    // the last frame, so drop it here: the parent re-renders on
                    // its own clock, but this component is memoised on `sid`.
                    if frame.read().is_some()
                        && at.read().elapsed() >= std::time::Duration::from_millis(1500)
                    {
                        frame.set(None);
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            }
        }
    });

    let fresh =
        frame.read().is_some() && at.read().elapsed() < std::time::Duration::from_millis(1500);
    rsx! {
        if fresh {
            if let Some(src) = frame.read().as_ref() {
                img {
                    class: "max-h-[46vh] w-full max-w-3xl rounded-xl border border-teal-500/40 bg-black object-contain shadow-panel",
                    src: "{src}",
                    alt: "Shared screen",
                }
            }
        }
    }
}

#[component]
fn VoiceChannelView(space_id: SpaceId, room: SpaceRoom) -> Element {
    let client = use_context::<XmppClient>();
    let client_state = use_context::<ClientState>();
    let storage = use_context::<Arc<dyn StorageBackend>>();
    let _rev = *client_state.revision.read();
    let error = use_signal(|| Option::<String>::None);
    let mut joining = use_signal(|| false);
    let mut revision = client_state.revision;

    use_future(move || async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            let next = *revision.read() + 1;
            revision.set(next);
        }
    });

    let room_jid = room.conference.parse::<xmpp_core::Jid>().ok();
    let conference_sid = room_jid
        .as_ref()
        .and_then(|jid| client.conference.session(jid))
        .map(|session| session.sid);
    let screen_sharing = room_jid
        .as_ref()
        .is_some_and(|jid| client.conference.screen_session(jid).is_some());
    let channel = client.voice.channel();
    let joined = client.voice.is_joined()
        && channel
            .as_ref()
            .is_some_and(|channel| channel.room == room.conference);
    let participants = if joined {
        client.voice.participants()
    } else {
        Vec::new()
    };
    let muted = participants
        .iter()
        .find(|participant| participant.is_self)
        .is_some_and(|participant| participant.muted);

    let join_client = client.clone();
    let join_storage = storage.clone();
    let join_space = space_id.clone();
    let join_room = room.clone();
    let on_join = move |_| {
        let client = join_client.clone();
        let storage = join_storage.clone();
        let space_id = join_space.clone();
        let room = join_room.clone();
        let mut error = error;
        let mut joining = joining;
        joining.set(true);
        spawn(async move {
            let username = client
                .connection
                .jid()
                .and_then(|jid| jid.node().map(|node| node.to_string()))
                .unwrap_or_else(|| "jabu".to_string());
            let microphone = storage
                .get_setting("calls.microphone")
                .ok()
                .flatten()
                .and_then(|value| value.as_str().map(str::to_string))
                .filter(|value| !value.is_empty());
            let request = xmpp_core::VoiceJoin {
                channel: xmpp_core::VoiceChannel {
                    space_server: space_id.server.clone(),
                    space_node: space_id.node.clone(),
                    room: room.conference.clone(),
                    name: room.name.clone(),
                },
                username,
                microphone,
                muted: false,
            };
            match client.voice.join(request).await {
                Ok(()) => error.set(None),
                Err(err) => error.set(Some(err.to_string())),
            }
            joining.set(false);
        });
    };

    let leave_client_header = client.clone();
    let on_leave_header = move |_| {
        let client = leave_client_header.clone();
        spawn(async move {
            let _ = client.voice.leave().await;
        });
    };
    let leave_client_bar = client.clone();
    let on_leave_bar = move |_| {
        let client = leave_client_bar.clone();
        spawn(async move {
            let _ = client.voice.leave().await;
        });
    };
    let mute_client = client.clone();
    let on_mute = move |_| {
        let client = mute_client.clone();
        spawn(async move {
            let _ = client.voice.set_muted(!muted).await;
        });
    };
    let screen_client = client.clone();
    let on_screen = move |_| {
        let client = screen_client.clone();
        let mut error = error;
        spawn(async move {
            match client.voice.set_screen_sharing(!screen_sharing).await {
                Ok(()) => error.set(None),
                Err(err) => error.set(Some(err.to_string())),
            }
        });
    };

    let name = if room.name.is_empty() {
        room.conference.clone()
    } else {
        room.name.clone()
    };

    rsx! {
        main { class: "flex min-w-0 flex-1 flex-col bg-gray-950",
            div { class: "flex h-14 shrink-0 items-center gap-3 border-b border-gray-800 px-4",
                div { class: "flex h-9 w-9 items-center justify-center rounded-xl bg-teal-500/15 text-teal-300",
                    Icon { name: IconName::Volume, class: "h-4 w-4" }
                }
                div { class: "min-w-0 flex-1",
                    h2 { class: "truncate text-sm font-semibold text-gray-100", "{name}" }
                    p { class: "truncate text-[11px] text-gray-500",
                        if joined {
                            "Voice connected · {participants.len()} in channel"
                        } else {
                            "Voice channel"
                        }
                    }
                }
                if joined {
                    button {
                        class: "inline-flex items-center gap-1 rounded-lg bg-red-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-red-500",
                        onclick: on_leave_header,
                        Icon { name: IconName::PhoneOff, class: "h-3.5 w-3.5" }
                        "Leave"
                    }
                }
            }
            div { class: "flex min-h-0 flex-1 flex-col items-center justify-center gap-4 p-6",
                if !joined {
                    div { class: "flex h-16 w-16 items-center justify-center rounded-2xl bg-gray-800 text-gray-400",
                        Icon { name: IconName::Volume, class: "h-7 w-7" }
                    }
                    div { class: "text-center",
                        h3 { class: "text-base font-semibold text-gray-200", "{name}" }
                        p { class: "mt-1 max-w-sm text-xs text-gray-500",
                            "Join the voice channel. Audio is relayed by the room's SFU."
                        }
                    }
                    button {
                        class: "btn-primary px-5 py-2",
                        disabled: *joining.read(),
                        onclick: on_join,
                        if *joining.read() { "Connecting…" } else { "Join voice" }
                    }
                } else {
                    if let Some(sid) = conference_sid.as_ref() {
                        ConferenceScreen { sid: sid.clone() }
                    }
                    div { class: "grid max-w-3xl grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4",
                        for participant in participants.iter() {
                            {
                                let participant = participant.clone();
                                let id = participant.id.clone();
                                let volume_client = client.clone();
                                let tile_class = if participant.speaking {
                                    "flex flex-col items-center gap-2 rounded-xl border border-green-400/70 bg-green-500/10 p-3 shadow-[0_0_0_2px_rgba(74,222,128,0.25)]"
                                } else {
                                    "flex flex-col items-center gap-2 rounded-xl border border-gray-800 bg-gray-900/60 p-3"
                                };
                                let avatar_class = if participant.speaking {
                                    "flex h-12 w-12 items-center justify-center rounded-full bg-green-500 text-sm font-semibold text-white"
                                } else {
                                    "flex h-12 w-12 items-center justify-center rounded-full bg-teal-500 text-sm font-semibold text-white"
                                };
                                let percent = (participant.volume * 100.0).round() as i32;
                                rsx! {
                                    div {
                                        key: "{id}",
                                        class: "{tile_class}",
                                        div { class: "{avatar_class}",
                                            "{participant.name.chars().next().unwrap_or('?').to_uppercase()}"
                                        }
                                        div { class: "flex items-center gap-1",
                                            span { class: "max-w-full truncate text-xs text-gray-200", "{participant.name}" }
                                            if participant.screen_sharing {
                                                Icon { name: IconName::Monitor, class: "h-3 w-3 text-teal-300" }
                                            }
                                            if participant.muted {
                                                Icon { name: IconName::MicOff, class: "h-3 w-3 text-red-400" }
                                            }
                                        }
                                        if !participant.is_self {
                                            div { class: "flex w-full items-center gap-1",
                                                Icon { name: IconName::Volume, class: "h-3 w-3 shrink-0 text-gray-500" }
                                                input {
                                                    r#type: "range",
                                                    min: "0",
                                                    max: "1.5",
                                                    step: "0.05",
                                                    value: "{participant.volume}",
                                                    class: "h-1 w-full accent-teal-400",
                                                    oninput: move |e| {
                                                        let volume = e.value().parse::<f64>().unwrap_or(1.0);
                                                        let client = volume_client.clone();
                                                        let id = id.clone();
                                                        spawn(async move {
                                                            let _ = client.voice.set_participant_volume(&id, volume).await;
                                                        });
                                                    },
                                                }
                                                span { class: "w-8 text-right text-[10px] text-gray-500", "{percent}%" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "flex items-center gap-2 rounded-xl border border-gray-800 bg-gray-900/60 p-2",
                        button {
                            class: if muted { "inline-flex items-center gap-1 rounded-lg bg-red-600/20 px-3 py-2 text-xs text-red-300 hover:bg-red-600/30" } else { "inline-flex items-center gap-1 rounded-lg bg-gray-800 px-3 py-2 text-xs text-gray-200 hover:bg-gray-700" },
                            onclick: on_mute,
                            Icon { name: if muted { IconName::MicOff } else { IconName::Mic }, class: "h-4 w-4" }
                            if muted { "Unmute" } else { "Mute" }
                        }
                        button {
                            class: if screen_sharing { "inline-flex items-center gap-1 rounded-lg bg-teal-600 px-3 py-2 text-xs text-white hover:bg-teal-500" } else { "inline-flex items-center gap-1 rounded-lg bg-gray-800 px-3 py-2 text-xs text-gray-200 hover:bg-gray-700" },
                            onclick: on_screen,
                            Icon { name: IconName::Monitor, class: "h-4 w-4" }
                            if screen_sharing { "Stop sharing" } else { "Share screen" }
                        }
                        button {
                            class: "inline-flex items-center gap-1 rounded-lg bg-red-600 px-3 py-2 text-xs font-medium text-white hover:bg-red-500",
                            onclick: on_leave_bar,
                            Icon { name: IconName::PhoneOff, class: "h-4 w-4" }
                            "Leave"
                        }
                    }
                }
                if let Some(err) = error.read().as_ref() {
                    p { class: "max-w-md text-center text-xs text-red-400", "{err}" }
                }
            }
        }
    }
}

#[component]
fn RemoteCallVideo(sid: String, screen_expected: bool) -> Element {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let client = use_context::<XmppClient>();
    let mut screen = use_signal(|| Option::<String>::None);
    let mut camera = use_signal(|| Option::<String>::None);
    let mut camera_at = use_signal(std::time::Instant::now);
    let mut screen_at = use_signal(std::time::Instant::now);
    let mut screen_active = use_signal(|| screen_expected);

    use_future({
        let client = client.clone();
        let sid = sid.clone();
        move || {
            let client = client.clone();
            let sid = sid.clone();
            async move {
                let mut last_screen = u64::MAX;
                let mut last_camera = u64::MAX;
                loop {
                    if let Some(preview) = client.call_remote_screen(&sid) {
                        if preview.sequence != last_screen {
                            last_screen = preview.sequence;
                            screen.set(Some(format!(
                                "data:image/jpeg;base64,{}",
                                STANDARD.encode(preview.jpeg.as_slice())
                            )));
                            screen_at.set(std::time::Instant::now());
                            screen_active.set(true);
                        }
                    }
                    if let Some(preview) = client.call_remote_preview(&sid) {
                        if preview.sequence != last_camera {
                            last_camera = preview.sequence;
                            camera.set(Some(format!(
                                "data:image/jpeg;base64,{}",
                                STANDARD.encode(preview.jpeg.as_slice())
                            )));
                            camera_at.set(std::time::Instant::now());
                        }
                    }
                    if screen.read().is_some()
                        && screen_at.read().elapsed() >= std::time::Duration::from_millis(800)
                    {
                        screen.set(None);
                        screen_active.set(false);
                    }
                    if camera.read().is_some()
                        && camera_at.read().elapsed() >= std::time::Duration::from_millis(800)
                    {
                        camera.set(None);
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            }
        }
    });

    let fresh_screen = screen.read().is_some()
        && screen_at.read().elapsed() < std::time::Duration::from_millis(1500);

    rsx! {
        if fresh_screen {
            if let Some(src) = screen.read().as_ref() {
                img {
                    class: "max-h-[calc(100vh-10rem)] max-w-[min(90vw,960px)] rounded-xl border border-gray-700 object-contain shadow-panel",
                    src: "{src}",
                    alt: "Remote screen",
                }
            }
        } else if let Some(src) = camera.read().as_ref() {
            img {
                class: "max-h-[calc(100vh-10rem)] max-w-[min(90vw,960px)] rounded-xl border border-gray-700 object-contain shadow-panel",
                src: "{src}",
                alt: "Remote video",
            }
        } else {
            if *screen_active.read() {
                div { class: "rounded-xl border border-gray-800 bg-gray-950/80 px-4 py-3 text-xs text-gray-500 shadow-panel",
                    "Connecting to the other side's screen…"
                }
            }
        }
    }
}

#[component]
fn CallPreview(sid: String, #[props(default)] screen: bool) -> Element {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let client = use_context::<XmppClient>();
    let mut frame = use_signal(|| Option::<String>::None);

    use_future({
        let client = client.clone();
        let sid = sid.clone();
        move || {
            let client = client.clone();
            let sid = sid.clone();
            async move {
                loop {
                    let preview = if screen {
                        client.call_screen_preview(&sid)
                    } else {
                        client.call_preview(&sid)
                    };
                    if let Some(preview) = preview {
                        frame.set(Some(format!(
                            "data:image/jpeg;base64,{}",
                            STANDARD.encode(preview.jpeg.as_slice())
                        )));
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            }
        }
    });

    rsx! {
        div {
            class: if screen { "flex aspect-video w-48 items-center justify-center bg-gray-950" } else { "flex h-28 w-40 items-center justify-center bg-gray-950" },
            if let Some(src) = frame.read().as_ref() {
                img {
                    class: if screen { "h-full w-full object-contain" } else { "h-full w-full object-cover" },
                    src: "{src}",
                    alt: if screen { "Screen preview" } else { "Camera preview" },
                }
            } else {
                span { class: "text-[11px] text-gray-500",
                    if screen { "Starting screen share…" } else { "Starting camera…" }
                }
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
        let mut status_state = state.clone();
        status_state.last_error.set(None);
        status_state
            .connection_state
            .set(ConnectionState::Connecting);
        status_state.bump();
        spawn(async move {
            if let Err(e) = connect_client.connection.connect().await {
                let message = e.to_string();
                error.set(Some(message.clone()));
                status_state.last_error.set(Some(message.clone()));
                status_state
                    .connection_state
                    .set(ConnectionState::Failed { reason: message });
                status_state.bump();
                busy.set(false);
            }
        });
    };

    rsx! {
        div { class: "relative flex min-h-screen items-center justify-center overflow-hidden bg-gray-950 px-4 py-10 text-gray-100",
            div { class: "pointer-events-none absolute -left-40 -top-40 h-[30rem] w-[30rem] rounded-full bg-teal-500/10 blur-3xl" }
            div { class: "pointer-events-none absolute -bottom-40 -right-32 h-[26rem] w-[26rem] rounded-full bg-sky-500/10 blur-3xl" }

            div { class: "relative grid w-full max-w-5xl gap-10 md:grid-cols-[1.05fr_0.95fr] md:items-center",
                section { class: "hidden md:block",
                    div { class: "mb-8 flex items-center gap-3",
                        div { class: "flex h-12 w-12 items-center justify-center rounded-2xl bg-teal-500 text-xl font-black text-white shadow-lg shadow-teal-500/20",
                            "M"
                        }
                        div {
                            p { class: "text-sm font-semibold tracking-tight", "Jabu" }
                            p { class: "text-xs text-gray-500", "A native XMPP client" }
                        }
                    }
                    h1 { class: "max-w-md text-4xl font-semibold leading-tight tracking-tight text-gray-100",
                        "Your conversations, without the noise."
                    }
                    p { class: "mt-4 max-w-md text-sm leading-6 text-gray-400",
                        "A fast, private desktop client for XMPP. Chat, join rooms, share files and keep your history on your own device."
                    }
                    div { class: "mt-8 grid gap-3 text-sm text-gray-300",
                        div { class: "flex items-center gap-3",
                            span { class: "h-1.5 w-1.5 rounded-full bg-teal-400" }
                            "Direct messages, rooms and archives"
                        }
                        div { class: "flex items-center gap-3",
                            span { class: "h-1.5 w-1.5 rounded-full bg-teal-400" }
                            "Encrypted 1:1 chats with OMEMO"
                        }
                        div { class: "flex items-center gap-3",
                            span { class: "h-1.5 w-1.5 rounded-full bg-teal-400" }
                            "Local storage you control"
                        }
                    }
                }

                div { class: "panel p-6 sm:p-8",
                    div { class: "mb-6",
                        p { class: "eyebrow", "Welcome back" }
                        h2 { class: "mt-1 text-2xl font-semibold tracking-tight", "Sign in" }
                        p { class: "mt-1 text-sm text-gray-500", "Use your XMPP address and password." }
                    }

                    div { class: "space-y-4",
                        label { class: "block",
                            span { class: "eyebrow", "JID" }
                            input {
                                class: "field mt-1.5",
                                placeholder: "you@example.com",
                                autocomplete: "username",
                                value: "{jid}",
                                oninput: move |e| jid.set(e.value()),
                            }
                        }
                        label { class: "block",
                            span { class: "eyebrow", "Password" }
                            input {
                                class: "field mt-1.5",
                                r#type: "password",
                                autocomplete: "current-password",
                                placeholder: "Your password",
                                value: "{password}",
                                oninput: move |e| password.set(e.value()),
                            }
                        }
                        label { class: "block",
                            span { class: "eyebrow", "Resource" }
                            input {
                                class: "field mt-1.5",
                                placeholder: "jabu",
                                value: "{resource}",
                                oninput: move |e| resource.set(e.value()),
                            }
                        }
                        label { class: "flex cursor-pointer items-center gap-2 text-xs text-gray-400",
                            input {
                                class: "h-4 w-4 rounded border-gray-600 bg-gray-900 accent-teal-500",
                                r#type: "checkbox",
                                checked: *remember.read(),
                                onchange: move |e| remember.set(e.checked()),
                            }
                            "Remember this account"
                        }
                    }

                    if let Some(err) = error.read().as_ref() {
                        div { class: "mt-4 rounded-xl border border-red-500/20 bg-red-500/10 px-3 py-2 text-xs text-red-300",
                            "{err}"
                        }
                    }

                    button {
                        class: "btn-primary mt-6 w-full py-2.5",
                        disabled: *busy.read(),
                        onclick: submit,
                        if *busy.read() { "Connecting…" } else { "Sign in" }
                    }
                    p { class: "mt-4 text-center text-[11px] text-gray-600",
                        "Your password is only sent to your XMPP server."
                    }
                }
            }
        }
    }
}

/// Slim desktop titlebar with the drag region and window controls.
#[component]
fn DesktopTitlebar() -> Element {
    let client_state = use_context::<ClientState>();
    let state = (*client_state.connection_state.read()).clone();

    rsx! {
        header { class: "flex h-8 shrink-0 items-center justify-between border-b border-gray-800 bg-gray-900/80 px-2",
            div {
                class: "flex min-w-0 items-center gap-2",
                onmousedown: move |_| {
                    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
                    dioxus::desktop::window().drag();
                },
                div { class: "flex h-5 w-5 items-center justify-center rounded-md bg-teal-500 text-[10px] font-black text-white",
                    "M"
                }
                span { class: "text-xs font-semibold tracking-tight text-gray-100", "Jabu" }
                div { class: "mx-1 h-3 w-px bg-gray-800" }
                ConnectionIndicator { state: state.clone() }
                span { class: "hidden truncate text-[11px] text-gray-500 sm:block", "{format_connection_state(&state)}" }
            }
            WindowControls {}
        }
    }
}

#[component]
fn WindowControls() -> Element {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        let window = dioxus::desktop::window();
        let minimize = window.clone();
        let maximize = window.clone();
        let close = window;
        rsx! {
            div { class: "ml-1 flex items-center gap-0.5 border-l border-gray-800 pl-2",
                button {
                    class: "flex h-7 w-7 items-center justify-center rounded-lg text-gray-500 transition-colors hover:bg-gray-800 hover:text-gray-200",
                    title: "Minimize",
                    onclick: move |_| minimize.set_minimized(true),
                    Icon { name: IconName::Minus, class: "h-4 w-4" }
                }
                button {
                    class: "flex h-7 w-7 items-center justify-center rounded-lg text-gray-500 transition-colors hover:bg-gray-800 hover:text-gray-200",
                    title: "Maximize",
                    onclick: move |_| maximize.toggle_maximized(),
                    Icon { name: IconName::Square, class: "h-4 w-4" }
                }
                button {
                    class: "flex h-7 w-7 items-center justify-center rounded-lg text-gray-500 transition-colors hover:bg-red-600 hover:text-white",
                    title: "Close",
                    onclick: move |_| close.close(),
                    Icon { name: IconName::X, class: "h-4 w-4" }
                }
            }
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    rsx! {}
}

#[component]
fn ConnectionBanner() -> Element {
    let client = use_context::<XmppClient>();
    let client_state = use_context::<ClientState>();
    let state = (*client_state.connection_state.read()).clone();
    if state.is_connected() {
        return rsx! {};
    }
    let (tone, title, detail) = match &state {
        ConnectionState::Connecting | ConnectionState::Reconnecting { .. } => (
            "border-amber-500/20 bg-amber-500/10 text-amber-200",
            "Connecting to server…",
            "Waiting for the XMPP connection and authentication.",
        ),
        ConnectionState::Failed { reason } => (
            "border-red-500/20 bg-red-500/10 text-red-200",
            "Could not connect",
            reason.as_str(),
        ),
        _ => (
            "border-red-500/20 bg-red-500/10 text-red-200",
            "Disconnected",
            "You are not connected to the XMPP server.",
        ),
    };
    let cs = client_state.clone();
    let retry_client = client.clone();
    let retry = move |_| {
        let client = retry_client.clone();
        let mut cs = cs.clone();
        cs.last_error.set(None);
        cs.connection_state.set(ConnectionState::Connecting);
        cs.bump();
        spawn(async move {
            if let Err(e) = client.connection.connect().await {
                let message = e.to_string();
                cs.last_error.set(Some(message.clone()));
                cs.connection_state
                    .set(ConnectionState::Failed { reason: message });
                cs.bump();
            }
        });
    };

    rsx! {
        div { class: "flex shrink-0 items-center justify-between gap-3 border-b px-4 py-2 text-xs {tone}",
            div { class: "min-w-0",
                span { class: "font-semibold", "{title}" }
                span { class: "ml-2 truncate opacity-80", "{detail}" }
            }
            button {
                class: "shrink-0 rounded-lg border border-current/30 px-2.5 py-1 font-medium hover:bg-white/10",
                onclick: retry,
                "Retry"
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
    let storage = use_context::<Arc<dyn StorageBackend>>();
    let mut menu_open = use_signal(|| false);
    let mut show_settings = use_signal(|| false);
    let mut show_accounts = use_signal(|| false);
    let mut show_presence = use_signal(|| false);
    let mut show_profile = use_signal(|| false);
    let mut show_gateways = use_signal(|| false);
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
            let _ = client.voice.leave().await;
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
            button {
                class: "flex h-8 w-8 items-center justify-center rounded-lg text-gray-400 transition-colors hover:bg-gray-800 hover:text-gray-100",
                title: "Account menu",
                onclick: move |_| {
                    let next = !*menu_open.read();
                    menu_open.set(next);
                },
                Icon { name: IconName::Setting, class: "h-4 w-4" }
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
            if *show_profile.read() {
                ProfileEditor { onclose: move |_| show_profile.set(false) }
            }
            if *show_gateways.read() {
                GatewaySetup { onclose: move |_| show_gateways.set(false) }
            }
            div {
                class: if *menu_open.read() {
                    "absolute right-0 mt-2 w-48 rounded-lg border border-gray-700 bg-gray-800 shadow-lg"
                } else {
                    "absolute right-0 mt-2 w-48 rounded-lg border border-gray-700 bg-gray-800 opacity-0 invisible shadow-lg transition-all group-hover:visible group-hover:opacity-100"
                },
                div { class: "py-1",
                    button {
                        class: "block w-full text-left px-4 py-2 text-sm text-gray-300 hover:bg-gray-700",
                        onclick: move |_| {
                            show_accounts.set(true);
                            menu_open.set(false);
                        },
                        "Accounts"
                    }
                    button {
                        class: "block w-full text-left px-4 py-2 text-sm text-gray-300 hover:bg-gray-700",
                        onclick: move |_| {
                            show_presence.set(true);
                            menu_open.set(false);
                        },
                        "Presence"
                    }
                    button {
                        class: "block w-full text-left px-4 py-2 text-sm text-gray-300 hover:bg-gray-700",
                        onclick: move |_| {
                            show_profile.set(true);
                            menu_open.set(false);
                        },
                        "Edit profile"
                    }
                    button {
                        class: "block w-full text-left px-4 py-2 text-sm text-gray-300 hover:bg-gray-700",
                        onclick: move |_| {
                            show_gateways.set(true);
                            menu_open.set(false);
                        },
                        "Gateways"
                    }
                    button {
                        class: "block w-full text-left px-4 py-2 text-sm text-gray-300 hover:bg-gray-700",
                        onclick: move |_| {
                            show_settings.set(true);
                            menu_open.set(false);
                        },
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
                        class: "text-gray-400 hover:text-gray-100",
                        title: "Close",
                        onclick: move |_| onclose.call(()),
                        Icon { name: IconName::X, class: "h-4 w-4" }
                    }
                }
                label { class: "block text-xs text-gray-400",
                    "Status"
                    select {
                        class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-gray-100",
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
                        class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-gray-100",
                        placeholder: "What are you up to?",
                        value: "{status}",
                        oninput: move |e| status.set(e.value()),
                    }
                }
                label { class: "block text-xs text-gray-400",
                    "Priority"
                    input {
                        class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-gray-100",
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
    let error = use_signal(|| Option::<String>::None);
    let busy = use_signal(|| false);

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
                let _ = client.voice.leave().await;
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
                    let _ = client.voice.leave().await;
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
            class: "fixed inset-0 z-50 flex items-center justify-center overflow-y-auto bg-black/60 p-4",
            onclick: move |_| onclose.call(()),
            div {
                class: "max-h-[calc(100vh-2rem)] w-[28rem] max-w-[calc(100vw-2rem)] overflow-y-auto rounded-2xl border border-gray-800 bg-gray-900 p-4 shadow-panel space-y-3",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center justify-between",
                    h3 { class: "font-medium", "Accounts" }
                    button {
                        class: "text-gray-400 hover:text-gray-100",
                        title: "Close",
                        onclick: move |_| onclose.call(()),
                        Icon { name: IconName::X, class: "h-4 w-4" }
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
                        let switch = switch.clone();
                        let remove = remove.clone();
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
    let client = use_context::<XmppClient>();
    let storage = use_context::<Arc<dyn StorageBackend>>();
    let mut notifications = client_state.notifications_enabled;
    let mut timestamps = client_state.show_timestamps;
    let mut theme = client_state.theme;
    let camera_devices = use_signal(|| client.call_camera_devices());
    let mut camera = use_signal(|| {
        storage
            .get_setting("calls.camera")
            .ok()
            .flatten()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_default()
    });
    let audio_devices = use_signal(|| client.call_audio_devices());
    let mut microphone = use_signal(|| {
        storage
            .get_setting("calls.microphone")
            .ok()
            .flatten()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_default()
    });
    let camera_storage = storage.clone();
    let camera_client = client.clone();
    let microphone_storage = storage.clone();
    let microphone_client = client.clone();
    // The SFU component of this deployment. Empty means "only the SFU already
    // in the room", which is what a room invited by Movim looks like.
    let mut sfu = use_signal(|| {
        storage
            .get_setting("calls.sfu")
            .ok()
            .flatten()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_default()
    });
    let sfu_storage = storage.clone();

    let theme_storage = storage.clone();
    let change_theme = move |e: FormEvent| {
        let value = e.value();
        theme.set(Theme::from_slug(&value));
        let _ = theme_storage.set_setting("ui.theme", &serde_json::Value::String(value));
    };

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
                class: "max-h-[calc(100vh-2rem)] w-[28rem] max-w-[calc(100vw-2rem)] overflow-y-auto rounded-lg bg-gray-800 p-4 space-y-3",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center justify-between",
                    h3 { class: "font-medium", "Settings" }
                    button {
                        class: "text-gray-400 hover:text-gray-100",
                        title: "Close",
                        onclick: move |_| onclose.call(()),
                        Icon { name: IconName::X, class: "h-4 w-4" }
                    }
                }
                label { class: "flex items-center justify-between text-sm text-gray-300",
                    "Theme"
                    select {
                        class: "rounded bg-gray-900 px-2 py-1 text-sm text-gray-100",
                        value: "{theme.read().slug()}",
                        onchange: change_theme,
                        for (value, label) in Theme::ALL {
                            option { value: "{value.slug()}", "{label}" }
                        }
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
                if !audio_devices.read().is_empty() {
                    label { class: "flex items-center justify-between gap-3 text-sm text-gray-300",
                        "Microphone"
                        select {
                            class: "max-w-56 rounded bg-gray-900 px-2 py-1 text-sm text-gray-100",
                            value: "{microphone.read()}",
                            onchange: move |e| {
                                let value = e.value();
                                microphone.set(value.clone());
                                let _ = microphone_storage.set_setting(
                                    "calls.microphone",
                                    &serde_json::json!(value),
                                );
                                let client = microphone_client.clone();
                                spawn(async move {
                                    let selected = (!value.is_empty()).then_some(value.as_str());
                                    let _ = client.set_default_call_audio(selected).await;
                                });
                            },
                            option { value: "", "Automatic" }
                            for device in audio_devices.read().iter() {
                                option { value: "{device.id}", "{device.name}" }
                            }
                        }
                    }
                }
                if !camera_devices.read().is_empty() {
                    label { class: "flex items-center justify-between gap-3 text-sm text-gray-300",
                        "Camera"
                        select {
                            class: "max-w-56 rounded bg-gray-900 px-2 py-1 text-sm text-gray-100",
                            value: "{camera.read()}",
                            onchange: move |e| {
                                let value = e.value();
                                camera.set(value.clone());
                                let _ = camera_storage.set_setting(
                                    "calls.camera",
                                    &serde_json::json!(value),
                                );
                                let client = camera_client.clone();
                                spawn(async move {
                                    let selected = (!value.is_empty()).then_some(value.as_str());
                                    let _ = client.set_default_call_camera(selected).await;
                                });
                            },
                            option { value: "", "Automatic" }
                            for device in camera_devices.read().iter() {
                                option { value: "{device.id}", "{device.name}" }
                            }
                        }
                    }
                }
                label { class: "flex items-center justify-between gap-3 text-sm text-gray-300",
                    "Call SFU"
                    input {
                        class: "max-w-56 rounded bg-gray-900 px-2 py-1 text-sm text-gray-100",
                        placeholder: "sfu.example.org",
                        value: "{sfu.read()}",
                        onchange: move |e| {
                            let value = e.value().trim().to_string();
                            sfu.set(value.clone());
                            let _ = sfu_storage.set_setting("calls.sfu", &serde_json::json!(value));
                        },
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

#[component]
fn SpaceRail() -> Element {
    let client_state = use_context::<ClientState>();
    let client = use_context::<XmppClient>();
    let _rev = *client_state.revision.read();
    let spaces = client_state.spaces.read().clone();
    let selected_space = client_state.selected_space.read().clone();
    let section = *client_state.section.read();
    let unread = client
        .chats
        .list()
        .iter()
        .map(xmpp_core::Chat::unread_count)
        .sum::<usize>()
        + client
            .mucs
            .list()
            .iter()
            .map(xmpp_core::muc::MucRoom::unread_count)
            .sum::<usize>();
    let mut show_add = use_signal(|| false);
    let mut search_state = client_state.clone();

    rsx! {
        aside { class: "flex w-16 shrink-0 flex-col items-center border-r border-gray-800 bg-gray-950 py-2 md:w-[72px]",
            div { class: "flex min-h-0 flex-1 flex-col items-center gap-1 overflow-y-auto px-1",
                RailButton {
                    label: "Chats",
                    icon: IconName::Message,
                    active: selected_space.is_none() && section == AppSection::Home,
                    badge: unread,
                    onclick: { let mut cs = client_state.clone(); move |_| activate_section(&mut cs, AppSection::Home) },
                }
                RailButton {
                    label: "Contacts",
                    icon: IconName::User,
                    active: selected_space.is_none() && section == AppSection::Contacts,
                    onclick: { let mut cs = client_state.clone(); move |_| activate_section(&mut cs, AppSection::Contacts) },
                }
                RailButton {
                    label: "Rooms",
                    icon: IconName::Hash,
                    active: selected_space.is_none() && section == AppSection::Rooms,
                    onclick: { let mut cs = client_state.clone(); move |_| activate_section(&mut cs, AppSection::Rooms) },
                }
                RailButton {
                    label: "Feed",
                    icon: IconName::Globe,
                    active: selected_space.is_none() && section == AppSection::Feed,
                    onclick: { let mut cs = client_state.clone(); move |_| activate_section(&mut cs, AppSection::Feed) },
                }
                RailButton {
                    label: "Calls",
                    icon: IconName::Phone,
                    active: selected_space.is_none() && section == AppSection::Calls,
                    onclick: { let mut cs = client_state.clone(); move |_| activate_section(&mut cs, AppSection::Calls) },
                }
                div { class: "my-1 h-px w-8 shrink-0 bg-gray-800" }
                for space in spaces {
                    {
                        let mut space = space;
                        space.unread = space
                            .rooms
                            .iter()
                            .filter_map(|room| {
                                client
                                    .mucs
                                    .get_by_bare(&room.conference)
                                    .map(|room| room.unread_count())
                            })
                            .sum();
                        let id = space.id.clone();
                        let first_room = space.first_room().map(|room| room.conference.clone());
                        let active = selected_space.as_ref().is_some_and(|current| current == &id);
                        let mut cs = client_state.clone();
                        let onclick = move |_| {
                            cs.selected_space.set(Some(id.clone()));
                            cs.section.set(AppSection::Home);
                            if let Some(room) = first_room.clone() {
                                cs.selected_chat.set(Some(room));
                            }
                            cs.bump();
                        };
                        rsx! {
                            SpaceRailItem { space: space.clone(), active, onclick }
                        }
                    }
                }
                button {
                    class: "flex h-11 w-11 shrink-0 items-center justify-center rounded-2xl border border-dashed border-gray-700 text-gray-500 transition-colors hover:border-teal-500 hover:text-teal-400",
                    title: "Add a space",
                    onclick: move |_| show_add.set(true),
                    Icon { name: IconName::Plus, class: "h-5 w-5" }
                }
            }
            div { class: "flex flex-col items-center gap-1 border-t border-gray-800 px-1 pt-2",
                button {
                    class: "flex h-10 w-10 items-center justify-center rounded-2xl text-gray-500 hover:bg-gray-800 hover:text-gray-100",
                    title: "Search messages, contacts and rooms",
                    onclick: move |_| {
                        search_state.search_open.set(true);
                        search_state.bump();
                    },
                    Icon { name: IconName::Search, class: "h-5 w-5" }
                }
            }
        }
        if *show_add.read() {
            AddSpaceDialog { onclose: move |_| show_add.set(false) }
        }
    }
}

fn activate_section(state: &mut ClientState, section: AppSection) {
    state.section.set(section);
    state.selected_space.set(None);
    state.bump();
}

#[component]
fn RailButton(
    label: &'static str,
    icon: IconName,
    active: bool,
    #[props(default = 0)] badge: usize,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let class = if active {
        "relative flex h-11 w-11 shrink-0 items-center justify-center rounded-2xl bg-teal-500 text-white shadow-lg shadow-teal-500/20"
    } else {
        "relative flex h-11 w-11 shrink-0 items-center justify-center rounded-2xl bg-gray-900 text-gray-400 transition-colors hover:bg-gray-800 hover:text-gray-100"
    };
    rsx! {
        button { class: "{class}", title: "{label}", "aria-label": "{label}", onclick: move |e| onclick.call(e),
            Icon { name: icon, class: "h-5 w-5" }
            if badge > 0 {
                span { class: "absolute -right-1 -top-1 flex h-4 min-w-4 items-center justify-center rounded-md bg-red-500 px-1 text-[10px] font-bold text-white",
                    "{badge}"
                }
            }
        }
    }
}

#[component]
fn SpaceRailItem(space: Space, active: bool, onclick: EventHandler<MouseEvent>) -> Element {
    let name = space.name.clone();
    let initial = initial_of(&name);
    let avatar = space.avatar.clone();
    let unread = space.unread;
    let locked = space.locked;
    let pending = space.pending;
    let class = if active {
        "relative flex h-11 w-11 shrink-0 items-center justify-center overflow-hidden rounded-2xl bg-teal-500 text-sm font-bold text-white shadow-lg shadow-teal-500/20"
    } else {
        "relative flex h-11 w-11 shrink-0 items-center justify-center overflow-hidden rounded-2xl bg-gray-900 text-sm font-bold text-gray-300 transition-colors hover:bg-gray-800 hover:text-gray-100"
    };
    rsx! {
        button { class: "{class}", title: "{name}", onclick: move |e| onclick.call(e),
            if let Some(src) = avatar {
                img { class: "h-full w-full object-cover", src: "{src}", alt: "{name}" }
            } else {
                "{initial}"
            }
            if unread > 0 {
                span { class: "absolute -right-1 -top-1 flex h-4 min-w-4 items-center justify-center rounded-md bg-red-500 px-1 text-[10px] font-bold text-white",
                    "{unread}"
                }
            }
            if locked {
                span { class: "absolute bottom-0 left-0 flex h-4 w-4 items-center justify-center rounded-tr-md bg-gray-950/80 text-gray-300",
                    Icon { name: IconName::Lock, class: "h-2.5 w-2.5" }
                }
            }
            if pending {
                span { class: "absolute bottom-0 right-0 h-2 w-2 rounded-tl-md bg-amber-400" }
            }
        }
    }
}

#[component]
fn ContextSidebar() -> Element {
    let client_state = use_context::<ClientState>();
    let _rev = *client_state.revision.read();
    let section = *client_state.section.read();
    let selected_space = client_state.selected_space.read().clone();
    let spaces = client_state.spaces.read().clone();
    let selected_chat = client_state.selected_chat.read().clone();
    let profile_jid = client_state.selected_profile.read().clone();
    let mut profile_state = client_state.clone();
    let space = selected_space
        .as_ref()
        .and_then(|id| spaces.iter().find(|space| &space.id == id).cloned());
    let mobile_class = if selected_chat.is_some() {
        "hidden md:flex"
    } else {
        "flex"
    };

    rsx! {
        aside { class: "w-full shrink-0 flex-col border-r border-gray-800 bg-gray-900/70 md:flex md:w-[270px] {mobile_class}",
            if let Some(space) = space {
                SpaceSidebar { space }
            } else {
                div { class: "flex h-14 shrink-0 items-center justify-between border-b border-gray-800 px-3",
                    div {
                        h2 { class: "text-sm font-semibold text-gray-100", "{section_title(section)}" }
                        p { class: "text-[11px] text-gray-500", "{section_subtitle(section)}" }
                    }
                    button {
                        class: "flex h-8 w-8 items-center justify-center rounded-lg text-gray-400 hover:bg-gray-800 hover:text-gray-100",
                        title: "Search",
                        onclick: {
                            let mut cs = client_state.clone();
                            move |_| {
                                cs.search_open.set(true);
                                cs.bump();
                            }
                        },
                        Icon { name: IconName::Search, class: "h-4 w-4" }
                    }
                }
                div { class: "min-h-0 flex-1 overflow-y-auto",
                    match section {
                        AppSection::Home => rsx! { ChatList {} },
                        AppSection::Contacts => rsx! { ContactsList {} },
                        AppSection::Rooms => rsx! { RoomsList {} },
                        AppSection::Calls => rsx! { CallsList {} },
                        AppSection::Feed => rsx! {
                            div { class: "p-6 text-center text-sm text-gray-500",
                                "Your social feed is open in the main area."
                            }
                        },
                    }
                }
                UserArea {}
            }
            if let Some(profile_jid) = profile_jid {
                ContactProfile {
                    jid: profile_jid,
                    onclose: move |_| {
                        profile_state.show_profile(None);
                        profile_state.bump();
                    },
                }
            }
        }
    }
}

fn section_title(section: AppSection) -> &'static str {
    match section {
        AppSection::Home => "Chats",
        AppSection::Contacts => "Contacts",
        AppSection::Rooms => "Rooms",
        AppSection::Calls => "Calls",
        AppSection::Feed => "Feed",
    }
}

fn section_subtitle(section: AppSection) -> &'static str {
    match section {
        AppSection::Home => "Direct messages and rooms",
        AppSection::Contacts => "People on your roster",
        AppSection::Rooms => "Discover and manage rooms",
        AppSection::Calls => "Recent conversations",
        AppSection::Feed => "Communities and posts",
    }
}

#[component]
fn SpaceSidebar(space: Space) -> Element {
    let client = use_context::<XmppClient>();
    let client_state = use_context::<ClientState>();
    let name = space.name.clone();
    let initial = initial_of(&name);
    let avatar = space.avatar.clone();
    let description = space.description.clone();
    let members = space.members;
    let notify = space.notify;
    let rooms = space.rooms.clone();
    let directories = space.directories.clone();
    let space_id = space.id.clone();
    let backend_id =
        xmpp_core::spaces::SpaceId::new(space.id.server.clone(), space.id.node.clone());
    let mut show_info = use_signal(|| false);
    let mut show_add_room = use_signal(|| false);
    let info_space = space.clone();
    let mut pin_state = client_state.clone();
    let pin_id = space_id.clone();
    let pin_client = client.clone();
    let pin_backend_id = backend_id.clone();
    let notify_backend_id = backend_id.clone();
    let info_backend_id = backend_id.clone();
    let room_backend_id = backend_id.clone();
    let toggle_pin = move |_| {
        let next = !space.pinned;
        pin_client.set_space_pinned(&pin_backend_id, next);
        let mut spaces = pin_state.spaces.write();
        if let Some(space) = spaces.iter_mut().find(|space| space.id == pin_id) {
            space.pinned = !space.pinned;
        }
        drop(spaces);
        pin_state.bump();
    };
    let mut notify_state = client_state.clone();
    let notify_id = space_id.clone();
    let mut ordered: Vec<(Option<SpaceDirectory>, Vec<SpaceRoom>)> = Vec::new();
    if !directories.is_empty() {
        let mut directories = directories;
        directories.sort_by_key(|directory| directory.order);
        for directory in &directories {
            let grouped: Vec<SpaceRoom> = rooms
                .iter()
                .filter(|room| room.directory_id.as_deref() == Some(directory.id.as_str()))
                .cloned()
                .collect();
            ordered.push((Some(directory.clone()), grouped));
        }
        let ungrouped: Vec<SpaceRoom> = rooms
            .iter()
            .filter(|room| {
                room.directory_id
                    .as_ref()
                    .is_none_or(|id| directories.iter().all(|directory| &directory.id != id))
            })
            .cloned()
            .collect();
        if !ungrouped.is_empty() {
            ordered.push((None, ungrouped));
        }
    } else if !rooms.is_empty() {
        ordered.push((None, rooms));
    }

    rsx! {
        div { class: "flex h-full flex-col",
            div { class: "flex h-14 shrink-0 items-center gap-2 border-b border-gray-800 px-3",
                div { class: "flex h-9 w-9 items-center justify-center overflow-hidden rounded-xl bg-teal-500 text-sm font-bold text-white",
                    if let Some(src) = avatar {
                        img { class: "h-full w-full object-cover", src: "{src}", alt: "{name}" }
                    } else {
                        "{initial}"
                    }
                }
                div { class: "min-w-0 flex-1",
                    h2 { class: "truncate text-sm font-semibold text-gray-100", "{name}" }
                    p { class: "truncate text-[11px] text-gray-500", "{description}" }
                }
                button {
                    class: if space.pinned { "flex h-7 w-7 items-center justify-center rounded-lg text-teal-400 hover:bg-gray-800" } else { "flex h-7 w-7 items-center justify-center rounded-lg text-gray-500 hover:bg-gray-800 hover:text-gray-200" },
                    title: if space.pinned { "Unpin space" } else { "Pin space" },
                    onclick: toggle_pin,
                    Icon { name: IconName::Pin, class: "h-3.5 w-3.5" }
                }
                button {
                    class: "flex h-7 w-7 items-center justify-center rounded-lg text-gray-500 hover:bg-gray-800 hover:text-gray-200",
                    title: "Space information",
                    onclick: move |_| show_info.set(true),
                    Icon { name: IconName::Setting, class: "h-3.5 w-3.5" }
                }
            }
            div { class: "flex items-center gap-1 border-b border-gray-800 px-2 py-2",
                span { class: "rounded bg-gray-800 px-2 py-1 text-[10px] text-gray-400", "{members} members" }
                select {
                    class: "ml-auto rounded bg-gray-800 px-2 py-1 text-[10px] text-gray-300",
                    value: "{notify.slug()}",
                    onchange: {
                        let client = client.clone();
                        let backend_id = notify_backend_id.clone();
                        move |e| {
                        let next = SpaceNotify::from_slug(&e.value());
                        client.set_space_notify(
                            &backend_id,
                            match next {
                                SpaceNotify::Always => xmpp_core::spaces::SpaceNotify::Always,
                                SpaceNotify::Mentions => xmpp_core::spaces::SpaceNotify::Mentions,
                                SpaceNotify::Never => xmpp_core::spaces::SpaceNotify::Never,
                            },
                        );
                        let mut spaces = notify_state.spaces.write();
                        if let Some(space) = spaces.iter_mut().find(|space| space.id == notify_id) {
                            space.notify = next;
                        }
                        drop(spaces);
                        notify_state.bump();
                        }
                    },
                    for option in SpaceNotify::ALL {
                        option { value: "{option.slug()}", "{option.label()}" }
                    }
                }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto p-2",
                if ordered.is_empty() {
                    div { class: "flex h-full flex-col items-center justify-center gap-2 px-4 text-center",
                        div { class: "flex h-12 w-12 items-center justify-center rounded-2xl bg-gray-800 text-gray-500",
                            Icon { name: IconName::Hash, class: "h-5 w-5" }
                        }
                        p { class: "text-sm font-medium text-gray-300", "No rooms yet" }
                        p { class: "text-xs text-gray-500", "Rooms added to this space will appear here." }
                    }
                } else {
                    for (directory, rooms) in ordered {
                        if let Some(directory) = directory {
                            div { class: "flex items-center justify-between px-1 pb-1 pt-2",
                                span { class: "text-[11px] font-semibold uppercase tracking-[0.12em] text-gray-500", "{directory.name}" }
                                span { class: "text-[10px] text-gray-600", "{rooms.len()}" }
                            }
                        }
                        for room in rooms {
                            {
                                let conference = room.conference.clone();
                                let conference_select = conference.clone();
                                let room_name = if room.name.is_empty() { conference.clone() } else { room.name.clone() };
                                let room_initial = initial_of(&room_name);
                                let room_avatar = room.avatar.clone();
                                let room_kind = room.kind;
                                let joined_here = client.voice.is_joined()
                                    && client
                                        .voice
                                        .channel()
                                        .is_some_and(|channel| channel.room == conference);
                                let room_unread = client
                                    .mucs
                                    .get_by_bare(&conference)
                                    .map(|room| room.unread_count())
                                    .unwrap_or(room.unread);
                                let active = client_state.selected_chat.read().as_deref() == Some(conference.as_str());
                                let client_room = client.clone();
                                let mut cs = client_state.clone();
                                let onclick = move |_| {
                                    if room_kind == SpaceRoomKind::Text
                                        && client_room
                                            .mucs
                                            .get_by_bare(&conference)
                                            .is_none_or(|room| room.needs_join())
                                    {
                                        let nick = client_room
                                            .connection
                                            .jid()
                                            .and_then(|jid| jid.node().map(|node| node.to_string()))
                                            .unwrap_or_else(|| "me".to_string());
                                        let known = client_room
                                            .mucs
                                            .get_by_bare(&conference)
                                            .is_some();
                                        let _ = client_room
                                            .ensure_room_joined_str(&conference, &nick);
                                        if !known {
                                            let _ = client_room
                                                .query_muc_mam_str(&conference, 50);
                                        }
                                    }
                                    cs.selected_chat.set(Some(conference_select.clone()));
                                    cs.bump();
                                };
                                let class = if active {
                                    "group flex w-full items-center gap-2 rounded-lg bg-gray-700 px-2 py-1.5 text-left"
                                } else {
                                    "group flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left hover:bg-gray-800"
                                };
                                rsx! {
                                    button { class: "{class}", onclick,
                                        div { class: if room_kind == SpaceRoomKind::Voice { "flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-gray-800 text-gray-400" } else { "flex h-8 w-8 shrink-0 items-center justify-center overflow-hidden rounded-lg bg-gray-800 text-xs font-semibold text-gray-300" },
                                            if room_kind == SpaceRoomKind::Voice {
                                                Icon { name: IconName::Volume, class: "h-4 w-4" }
                                            } else if let Some(src) = room_avatar {
                                                img { class: "h-full w-full object-cover", src: "{src}", alt: "{room_name}" }
                                            } else {
                                                "{room_initial}"
                                            }
                                        }
                                        div { class: "min-w-0 flex-1",
                                            div { class: "flex items-center gap-1",
                                                span { class: "truncate text-sm text-gray-200", "{room_name}" }
                                                if joined_here {
                                                    span { class: "text-green-400", title: "Connected",
                                                        Icon { name: IconName::Volume, class: "h-3 w-3" }
                                                    }
                                                } else if room.in_call {
                                                    span { class: "text-green-400", title: "Call in progress",
                                                        Icon { name: IconName::Phone, class: "h-3 w-3" }
                                                    }
                                                }
                                            }
                                        }
                                        if room_unread > 0 {
                                            span { class: "flex h-4 min-w-4 items-center justify-center rounded bg-red-500 px-1 text-[10px] font-bold text-white",
                                                "{room_unread}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "border-t border-gray-800 p-2",
                button {
                    class: "flex w-full items-center justify-center gap-1 rounded-lg bg-gray-800 px-2 py-1.5 text-xs font-medium text-gray-300 hover:bg-gray-700",
                    title: "Add a room to this space",
                    onclick: move |_| show_add_room.set(true),
                    Icon { name: IconName::Plus, class: "h-3.5 w-3.5" }
                    span { "Add room" }
                }
            }
        }
        if *show_info.read() {
            SpaceInfoDialog {
                space: info_space,
                space_id: info_backend_id.clone(),
                onclose: move |_| show_info.set(false),
            }
        }
        if *show_add_room.read() {
            AddSpaceRoomDialog {
                space_id: room_backend_id,
                onclose: move |_| show_add_room.set(false),
            }
        }
    }
}

#[component]
fn SpaceInfoDialog(
    space: Space,
    space_id: xmpp_core::spaces::SpaceId,
    onclose: EventHandler<()>,
) -> Element {
    let client = use_context::<XmppClient>();
    let mut show_invite = use_signal(|| false);
    let mut show_members = use_signal(|| false);
    let mut show_config = use_signal(|| false);
    let mut confirm_destroy = use_signal(|| false);
    let initial = initial_of(&space.name);
    let is_manager = space.affiliation.is_manager();
    let avatar = space.avatar.clone();
    let name = space.name.clone();
    let description = space.description.clone();
    let server = space.id.server.clone();
    let node = space.id.node.clone();
    let members = space.members;
    let notify = space.notify;

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4",
            onclick: move |_| onclose.call(()),
            div {
                class: "w-full max-w-md rounded-xl border border-gray-700 bg-gray-800 p-4 space-y-4",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center justify-between",
                    h3 { class: "text-sm font-semibold", "Space information" }
                    button {
                        class: "text-gray-400 hover:text-gray-100",
                        onclick: move |_| onclose.call(()),
                        Icon { name: IconName::X, class: "h-4 w-4" }
                    }
                }
                div { class: "flex items-center gap-3",
                    div { class: "flex h-12 w-12 items-center justify-center overflow-hidden rounded-2xl bg-teal-500 text-lg font-bold text-white",
                        if let Some(src) = avatar {
                            img { class: "h-full w-full object-cover", src: "{src}", alt: "{name}" }
                        } else {
                            "{initial}"
                        }
                    }
                    div { class: "min-w-0",
                        h4 { class: "truncate text-sm font-semibold text-gray-100", "{name}" }
                        p { class: "truncate text-xs text-gray-400", "{description}" }
                    }
                }
                div { class: "grid grid-cols-2 gap-2 text-xs",
                    div { class: "rounded-lg bg-gray-900 px-3 py-2",
                        p { class: "text-gray-500", "Members" }
                        p { class: "text-gray-200", "{members}" }
                    }
                    div { class: "rounded-lg bg-gray-900 px-3 py-2",
                        p { class: "text-gray-500", "Notifications" }
                        p { class: "text-gray-200", "{notify.label()}" }
                    }
                }
                div { class: "rounded-lg bg-gray-900 px-3 py-2 text-xs",
                    p { class: "text-gray-500", "Address" }
                    p { class: "truncate font-mono text-gray-300", "{server} / {node}" }
                }
                div { class: "grid grid-cols-2 gap-2",
                    if is_manager {
                        button {
                            class: "btn-secondary px-3 text-xs",
                            onclick: move |_| show_invite.set(true),
                            "Invite"
                        }
                    }
                    button {
                        class: "btn-secondary px-3 text-xs",
                        onclick: move |_| show_members.set(true),
                        "Members"
                    }
                    if is_manager {
                        button {
                            class: "btn-secondary px-3 text-xs",
                            onclick: move |_| show_config.set(true),
                            "Configure"
                        }
                    }
                    button {
                        class: "btn-danger px-3 text-xs",
                        onclick: {
                            let client = client.clone();
                            let space_id = space_id.clone();
                            let onclose = onclose;
                            move |_| {
                                let client = client.clone();
                                let space_id = space_id.clone();
                                spawn(async move {
                                    let _ = client.leave_space(&space_id).await;
                                    onclose.call(());
                                });
                            }
                        },
                        "Leave"
                    }
                    if is_manager {
                        button {
                            class: "btn-secondary px-3 text-xs text-red-300",
                            onclick: {
                                let client = client.clone();
                                let space_id = space_id.clone();
                                let onclose = onclose;
                                move |_| {
                                    if !*confirm_destroy.read() {
                                        confirm_destroy.set(true);
                                        return;
                                    }
                                    let client = client.clone();
                                    let space_id = space_id.clone();
                                    spawn(async move {
                                        let _ = client.destroy_space(&space_id).await;
                                        onclose.call(());
                                    });
                                }
                            },
                            if *confirm_destroy.read() { "Confirm destroy" } else { "Destroy" }
                        }
                    }
                }
                p { class: "text-center text-[11px] text-gray-500",
                    "Member management and configuration are not connected yet."
                }
            }
        }
        if *show_invite.read() {
            SpaceInviteDialog {
                space_id: space_id.clone(),
                onclose: move |_| show_invite.set(false),
            }
        }
        if *show_members.read() {
            SpaceMembersDialog {
                space_id: space_id.clone(),
                onclose: move |_| show_members.set(false),
            }
        }
        if *show_config.read() {
            SpaceConfigDialog {
                space_id: space_id.clone(),
                onclose: move |_| show_config.set(false),
            }
        }
    }
}

#[component]
fn SpaceInviteDialog(space_id: xmpp_core::spaces::SpaceId, onclose: EventHandler<()>) -> Element {
    let client = use_context::<XmppClient>();
    let mut jid = use_signal(String::new);
    let mut affiliation = use_signal(|| xmpp_core::spaces::SpaceAffiliation::Member);
    let error = use_signal(|| Option::<String>::None);

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4",
            onclick: move |_| onclose.call(()),
            div {
                class: "w-full max-w-md rounded-xl border border-gray-700 bg-gray-800 p-4 space-y-3",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center justify-between",
                    h3 { class: "text-sm font-semibold", "Invite to space" }
                    button {
                        class: "text-gray-400 hover:text-gray-100",
                        onclick: move |_| onclose.call(()),
                        Icon { name: IconName::X, class: "h-4 w-4" }
                    }
                }
                label { class: "block text-xs text-gray-400",
                    "JID"
                    input {
                        class: "field mt-1 text-xs",
                        placeholder: "user@example.org",
                        value: "{jid}",
                        oninput: move |e| jid.set(e.value()),
                    }
                }
                label { class: "block text-xs text-gray-400",
                    "Affiliation"
                    select {
                        class: "field mt-1 text-xs",
                        value: match *affiliation.read() {
                            xmpp_core::spaces::SpaceAffiliation::Admin => "admin",
                            xmpp_core::spaces::SpaceAffiliation::Owner => "owner",
                            _ => "member",
                        },
                        onchange: move |e| {
                            affiliation.set(match e.value().as_str() {
                                "admin" => xmpp_core::spaces::SpaceAffiliation::Admin,
                                "owner" => xmpp_core::spaces::SpaceAffiliation::Owner,
                                _ => xmpp_core::spaces::SpaceAffiliation::Member,
                            });
                        },
                        option { value: "member", "Member" }
                        option { value: "admin", "Admin" }
                        option { value: "owner", "Owner" }
                    }
                }
                if let Some(err) = error.read().as_ref() {
                    p { class: "text-xs text-red-400", "{err}" }
                }
                button {
                    class: "btn-primary w-full text-xs",
                    onclick: {
                        let client = client.clone();
                        let space_id = space_id.clone();
                        let onclose = onclose;
                        let mut error = error;
                        move |_| {
                            let target = jid.read().trim().to_string();
                            if target.parse::<xmpp_core::Jid>().is_err() {
                                error.set(Some("Enter a valid JID.".to_string()));
                                return;
                            }
                            let target_affiliation = *affiliation.read();
                            let client = client.clone();
                            let space_id = space_id.clone();
                            let mut error = error;
                            spawn(async move {
                                match client
                                    .invite_space_member(&space_id, &target, target_affiliation)
                                    .await
                                {
                                    Ok(()) => onclose.call(()),
                                    Err(err) => error.set(Some(err.to_string())),
                                }
                            });
                        }
                    },
                    "Send invitation"
                }
            }
        }
    }
}

#[component]
fn SpaceMembersDialog(space_id: xmpp_core::spaces::SpaceId, onclose: EventHandler<()>) -> Element {
    let client = use_context::<XmppClient>();
    let mut members = use_signal(Vec::<xmpp_core::spaces::SpaceMember>::new);
    let mut error = use_signal(|| Option::<String>::None);
    let loading = use_signal(|| true);

    use_future({
        let client = client.clone();
        let space_id = space_id.clone();
        move || {
            let client = client.clone();
            let space_id = space_id.clone();
            let mut loading = loading;
            async move {
                match client.fetch_space_members(&space_id).await {
                    Ok(list) => members.set(list),
                    Err(err) => error.set(Some(err.to_string())),
                }
                loading.set(false);
            }
        }
    });

    let refresh = {
        let client = client.clone();
        let space_id = space_id.clone();
        let members = members;
        let error = error;
        let loading = loading;
        move || {
            let client = client.clone();
            let space_id = space_id.clone();
            let mut members = members;
            let mut error = error;
            let mut loading = loading;
            loading.set(true);
            spawn(async move {
                match client.fetch_space_members(&space_id).await {
                    Ok(list) => members.set(list),
                    Err(err) => error.set(Some(err.to_string())),
                }
                loading.set(false);
            });
        }
    };

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4",
            onclick: move |_| onclose.call(()),
            div {
                class: "w-full max-w-lg max-h-[85vh] overflow-y-auto rounded-xl border border-gray-700 bg-gray-800 p-4 space-y-3",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center justify-between",
                    h3 { class: "text-sm font-semibold", "Space members" }
                    button {
                        class: "text-gray-400 hover:text-gray-100",
                        onclick: move |_| onclose.call(()),
                        Icon { name: IconName::X, class: "h-4 w-4" }
                    }
                }
                if let Some(err) = error.read().as_ref() {
                    p { class: "text-xs text-red-400", "{err}" }
                }
                if *loading.read() {
                    p { class: "text-xs text-gray-400", "Loading members…" }
                } else if members.read().is_empty() {
                    p { class: "text-xs text-gray-500", "No members returned by the service." }
                }
                for member in members.read().iter() {
                    {
                        let jid = member.jid.clone();
                        let affiliation = member.affiliation;
                        let subscription = member.subscription;
                        let is_pending = subscription == xmpp_core::spaces::SpaceSubscription::Pending;
                        let client_aff = client.clone();
                        let space_aff = space_id.clone();
                        let jid_aff = jid.clone();
                        let refresh_aff = refresh.clone();
                        let client_sub = client.clone();
                        let space_sub = space_id.clone();
                        let jid_sub = jid.clone();
                        let refresh_sub = refresh.clone();
                        rsx! {
                            div { class: "rounded-lg border border-gray-700 bg-gray-900 px-3 py-2 space-y-1",
                                div { class: "flex items-center justify-between gap-2",
                                    span { class: "truncate text-xs font-medium text-gray-200", "{jid}" }
                                    if is_pending {
                                        span { class: "rounded bg-amber-500/15 px-1.5 py-0.5 text-[10px] text-amber-300", "Pending" }
                                    }
                                }
                                div { class: "flex items-center gap-2 text-[11px] text-gray-500",
                                    span { "{affiliation:?}" }
                                    span { "{subscription:?}" }
                                }
                                if is_pending {
                                    div { class: "flex gap-2 pt-1",
                                        button {
                                            class: "rounded bg-teal-700 px-2 py-1 text-[11px] text-white hover:bg-teal-600",
                                            onclick: {
                                                let client = client_sub.clone();
                                                let space_id = space_sub.clone();
                                                let jid = jid_sub.clone();
                                                let refresh = refresh_sub.clone();
                                                move |_| {
                                                    let client = client.clone();
                                                    let space_id = space_id.clone();
                                                    let jid = jid.clone();
                                                    let refresh = refresh.clone();
                                                    spawn(async move {
                                                        let _ = client
                                                            .set_space_subscription(
                                                                &space_id,
                                                                &jid,
                                                                xmpp_core::spaces::SpaceSubscription::Subscribed,
                                                            )
                                                            .await;
                                                        refresh();
                                                    });
                                                }
                                            },
                                            "Approve"
                                        }
                                        button {
                                            class: "rounded bg-red-700 px-2 py-1 text-[11px] text-white hover:bg-red-600",
                                            onclick: {
                                                let client = client_sub.clone();
                                                let space_id = space_sub.clone();
                                                let jid = jid_sub.clone();
                                                let refresh = refresh_sub.clone();
                                                move |_| {
                                                    let client = client.clone();
                                                    let space_id = space_id.clone();
                                                    let jid = jid.clone();
                                                    let refresh = refresh.clone();
                                                    spawn(async move {
                                                        let _ = client
                                                            .set_space_subscription(
                                                                &space_id,
                                                                &jid,
                                                                xmpp_core::spaces::SpaceSubscription::None,
                                                            )
                                                            .await;
                                                        refresh();
                                                    });
                                                }
                                            },
                                            "Deny"
                                        }
                                    }
                                } else {
                                    div { class: "flex items-center gap-2 pt-1",
                                        select {
                                            class: "rounded bg-gray-800 px-2 py-1 text-[11px] text-gray-300",
                                            value: match affiliation {
                                                xmpp_core::spaces::SpaceAffiliation::Owner => "owner",
                                                xmpp_core::spaces::SpaceAffiliation::Admin => "admin",
                                                xmpp_core::spaces::SpaceAffiliation::Member => "member",
                                                xmpp_core::spaces::SpaceAffiliation::Outcast => "outcast",
                                                _ => "none",
                                            },
                                            onchange: {
                                                let client = client_aff.clone();
                                                let space_id = space_aff.clone();
                                                let jid = jid_aff.clone();
                                                let refresh = refresh_aff.clone();
                                                move |e| {
                                                    let next = match e.value().as_str() {
                                                        "owner" => xmpp_core::spaces::SpaceAffiliation::Owner,
                                                        "admin" => xmpp_core::spaces::SpaceAffiliation::Admin,
                                                        "member" => xmpp_core::spaces::SpaceAffiliation::Member,
                                                        "outcast" => xmpp_core::spaces::SpaceAffiliation::Outcast,
                                                        _ => xmpp_core::spaces::SpaceAffiliation::None,
                                                    };
                                                    let client = client.clone();
                                                    let space_id = space_id.clone();
                                                    let jid = jid.clone();
                                                    let refresh = refresh.clone();
                                                    spawn(async move {
                                                        let _ = client
                                                            .set_space_affiliations(
                                                                &space_id,
                                                                &[(&jid, next)],
                                                            )
                                                            .await;
                                                        refresh();
                                                    });
                                                }
                                            },
                                            option { value: "none", "No affiliation" }
                                            option { value: "member", "Member" }
                                            option { value: "admin", "Admin" }
                                            option { value: "owner", "Owner" }
                                            option { value: "outcast", "Outcast" }
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
fn SpaceConfigDialog(space_id: xmpp_core::spaces::SpaceId, onclose: EventHandler<()>) -> Element {
    let client = use_context::<XmppClient>();
    let mut form = use_signal(|| Option::<xmpp_core::DataForm>::None);
    let mut values = use_signal(HashMap::<String, String>::new);
    let mut error = use_signal(|| Option::<String>::None);
    let saving = use_signal(|| false);

    use_future({
        let client = client.clone();
        let space_id = space_id.clone();
        move || {
            let client = client.clone();
            let space_id = space_id.clone();
            async move {
                match client.space_config(&space_id).await {
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

    let save = {
        let client = client.clone();
        let space_id = space_id.clone();
        let onclose = onclose;
        move |_| {
            let client = client.clone();
            let space_id = space_id.clone();
            let mut saving = saving;
            let mut error = error;
            let values = values;
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
            spawn(async move {
                match client
                    .submit_space_config(&space_id, parsed.form.as_ref(), &pairs)
                    .await
                {
                    Ok(()) => onclose.call(()),
                    Err(err) => error.set(Some(err.to_string())),
                }
                saving.set(false);
            });
        }
    };

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4",
            onclick: move |_| onclose.call(()),
            div {
                class: "w-full max-w-lg max-h-[90vh] overflow-y-auto rounded-xl border border-gray-700 bg-gray-800 p-4 space-y-3",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center justify-between",
                    h3 { class: "text-sm font-semibold", "Space configuration" }
                    button {
                        class: "text-gray-400 hover:text-gray-100",
                        onclick: move |_| onclose.call(()),
                        Icon { name: IconName::X, class: "h-4 w-4" }
                    }
                }
                if let Some(err) = error.read().as_ref() {
                    p { class: "text-xs text-red-400", "{err}" }
                }
                if let Some(parsed) = form.read().as_ref() {
                    if let Some(title) = parsed.title.clone() {
                        p { class: "text-xs font-medium text-gray-300", "{title}" }
                    }
                    for field in parsed.fields.iter().filter(|field| !field.is_hidden()) {
                        {
                            let var = field.var.clone();
                            let label = field.label.clone();
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
                                            class: "field mt-1 text-xs",
                                            value: "{value}",
                                            onchange: {
                                                let var = var.clone();
                                                move |e| { values.write().insert(var.clone(), e.value()); }
                                            },
                                            option { value: "1", "Yes" }
                                            option { value: "0", "No" }
                                        }
                                    } else if !options.is_empty() {
                                        select {
                                            class: "field mt-1 text-xs",
                                            value: "{value}",
                                            onchange: {
                                                let var = var.clone();
                                                move |e| { values.write().insert(var.clone(), e.value()); }
                                            },
                                            for (option_label, option_value) in options {
                                                option { value: "{option_value}", "{option_label}" }
                                            }
                                        }
                                    } else {
                                        input {
                                            class: "field mt-1 text-xs",
                                            value: "{value}",
                                            oninput: {
                                                let var = var.clone();
                                                move |e| { values.write().insert(var.clone(), e.value()); }
                                            },
                                        }
                                    }
                                }
                            }
                        }
                    }
                    button {
                        class: "btn-primary w-full text-xs",
                        disabled: *saving.read(),
                        onclick: save,
                        "Save configuration"
                    }
                } else if error.read().is_none() {
                    p { class: "text-xs text-gray-400", "Loading configuration…" }
                }
            }
        }
    }
}

#[component]
fn AddSpaceRoomDialog(space_id: xmpp_core::spaces::SpaceId, onclose: EventHandler<()>) -> Element {
    let client = use_context::<XmppClient>();
    let mut conference = use_signal(String::new);
    let mut name = use_signal(String::new);
    let mut kind = use_signal(|| SpaceRoomKind::Text);
    let mut autojoin = use_signal(|| false);
    let error = use_signal(|| Option::<String>::None);

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4",
            onclick: move |_| onclose.call(()),
            div {
                class: "w-full max-w-md rounded-xl border border-gray-700 bg-gray-800 p-4 space-y-3",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center justify-between",
                    h3 { class: "text-sm font-semibold", "Add a room" }
                    button {
                        class: "text-gray-400 hover:text-gray-100",
                        onclick: move |_| onclose.call(()),
                        Icon { name: IconName::X, class: "h-4 w-4" }
                    }
                }
                label { class: "flex items-center justify-between gap-3 text-xs text-gray-300",
                    "Room type"
                    select {
                        class: "rounded bg-gray-900 px-2 py-1 text-xs text-gray-100",
                        value: match *kind.read() {
                            SpaceRoomKind::Text => "text",
                            SpaceRoomKind::Voice => "voice",
                        },
                        onchange: move |e| {
                            kind.set(if e.value() == "voice" {
                                SpaceRoomKind::Voice
                            } else {
                                SpaceRoomKind::Text
                            });
                        },
                        option { value: "text", "Text channel" }
                        option { value: "voice", "Voice channel" }
                    }
                }
                if *kind.read() == SpaceRoomKind::Text {
                    label { class: "block text-xs text-gray-400",
                        "Room address"
                        input {
                            class: "field mt-1 text-xs",
                            placeholder: "room@conference.example.com",
                            value: "{conference}",
                            oninput: move |e| conference.set(e.value()),
                        }
                    }
                }
                label { class: "block text-xs text-gray-400",
                    "Display name"
                    input {
                        class: "field mt-1 text-xs",
                        placeholder: if *kind.read() == SpaceRoomKind::Voice { "Voice channel name" } else { "Optional" },
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                    }
                }
                if *kind.read() == SpaceRoomKind::Text {
                    label { class: "flex items-center gap-2 text-xs text-gray-300",
                        input {
                            r#type: "checkbox",
                            checked: *autojoin.read(),
                            onchange: move |e| autojoin.set(e.checked()),
                        }
                        "Join automatically"
                    }
                }
                if let Some(err) = error.read().as_ref() {
                    p { class: "text-xs text-red-400", "{err}" }
                }
                button {
                    class: "btn-primary w-full text-xs",
                    onclick: {
                        let client = client.clone();
                        let space_id = space_id.clone();
                        let onclose = onclose;
                        let mut error = error;
                        move |_| {
                            let room_kind = *kind.read();
                            let display = name.read().trim().to_string();
                            let domain = client
                                .connection
                                .jid()
                                .map(|jid| jid.domain().to_string())
                                .unwrap_or_else(|| "localhost".to_string());
                            // Chatrooms live on the conference service, never on
                            // the account's own domain: `room@localhost` is a
                            // user, and a group message sent there reaches
                            // nobody.
                            let conference_service = if domain.starts_with("conference.") {
                                domain.clone()
                            } else {
                                format!("conference.{domain}")
                            };
                            let value = if room_kind == SpaceRoomKind::Voice {
                                if display.is_empty() {
                                    error.set(Some("Enter a channel name.".to_string()));
                                    return;
                                }
                                // Reuse a channel of the same name: two clients
                                // adding "vocal" must land in one room, not one
                                // room each.
                                if let Some(existing) = client.spaces.get(&space_id).and_then(|space| {
                                    space
                                        .rooms
                                        .iter()
                                        .find(|room| {
                                            room.kind == xmpp_core::spaces::SpaceRoomKind::Voice
                                                && room.name.eq_ignore_ascii_case(&display)
                                        })
                                        .map(|room| room.conference.clone())
                                }) {
                                    existing
                                } else {
                                    let slug: String = display
                                        .to_lowercase()
                                        .chars()
                                        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                                        .collect();
                                    format!("voice-{slug}-{}@{conference_service}", &uuid::Uuid::new_v4().simple().to_string()[..8])
                                }
                            } else {
                                let raw = conference.read().trim().to_string();
                                if raw.is_empty() {
                                    error.set(Some("Enter a room address.".to_string()));
                                    return;
                                }
                                let value = if raw.contains('@') {
                                    raw
                                } else {
                                    format!("{raw}@{conference_service}")
                                };
                                match value.parse::<xmpp_core::Jid>() {
                                    Ok(jid) if jid.domain().to_string() == domain => {
                                        error.set(Some(format!(
                                            "{value} is a user address; chatrooms are on {conference_service}."
                                        )));
                                        return;
                                    }
                                    Ok(_) => value,
                                    Err(_) => {
                                        error.set(Some("Enter a valid room address.".to_string()));
                                        return;
                                    }
                                }
                            };
                            let room = xmpp_core::spaces::SpaceRoom {
                                conference: value,
                                name: display,
                                kind: match room_kind {
                                    SpaceRoomKind::Text => xmpp_core::spaces::SpaceRoomKind::Text,
                                    SpaceRoomKind::Voice => xmpp_core::spaces::SpaceRoomKind::Voice,
                                },
                                autojoin: *autojoin.read(),
                                ..xmpp_core::spaces::SpaceRoom::default()
                            };
                            let client = client.clone();
                            let space_id = space_id.clone();
                            let mut error = error;
                            spawn(async move {
                                match client.save_space_room(&space_id, &room).await {
                                    Ok(()) => onclose.call(()),
                                    Err(err) => error.set(Some(err.to_string())),
                                }
                            });
                        }
                    },
                    "Add room"
                }
            }
        }
    }
}

#[component]
fn UserArea() -> Element {
    let client = use_context::<XmppClient>();
    let client_state = use_context::<ClientState>();
    let _rev = *client_state.revision.read();
    let own_bare = client
        .connection
        .jid()
        .map(|jid| jid.to_bare().to_string())
        .unwrap_or_default();
    let item = client.roster.get_by_bare(&own_bare);
    let name = item
        .as_ref()
        .map(|item| item.display_name())
        .unwrap_or_else(|| own_bare.clone());
    let status = item
        .as_ref()
        .map(|item| presence_text(item.best_show()))
        .unwrap_or("Offline");
    let avatar = item.as_ref().and_then(avatar_data_uri);
    let initial = initial_of(&name);

    rsx! {
        div { class: "flex shrink-0 items-center gap-2 border-t border-gray-800 bg-gray-950/40 p-2",
            div { class: "relative",
                div { class: "flex h-9 w-9 items-center justify-center overflow-hidden rounded-full bg-teal-500 text-xs font-bold text-white",
                    if let Some(src) = avatar {
                        img { class: "h-full w-full object-cover", src: "{src}", alt: "{name}" }
                    } else {
                        "{initial}"
                    }
                }
                span { class: "absolute -bottom-0.5 -right-0.5 h-3 w-3 rounded-full border-2 border-gray-900 bg-green-500" }
            }
            div { class: "min-w-0 flex-1",
                p { class: "truncate text-xs font-semibold text-gray-100", "{name}" }
                p { class: "truncate text-[11px] text-gray-500", "{status}" }
            }
            UserMenu {}
        }
    }
}

#[component]
fn AddSpaceDialog(onclose: EventHandler<()>) -> Element {
    let client = use_context::<XmppClient>();
    let storage = use_context::<Arc<dyn StorageBackend>>();
    let mut uri = use_signal(String::new);
    let mut name = use_signal(String::new);
    let mut service = use_signal(|| {
        storage
            .get_setting("spaces.service")
            .ok()
            .flatten()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_default()
    });
    let mut private = use_signal(|| false);
    let error = use_signal(|| Option::<String>::None);
    let service_storage = storage.clone();

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4",
            onclick: move |_| onclose.call(()),
            div {
                class: "w-full max-w-md rounded-xl border border-gray-700 bg-gray-800 p-4 space-y-4",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center justify-between",
                    h3 { class: "text-sm font-semibold", "Add a space" }
                    button {
                        class: "text-gray-400 hover:text-gray-100",
                        onclick: move |_| onclose.call(()),
                        Icon { name: IconName::X, class: "h-4 w-4" }
                    }
                }
                div { class: "space-y-2",
                    p { class: "text-xs text-gray-400", "Join an existing space" }
                    div { class: "flex gap-2",
                        input {
                            class: "field flex-1 text-xs",
                            placeholder: "xmpp:server.com?;node=key",
                            value: "{uri}",
                            oninput: move |e| uri.set(e.value()),
                        }
                        button {
                            class: "btn-primary px-3 text-xs",
                            onclick: {
                                let client = client.clone();
                                let mut error = error;
                                let onclose = onclose;
                                move |_| {
                                    let value = uri.read().trim().to_string();
                                    let Some(id) = xmpp_core::spaces::parse_space_uri(&value) else {
                                        error.set(Some("Enter a valid space invitation link.".to_string()));
                                        return;
                                    };
                                    let client = client.clone();
                                    let mut error = error;
                                    spawn(async move {
                                        match client.join_space(&id).await {
                                            Ok(()) => {
                                                let _ = client.fetch_space_info(&id).await;
                                                let _ = client.fetch_space_items(&id).await;
                                                onclose.call(());
                                            }
                                            Err(err) => error.set(Some(err.to_string())),
                                        }
                                    });
                                }
                            },
                            "Join"
                        }
                    }
                }
                div { class: "space-y-2 border-t border-gray-700 pt-3",
                    p { class: "text-xs text-gray-400", "Create a new space" }
                    input {
                        class: "field w-full text-xs",
                        placeholder: "Spaces service (automatic if empty)",
                        value: "{service}",
                        oninput: move |e| service.set(e.value()),
                    }
                    label { class: "flex items-center justify-between gap-3 text-xs text-gray-300",
                        "Visibility"
                        select {
                            class: "rounded bg-gray-900 px-2 py-1 text-xs text-gray-100",
                            value: if *private.read() { "private" } else { "public" },
                            onchange: move |e| private.set(e.value() == "private"),
                            option { value: "public", "Public" }
                            option { value: "private", "Private" }
                        }
                    }
                    div { class: "flex gap-2",
                        input {
                            class: "field flex-1 text-xs",
                            placeholder: "Space name",
                            value: "{name}",
                            oninput: move |e| name.set(e.value()),
                        }
                        button {
                            class: "btn-primary px-3 text-xs",
                            onclick: {
                                let client = client.clone();
                                let storage = service_storage.clone();
                                let mut error = error;
                                let onclose = onclose;
                                move |_| {
                                    let title = name.read().trim().to_string();
                                    if title.is_empty() {
                                        error.set(Some("Enter a space name.".to_string()));
                                        return;
                                    }
                                    let chosen = service.read().trim().to_string();
                                    let private = *private.read();
                                    let client = client.clone();
                                    let storage = storage.clone();
                                    let mut error = error;
                                    spawn(async move {
                                        let service = if chosen.is_empty() {
                                            match client.resolve_spaces_service().await {
                                                Ok(service) => service,
                                                Err(err) => {
                                                    error.set(Some(err.to_string()));
                                                    return;
                                                }
                                            }
                                        } else {
                                            let Ok(service) = chosen.parse::<xmpp_core::Jid>() else {
                                                error.set(Some(
                                                    "The spaces service address is invalid."
                                                        .to_string(),
                                                ));
                                                return;
                                            };
                                            client.set_spaces_service(Some(&service.to_string()));
                                            let _ = storage.set_setting(
                                                "spaces.service",
                                                &serde_json::json!(service.to_string()),
                                            );
                                            service
                                        };
                                        match client.create_space(&service, &title, private).await {
                                            Ok(id) => {
                                                let _ = client.fetch_space_info(&id).await;
                                                let _ = client.fetch_space_items(&id).await;
                                                onclose.call(());
                                            }
                                            Err(err) => error.set(Some(err.to_string())),
                                        }
                                    });
                                }
                            },
                            "Create"
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
                        class: "text-gray-400 hover:text-gray-100",
                        title: "Close",
                        onclick: move |_| onclose.call(()),
                        Icon { name: IconName::X, class: "h-4 w-4" }
                    }
                }
                if let Some(src) = preview {
                    img { class: "w-16 h-16 rounded-full object-cover", src: "{src}" }
                }
                label { class: "block text-xs text-gray-400",
                    "Full name"
                    input {
                        class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-gray-100 focus:outline-none focus:ring-1 focus:ring-teal-500",
                        value: "{full_name}",
                        oninput: move |e| full_name.set(e.value()),
                    }
                }
                label { class: "block text-xs text-gray-400",
                    "Nickname"
                    input {
                        class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-gray-100 focus:outline-none focus:ring-1 focus:ring-teal-500",
                        value: "{nickname}",
                        oninput: move |e| nickname.set(e.value()),
                    }
                }
                label { class: "block text-xs text-gray-400",
                    "Organisation"
                    input {
                        class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-gray-100 focus:outline-none focus:ring-1 focus:ring-teal-500",
                        value: "{organization}",
                        oninput: move |e| organization.set(e.value()),
                    }
                }
                label { class: "block text-xs text-gray-400",
                    "Title"
                    input {
                        class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-gray-100 focus:outline-none focus:ring-1 focus:ring-teal-500",
                        value: "{title}",
                        oninput: move |e| title.set(e.value()),
                    }
                }
                div { class: "grid grid-cols-2 gap-2",
                    label { class: "block text-xs text-gray-400",
                        "Email"
                        input {
                            class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-gray-100 focus:outline-none focus:ring-1 focus:ring-teal-500",
                            value: "{email}",
                            oninput: move |e| email.set(e.value()),
                        }
                    }
                    label { class: "block text-xs text-gray-400",
                        "Phone"
                        input {
                            class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-gray-100 focus:outline-none focus:ring-1 focus:ring-teal-500",
                            value: "{phone}",
                            oninput: move |e| phone.set(e.value()),
                        }
                    }
                }
                label { class: "block text-xs text-gray-400",
                    "Website"
                    input {
                        class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-gray-100 focus:outline-none focus:ring-1 focus:ring-teal-500",
                        value: "{url}",
                        oninput: move |e| url.set(e.value()),
                    }
                }
                label { class: "block text-xs text-gray-400",
                    "Note"
                    input {
                        class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-gray-100 focus:outline-none focus:ring-1 focus:ring-teal-500",
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

/// Coarse timestamp formatting for the conversation list.
fn trailing_emoji_query(text: &str) -> Option<(usize, String)> {
    let start = text.rfind(':')?;
    if start > 0 && !text[..start].ends_with(char::is_whitespace) {
        return None;
    }
    let query = text[start + 1..].trim();
    if query.is_empty() || query.len() > 32 || query.chars().any(char::is_whitespace) {
        return None;
    }
    Some((start, query.to_string()))
}

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
    is_self: bool,
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
        if !account.is_empty() && bare == account {
            continue;
        }
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
            is_self: false,
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
            is_self: false,
        });
    }

    if !account.is_empty() {
        let self_chat = client.chats.get_by_bare(&account);
        let (text, time) = match self_chat.as_ref().and_then(|chat| chat.last_message()) {
            Some(message) => (message.display_body(), Some(message.timestamp)),
            None => ("Message yourself".to_string(), None),
        };
        entries.push(ConvEntry {
            draft: load_draft(storage.as_ref(), &account, &account),
            bare: account.clone(),
            name: "Note to Self".to_string(),
            last_text: text,
            last_time: time,
            unread: self_chat
                .as_ref()
                .map(|chat| chat.unread_count())
                .unwrap_or(0),
            online: true,
            is_muc: false,
            avatar_src: None,
            is_pinned: self_chat
                .as_ref()
                .is_some_and(|chat| chat.preferences().is_pinned),
            is_archived: false,
            notifications_enabled: true,
            is_self: true,
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
                    class: "w-full px-3 py-1.5 rounded bg-gray-800 text-sm text-gray-100 placeholder-gray-500 focus:outline-none focus:ring-1 focus:ring-teal-500",
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
                        "px-2 py-1 rounded text-xs text-gray-400 hover:text-gray-100 hover:bg-gray-700"
                    },
                    onclick: move |_| {
                        let next = !*unread_only.read();
                        unread_only.set(next);
                    },
                    "Unread"
                }
                if archived_count > 0 || showing_archived {
                    button {
                        class: "inline-flex items-center gap-1 px-2 py-1 rounded text-xs text-gray-400 hover:text-gray-100 hover:bg-gray-700",
                        onclick: move |_| {
                            let next = !*show_archived.read();
                            show_archived.set(next);
                        },
                        if showing_archived {
                            Icon { name: IconName::ArrowLeft, class: "h-3 w-3" }
                            span { "Back to chats" }
                        } else {
                            Icon { name: IconName::Archive, class: "h-3 w-3" }
                            span { "Archived ({archived_count})" }
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
                    let is_self = entry.is_self;
                    let is_selected = selected.as_deref() == Some(entry.bare.as_str());
                    let mut cs = client_state.clone();
                    let client2 = client.clone();
                    let bare2 = bare.clone();
                    rsx! {
                        ChatListItem {
                            entry: entry,
                            selected: is_selected,
                            onselect: move |_| {
                                if is_self {
                                    if let Ok(jid) = bare2.parse::<xmpp_core::Jid>() {
                                        client2.chats.get_or_create(&jid);
                                    }
                                }
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
    let is_self = entry.is_self;
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
                class: "flex min-w-0 flex-1 items-center gap-2.5 p-2 text-left",
                onclick: move |e| onselect.call(e),
                div { class: "relative",
                    div { class: "flex h-10 w-10 items-center justify-center overflow-hidden rounded-full bg-teal-500 text-sm font-bold text-white",
                        if is_self {
                            Icon { name: IconName::Note, class: "h-5 w-5" }
                        } else if let Some(src) = avatar_src {
                            img { class: "w-full h-full object-cover", src: "{src}", alt: "{avatar}" }
                        } else {
                            "{avatar}"
                        }
                    }
                    if is_muc {
                        div { class: "absolute -bottom-1 -right-1 w-5 h-5 bg-gray-600 border-2 border-gray-800 rounded-full flex items-center justify-center text-[10px] font-bold",
                            Icon { name: IconName::Hash, class: "h-2.5 w-2.5" }
                        }
                    } else if online && !is_self {
                        div { class: "absolute bottom-0 right-0 w-3 h-3 bg-green-500 border-2 border-gray-800 rounded-full" }
                    }
                }
                div { class: "flex-1 min-w-0",
                    div { class: "flex items-center justify-between",
                        div { class: "flex items-center gap-1 min-w-0",
                            h4 { class: "font-medium truncate", "{name}" }
                            if is_pinned {
                                span { class: "inline-flex", title: "Pinned",
                                    Icon { name: IconName::Pin, class: "h-3 w-3" }
                                }
                            }
                            if muted {
                                span { class: "inline-flex", title: "Muted",
                                    Icon { name: IconName::BellOff, class: "h-3 w-3" }
                                }
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
                div { class: "hidden flex-col pr-1 group-hover:flex",
                    button {
                        class: "inline-flex items-center justify-center px-1 text-xs text-gray-400 hover:text-gray-100",
                        title: if is_pinned { "Unpin" } else { "Pin" },
                        onclick: pin,
                        Icon { name: IconName::Pin, class: "h-3 w-3" }
                    }
                    button {
                        class: "inline-flex items-center justify-center px-1 text-xs text-gray-400 hover:text-gray-100",
                        title: if muted { "Unmute" } else { "Mute" },
                        onclick: mute,
                        Icon { name: IconName::BellOff, class: "h-3 w-3" }
                    }
                    button {
                        class: "inline-flex items-center justify-center px-1 text-xs text-gray-400 hover:text-gray-100",
                        title: if is_archived { "Unarchive" } else { "Archive" },
                        onclick: archive,
                        Icon { name: IconName::Archive, class: "h-3 w-3" }
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
    let status = use_signal(|| Option::<String>::None);
    let saving = use_signal(|| false);
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
    let cs = client_state.clone();
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
                        class: "text-gray-400 hover:text-gray-100",
                        title: "Close",
                        onclick: move |_| onclose.call(()),
                        Icon { name: IconName::X, class: "h-4 w-4" }
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
                                            class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-gray-100",
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
                                            class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-gray-100",
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
                                            class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-gray-100 focus:outline-none focus:ring-1 focus:ring-teal-500",
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
                            class: "px-3 py-2 rounded bg-red-700 hover:bg-red-600 text-sm text-white",
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
                        class: "text-gray-400 hover:text-gray-100",
                        title: "Close",
                        onclick: move |_| onclose.call(()),
                        Icon { name: IconName::X, class: "h-4 w-4" }
                    }
                }
                input {
                    class: "w-full px-3 py-2 rounded bg-gray-900 text-sm text-gray-100 placeholder-gray-500",
                    placeholder: "user@example.org",
                    value: "{jid}",
                    oninput: move |e| jid.set(e.value()),
                }
                input {
                    class: "w-full px-3 py-2 rounded bg-gray-900 text-sm text-gray-100 placeholder-gray-500",
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
    let error = use_signal(|| Option::<String>::None);
    let cs = client_state.clone();

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
                        class: "text-gray-400 hover:text-gray-100",
                        title: "Close",
                        onclick: move |_| onclose.call(()),
                        Icon { name: IconName::X, class: "h-4 w-4" }
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
                        class: "px-3 py-1.5 rounded bg-red-600 hover:bg-red-500 text-sm text-white",
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
                        class: "text-gray-400 hover:text-gray-100",
                        title: "Close",
                        onclick: move |_| onclose.call(()),
                        Icon { name: IconName::X, class: "h-4 w-4" }
                    }
                }
                input {
                    class: "w-full px-3 py-2 rounded bg-gray-900 text-sm text-gray-100 placeholder-gray-500 focus:outline-none focus:ring-1 focus:ring-teal-500",
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
    let subscription = item.subscription;
    let pending_in = item.is_pending_in;
    let pending_out = item.is_pending_out;
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

    let mut cs_subscription = client_state.clone();
    let subscription_client = client.clone();
    let subscription_jid = item.jid.clone();
    let request_presence = move |_| {
        let _ = subscription_client.add_contact(&subscription_jid, None);
        cs_subscription.bump();
    };
    let mut cs_approve = client_state.clone();
    let approve_client = client.clone();
    let approve_jid = item.jid.clone();
    let approve = move |_| {
        let _ = approve_client.roster.approve_subscription(&approve_jid);
        cs_approve.bump();
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
                        class: "text-gray-400 hover:text-gray-100",
                        title: "Close",
                        onclick: move |_| onclose.call(()),
                        Icon { name: IconName::X, class: "h-4 w-4" }
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
                if pending_in {
                    div { class: "flex items-center justify-between gap-2 rounded-xl border border-teal-500/20 bg-teal-500/5 px-3 py-2",
                        span { class: "text-xs text-teal-200", "This contact wants to see your presence." }
                        button {
                            class: "rounded-lg bg-teal-600 px-3 py-1 text-xs font-medium text-white hover:bg-teal-500",
                            onclick: approve,
                            "Accept"
                        }
                    }
                } else if pending_out {
                    p { class: "rounded-xl border border-amber-500/20 bg-amber-500/10 px-3 py-2 text-xs text-amber-200",
                        "Presence request sent. Waiting for approval."
                    }
                } else if subscription != SubscriptionState::Both {
                    div { class: "flex items-center justify-between gap-2 rounded-xl border border-gray-700 bg-gray-900/60 px-3 py-2",
                        span { class: "text-xs text-gray-400", "Presence is not shared in both directions." }
                        button {
                            class: "rounded-lg bg-gray-700 px-3 py-1 text-xs font-medium text-gray-100 hover:bg-gray-600",
                            onclick: request_presence,
                            "Request presence"
                        }
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
                            class: "flex-1 px-3 py-1.5 bg-gray-700 border border-gray-600 rounded-lg text-sm text-gray-100",
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
                            class: "flex-1 px-3 py-1.5 bg-gray-700 border border-gray-600 rounded-lg text-sm text-gray-100",
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
                            class: "text-xs text-gray-400 hover:text-gray-100",
                            onclick: move |_| refresh_button(),
                            "Refresh"
                        }
                        button {
                            class: "text-gray-400 hover:text-gray-100",
                            title: "Close",
                            onclick: move |_| onclose.call(()),
                            Icon { name: IconName::X, class: "h-4 w-4" }
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
                                class: "text-xs text-gray-400 hover:text-gray-100",
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
                                                    class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-gray-100",
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
                                                    class: "mt-1 w-full px-3 py-2 rounded bg-gray-900 text-sm text-gray-100 focus:outline-none focus:ring-1 focus:ring-teal-500",
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
    let pending: Vec<RosterItem> = items
        .iter()
        .filter(|item| item.is_pending_in)
        .cloned()
        .collect();
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
            if !pending.is_empty() {
                div { class: "space-y-1 rounded-xl border border-teal-500/20 bg-teal-500/5 p-2",
                    p { class: "px-1 text-[11px] font-semibold uppercase tracking-[0.16em] text-teal-300",
                        "Contact requests"
                    }
                    for item in pending {
                        {
                            let name = item.display_name();
                            let jid = item.jid.clone();
                            let approve_client = client.clone();
                            let approve_jid = jid.clone();
                            let mut approve_state = client_state.clone();
                            let deny_client = client.clone();
                            let deny_jid = jid.clone();
                            let mut deny_state = client_state.clone();
                            rsx! {
                                div { class: "flex items-center gap-2 rounded-lg bg-gray-900/60 px-2 py-1.5",
                                    div { class: "min-w-0 flex-1",
                                        p { class: "truncate text-sm font-medium", "{name}" }
                                        p { class: "truncate text-xs text-gray-500", "{jid}" }
                                    }
                                    button {
                                        class: "rounded-lg bg-teal-600 px-2 py-1 text-xs font-medium text-white hover:bg-teal-500",
                                        onclick: move |_| {
                                            let _ = approve_client.roster.approve_subscription(&approve_jid);
                                            approve_state.bump();
                                        },
                                        "Accept"
                                    }
                                    button {
                                        class: "rounded-lg bg-gray-700 px-2 py-1 text-xs text-gray-300 hover:bg-gray-600",
                                        onclick: move |_| {
                                            let _ = deny_client.roster.deny_subscription(&deny_jid);
                                            deny_state.bump();
                                        },
                                        "Decline"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "flex gap-1 px-1 pb-1",
                input {
                    class: "flex-1 px-3 py-1.5 bg-gray-700 border border-gray-600 rounded-lg text-sm text-gray-100 placeholder-gray-400 focus:outline-none focus:ring-1 focus:ring-teal-500",
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
                        "inline-flex items-center gap-1 px-2 py-1 rounded text-xs bg-red-600 text-white"
                    } else {
                        "inline-flex items-center gap-1 px-2 py-1 rounded text-xs text-gray-400 hover:text-gray-100 hover:bg-gray-700"
                    },
                    onclick: move |_| {
                        let next = !*show_blocked.read();
                        show_blocked.set(next);
                    },
                    if *show_blocked.read() {
                        Icon { name: IconName::ArrowLeft, class: "h-3 w-3" }
                        span { "All contacts" }
                    } else {
                        span { "Blocked ({blocked_count})" }
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
    let pending_out = item.is_pending_out;
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
                    p { class: "text-xs text-gray-400 truncate",
                        if pending_out {
                            "Awaiting approval"
                        } else {
                            "{status}"
                        }
                    }
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

/// Call history.
#[component]
fn CallsList() -> Element {
    let client = use_context::<XmppClient>();
    let storage = use_context::<Arc<dyn StorageBackend>>();
    let client_state = use_context::<ClientState>();
    let _ = *client_state.revision.read();
    let account = client
        .connection
        .jid()
        .map(|jid| jid.to_bare().to_string())
        .unwrap_or_default();
    let calls = storage.get_calls(&account, 100).unwrap_or_default();
    let state = client_state.clone();

    rsx! {
        div { class: "p-2 space-y-1",
            if calls.is_empty() {
                div { class: "p-6 text-center text-sm text-gray-500", "No calls yet" }
            }
            for call in calls {
                {
                    let peer = call.peer_jid.clone();
                    let name = client
                        .roster
                        .get_by_bare(&peer)
                        .map(|item| item.display_name())
                        .unwrap_or_else(|| peer.clone());
                    let missed = call.direction == "incoming"
                        && matches!(call.state.as_str(), "missed" | "declined" | "rejected");
                    let icon = if missed {
                        IconName::PhoneMissed
                    } else if call.direction == "incoming" {
                        IconName::PhoneIncoming
                    } else {
                        IconName::PhoneOutgoing
                    };
                    let tone = if missed { "text-red-400" } else { "text-gray-400" };
                    let duration = if call.duration_secs > 0 {
                        format!("{:02}:{:02}", call.duration_secs / 60, call.duration_secs % 60)
                    } else {
                        String::new()
                    };
                    let time = format_time(call.started_at);
                    let mut open_state = state.clone();
                    let open_peer = peer.clone();
                    let call_client = client.clone();
                    let call_peer = peer.clone();
                    rsx! {
                        div { class: "flex items-center gap-2 rounded-lg p-2 hover:bg-gray-700",
                            button {
                                class: "flex min-w-0 flex-1 items-center gap-3 text-left",
                                onclick: move |_| {
                                    open_state.select(Some(open_peer.clone()));
                                    open_state.bump();
                                },
                                div { class: "flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-gray-700 {tone}",
                                    Icon { name: icon, class: "h-4 w-4" }
                                }
                                div { class: "min-w-0",
                                    p { class: "truncate text-sm font-medium", "{name}" }
                                    p { class: "text-xs text-gray-500", "{time} {duration}" }
                                }
                            }
                            button {
                                class: "inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-gray-700 text-gray-200 hover:bg-teal-500 hover:text-white",
                                title: "Call back",
                                onclick: move |_| {
                                    let client = call_client.clone();
                                    let peer = call_peer.clone();
                                    spawn(async move {
                                        if let Ok(jid) = peer.parse::<xmpp_core::Jid>() {
                                            let _ = client.start_call(&jid, CallKind::Audio).await;
                                        }
                                    });
                                },
                                Icon { name: IconName::Phone, class: "h-4 w-4" }
                            }
                        }
                    }
                }
            }
        }
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
                    class: "w-full px-3 py-1.5 bg-gray-700 border border-gray-600 rounded-lg text-sm text-gray-100 placeholder-gray-400 focus:outline-none focus:ring-1 focus:ring-teal-500",
                    placeholder: "room@conference.example.com",
                    value: "{new_room}",
                    oninput: move |e| new_room.set(e.value()),
                }
                div { class: "flex gap-1",
                    input {
                        class: "flex-1 px-3 py-1.5 bg-gray-700 border border-gray-600 rounded-lg text-sm text-gray-100 placeholder-gray-400 focus:outline-none focus:ring-1 focus:ring-teal-500",
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
                        class: "px-3 py-1.5 bg-gray-700 hover:bg-gray-600 rounded-lg text-sm text-gray-100",
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
                                div { class: "w-10 h-10 rounded-full bg-gray-600 flex items-center justify-center font-bold text-gray-100",
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
    reactions: Vec<ReactionView>,
    edited: bool,
    encrypted: bool,
    media: Vec<xmpp_core::MediaAttachment>,
    sticker_cid: Option<String>,
    sticker_from: Option<String>,
    sticker_alt: Option<String>,
    forwarded_from: Option<String>,
    mentions: Vec<String>,
    attention: bool,
    reply_to: Option<String>,
    reply_preview: Option<(String, String)>,
    message_ids: Vec<String>,
}

#[derive(Clone, PartialEq)]
struct ReactionView {
    emoji: String,
    count: usize,
    mine: bool,
    from: Vec<String>,
}

fn group_reactions<I, F, G>(reactions: I, emoji_of: F, from_of: G, own: &str) -> Vec<ReactionView>
where
    I: IntoIterator,
    F: Fn(&I::Item) -> String,
    G: Fn(&I::Item) -> String,
{
    let mut grouped: std::collections::BTreeMap<String, (usize, bool, Vec<String>)> =
        std::collections::BTreeMap::new();
    for reaction in reactions {
        let emoji = emoji_of(&reaction);
        let from = from_of(&reaction);
        let entry = grouped.entry(emoji).or_insert((0, false, Vec::new()));
        entry.0 += 1;
        entry.1 |= from == own;
        entry.2.push(from);
    }
    grouped
        .into_iter()
        .map(|(emoji, (count, mine, from))| ReactionView {
            emoji,
            count,
            mine,
            from,
        })
        .collect()
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
                        class: "w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-gray-100 placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-teal-500 resize-none min-h-[72px]",
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

    let section = *client_state.section.read();
    if section == AppSection::Feed {
        return rsx! { FeedView {} };
    }

    let _rev = *client_state.revision.read();
    let selected = (*client_state.selected_chat.read()).clone();
    let mobile_class = if selected.is_some() {
        "flex"
    } else {
        "hidden md:flex"
    };
    let mut reply_target = use_signal(|| Option::<(String, String)>::None);
    let disco_client = client.clone();
    let disco_selection = client_state.selected_chat;
    use_effect(move || {
        if let Some(bare) = disco_selection.read().clone() {
            if let Ok(jid) = bare.parse::<xmpp_core::Jid>() {
                let _ = disco_client.request_disco_info(&jid);
            }
        }
    });

    // Opening a room must (re)join it. Rooms restored from storage are present
    // in `mucs` but disconnected, so a room that was already known used to be
    // opened without sending presence: the client then showed only its own
    // local echoes and never received anyone else's messages.
    let join_client = client.clone();
    let join_spaces = client_state.spaces;
    let join_selection = client_state.selected_chat;
    let join_revision = client_state.revision;
    let join_connection = client_state.connection_state;
    use_effect(move || {
        // Re-run as connection state and room state settle.
        let _rev = *join_revision.read();
        let connected = matches!(
            *join_connection.read(),
            ConnectionState::Connected | ConnectionState::Authenticated
        );
        if !connected {
            // Joining before the stream is up would send presence into a
            // channel that has nowhere to go; wait for the state to change.
            return;
        }
        let Some(bare) = join_selection.read().clone() else {
            return;
        };
        let in_space_text = join_spaces.read().iter().any(|space| {
            space
                .rooms
                .iter()
                .any(|room| room.conference == bare && room.kind == SpaceRoomKind::Text)
        });
        let is_voice = join_spaces.read().iter().any(|space| {
            space
                .rooms
                .iter()
                .any(|room| room.conference == bare && room.kind == SpaceRoomKind::Voice)
        });
        if is_voice {
            // The voice channel joins its own room with the SFU nick.
            return;
        }
        let known = join_client.mucs.get_by_bare(&bare);
        if known.is_none() && !in_space_text {
            // A one-to-one conversation, not a room.
            return;
        }
        if known.as_ref().is_some_and(|room| !room.needs_join()) {
            return;
        }
        let nick = join_client
            .connection
            .jid()
            .and_then(|jid| jid.node().map(|node| node.to_string()))
            .unwrap_or_else(|| "me".to_string());
        if let Err(error) = join_client.ensure_room_joined_str(&bare, &nick) {
            tracing::warn!(room = %bare, %error, "could not join the room");
        }
    });

    let mut empty_state = client_state.clone();
    let Some(bare) = selected else {
        return rsx! {
            main { class: "min-w-0 flex-1 items-center justify-center bg-gray-950 p-8 {mobile_class}",
                div { class: "panel w-full max-w-md p-8 text-center",
                    div { class: "mx-auto mb-5 flex h-14 w-14 items-center justify-center rounded-2xl bg-teal-500/10 text-2xl text-teal-300",
                        "M"
                    }
                    h2 { class: "text-lg font-semibold tracking-tight text-gray-100", "No conversation selected" }
                    p { class: "mx-auto mt-2 max-w-sm text-sm leading-6 text-gray-500",
                        "Choose a chat or room from the sidebar, or search across your messages and contacts."
                    }
                    button {
                        class: "btn-primary mt-6",
                        onclick: move |_| {
                            empty_state.search_open.set(true);
                            empty_state.bump();
                        },
                        "Search conversations"
                    }
                }
            }
        };
    };

    let voice_room = client_state.spaces.read().iter().find_map(|space| {
        space
            .rooms
            .iter()
            .find(|room| room.conference == bare && room.kind == SpaceRoomKind::Voice)
            .map(|room| (space.id.clone(), room.clone()))
    });
    if let Some((space_id, room)) = voice_room {
        return rsx! {
            VoiceChannelView { space_id, room }
        };
    }

    let own_bare = client
        .connection
        .jid()
        .map(|jid| jid.to_bare().to_string())
        .unwrap_or_default();
    let is_self = !own_bare.is_empty() && bare == own_bare;
    if is_self {
        if let Ok(jid) = bare.parse::<xmpp_core::Jid>() {
            client.chats.get_or_create(&jid);
        }
    }
    let is_muc = client.mucs.get_by_bare(&bare).is_some();
    let can_admin = client
        .mucs
        .get_by_bare(&bare)
        .is_some_and(|room| room.can_admin());
    let restrictions = client
        .disco
        .reaction_restrictions(&bare)
        .unwrap_or_default();
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
    } else if is_self {
        (
            "Note to Self".to_string(),
            "Private notes".to_string(),
            "text-xs text-gray-400",
        )
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
        let self_nick = room.self_nick().unwrap_or_default();
        for m in room.messages() {
            messages.push(muc_to_display(&m, &self_nick));
        }
    } else if let Some(chat) = client.chats.get_by_bare(&bare) {
        let other_name = client
            .roster
            .get(&chat.jid)
            .map(|r| r.display_name())
            .unwrap_or_else(|| bare.clone());
        for m in chat.messages() {
            messages.push(chat_to_display(&m, &other_name, &own_bare));
        }
    }

    let mut previews = HashMap::<String, (String, String)>::new();
    for message in &messages {
        for id in &message.message_ids {
            previews.insert(id.clone(), (message.sender.clone(), message.body.clone()));
        }
    }
    for message in &mut messages {
        if let Some(reply_id) = message.reply_to.as_ref() {
            message.reply_preview = previews.get(reply_id).cloned();
        }
    }

    let needle = search.read().trim().to_lowercase();
    if !needle.is_empty() {
        messages.retain(|m| m.body.to_lowercase().contains(&needle));
    }

    rsx! {
        main { class: "min-w-0 flex-1 flex-col bg-gray-950 {mobile_class}",
            header { class: "flex h-14 shrink-0 items-center justify-between border-b border-gray-800 bg-gray-900/80 px-3 backdrop-blur md:px-4",
                div { class: "flex min-w-0 items-center gap-2.5",
                    button {
                        class: "flex h-8 w-8 items-center justify-center rounded-lg text-gray-400 hover:bg-gray-800 hover:text-gray-100 md:hidden",
                        title: "Back",
                        onclick: {
                            let mut cs = client_state.clone();
                            move |_| {
                                cs.select(None);
                                cs.bump();
                            }
                        },
                        Icon { name: IconName::ArrowLeft, class: "h-4 w-4" }
                    }
                    div { class: "flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-teal-500 text-sm font-bold text-white",
                        if is_self {
                            Icon { name: IconName::Note, class: "h-4 w-4" }
                        } else {
                            "{avatar}"
                        }
                    }
                    div {
                        h3 { class: "font-medium", "{title}" }
                        p { class: "{subtitle_class}", "{subtitle}" }
                    }
                }
                div { class: "flex items-center gap-2",
                    if !is_muc && !is_self {
                        {
                            let audio_client = client.clone();
                            let audio_bare = bare.clone();
                            let video_client = client.clone();
                            let video_bare = bare.clone();
                            rsx! {
                                button {
                                    class: "inline-flex h-8 w-8 items-center justify-center rounded-lg bg-gray-700 text-gray-200 hover:bg-teal-500 hover:text-white",
                                    title: "Start an audio call",
                                    onclick: move |_| {
                                        let client = audio_client.clone();
                                        let bare = audio_bare.clone();
                                        spawn(async move {
                                            if let Ok(jid) = bare.parse::<xmpp_core::Jid>() {
                                                let _ = client.start_call(&jid, CallKind::Audio).await;
                                            }
                                        });
                                    },
                                    Icon { name: IconName::Phone, class: "h-4 w-4" }
                                }
                                button {
                                    class: "inline-flex h-8 w-8 items-center justify-center rounded-lg bg-gray-700 text-gray-200 hover:bg-teal-500 hover:text-white",
                                    title: "Start a video call",
                                    onclick: move |_| {
                                        let client = video_client.clone();
                                        let bare = video_bare.clone();
                                        spawn(async move {
                                            if let Ok(jid) = bare.parse::<xmpp_core::Jid>() {
                                                let _ = client.start_call(&jid, CallKind::Video).await;
                                            }
                                        });
                                    },
                                    Icon { name: IconName::Video, class: "h-4 w-4" }
                                }
                            }
                        }
                    }
                    input {
                        class: "w-40 px-2 py-1 rounded bg-gray-900 text-xs text-gray-100 placeholder-gray-500 focus:outline-none focus:ring-1 focus:ring-teal-500",
                        r#type: "search",
                        placeholder: "Search messages",
                        value: "{search}",
                        oninput: move |e| search.set(e.value()),
                    }
                    if !messages.is_empty() {
                        button {
                            class: "text-xs px-2 py-1 rounded bg-gray-700 text-gray-100 hover:bg-red-600 hover:text-white",
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
                                    class: "inline-flex items-center gap-1 text-xs px-2 py-1 rounded bg-gray-700 hover:bg-gray-600",
                                    onclick: move |_| {
                                        let next = !*show_members.read();
                                        show_members.set(next);
                                    },
                                    Icon { name: IconName::Users, class: "h-3 w-3" }
                                    span { "Members ({count})" }
                                }
                                button {
                                    class: "inline-flex items-center gap-1 text-xs px-2 py-1 rounded bg-gray-700 hover:bg-gray-600",
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
                                    Icon { name: IconName::Pencil, class: "h-3 w-3" }
                                    span { "Subject" }
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
                                    class: "inline-flex items-center gap-1 text-xs px-2 py-1 rounded bg-gray-700 hover:bg-gray-600",
                                    onclick: move |_| {
                                        let next = !*show_devices.read();
                                        show_devices.set(next);
                                        if let Some(chat) = c.chats.get_by_bare(&j) {
                                            let _ = c.fetch_omemo(&chat.jid);
                                        }
                                    },
                                    Icon { name: IconName::LockKeyhole, class: "h-3 w-3" }
                                    span { "Devices" }
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
                                class: "flex-1 px-3 py-1.5 rounded bg-gray-900 text-sm text-gray-100 focus:outline-none focus:ring-1 focus:ring-teal-500",
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

            div { class: "flex-1 space-y-4 overflow-y-auto p-4 md:p-6",
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
                                class: "mx-auto block text-xs text-gray-400 hover:text-gray-100 bg-gray-800 rounded px-3 py-1",
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
                                restrictions: restrictions.clone(),
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
                        class: "text-gray-400 hover:text-gray-100 text-xs px-2",
                        title: "Cancel reply",
                        onclick: move |_| reply_target.set(None),
                        Icon { name: IconName::X, class: "h-4 w-4" }
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
                                            class: "ml-auto px-2 py-0.5 rounded bg-red-700 hover:bg-red-600 text-white",
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
                                                class: "px-2 py-0.5 rounded bg-red-800 hover:bg-red-700 text-white",
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
                                        class: "text-xs px-2 py-0.5 rounded bg-emerald-700 hover:bg-emerald-600 text-white",
                                        onclick: move |_| {
                                            trust_client
                                                .omemo
                                                .set_trust(&target_trust, id, TrustLevel::Trusted);
                                            trust_state.bump();
                                        },
                                        "Trust"
                                    }
                                    button {
                                        class: "text-xs px-2 py-0.5 rounded bg-red-700 hover:bg-red-600 text-white",
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
        TextSegment::Plain(text) => rsx! { EmojiText { text: text.clone() } },
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

enum EmojiNode {
    Text(String),
    Alias { uri: String, alias: String },
}

#[component]
fn EmojiText(text: String) -> Element {
    let storage = use_context::<Arc<dyn StorageBackend>>();
    let aliases = emoji_packs::alias_map(storage.as_ref());
    let mut nodes = Vec::new();
    let mut remaining = text.as_str();
    loop {
        let Some(start) = remaining.find(':') else {
            if !remaining.is_empty() {
                nodes.push(EmojiNode::Text(remaining.to_string()));
            }
            break;
        };
        let Some(end) = remaining[start + 1..].find(':') else {
            nodes.push(EmojiNode::Text(remaining.to_string()));
            break;
        };
        let alias = &remaining[start + 1..start + 1 + end];
        if let Some(uri) = aliases.get(alias) {
            if start > 0 {
                nodes.push(EmojiNode::Text(remaining[..start].to_string()));
            }
            nodes.push(EmojiNode::Alias {
                uri: uri.clone(),
                alias: alias.to_string(),
            });
            remaining = &remaining[start + 1 + end + 1..];
        } else {
            nodes.push(EmojiNode::Text(remaining[..start + 1].to_string()));
            remaining = &remaining[start + 1..];
        }
    }

    rsx! {
        for node in nodes {
            match node {
                EmojiNode::Text(text) => rsx! { EmojiGraphemes { text } },
                EmojiNode::Alias { uri, alias } => rsx! {
                    img {
                        class: "emoji inline-block h-[1.4em] w-[1.4em] object-contain align-[-0.25em]",
                        src: "{uri}",
                        alt: ":{alias}:",
                        title: ":{alias}:",
                        loading: "lazy",
                    }
                },
            }
        }
    }
}

#[component]
fn EmojiGraphemes(text: String) -> Element {
    let graphemes: Vec<String> =
        unicode_segmentation::UnicodeSegmentation::graphemes(text.as_str(), true)
            .map(str::to_string)
            .collect();
    rsx! {
        for grapheme in graphemes {
            { render_emoji_grapheme(&grapheme) }
        }
    }
}

fn render_emoji_grapheme(grapheme: &str) -> Element {
    let Some(entry) = ::emoji::lookup_by_glyph::lookup(grapheme) else {
        return rsx! { span { "{grapheme}" } };
    };
    let title = format!(":{}:", emoji::shortcut(entry.name));
    if let Some(src) = emoji::image_for_glyph(grapheme) {
        rsx! {
            img {
                class: "emoji inline-block h-[1.2em] w-[1.2em] align-[-0.2em]",
                src: "{src}",
                alt: "{grapheme}",
                title: "{title}",
                loading: "lazy",
            }
        }
    } else {
        rsx! { span { title: "{title}", "{grapheme}" } }
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

fn chat_to_display(m: &ChatMessage, other_name: &str, own_bare: &str) -> DisplayMessage {
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
        reactions: group_reactions(
            m.reactions.iter(),
            |r| r.emoji.clone(),
            |r| r.from_jid.to_bare().to_string(),
            own_bare,
        ),
        edited: m.is_edited,
        encrypted: m.encryption.is_some(),
        media: m.media.clone(),
        sticker_cid: m.sticker_cid.clone(),
        sticker_from: Some(m.from_jid.to_string()),
        sticker_alt: m.sticker_alt.clone(),
        forwarded_from: m
            .forward_info
            .as_ref()
            .map(|forward| forward.original_jid.to_string()),
        mentions: m.mentions.clone(),
        attention: m.attention,
        reply_to: m.reply_to.clone(),
        reply_preview: None,
        message_ids: [Some(m.id.clone()), m.origin_id.clone(), m.stanza_id.clone()]
            .into_iter()
            .flatten()
            .collect(),
    }
}

fn muc_to_display(m: &MucMessage, self_nick: &str) -> DisplayMessage {
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
        reactions: group_reactions(
            m.reactions.iter(),
            |r| r.emoji.clone(),
            |r| r.from_nick.clone(),
            self_nick,
        ),
        edited: m.is_edited,
        encrypted: m.encryption.is_some(),
        media: m.media.clone(),
        sticker_cid: m.sticker_cid.clone(),
        sticker_from: m.from_jid.as_ref().map(ToString::to_string),
        sticker_alt: m.sticker_alt.clone(),
        forwarded_from: m
            .forward_info
            .as_ref()
            .map(|forward| forward.original_jid.to_string()),
        mentions: m.mentions.clone(),
        attention: m.attention,
        reply_to: m.reply_to.clone(),
        reply_preview: None,
        message_ids: [Some(m.id.clone()), m.origin_id.clone(), m.stanza_id.clone()]
            .into_iter()
            .flatten()
            .collect(),
    }
}

/// Message bubble
#[component]
fn MessageBubble(
    msg: DisplayMessage,
    target: String,
    #[props(default = false)] is_muc: bool,
    restrictions: xmpp_core::ReactionRestrictions,
    onreply: EventHandler<DisplayMessage>,
    onforward: EventHandler<String>,
) -> Element {
    let client = use_context::<XmppClient>();
    let client_state = use_context::<ClientState>();
    let storage = use_context::<Arc<dyn StorageBackend>>();
    let cs = client_state.clone();
    let mut show_reaction_picker = use_signal(|| false);

    let is_own = msg.is_own;
    let can_moderate = is_muc
        && !is_own
        && client
            .mucs
            .get_by_bare(&target)
            .is_some_and(|room| room.can_moderate());
    let sender = msg.sender.clone();
    let body = msg.body.clone();
    let time = msg.time.clone();
    let status = msg.status.clone();
    let edited = msg.edited;
    let encrypted = msg.encrypted;
    let reactions = msg.reactions.clone();
    let current_mine: Vec<String> = reactions
        .iter()
        .filter(|reaction| reaction.mine)
        .map(|reaction| reaction.emoji.clone())
        .collect();
    let max_reactions = restrictions.max();
    let reactions_allowed = max_reactions != Some(0);
    let own_account = client
        .connection
        .jid()
        .map(|jid| jid.to_bare().to_string())
        .unwrap_or_default();
    let quick_glyph = emoji::lookup_glyph("👍")
        .map(|entry| {
            emoji::entry_for_tone(entry, emoji::load_skin_tone(storage.as_ref(), &own_account))
                .glyph
        })
        .unwrap_or("👍");
    let quick_reaction_allowed =
        reactions_allowed && (restrictions.allows("👍") || restrictions.allows(quick_glyph));
    let reaction_allowlist = restrictions.allowlist.clone();
    let media = msg.media.clone();
    let sticker_cid = msg.sticker_cid.clone();
    let sticker_from = msg.sticker_from.clone();
    let sticker_alt = msg.sticker_alt.clone();
    let forwarded_from = msg.forwarded_from.clone();
    let is_forwarded = forwarded_from.is_some();
    let mentions = msg.mentions.clone();
    let attention = msg.attention;
    let reply_preview = msg.reply_preview.clone();
    let show_body = !body.is_empty()
        && sticker_cid.is_none()
        && !media.iter().any(|attachment| attachment.url == body);
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

    let action_class =
        "inline-flex h-7 w-7 items-center justify-center rounded text-gray-400 opacity-0 transition-all hover:bg-gray-700 hover:text-gray-100 group-hover:opacity-100";

    rsx! {
        div { class: "group relative flex gap-3 px-4 py-1 hover:bg-gray-900/40",
            div { class: "mt-0.5 flex h-9 w-9 shrink-0 items-center justify-center overflow-hidden rounded-full bg-teal-500 text-xs font-bold text-white",
                if is_own { "U" } else { "{avatar}" }
            }
            div { class: "min-w-0 flex-1",
                div { class: "flex items-baseline gap-2",
                    span { class: "text-sm font-semibold text-gray-100",
                        if is_own { "You" } else { "{sender}" }
                    }
                    if *client_state.show_timestamps.read() {
                        span { class: "text-[11px] text-gray-500", "{time}" }
                    }
                }
                div { class: "mt-0.5 text-sm leading-6 text-gray-200",
                    if let Some((reply_sender, reply_body)) = reply_preview {
                        div { class: "mb-2 rounded-lg border-l-2 border-teal-300/70 bg-black/20 px-2.5 py-1.5",
                            p { class: "text-[11px] font-semibold text-teal-200", "{reply_sender}" }
                            p { class: "truncate text-xs text-gray-300", "{reply_body}" }
                        }
                    }
                    if let Some(cid) = sticker_cid.clone() {
                        {
                            let alt = sticker_alt.clone().unwrap_or_else(|| "Sticker".to_string());
                            if let Some(src) = xmpp_core::bob::data_uri(&cid) {
                                rsx! {
                                    img {
                                        class: "mb-1 max-h-48 max-w-full rounded-xl object-contain",
                                        src: "{src}",
                                        alt: "{alt}",
                                    }
                                }
                            } else {
                                let client = client.clone();
                                let from = sticker_from.clone();
                                let cid = cid.clone();
                                rsx! {
                                    button {
                                        class: "mb-1 rounded-lg border border-gray-600/60 bg-black/20 px-3 py-2 text-xs text-gray-300 hover:bg-black/30",
                                        onclick: move |_| {
                                            if let Some(from) = from.as_ref().and_then(|from| from.parse::<xmpp_core::Jid>().ok()) {
                                                let _ = client.request_bob(&from, &cid);
                                            }
                                        },
                                        "Load sticker"
                                    }
                                }
                            }
                        }
                    }
                    if let Some(from) = forwarded_from.clone() {
                        div { class: "mb-2 rounded-lg border-l-2 border-gray-400/60 bg-black/15 px-2.5 py-1.5",
                            p { class: "text-[11px] font-semibold uppercase tracking-wide text-gray-300", "Forwarded message" }
                            p { class: "truncate text-[11px] text-gray-400", "from {from}" }
                            if show_body {
                                div { class: "mt-1 text-sm",
                                    FormattedText { text: body.clone() }
                                }
                            }
                        }
                    }
                    if mentions_me {
                        p { class: "text-[11px] text-yellow-300 mb-1", "mentioned you" }
                    }
                    if attention {
                        p { class: "inline-flex items-center gap-1 text-[11px] text-amber-300 mb-1",
                            Icon { name: IconName::TriangleAlert, class: "h-3 w-3" }
                            span { "Attention" }
                        }
                    }
                    if show_body && !is_forwarded {
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
                                        Icon { name: IconName::Paperclip, class: "h-4 w-4" }
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
                    div { class: "mt-1 flex flex-wrap gap-1",
                        for reaction in reactions {
                            {
                                let emoji = reaction.emoji.clone();
                                let count = reaction.count;
                                let mine = reaction.mine;
                                let names = reaction.from.join(", ");
                                let mine_set = current_mine.clone();
                                let client = client.clone();
                                let target = target.clone();
                                let mid = msg_id.clone();
                                let mut cs = cs.clone();
                                let class = if mine {
                                    "flex items-center gap-1 rounded-full border border-teal-400/40 bg-teal-500/20 px-2 py-0.5 text-xs text-teal-100"
                                } else {
                                    "flex items-center gap-1 rounded-full border border-gray-700 bg-gray-800 px-2 py-0.5 text-xs text-gray-300 hover:bg-gray-700"
                                };
                                rsx! {
                                    button {
                                        class: "{class}",
                                        title: if names.is_empty() { emoji.clone() } else { names },
                                        onclick: move |_| {
                                            do_reaction_toggle(
                                                &client,
                                                &target,
                                                &mid,
                                                &emoji,
                                                is_muc,
                                                mine,
                                                &mine_set,
                                                max_reactions,
                                            );
                                            cs.bump();
                                        },
                                        if let Some(src) = emoji::image_for_glyph(&reaction.emoji) {
                                            img { class: "h-4 w-4", src: "{src}", alt: "{reaction.emoji}" }
                                        } else {
                                            span { "{reaction.emoji}" }
                                        }
                                        span { "{count}" }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "flex items-center gap-1 mt-1 text-xs text-gray-500",
                    if encrypted {
                        span { class: "inline-flex", title: "OMEMO encrypted",
                            Icon { name: IconName::Lock, class: "h-3 w-3" }
                        }
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
                        Icon { name: IconName::Reply, class: "h-3 w-3" }
                    }
                    if !is_muc {
                        {
                            let onforward = onforward;
                            let forward_id = msg_id.clone();
                            rsx! {
                                button {
                                    class: "{action_class}",
                                    title: "Forward",
                                    onclick: move |_| onforward.call(forward_id.clone()),
                                    Icon { name: IconName::Forward, class: "h-3 w-3" }
                                }
                            }
                        }
                    }
                    if quick_reaction_allowed {
                        {
                            let client = client.clone();
                            let target = target.clone();
                            let mid = msg_id.clone();
                            let mine_set = current_mine.clone();
                            let mut cs = cs.clone();
                            rsx! {
                                button {
                                    class: "{action_class}",
                                    title: "React {quick_glyph}",
                                    onclick: move |_| {
                                        do_reaction_toggle(
                                            &client,
                                            &target,
                                            &mid,
                                            quick_glyph,
                                            is_muc,
                                            mine_set.contains(&quick_glyph.to_string()),
                                            &mine_set,
                                            max_reactions,
                                        );
                                        cs.bump();
                                    },
                                    "{quick_glyph}"
                                }
                            }
                        }
                    }
                    if reactions_allowed {
                        {
                            rsx! {
                                button {
                                    class: "{action_class}",
                                    title: "Add reaction",
                                    onclick: move |_| show_reaction_picker.set(true),
                                    Icon { name: IconName::Smile, class: "h-3 w-3" }
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
                                    Icon { name: IconName::Trash, class: "h-3 w-3" }
                                }
                            }
                        }
                    }
                    if can_moderate {
                        {
                            let client = client.clone();
                            let target = target.clone();
                            let mid = msg_id.clone();
                            let mut cs = cs.clone();
                            rsx! {
                                button {
                                    class: "{action_class} hover:text-red-400",
                                    title: "Delete for everyone",
                                    onclick: move |_| {
                                        do_moderate(&client, &target, &mid);
                                        cs.bump();
                                    },
                                    Icon { name: IconName::Trash, class: "h-3 w-3" }
                                }
                            }
                        }
                    }
                }
            }
        }
        if *show_reaction_picker.read() {
            {
                let client = client.clone();
                let target = target.clone();
                let mid = msg_id.clone();
                let mine_set = current_mine.clone();
                let mut cs = cs.clone();
                rsx! {
                    EmojiPicker {
                        onpick: move |emoji: String| {
                            do_reaction_toggle(
                                &client,
                                &target,
                                &mid,
                                &emoji,
                                is_muc,
                                mine_set.contains(&emoji),
                                &mine_set,
                                max_reactions,
                            );
                            cs.bump();
                        },
                        onclose: move |_| show_reaction_picker.set(false),
                        allowlist: reaction_allowlist.clone(),
                    }
                }
            }
        }
    }
}

fn do_reaction_toggle(
    client: &XmppClient,
    target: &str,
    msg_id: &str,
    emoji: &str,
    is_muc: bool,
    mine: bool,
    current: &[String],
    max: Option<usize>,
) {
    let mut next = current.to_vec();
    if mine {
        next.retain(|item| item != emoji);
    } else if !next.iter().any(|item| item == emoji) {
        if max.is_some_and(|max| next.len() >= max) {
            return;
        }
        next.push(emoji.to_string());
    }
    let reactions: Vec<&str> = next.iter().map(String::as_str).collect();
    if is_muc {
        if let Some(room) = client.mucs.get_by_bare(target) {
            let _ = client.send_muc_reaction(&room.jid, msg_id, &reactions);
        }
    } else if let Some(chat) = client.chats.get_by_bare(target) {
        let _ = client.send_reaction(&chat.jid, msg_id, &reactions);
    }
}

fn do_retract(client: &XmppClient, target: &str, msg_id: &str) {
    if let Some(chat) = client.chats.get_by_bare(target) {
        let _ = client.send_retract(&chat.jid, msg_id);
    }
}

fn do_moderate(client: &XmppClient, target: &str, msg_id: &str) {
    if let Some(room) = client.mucs.get_by_bare(target) {
        let _ = client.send_muc_moderation(&room.jid, msg_id, None);
    }
}

/// Message status indicator
#[component]
fn MessageStatus(status: String) -> Element {
    let (icon, color) = match status.as_str() {
        "sending" => (IconName::Clock, "text-yellow-400"),
        "sent" => (IconName::Check, "text-gray-400"),
        "delivered" => (IconName::CheckCheck, "text-gray-400"),
        "read" => (IconName::CheckCheck, "text-teal-400"),
        "failed" => (IconName::CircleX, "text-red-400"),
        _ => return rsx! {},
    };

    rsx! {
        Icon { name: icon, class: "h-3 w-3 {color}" }
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
    let skin_tone = emoji::load_skin_tone(storage.as_ref(), &account);
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
    let mut show_emoji = use_signal(|| false);
    let mut show_stickers = use_signal(|| false);

    let mut cs = client_state.clone();
    let client_send = client.clone();
    let client_typing = client.clone();
    let target_typing = target.clone();
    let target_send = target.clone();
    let send_storage = storage.clone();
    let send_account = account.clone();

    let send = move || {
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
            } else {
                tracing::warn!(room = %bare, "not in this room: message not sent");
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

    let text_value = message.read().clone();
    let emoji_query = trailing_emoji_query(&text_value);
    let suggestions: Vec<&'static emoji::EmojiEntry> = emoji_query
        .as_ref()
        .map(|(_, query)| emoji::search(query, 8))
        .unwrap_or_default();
    let omemo_class = if *omemo.read() {
        "flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-teal-500/15 text-teal-400"
    } else {
        "flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-gray-400 hover:bg-gray-800 hover:text-gray-100"
    };

    rsx! {
        div { class: "border-t border-gray-800 bg-gray-900/80 p-3 md:p-4",
            if let Some((start, _)) = emoji_query.clone() {
                if !suggestions.is_empty() {
                    div { class: "mb-2 flex gap-1 overflow-x-auto",
                        for entry in suggestions {
                            {
                                let display = emoji::entry_for_tone(entry, skin_tone);
                                let glyph = display.glyph;
                                let title = entry.shortcut.clone();
                                let data_uri = display.data_uri();
                                let mut pick_message = message;
                                let start = start;
                                rsx! {
                                    button {
                                        class: "flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-gray-800 hover:bg-gray-700",
                                        title: ":{title}:",
                                        onclick: move |_| {
                                            let mut next = pick_message.read().clone();
                                            next.truncate(start);
                                            next.push_str(glyph);
                                            next.push(' ');
                                            pick_message.set(next);
                                        },
                                        if let Some(src) = data_uri {
                                            img { class: "h-6 w-6", src: "{src}", alt: "{glyph}" }
                                        } else {
                                            span { class: "text-xl", "{glyph}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "flex items-end gap-2",
                div { class: "flex min-h-[52px] flex-1 items-center gap-1 rounded-xl border border-gray-800 bg-gray-900 px-2 py-1.5 transition-colors focus-within:border-teal-500",
                    label {
                        class: "flex h-8 w-8 shrink-0 cursor-pointer items-center justify-center rounded-full border border-gray-600 text-gray-400 transition-colors hover:border-gray-500 hover:text-gray-100",
                        title: "Attach a file",
                        input {
                            r#type: "file",
                            class: "hidden",
                            multiple: true,
                            disabled: *uploading.read(),
                            onchange: on_file,
                        }
                        if *uploading.read() {
                            Icon { name: IconName::RefreshCw, class: "h-4 w-4 animate-spin" }
                        } else {
                            Icon { name: IconName::Plus, class: "h-4 w-4" }
                        }
                    }
                    textarea {
                        class: "max-h-40 min-h-[36px] flex-1 resize-none bg-transparent px-2 py-1.5 text-sm leading-5 text-gray-100 placeholder-gray-500 outline-none",
                        placeholder: "Send a message...",
                        value: "{message}",
                        oninput: on_input,
                        onkeydown: {
                            let mut send = send.clone();
                            move |e: KeyboardEvent| {
                                if e.key() == Key::Enter && !e.modifiers().shift() && !e.is_composing() {
                                    e.prevent_default();
                                    send();
                                }
                            }
                        },
                        rows: 1,
                    }
                    if *uploading.read() {
                        button {
                            class: "shrink-0 rounded px-1.5 py-1 text-[11px] text-red-400 hover:text-red-300",
                            title: "Stop uploading the remaining files",
                            onclick: move |_| upload_cancel.set(true),
                            "Cancel"
                        }
                    }
                    if !is_muc {
                        button {
                            class: "{omemo_class}",
                            title: "Toggle OMEMO encryption",
                            onclick: toggle_omemo,
                            Icon { name: IconName::Lock, class: "h-4 w-4" }
                        }
                    }
                    button {
                        class: "flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-gray-400 transition-colors hover:bg-gray-800 hover:text-gray-100",
                        title: "Send a sticker",
                        onclick: move |_| show_stickers.set(true),
                        Icon { name: IconName::Puzzle, class: "h-4 w-4" }
                    }
                    button {
                        class: "flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-gray-400 transition-colors hover:bg-gray-800 hover:text-gray-100",
                        title: "Insert emoji",
                        onclick: move |_| show_emoji.set(true),
                        Icon { name: IconName::Smile, class: "h-4 w-4" }
                    }
                }
                button {
                    class: "flex h-[52px] w-11 shrink-0 items-center justify-center rounded-lg bg-teal-800 text-white transition-colors hover:bg-teal-700 disabled:cursor-not-allowed disabled:opacity-50",
                    disabled: message.read().trim().is_empty(),
                    onclick: {
                        let mut send = send.clone();
                        move |_| send()
                    },
                    Icon { name: IconName::Send, class: "h-5 w-5" }
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
            if *show_stickers.read() {
                StickerPicker {
                    target: target.clone(),
                    is_muc,
                    onclose: move |_| show_stickers.set(false),
                }
            }
            if *show_emoji.read() {
                {
                    let mut pick_message = message;
                    rsx! {
                        EmojiPicker {
                            onpick: move |glyph: String| {
                                let mut next = pick_message.read().clone();
                                next.push_str(&glyph);
                                next.push(' ');
                                pick_message.set(next);
                            },
                            onclose: move |_| show_emoji.set(false),
                        }
                    }
                }
            }
        }
    }
}
