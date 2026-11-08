//! Localization catalogues and runtime translator for Jabu.
//!
//! Catalogues are Fluent (`.ftl`) files embedded at compile time, so the
//! desktop, web and Android builds do not need to locate translation files at
//! runtime. The translator holds only a language tag; parsed bundles are
//! cached per thread, which keeps the type cheap to clone and usable in
//! Dioxus components.

use chrono::{DateTime, Datelike, Timelike, Utc};
use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};

pub use fluent_bundle::FluentArgs as Args;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
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
    negotiate_tag(requested).to_string()
}

fn negotiate_tag(requested: &str) -> &'static str {
    let cleaned = requested
        .split(['.', '@'])
        .next()
        .unwrap_or(requested)
        .replace('_', "-");
    if let Ok(langid) = cleaned.parse::<LanguageIdentifier>() {
        let exact = langid.to_string();
        if let Some(language) = supported(&exact) {
            return language;
        }
        let base = langid.language.to_string();
        if let Some(language) = supported(&base) {
            return language;
        }
    }
    FALLBACK
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

/// Message ids defined by a shipped catalogue.
///
/// Used by the catalogue parity test and the `i18n-check` helper so
/// contributors can compare translations without parsing Fluent themselves.
#[must_use]
pub fn keys(tag: &str) -> BTreeSet<String> {
    let tag = negotiate(tag);
    let mut ids = BTreeSet::new();
    for source in resources(&tag) {
        let Ok(parsed) = fluent_syntax::parser::parse(*source) else {
            continue;
        };
        for entry in parsed.body {
            if let fluent_syntax::ast::Entry::Message(message) = entry {
                ids.insert(message.id.name.to_string());
            }
        }
    }
    ids
}

/// Whether a message id exists in a catalogue (or the English fallback).
#[must_use]
pub fn has_message(tag: &str, id: &str) -> bool {
    let tag = negotiate(tag);
    let primary = bundle(&tag);
    primary.get_message(id).is_some()
        || (tag != FALLBACK && bundle(FALLBACK).get_message(id).is_some())
}

const MONTHS: [&str; 12] = [
    "date-month-1",
    "date-month-2",
    "date-month-3",
    "date-month-4",
    "date-month-5",
    "date-month-6",
    "date-month-7",
    "date-month-8",
    "date-month-9",
    "date-month-10",
    "date-month-11",
    "date-month-12",
];

fn month_key(index: u32) -> &'static str {
    MONTHS
        .get(index as usize)
        .copied()
        .unwrap_or("date-month-1")
}

/// Grouping and decimal separators.
///
/// These are locale data rather than translatable prose, so they live in code.
/// Languages that use a narrow no-break space between groups share the French
/// branch.
fn number_separators(tag: &str) -> (char, char) {
    let language = tag
        .split(['-', '_'])
        .next()
        .unwrap_or(tag)
        .to_ascii_lowercase();
    match language.as_str() {
        "fr" | "de" | "es" | "it" | "pt" | "ru" | "uk" | "pl" | "cs" | "sv" | "fi" | "nb"
        | "nn" => ('\u{202f}', ','),
        _ => (',', '.'),
    }
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Translator {
    tag: &'static str,
}

impl Translator {
    /// Creates a translator. Unknown tags fall back to English.
    #[must_use]
    pub fn for_tag(tag: &str) -> Self {
        Self {
            tag: negotiate_tag(tag),
        }
    }

    /// The resolved BCP-47 tag.
    #[must_use]
    pub fn tag(&self) -> &str {
        self.tag
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

    /// Formats a date in the language's medium style, for example
    /// `Nov 10, 2026` or `10 nov. 2026`.
    #[must_use]
    pub fn date(&self, time: DateTime<Utc>) -> String {
        let month = self.t(month_key(time.month0()));
        let mut args = FluentArgs::new();
        args.set("day", time.day());
        args.set("month", month);
        args.set("year", time.year());
        self.format("date-medium", Some(&args))
    }

    /// Formats a clock time as 24-hour `HH:MM`.
    ///
    /// Minutes are padded in Rust so Fluent never sees a bare integer and
    /// cannot apply locale digit shaping to the padding.
    #[must_use]
    pub fn clock(&self, time: DateTime<Utc>) -> String {
        let mut args = FluentArgs::new();
        args.set("hour", format!("{:02}", time.hour()));
        args.set("minute", format!("{:02}", time.minute()));
        self.format("time-hm", Some(&args))
    }

    /// Formats a date followed by a clock time.
    #[must_use]
    pub fn date_time(&self, time: DateTime<Utc>) -> String {
        let mut args = FluentArgs::new();
        args.set("date", self.date(time));
        args.set("time", self.clock(time));
        self.format("date-time", Some(&args))
    }

    /// Formats an integer with this language's grouping separator.
    #[must_use]
    pub fn number(&self, value: i64) -> String {
        let (group, _) = number_separators(&self.tag);
        let negative = value < 0;
        let digits = value.unsigned_abs().to_string();
        let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
        if negative {
            out.push('-');
        }
        for (index, ch) in digits.chars().enumerate() {
            if index > 0 && (digits.len() - index) % 3 == 0 {
                out.push(group);
            }
            out.push(ch);
        }
        out
    }

    /// Formats a decimal with a fixed number of fraction digits and this
    /// language's grouping and decimal separators.
    #[must_use]
    pub fn decimal(&self, value: f64, digits: usize) -> String {
        let (group, decimal) = number_separators(&self.tag);
        let rendered = format!("{value:.digits$}");
        let (integer, fraction) = rendered.split_once('.').unwrap_or((&rendered, ""));
        let negative = integer.starts_with('-');
        let digits_only = integer.trim_start_matches('-');
        let mut out = String::new();
        if negative {
            out.push('-');
        }
        for (index, ch) in digits_only.chars().enumerate() {
            if index > 0 && (digits_only.len() - index) % 3 == 0 {
                out.push(group);
            }
            out.push(ch);
        }
        if digits > 0 {
            out.push(decimal);
            out.push_str(fraction);
        }
        out
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
    use std::collections::BTreeSet;

    fn ids(tag: &str) -> BTreeSet<String> {
        keys(tag)
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
    fn message_lookup_sees_fallbacks() {
        assert!(has_message("en", "common-close"));
        assert!(has_message("fr", "common-close"));
        assert!(!has_message("fr", "does-not-exist"));
    }

    #[test]
    fn formats_dates_and_times_per_language() {
        let stamp = chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 11, 10, 14, 5, 0).unwrap();
        let english = Translator::for_tag("en");
        assert_eq!(english.date(stamp), "Nov 10, 2026");
        assert_eq!(english.clock(stamp), "14:05");
        assert_eq!(english.date_time(stamp), "Nov 10, 2026, 14:05");

        let french = Translator::for_tag("fr");
        assert_eq!(french.date(stamp), "10 nov. 2026");
        assert_eq!(french.clock(stamp), "14:05");
        assert_eq!(french.date_time(stamp), "10 nov. 2026 à 14:05");

        let morning = chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 1, 2, 3, 4, 0).unwrap();
        assert_eq!(english.date(morning), "Jan 2, 2026");
        assert_eq!(french.date(morning), "2 janv. 2026");
        assert_eq!(english.clock(morning), "03:04");
    }

    #[test]
    fn formats_numbers_per_language() {
        let english = Translator::for_tag("en");
        assert_eq!(english.number(1_234_567), "1,234,567");
        assert_eq!(english.number(-42), "-42");
        assert_eq!(english.decimal(1234.5, 1), "1,234.5");
        assert_eq!(english.decimal(0.25, 2), "0.25");

        let french = Translator::for_tag("fr");
        assert_eq!(french.number(1_234_567), "1\u{202f}234\u{202f}567");
        assert_eq!(french.number(999), "999");
        assert_eq!(french.decimal(1234.5, 1), "1\u{202f}234,5");
        assert_eq!(french.decimal(-0.25, 2), "-0,25");
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
