// SPDX-License-Identifier: MIT

use std::sync::Arc;

use dioxus::prelude::*;
use jabu_gif::{GifError, GifItem, GifPage, GifProvider, GifQuery, GifTag, WeftProvider};
use storage::StorageBackend;

use crate::icons::{Icon, IconName};
use crate::locale;

const NODE_KEY: &str = "gif.weft.node";

/// The public Weft picker. The provider stays behind the `GifProvider` trait so
/// another source can replace it without changing the composer.
#[component]
pub fn GifPicker(
    target: String,
    is_muc: bool,
    onclose: EventHandler<()>,
    on_sent: EventHandler<()>,
) -> Element {
    let i18n = locale::use_i18n();
    let storage = use_context::<Arc<dyn StorageBackend>>();
    let client = crate::active_client();
    let initial_node = load_node(storage.as_ref());
    let account_domain = client.connection.jid().map(|jid| jid.domain().to_string());
    let mut node_url = use_signal(|| initial_node.clone().unwrap_or_default());
    let mut editing_node = use_signal(|| initial_node.is_none());
    let mut discovering = use_signal(|| initial_node.is_none() && account_domain.is_some());
    let mut discovered = use_signal(|| false);
    let mut discovery_failed = use_signal(|| false);
    let mut provider = use_signal(|| Option::<Arc<WeftProvider>>::None);
    let mut node_name = use_signal(|| Option::<String>::None);
    let mut items = use_signal(Vec::<GifItem>::new);
    let mut tags = use_signal(Vec::<GifTag>::new);
    let mut query = use_signal(String::new);
    let mut cursor = use_signal(|| Option::<String>::None);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| Option::<String>::None);
    let mut sending = use_signal(|| Option::<String>::None);

    let mut load = move |url: String| {
        loading.set(true);
        error.set(None);
        items.set(Vec::new());
        cursor.set(None);
        spawn(async move {
            match connect_provider(url).await {
                Ok((new_provider, new_tags, page)) => {
                    node_name.set(Some(new_provider.name().to_string()));
                    provider.set(Some(new_provider));
                    tags.set(new_tags);
                    items.set(page.items);
                    cursor.set(page.next_cursor);
                }
                Err(failure) => error.set(Some(error_text(&i18n, failure))),
            }
            loading.set(false);
        });
    };

    use_future(move || {
        let url = initial_node.clone();
        let domain = account_domain.clone();
        async move {
            let url = match (url, domain) {
                (Some(url), _) => Some(url),
                (None, Some(domain)) => match jabu_gif::discover_from_domain(&domain).await {
                    Ok(document) => {
                        discovered.set(true);
                        node_url.set(document.node.clone());
                        editing_node.set(false);
                        Some(document.node)
                    }
                    Err(_) => {
                        discovery_failed.set(true);
                        discovering.set(false);
                        editing_node.set(true);
                        None
                    }
                },
                (None, None) => None,
            };
            if let Some(url) = url {
                loading.set(true);
                match connect_provider(url).await {
                    Ok((new_provider, new_tags, page)) => {
                        node_name.set(Some(new_provider.name().to_string()));
                        provider.set(Some(new_provider));
                        tags.set(new_tags);
                        items.set(page.items);
                        cursor.set(page.next_cursor);
                    }
                    Err(failure) => error.set(Some(error_text(&i18n, failure))),
                }
                loading.set(false);
            }
            discovering.set(false);
        }
    });

    let search = move |text: String| {
        let Some(provider) = provider.read().clone() else {
            return;
        };
        loading.set(true);
        error.set(None);
        items.set(Vec::new());
        cursor.set(None);
        spawn(async move {
            let result = if text.trim().is_empty() {
                provider.trending(GifQuery::default().with_limit(24)).await
            } else {
                provider.search(GifQuery::new(text).with_limit(24)).await
            };
            match result {
                Ok(page) => {
                    items.set(page.items);
                    cursor.set(page.next_cursor);
                }
                Err(failure) => error.set(Some(error_text(&i18n, failure))),
            }
            loading.set(false);
        });
    };

    let load_more = move |_| {
        let Some(provider) = provider.read().clone() else {
            return;
        };
        let Some(next) = cursor.read().clone() else {
            return;
        };
        let text = query.read().clone();
        loading.set(true);
        error.set(None);
        spawn(async move {
            let request = if text.trim().is_empty() {
                GifQuery::default()
            } else {
                GifQuery::new(text)
            }
            .with_limit(24)
            .with_cursor(next);
            let result = if request.text.trim().is_empty() {
                provider.trending(request).await
            } else {
                provider.search(request).await
            };
            match result {
                Ok(page) => {
                    let mut current = items.read().clone();
                    current.extend(page.items);
                    items.set(current);
                    cursor.set(page.next_cursor);
                }
                Err(failure) => error.set(Some(error_text(&i18n, failure))),
            }
            loading.set(false);
        });
    };

    let save_node = {
        let storage = storage.clone();
        move |_| {
            let url = node_url.read().trim().to_string();
            if url.is_empty() {
                return;
            }
            if let Err(failure) = storage.set_setting(NODE_KEY, &serde_json::json!(url)) {
                error.set(Some(failure.to_string()));
                return;
            }
            editing_node.set(false);
            discovered.set(false);
            discovery_failed.set(false);
            load(url);
        }
    };

    let send = {
        let client = client.clone();
        let target = target.clone();
        move |item: GifItem| {
            let Some(provider) = provider.read().clone() else {
                return;
            };
            if sending.read().is_some() {
                return;
            }
            sending.set(Some(item.id.clone()));
            error.set(None);
            let client = client.clone();
            let target = target.clone();
            spawn(async move {
                let result = send_item(&client, &provider, &item, &target, is_muc).await;
                match result {
                    Ok(()) => {
                        sending.set(None);
                        on_sent.call(());
                        onclose.call(());
                        return;
                    }
                    Err(failure) => error.set(Some(failure)),
                }
                sending.set(None);
            });
        }
    };

    let provider_name = node_name.read().clone();
    let current_query = query.read().clone();
    let is_loading = *loading.read();
    let sending_id = sending.read().clone();
    let has_more = cursor.read().is_some();

    rsx! {
        div {
            class: "fixed inset-0 z-[60] flex items-center justify-center bg-black/60 p-4",
            onclick: move |_| onclose.call(()),
            div {
                class: "flex h-[34rem] w-full max-w-[44rem] flex-col overflow-hidden rounded-2xl border border-gray-800 bg-gray-900 shadow-panel",
                onclick: move |event| event.stop_propagation(),
                div { class: "flex items-center justify-between border-b border-gray-800 px-4 py-3",
                    div { class: "flex min-w-0 items-center gap-2",
                        h3 { class: "text-sm font-semibold", {i18n.t("gif-title")} }
                        if let Some(name) = provider_name.clone() {
                            span { class: "truncate text-[11px] text-gray-500", "{name}" }
                        } else if *discovered.read() {
                            span { class: "truncate text-[11px] text-teal-400", {i18n.t("gif-discovered")} }
                        }
                    }
                    div { class: "flex items-center gap-2",
                        button {
                            class: "flex h-8 w-8 items-center justify-center rounded-lg text-gray-400 hover:bg-gray-800 hover:text-gray-100",
                            title: {i18n.t("gif-node")},
                            "aria-label": {i18n.t("gif-node")},
                            onclick: move |_| {
                                let next = !*editing_node.read();
                                editing_node.set(next);
                            },
                            Icon { name: IconName::Setting, class: "h-4 w-4" }
                        }
                        button {
                            class: "flex h-8 w-8 items-center justify-center rounded-lg text-gray-400 hover:bg-gray-800 hover:text-gray-100",
                            title: {i18n.t("common-close")},
                            "aria-label": {i18n.t("common-close")},
                            onclick: move |_| onclose.call(()),
                            Icon { name: IconName::X, class: "h-4 w-4" }
                        }
                    }
                }
                if *discovering.read() {
                    div { class: "flex flex-1 flex-col items-center justify-center gap-3 p-8 text-center",
                        Icon { name: IconName::RefreshCw, class: "h-6 w-6 animate-spin text-teal-400" }
                        p { class: "text-sm text-gray-400", role: "status", {i18n.t("gif-discovering")} }
                    }
                } else if *editing_node.read() || provider.read().is_none() {
                    div { class: "flex flex-1 flex-col items-center justify-center gap-3 p-8 text-center",
                        if *discovery_failed.read() {
                            p { class: "text-sm text-amber-300", {i18n.t("gif-discovery-failed")} }
                        }
                        p { class: "text-sm text-gray-300", {i18n.t("gif-node-hint")} }
                        input {
                            class: "w-full max-w-md rounded-lg border border-gray-700 bg-gray-950 px-3 py-2 text-sm text-gray-100 outline-none focus:border-teal-500",
                            r#type: "url",
                            inputmode: "url",
                            placeholder: "https://gifs.example",
                            value: "{node_url}",
                            oninput: move |event| node_url.set(event.value()),
                            onkeydown: {
                                let mut save_node = save_node.clone();
                                move |event: KeyboardEvent| {
                                    if event.key() == Key::Enter {
                                        event.prevent_default();
                                        save_node(());
                                    }
                                }
                            },
                        }
                        button {
                            class: "btn-primary disabled:opacity-50",
                            disabled: node_url.read().trim().is_empty(),
                            onclick: {
                                let mut save_node = save_node.clone();
                                move |_| save_node(())
                            },
                            {i18n.t("gif-save-node")}
                        }
                        if let Some(message) = error.read().as_ref() {
                            p { class: "text-xs text-red-400", role: "alert", "{message}" }
                        }
                    }
                } else {
                    div { class: "flex items-center gap-2 border-b border-gray-800 px-3 py-2",
                        input {
                            class: "min-w-0 flex-1 rounded-lg border border-gray-700 bg-gray-950 px-3 py-2 text-sm text-gray-100 outline-none focus:border-teal-500",
                            r#type: "search",
                            placeholder: {i18n.t("gif-search-placeholder")},
                            value: "{current_query}",
                            oninput: move |event| query.set(event.value()),
                            onkeydown: {
                                let mut search = search.clone();
                                move |event: KeyboardEvent| {
                                    if event.key() == Key::Enter {
                                        event.prevent_default();
                                        search(query.read().clone());
                                    }
                                }
                            },
                        }
                        button {
                            class: "btn-primary shrink-0",
                            disabled: is_loading,
                            onclick: {
                                let mut search = search.clone();
                                move |_| search(query.read().clone())
                            },
                            Icon { name: IconName::Search, class: "h-4 w-4" }
                        }
                    }
                    if !tags.read().is_empty() {
                        div { class: "flex gap-1 overflow-x-auto border-b border-gray-800 px-3 py-2",
                            for tag in tags.read().iter().cloned() {
                                {
                                    let label = tag.tag.clone();
                                    let mut search = search.clone();
                                    rsx! {
                                        button {
                                            class: "shrink-0 rounded-full bg-gray-800 px-2.5 py-1 text-[11px] text-gray-300 hover:bg-gray-700",
                                            onclick: move |_| {
                                                query.set(label.clone());
                                                search(label.clone());
                                            },
                                            "#{tag.tag}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if let Some(message) = error.read().as_ref() {
                        p { class: "px-4 pt-2 text-xs text-red-400", role: "alert", "{message}" }
                    }
                    if is_loading && items.read().is_empty() {
                        p { class: "p-6 text-center text-sm text-gray-500", role: "status", {i18n.t("gif-loading")} }
                    } else if !is_loading && items.read().is_empty() && error.read().is_none() {
                        p { class: "p-6 text-center text-sm text-gray-500", {i18n.t("gif-empty")} }
                    } else {
                        div { class: "grid flex-1 grid-cols-3 content-start gap-2 overflow-y-auto p-3 sm:grid-cols-4",
                            for item in items.read().iter().cloned() {
                                {
                                    let item_for_send = item.clone();
                                    let mut send = send.clone();
                                    let is_sending = sending_id.as_deref() == Some(item.id.as_str());
                                    rsx! {
                                        button {
                                            class: "group flex min-w-0 flex-col gap-1 text-start disabled:opacity-60",
                                            disabled: sending_id.is_some(),
                                            title: "{item.title}",
                                            onclick: move |_| send(item_for_send.clone()),
                                            div { class: "relative aspect-square w-full overflow-hidden rounded-lg bg-gray-800",
                                                img {
                                                    class: "h-full w-full object-cover transition-transform group-hover:scale-[1.02]",
                                                    src: "{item.preview_url}",
                                                    alt: "{item.title}",
                                                    loading: "lazy",
                                                }
                                                if item.mime == "image/gif" {
                                                    span { class: "absolute left-1 top-1 rounded bg-black/70 px-1.5 py-0.5 text-[10px] font-semibold text-white", "GIF" }
                                                }
                                                if is_sending {
                                                    div { class: "absolute inset-0 flex items-center justify-center bg-black/60",
                                                        Icon { name: IconName::RefreshCw, class: "h-5 w-5 animate-spin text-white" }
                                                    }
                                                }
                                            }
                                            span { class: "truncate text-[11px] text-gray-300", "{item.title}" }
                                            if let Some(spdx) = item.licence.spdx.clone() {
                                                span { class: "truncate text-[10px] text-gray-500", "{spdx}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if has_more {
                            div { class: "border-t border-gray-800 p-2 text-center",
                                button {
                                    class: "text-xs text-teal-400 hover:text-teal-300 disabled:opacity-50",
                                    disabled: is_loading,
                                    onclick: load_more,
                                    {i18n.t("gif-load-more")}
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn connect_provider(
    url: String,
) -> Result<(Arc<WeftProvider>, Vec<GifTag>, GifPage), GifError> {
    let provider = Arc::new(WeftProvider::connect(url).await?);
    let tags = provider.tags().await.unwrap_or_default();
    let page = provider
        .trending(GifQuery::default().with_limit(24))
        .await?;
    Ok((provider, tags, page))
}

async fn send_item(
    client: &xmpp_core::XmppClient,
    provider: &WeftProvider,
    item: &GifItem,
    target: &str,
    is_muc: bool,
) -> Result<(), String> {
    let asset = provider
        .fetch(item)
        .await
        .map_err(|failure| error_text(&locale::Translator::for_tag("en"), failure))?;
    let size = asset.bytes.len() as u64;
    let duration = asset.duration_ms.map(|ms| ms as f64 / 1000.0);
    let slot = client
        .upload_file(&asset.filename, asset.bytes, &asset.mime)
        .await
        .map_err(|failure| failure.to_string())?;
    if is_muc {
        let room = client
            .mucs
            .get_by_bare(target)
            .ok_or_else(|| "Not in this room".to_string())?;
        let nick = room.self_nick().unwrap_or_else(|| "me".to_string());
        client
            .send_muc_file_message_with_metadata(
                &room.jid,
                &nick,
                &slot.get_url,
                &asset.filename,
                &asset.mime,
                size,
                asset.width,
                asset.height,
                duration,
            )
            .map(|_| ())
            .map_err(|failure| failure.to_string())
    } else {
        let chat = client
            .chats
            .get_by_bare(target)
            .ok_or_else(|| "Chat not found".to_string())?;
        client
            .send_file_message_with_metadata(
                &chat.jid,
                &slot.get_url,
                &asset.filename,
                &asset.mime,
                size,
                asset.width,
                asset.height,
                duration,
            )
            .map(|_| ())
            .map_err(|failure| failure.to_string())
    }
}

fn load_node(storage: &dyn StorageBackend) -> Option<String> {
    storage
        .get_setting(NODE_KEY)
        .ok()
        .flatten()
        .and_then(|value| value.as_str().map(str::to_string))
        .filter(|value| !value.trim().is_empty())
        .or_else(|| option_env!("JABU_WEFT_NODE").map(str::to_string))
}

fn error_text(i18n: &locale::Translator, error: GifError) -> String {
    match error {
        GifError::RateLimited { retry_after } => match retry_after {
            Some(seconds) => format!("{} ({seconds}s)", i18n.t("gif-rate-limited")),
            None => i18n.t("gif-rate-limited"),
        },
        GifError::NotFound => i18n.t("gif-not-found"),
        GifError::NotConfigured => i18n.t("gif-node-hint"),
        GifError::Unavailable(detail) => format!("{}: {detail}", i18n.t("gif-unavailable")),
        GifError::InvalidItem(detail) => format!("{}: {detail}", i18n.t("gif-error")),
        GifError::Provider(detail) => format!("{}: {detail}", i18n.t("gif-error")),
    }
}
