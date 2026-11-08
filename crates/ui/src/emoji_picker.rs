use std::collections::HashSet;
use std::sync::Arc;

use dioxus::prelude::*;
use storage::StorageBackend;

use crate::emoji::{self, EmojiEntry};
use crate::emoji_packs::{self, CustomEmoji};
use crate::icons::{Icon, IconName};
use crate::locale;
use crate::locale::Translator;

fn skin_tone_key(index: usize) -> &'static str {
    match index {
        0 => "emoji-tone-light",
        1 => "emoji-tone-medium-light",
        2 => "emoji-tone-medium",
        3 => "emoji-tone-medium-dark",
        _ => "emoji-tone-dark",
    }
}

fn group_label(i18n: &Translator, group: &str) -> String {
    let key = match group {
        "Recent" => "emoji-recent",
        "Favorites" => "emoji-favorites",
        "Custom" => "emoji-custom",
        "Smileys & Emotion" => "emoji-group-smileys",
        "People & Body" => "emoji-group-people",
        "Animals & Nature" => "emoji-group-animals",
        "Food & Drink" => "emoji-group-food",
        "Travel & Places" => "emoji-group-travel",
        "Activities" => "emoji-group-activities",
        "Objects" => "emoji-group-objects",
        "Symbols" => "emoji-group-symbols",
        "Flags" => "emoji-group-flags",
        "Component" => "emoji-group-component",
        "Skin Tones" => "emoji-group-skin-tones",
        _ => return group.to_string(),
    };
    i18n.t(key)
}

#[component]
pub fn EmojiPicker(
    onpick: EventHandler<String>,
    onclose: EventHandler<()>,
    allowlist: Option<Vec<String>>,
) -> Element {
    let i18n = locale::use_i18n();
    let client = crate::active_client();
    let storage = use_context::<Arc<dyn StorageBackend>>();
    let account = client
        .connection
        .jid()
        .map(|jid| jid.to_bare().to_string())
        .unwrap_or_default();
    let mut query = use_signal(String::new);
    let refresh = use_signal(|| 0u32);
    let import_error = use_signal(|| Option::<String>::None);
    let mut manage = use_signal(|| false);
    let mut edit_favorites = use_signal(|| false);
    let skin_tone = use_signal(|| emoji::load_skin_tone(storage.as_ref(), &account));
    let _ = refresh.read();
    let packs = emoji_packs::load_packs(storage.as_ref());
    let recent = emoji::load_recent(storage.as_ref(), &account);
    let favorites = emoji::load_favorites(storage.as_ref(), &account);
    let favorites_set: HashSet<String> = favorites.iter().cloned().collect();
    let tone = *skin_tone.read();
    let allowlist_ref = allowlist.as_ref();
    let groups = emoji::groups();
    let initial_group = if recent.is_empty() {
        groups.first().copied().unwrap_or("Recent").to_string()
    } else {
        "Recent".to_string()
    };
    let mut active_group = use_signal(|| initial_group);
    let needle = query.read().trim().to_string();
    let custom_mode = active_group.read().as_str() == "Custom";
    let custom_entries: Vec<CustomEmoji> = if custom_mode {
        packs
            .iter()
            .flat_map(|pack| pack.emojis.iter().cloned())
            .filter(|entry| {
                needle.is_empty()
                    || entry.alias.contains(&needle.to_lowercase())
                    || entry.name.to_lowercase().contains(&needle.to_lowercase())
            })
            .filter(|entry| {
                allowlist_ref.is_none_or(|allowed| {
                    allowed.iter().any(|item| {
                        item == &entry.alias
                            || item == &entry.name
                            || item == &format!(":{}:", entry.alias)
                    })
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    let mut entries: Vec<&'static EmojiEntry> = if custom_mode {
        Vec::new()
    } else if !needle.is_empty() {
        emoji::search(&needle, 240)
    } else if active_group.read().as_str() == "Recent" {
        recent
            .iter()
            .filter_map(|glyph| emoji::lookup_glyph(glyph))
            .collect()
    } else if active_group.read().as_str() == "Favorites" {
        favorites
            .iter()
            .filter_map(|glyph| emoji::lookup_glyph(glyph))
            .collect()
    } else {
        let group = active_group.read().clone();
        emoji::catalog()
            .iter()
            .filter(|entry| entry.group == group)
            .collect()
    };
    if let Some(allowed) = allowlist_ref {
        entries.retain(|entry| {
            allowed.iter().any(|item| {
                item == entry.glyph
                    || emoji::variants_for(entry.glyph)
                        .iter()
                        .any(|variant| variant.glyph == item.as_str())
            })
        });
    }
    rsx! {
        div {
            class: "fixed inset-0 z-[60] flex items-center justify-center bg-black/60 p-4",
            onclick: move |_| onclose.call(()),
            div {
                class: "flex h-[30rem] w-full max-w-[28rem] flex-col overflow-hidden rounded-2xl border border-gray-800 bg-gray-900 shadow-panel",
                onclick: move |event| event.stop_propagation(),
                div { class: "flex items-center justify-between border-b border-gray-800 px-4 py-3",
                    h3 { class: "text-sm font-semibold", {i18n.t("emoji-title")} }
                    div { class: "flex items-center gap-3",
                        button {
                            class: if *edit_favorites.read() { "inline-flex items-center gap-1 text-[11px] text-amber-300" } else { "inline-flex items-center gap-1 text-[11px] text-gray-400 hover:text-gray-100" },
                            title: {i18n.t("emoji-favorite")},
                            onclick: move |_| edit_favorites.toggle(),
                            if *edit_favorites.read() {
                                {i18n.t("common-done")}
                            } else {
                                Icon { name: IconName::Star, class: "h-3 w-3" }
                                span { {i18n.t("emoji-favorites")} }
                            }
                        }
                        if !packs.is_empty() {
                            button {
                                class: if *manage.read() { "text-[11px] text-teal-300" } else { "text-[11px] text-gray-400 hover:text-gray-100" },
                                onclick: move |_| manage.toggle(),
                                {i18n.t("emoji-manage")}
                            }
                        }
                        label {
                            class: "cursor-pointer text-[11px] text-gray-400 hover:text-gray-100",
                            title: {i18n.t("emoji-import")},
                            {i18n.t("common-import")}
                            input {
                                class: "hidden",
                                r#type: "file",
                                accept: ".zip,application/zip",
                                onchange: {
                                    let storage = storage.clone();
                                    move |event| {
                                        let Some(file) = event.files().into_iter().next() else {
                                            return;
                                        };
                                        let name = file.name();
                                        let pack_name = name
                                            .strip_suffix(".zip")
                                            .unwrap_or(&name)
                                            .to_string();
                                        let storage = storage.clone();
                                        let mut refresh = refresh;
                                        let mut import_error = import_error;
                                        spawn(async move {
                                            match file.read_bytes().await {
                                                Ok(bytes) => {
                                                    match emoji_packs::import_zip(&bytes, &pack_name)
                                                        .and_then(|pack| {
                                                            emoji_packs::add_pack(
                                                                storage.as_ref(),
                                                                pack,
                                                            )
                                                            .map_err(|error| error.to_string())
                                                        }) {
                                                        Ok(()) => {
                                                            refresh += 1;
                                                            import_error.set(None);
                                                        }
                                                        Err(error) => import_error.set(Some(error)),
                                                    }
                                                }
                                                Err(error) => import_error.set(Some(error.to_string())),
                                            }
                                        });
                                    }
                                },
                            }
                        }
                        button {
                            class: "text-gray-400 hover:text-gray-100",
                            title: {i18n.t("common-close")},
                            onclick: move |_| onclose.call(()),
                            Icon { name: IconName::X, class: "h-4 w-4" }
                        }
                    }
                }
                if let Some(error) = import_error.read().as_ref() {
                    p { class: "px-4 pt-2 text-xs text-red-400", "{error}" }
                }
                div { class: "px-3 pt-3",
                    input {
                        class: "field",
                        r#type: "search",
                        placeholder: {i18n.t("emoji-search")},
                        autofocus: true,
                        value: "{query}",
                        oninput: move |event| query.set(event.value()),
                    }
                }
                div { class: "flex items-center gap-1 px-3 pb-2",
                    span { class: "mr-1 text-[10px] uppercase tracking-wide text-gray-500", {i18n.t("emoji-tone")} }
                    for (index, glyph) in emoji::SKIN_TONE_GLYPHS.iter().enumerate() {
                        {
                            let selected = tone == index;
                            let class = if selected {
                                "flex h-7 w-7 items-center justify-center rounded-full bg-teal-500/30 ring-1 ring-teal-400"
                            } else {
                                "flex h-7 w-7 items-center justify-center rounded-full hover:bg-gray-800"
                            };
                            let label = if index == 0 {
                                i18n.t("emoji-default-tone")
                            } else {
                                i18n.t(skin_tone_key(index - 1))
                            };
                            let mut skin_tone = skin_tone;
                            let storage = storage.clone();
                            let account = account.clone();
                            rsx! {
                                button {
                                    class: "{class}",
                                    title: "{label}",
                                    onclick: move |_| {
                                        skin_tone.set(index);
                                        emoji::save_skin_tone(storage.as_ref(), &account, index);
                                    },
                                    span { class: "text-lg", "{glyph}" }
                                }
                            }
                        }
                    }
                }
                if *edit_favorites.read() {
                    p { class: "px-3 pb-2 text-[11px] text-amber-300",
                        {i18n.t("emoji-help")}
                    }
                }
                div { class: "flex gap-1 overflow-x-auto px-3 py-2",
                    for group in std::iter::once("Recent".to_string())
                        .chain(std::iter::once("Favorites".to_string()))
                        .chain((!packs.is_empty()).then_some("Custom".to_string()))
                        .chain(groups.iter().map(|group| (*group).to_string()))
                    {
                        {
                            let selected = active_group.read().as_str() == group.as_str();
                            let class = if selected {
                                "shrink-0 rounded-lg bg-teal-500 px-2.5 py-1 text-[11px] font-semibold text-white"
                            } else {
                                "shrink-0 rounded-lg bg-gray-800 px-2.5 py-1 text-[11px] text-gray-400 hover:bg-gray-700 hover:text-gray-100"
                            };
                            rsx! {
                                button {
                                    class: "{class}",
                                    onclick: move |_| {
                                        active_group.set(group.clone());
                                        query.set(String::new());
                                    },
                                    {group_label(&i18n, &group)}
                                }
                            }
                        }
                    }
                }
                if *manage.read() {
                    div { class: "flex-1 space-y-3 overflow-y-auto px-3 pb-3",
                        for pack in packs.iter().cloned() {
                            {
                                let pack_name = pack.name.clone();
                                let storage = storage.clone();
                                let mut refresh = refresh;
                                rsx! {
                                    div { class: "rounded-xl border border-gray-800 p-2",
                                        div { class: "mb-2 flex items-center justify-between",
                                            span { class: "text-xs font-semibold", "{pack.name}" }
                                            button {
                                                class: "text-[11px] text-red-400 hover:text-red-300",
                                                onclick: move |_| {
                                                    if emoji_packs::remove_pack(storage.as_ref(), &pack_name).is_ok() {
                                                        refresh += 1;
                                                    }
                                                },
                                                {i18n.t("emoji-remove-pack")}
                                            }
                                        }
                                        for emoji in pack.emojis.iter().cloned() {
                                            {
                                                let pack_name = pack.name.clone();
                                                let emoji_name = emoji.name.clone();
                                                let alias = emoji.alias.clone();
                                                let storage = storage.clone();
                                                let mut refresh = refresh;
                                                rsx! {
                                                    div { class: "flex items-center gap-2 py-1",
                                                        img { class: "h-7 w-7 object-contain", src: "{emoji.data_uri}", alt: "{emoji.name}" }
                                                        span { class: "min-w-0 flex-1 truncate text-[11px] text-gray-400", "{emoji.name}" }
                                                        input {
                                                            class: "field h-7 w-28 px-2 text-[11px]",
                                                            value: ":{alias}:",
                                                            title: {i18n.t("emoji-alias-hint")},
                                                            onchange: move |event| {
                                                                if emoji_packs::update_alias(
                                                                    storage.as_ref(),
                                                                    &pack_name,
                                                                    &emoji_name,
                                                                    &event.value(),
                                                                )
                                                                .is_ok()
                                                                {
                                                                    refresh += 1;
                                                                }
                                                            },
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
                } else {
                    div { class: "grid flex-1 grid-cols-8 content-start gap-1 overflow-y-auto px-3 pb-3",
                    if entries.is_empty() && custom_entries.is_empty() {
                        p { class: "col-span-8 py-8 text-center text-xs text-gray-500",
                            if !needle.is_empty() {
                                {i18n.t("emoji-no-match")}
                            } else if active_group.read().as_str() == "Favorites" {
                                {i18n.t("emoji-no-favorites")}
                            } else {
                                {i18n.t("emoji-none")}
                            }
                        }
                    }
                    for entry in custom_entries {
                        {
                            let alias = entry.alias.clone();
                            let alias_label = alias.clone();
                            let name = entry.name.clone();
                            let data_uri = entry.data_uri.clone();
                            let onpick = onpick;
                            rsx! {
                                button {
                                    class: "flex h-9 w-9 items-center justify-center rounded-lg hover:bg-gray-800",
                                    title: ":{alias}:",
                                    aria_label: name,
                                    onclick: move |_| {
                                        onpick.call(format!(":{alias}:"));
                                        onclose.call(());
                                    },
                                    img { class: "h-6 w-6 object-contain", src: "{data_uri}", alt: "{alias_label}" }
                                }
                            }
                        }
                    }
                    for entry in entries {
                        {
                            let display = emoji::entry_for_tone(entry, tone);
                            let display = if allowlist_ref.is_some_and(|allowed| {
                                allowed.iter().any(|item| item == entry.glyph)
                                    && !allowed.iter().any(|item| item == display.glyph)
                            }) {
                                entry
                            } else {
                                display
                            };
                            let glyph = display.glyph;
                            let name = entry.name;
                            let shortcut = entry.shortcut.clone();
                            let data_uri = display.data_uri();
                            let storage = storage.clone();
                            let account = account.clone();
                            let onpick = onpick;
                            let mut refresh = refresh;
                            let favorite = favorites_set.contains(glyph)
                                || favorites_set.contains(entry.glyph);
                            let class = if *edit_favorites.read() && favorite {
                                "flex h-9 w-9 items-center justify-center rounded-lg bg-amber-500/20 ring-1 ring-amber-400 hover:bg-amber-500/30"
                            } else if favorite {
                                "flex h-9 w-9 items-center justify-center rounded-lg ring-1 ring-amber-400/50 hover:bg-gray-800"
                            } else {
                                "flex h-9 w-9 items-center justify-center rounded-lg hover:bg-gray-800"
                            };
                            let title = if *edit_favorites.read() {
                                let mut args = locale::Args::new();
                                args.set("shortcut", shortcut.clone());
                                i18n.t_args("emoji-toggle-favorite", &args)
                            } else {
                                format!(":{shortcut}:")
                            };
                            rsx! {
                                button {
                                    class: "{class}",
                                    title: "{title}",
                                    aria_label: name,
                                    onclick: move |_| {
                                        if *edit_favorites.read() {
                                            emoji::toggle_favorite(storage.as_ref(), &account, glyph);
                                            refresh += 1;
                                        } else {
                                            emoji::remember_recent(storage.as_ref(), &account, glyph);
                                            onpick.call(glyph.to_string());
                                            onclose.call(());
                                        }
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
        }
    }
}
