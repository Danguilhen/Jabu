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
    let dir = if jabu_i18n::is_rtl_tag(tag) {
        "rtl"
    } else {
        "ltr"
    };
    let tag = tag.replace(['\'', '"'], "");
    #[cfg(debug_assertions)]
    tracing::debug!("document language: {tag} ({dir})");
    let _ = dioxus::document::eval(&format!(
        "document.documentElement.lang = '{tag}'; document.documentElement.dir = '{dir}';"
    ));
}

#[cfg(test)]
mod tests {
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
