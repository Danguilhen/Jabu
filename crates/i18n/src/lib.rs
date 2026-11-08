//! Localization catalogues and runtime translator for Jabu.
//!
//! Catalogues are Fluent (`.ftl`) files embedded at compile time, so the
//! desktop, web and Android builds do not need to locate translation files at
//! runtime. The translator holds only a language tag; parsed bundles are
//! cached per thread, which keeps the type cheap to clone and usable in
//! Dioxus components.

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};

pub use fluent_bundle::FluentArgs as Args;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use unic_langid::LanguageIdentifier;

/// Locale used when the requested one is not available.
pub const FALLBACK: &str = "en";

/// A language the application ships with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LanguageInfo {
    /// BCP-47 tag, for example `fr`.
    pub tag: &'static str,
    /// Endonym shown in the language picker.
    pub name: &'static str,
}

/// Languages with a shipped catalogue. English is the source of truth.
pub const SUPPORTED: &[LanguageInfo] = &[
    LanguageInfo {
        tag: "en",
        name: "English",
    },
    LanguageInfo {
        tag: "fr",
        name: "Français",
    },
];

const EN_COMMON: &str = include_str!("../locales/en/common.ftl");
const FR_COMMON: &str = include_str!("../locales/fr/common.ftl");

fn resources(tag: &str) -> &'static [&'static str] {
    match tag {
        "fr" => &[FR_COMMON],
        _ => &[EN_COMMON],
    }
}

type Bundle = FluentBundle<FluentResource>;

thread_local! {
    static BUNDLES: RefCell<HashMap<String, Rc<Bundle>>> = RefCell::new(HashMap::new());
}

fn build_bundle(tag: &str) -> Bundle {
    let langid: LanguageIdentifier = tag.parse().unwrap_or_else(|_| {
        FALLBACK
            .parse()
            .expect("the fallback language tag is always valid")
    });
    let mut bundle = Bundle::new(vec![langid]);
    bundle.set_use_isolating(false);
    for source in resources(tag) {
        match FluentResource::try_new((*source).to_string()) {
            Ok(resource) => {
                if let Err(errors) = bundle.add_resource(resource) {
                    for error in errors {
                        tracing::warn!(%tag, %error, "duplicate or invalid Fluent message");
                    }
                }
            }
            Err((_, errors)) => {
                for error in errors {
                    tracing::warn!(%tag, %error, "invalid Fluent syntax");
                }
            }
        }
    }
    bundle
}

fn bundle(tag: &str) -> Rc<Bundle> {
    BUNDLES.with(|cell| {
        let mut bundles = cell.borrow_mut();
        if let Some(bundle) = bundles.get(tag) {
            return Rc::clone(bundle);
        }
        let built = Rc::new(build_bundle(tag));
        bundles.insert(tag.to_string(), Rc::clone(&built));
        built
    })
}

fn format(bundle: &Bundle, id: &str, args: Option<&FluentArgs>) -> Option<String> {
    let message = bundle.get_message(id)?;
    let pattern = message.value()?;
    let mut errors = Vec::new();
    let value = bundle.format_pattern(pattern, args, &mut errors);
    for error in errors {
        tracing::warn!(%id, %error, "translation formatting failed");
    }
    Some(value.into_owned())
}

/// Resolves the system locale, if the platform exposes one.
#[must_use]
pub fn system_locale() -> Option<String> {
    sys_locale::get_locale().or_else(|| std::env::var("LANG").ok())
}

/// Negotiates a requested tag against the shipped catalogues.
///
/// Exact tags win, then the base language, then English. POSIX forms such as
/// `fr_FR.UTF-8` are accepted.
#[must_use]
pub fn negotiate(requested: &str) -> String {
    let cleaned = requested
        .split(['.', '@'])
        .next()
        .unwrap_or(requested)
        .replace('_', "-");
    if let Ok(langid) = cleaned.parse::<LanguageIdentifier>() {
        let exact = langid.to_string();
        if let Some(language) = supported(&exact) {
            return language.to_string();
        }
        let base = langid.language.to_string();
        if let Some(language) = supported(&base) {
            return language.to_string();
        }
    }
    FALLBACK.to_string()
}

/// Resolves a stored preference (`system` or a tag) to a shipped tag.
#[must_use]
pub fn resolve(preference: &str) -> String {
    let requested = if preference.trim().is_empty() || preference.eq_ignore_ascii_case("system") {
        system_locale().unwrap_or_else(|| FALLBACK.to_string())
    } else {
        preference.to_string()
    };
    negotiate(&requested)
}

fn supported(tag: &str) -> Option<&'static str> {
    SUPPORTED
        .iter()
        .find(|language| language.tag.eq_ignore_ascii_case(tag))
        .map(|language| language.tag)
}

/// Whether text in this language is written right to left.
#[must_use]
pub fn is_rtl_tag(tag: &str) -> bool {
    let language = tag
        .split(['-', '_'])
        .next()
        .unwrap_or(tag)
        .to_ascii_lowercase();
    matches!(language.as_str(), "ar" | "he" | "fa" | "ur")
}

/// A translator for one resolved language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Translator {
    tag: String,
}

impl Translator {
    /// Creates a translator. Unknown tags fall back to English.
    #[must_use]
    pub fn for_tag(tag: &str) -> Self {
        Self {
            tag: negotiate(tag),
        }
    }

    /// The resolved BCP-47 tag.
    #[must_use]
    pub fn tag(&self) -> &str {
        &self.tag
    }

    /// Whether this language is right to left.
    #[must_use]
    pub fn is_rtl(&self) -> bool {
        is_rtl_tag(&self.tag)
    }

    /// Translates a message without arguments.
    ///
    /// Missing translations fall back to English, then to `[[id]]`.
    #[must_use]
    pub fn t(&self, id: &str) -> String {
        self.format(id, None)
    }

    /// Translates a message with Fluent arguments.
    #[must_use]
    pub fn t_args(&self, id: &str, args: &FluentArgs) -> String {
        self.format(id, Some(args))
    }

    /// Translates a counted message using `$count`.
    #[must_use]
    pub fn tn(&self, id: &str, count: i64) -> String {
        let mut args = FluentArgs::new();
        args.set("count", count);
        self.format(id, Some(&args))
    }

    fn format(&self, id: &str, args: Option<&FluentArgs>) -> String {
        let primary = bundle(&self.tag);
        if let Some(value) = format(&primary, id, args) {
            return value;
        }
        if self.tag != FALLBACK {
            let fallback = bundle(FALLBACK);
            if let Some(value) = format(&fallback, id, args) {
                return value;
            }
        }
        tracing::warn!(%id, tag = %self.tag, "missing translation");
        format!("[[{id}]]")
    }
}

impl Default for Translator {
    fn default() -> Self {
        Self::for_tag(FALLBACK)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fluent_syntax::ast::Entry;
    use std::collections::BTreeSet;

    fn ids(tag: &str) -> BTreeSet<String> {
        let mut ids = BTreeSet::new();
        for source in resources(tag) {
            let parsed = fluent_syntax::parser::parse(*source).expect("catalogue parses");
            for entry in parsed.body {
                if let Entry::Message(message) = entry {
                    ids.insert(message.id.name.to_string());
                }
            }
        }
        ids
    }

    #[test]
    fn shipped_catalogues_cover_every_english_key() {
        let english = ids(FALLBACK);
        assert!(!english.is_empty());
        for language in SUPPORTED.iter().filter(|language| language.tag != FALLBACK) {
            assert_eq!(
                ids(language.tag),
                english,
                "{} is missing keys or has extra ones",
                language.tag
            );
        }
    }

    #[test]
    fn bundles_load_without_syntax_errors() {
        for language in SUPPORTED {
            let mut bundle = Bundle::new(vec![language.tag.parse().expect("tag")]);
            for source in resources(language.tag) {
                let resource =
                    FluentResource::try_new((*source).to_string()).expect("catalogue parses");
                bundle
                    .add_resource(resource)
                    .unwrap_or_else(|errors| panic!("{}: {errors:?}", language.tag));
            }
        }
    }

    #[test]
    fn missing_language_falls_back_to_english() {
        assert_eq!(negotiate("de-DE"), "en");
        assert_eq!(negotiate("fr-CA"), "fr");
        assert_eq!(negotiate("fr_FR.UTF-8"), "fr");
        assert_eq!(negotiate("en-GB"), "en");
        assert_eq!(resolve("system"), resolve("system"));
    }

    #[test]
    fn translates_and_pluralizes() {
        let english = Translator::for_tag("en");
        assert_eq!(english.t("common-close"), "Close");
        assert_eq!(english.tn("chat-unread", 1), "1 unread message");
        assert_eq!(english.tn("chat-unread", 3), "3 unread messages");

        let french = Translator::for_tag("fr");
        assert_eq!(french.t("common-close"), "Fermer");
        assert_eq!(french.tn("chat-unread", 0), "0 message non lu");
        assert_eq!(french.tn("chat-unread", 1), "1 message non lu");
        assert_eq!(french.tn("chat-unread", 2), "2 messages non lus");
    }

    #[test]
    fn missing_key_falls_back_then_marks_the_id() {
        let french = Translator::for_tag("fr");
        assert_eq!(french.t("common-attach"), "Joindre un fichier");
        assert_eq!(french.t("does-not-exist"), "[[does-not-exist]]");
    }

    #[test]
    fn unknown_tags_resolve_to_english() {
        assert_eq!(Translator::for_tag("xx").tag(), "en");
        assert_eq!(Translator::for_tag("fr").tag(), "fr");
    }

    #[test]
    fn rtl_detection_covers_common_scripts() {
        assert!(is_rtl_tag("ar"));
        assert!(is_rtl_tag("ar-EG"));
        assert!(is_rtl_tag("he"));
        assert!(!is_rtl_tag("fr"));
        assert!(!is_rtl_tag("en-US"));
    }
}
