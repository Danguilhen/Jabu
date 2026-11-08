//! UI bridge between the localization engine and Dioxus components.

use crate::ClientState;
use dioxus::prelude::*;
pub use jabu_i18n::{Args, LanguageInfo, Translator, SUPPORTED};

/// Translator for the active language.
///
/// Reading the language signal subscribes the calling component, so switching
/// languages re-renders every component that calls this hook.
#[must_use]
pub fn use_i18n() -> Translator {
    let state = use_context::<ClientState>();
    let tag = state.language.read().clone();
    Translator::for_tag(&tag)
}

/// Shipped languages as `(tag, endonym)` pairs.
#[must_use]
pub fn language_options() -> Vec<(&'static str, &'static str)> {
    SUPPORTED
        .iter()
        .map(|language| (language.tag, language.name))
        .collect()
}

/// Stores the preference, resolves it and applies the document language.
pub fn set_language(
    state: &mut ClientState,
    storage: &dyn storage::StorageBackend,
    preference: String,
) {
    let resolved = jabu_i18n::resolve(&preference);
    state.language_preference.set(preference.clone());
    state.language.set(resolved.clone());
    let _ = storage.set_setting("ui.language", &serde_json::json!(preference));
    apply_document_language(&resolved);
    state.bump();
}

/// Updates `<html lang>` and `dir` to match the resolved language.
pub fn apply_document_language(tag: &str) {
    let mut dir = if jabu_i18n::is_rtl_tag(tag) {
        "rtl".to_string()
    } else {
        "ltr".to_string()
    };
    #[cfg(debug_assertions)]
    if let Ok(forced) = std::env::var("JABU_DIR") {
        if forced == "rtl" || forced == "ltr" {
            dir = forced;
        }
    }
    let tag = tag.replace(['\'', '"'], "");
    #[cfg(debug_assertions)]
    tracing::debug!("document language: {tag} ({dir})");
    let _ = dioxus::document::eval(&format!(
        "document.documentElement.lang = '{tag}'; document.documentElement.dir = '{dir}';"
    ));
}

#[cfg(test)]
mod tests {
    /// Direction-sensitive layout must use logical utilities so an RTL
    /// language mirrors instead of staying LTR.
    #[test]
    fn ui_uses_logical_direction_utilities() {
        let sources = [
            include_str!("lib.rs"),
            include_str!("report.rs"),
            include_str!("emoji_picker.rs"),
            include_str!("sticker_picker.rs"),
        ];
        let banned = [
            "text-left",
            "text-right",
            "border-l-",
            "border-r-",
            "rounded-tl-",
            "rounded-tr-",
            "rounded-bl-",
            "rounded-br-",
        ];
        for source in sources {
            for needle in banned {
                assert!(
                    !source.contains(needle),
                    "use a logical class instead of {needle}"
                );
            }
            for prefix in ["ml-", "mr-", "pl-", "pr-"] {
                for (index, _) in source.match_indices(prefix) {
                    let before = source[..index].chars().next_back();
                    let logical = before.is_some_and(|ch| ch.is_alphanumeric() || ch == '-');
                    assert!(
                        logical,
                        "use a logical class instead of {prefix} near {:?}",
                        &source[index.saturating_sub(20)..(index + 20).min(source.len())]
                    );
                }
            }
        }
    }

    /// Every literal key used by the UI must exist in the English catalogue.
    #[test]
    fn used_keys_exist_in_the_catalogue() {
        let sources = [
            include_str!("lib.rs"),
            include_str!("report.rs"),
            include_str!("emoji_picker.rs"),
            include_str!("sticker_picker.rs"),
            include_str!("../../app/src/session_workers.rs"),
        ];
        let mut keys = Vec::new();
        for source in sources {
            for marker in [".t(\"", ".tn(\"", ".t_args(\""] {
                let mut rest = source;
                while let Some(index) = rest.find(marker) {
                    rest = &rest[index + marker.len()..];
                    if let Some(end) = rest.find('"') {
                        keys.push(rest[..end].to_string());
                    }
                }
            }
        }
        keys.sort();
        keys.dedup();
        assert!(!keys.is_empty());
        let missing: Vec<_> = keys
            .iter()
            .filter(|key| !jabu_i18n::has_message("en", key))
            .collect();
        assert!(missing.is_empty(), "missing catalogue keys: {missing:?}");
    }

    use super::*;

    #[test]
    fn language_options_include_english_and_french() {
        let options = language_options();
        assert!(options.iter().any(|(tag, _)| *tag == "en"));
        assert!(options
            .iter()
            .any(|(tag, name)| *tag == "fr" && *name == "Français"));
    }
}
