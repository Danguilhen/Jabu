use std::sync::Arc;

use dioxus::prelude::*;
use storage::StorageBackend;
use xmpp_core::XmppClient;

use crate::icons::{Icon, IconName};
use crate::sticker_packs::{self, CustomSticker};

#[component]
pub fn StickerPicker(target: String, is_muc: bool, onclose: EventHandler<()>) -> Element {
    let client = use_context::<XmppClient>();
    let storage = use_context::<Arc<dyn StorageBackend>>();
    let refresh = use_signal(|| 0u32);
    let error = use_signal(|| Option::<String>::None);
    let _ = refresh.read();
    let packs = sticker_packs::load_packs(storage.as_ref());
    let mut active_pack = use_signal(|| 0usize);
    let stickers: Vec<CustomSticker> = packs
        .get(*active_pack.read())
        .map(|pack| pack.stickers.clone())
        .unwrap_or_default();

    let send_client = client.clone();
    let send_target = target.clone();
    let send = move |sticker: CustomSticker| {
        let client = send_client.clone();
        let target = send_target.clone();
        let mut error = error;
        let Some((mime, bytes)) = sticker_packs::decode_data_uri(&sticker.data_uri) else {
            error.set(Some("Invalid sticker data".into()));
            return;
        };
        let result = if is_muc {
            client
                .mucs
                .get_by_bare(&target)
                .map(|room| {
                    let nick = room.self_nick().unwrap_or_else(|| "me".to_string());
                    client
                        .send_muc_sticker(&room.jid, &nick, &bytes, &mime, &sticker.name)
                        .map(|_| ())
                })
                .unwrap_or_else(|| Err(xmpp_core::XmppError::NotConnected))
        } else {
            client
                .chats
                .get_by_bare(&target)
                .map(|chat| {
                    client
                        .send_sticker(&chat.jid, &bytes, &mime, &sticker.name)
                        .map(|_| ())
                })
                .unwrap_or_else(|| Err(xmpp_core::XmppError::NotConnected))
        };
        match result {
            Ok(_) => onclose.call(()),
            Err(error_value) => error.set(Some(error_value.to_string())),
        }
    };

    rsx! {
        div {
            class: "fixed inset-0 z-[60] flex items-center justify-center bg-black/60 p-4",
            onclick: move |_| onclose.call(()),
            div {
                class: "flex h-[30rem] w-full max-w-[28rem] flex-col overflow-hidden rounded-2xl border border-gray-800 bg-gray-900 shadow-panel",
                onclick: move |event| event.stop_propagation(),
                div { class: "flex items-center justify-between border-b border-gray-800 px-4 py-3",
                    h3 { class: "text-sm font-semibold", "Stickers" }
                    div { class: "flex items-center gap-3",
                        label {
                            class: "cursor-pointer text-[11px] text-gray-400 hover:text-white",
                            title: "Import a sticker pack (.zip)",
                            "Import"
                            input {
                                class: "hidden",
                                id: "sticker-pack-import",
                                r#type: "file",
                                accept: ".zip,application/zip",
                                onchange: {
                                    let storage = storage.clone();
                                    move |event| {
                                        let Some(file) = event.files().into_iter().next() else {
                                            return;
                                        };
                                        let name = file.name();
                                        let pack_name = name.strip_suffix(".zip").unwrap_or(&name).to_string();
                                        let storage = storage.clone();
                                        let mut refresh = refresh;
                                        let mut error = error;
                                        spawn(async move {
                                            match file.read_bytes().await {
                                                Ok(bytes) => match sticker_packs::import_zip(&bytes, &pack_name)
                                                    .and_then(|pack| {
                                                        sticker_packs::add_pack(storage.as_ref(), pack)
                                                            .map_err(|error| error.to_string())
                                                    }) {
                                                    Ok(()) => {
                                                        refresh += 1;
                                                        error.set(None);
                                                    }
                                                    Err(message) => error.set(Some(message)),
                                                },
                                                Err(message) => error.set(Some(message.to_string())),
                                            }
                                        });
                                    }
                                },
                            }
                        }
                        button {
                            class: "text-gray-400 hover:text-white",
                            title: "Close",
                            onclick: move |_| onclose.call(()),
                            Icon { name: IconName::X, class: "h-4 w-4" }
                        }
                    }
                }
                if let Some(message) = error.read().as_ref() {
                    p { class: "px-4 pt-2 text-xs text-red-400", "{message}" }
                }
                if packs.is_empty() {
                    div { class: "flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center",
                        p { class: "text-sm text-gray-400", "No sticker packs yet." }
                        p { class: "text-xs text-gray-500",
                            "Add a ZIP of images or a Movim sticker pack with meta.json."
                        }
                        label {
                            class: "btn-primary mt-2 cursor-pointer",
                            r#for: "sticker-pack-import",
                            "Import sticker pack"
                        }
                    }
                } else {
                    div { class: "flex gap-1 overflow-x-auto border-b border-gray-800 px-3 py-2",
                        for (index, pack) in packs.iter().enumerate() {
                            {
                                let selected = *active_pack.read() == index;
                                let class = if selected {
                                    "flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-gray-800 ring-1 ring-teal-400"
                                } else {
                                    "flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-gray-800 hover:bg-gray-700"
                                };
                                let preview = pack.stickers.first().map(|sticker| sticker.data_uri.clone());
                                rsx! {
                                    button {
                                        class: "{class}",
                                        title: "{pack.name}",
                                        onclick: move |_| active_pack.set(index),
                                        if let Some(src) = preview {
                                            img { class: "h-7 w-7 object-contain", src: "{src}", alt: "{pack.name}" }
                                        } else {
                                            span { class: "text-xs", "?" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "grid flex-1 grid-cols-4 content-start gap-2 overflow-y-auto p-3",
                        for sticker in stickers {
                            {
                                let name = sticker.name.clone();
                                let data_uri = sticker.data_uri.clone();
                                let sticker = sticker.clone();
                                let send = send.clone();
                                rsx! {
                                    button {
                                        class: "flex h-20 items-center justify-center rounded-xl bg-gray-800 p-2 hover:bg-gray-700",
                                        title: "{name}",
                                        onclick: move |_| send(sticker.clone()),
                                        img { class: "max-h-16 max-w-full object-contain", src: "{data_uri}", alt: "{name}" }
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
