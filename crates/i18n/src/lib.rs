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

/// Declares the shipped catalogues in picker order.
///
/// English is the source of truth and the fallback. Add one tuple per
/// language; the catalogue is embedded at compile time.
macro_rules! catalogues {
    ($(($tag:literal, $name:literal, $path:literal)),+ $(,)?) => {
        /// Languages with a shipped catalogue.
        pub const SUPPORTED: &[LanguageInfo] = &[
            $(LanguageInfo { tag: $tag, name: $name }),+
        ];

        fn resources(tag: &str) -> &'static [&'static str] {
            match tag {
                $($tag => &[include_str!($path)],)+
                _ => &[include_str!("../locales/en/common.ftl")],
            }
        }
    };
}

catalogues! {
    ("en", "English", "../locales/en/common.ftl"),
    ("zh-Hans", "简体中文", "../locales/zh-Hans/common.ftl"),
    ("es", "Español", "../locales/es/common.ftl"),
    ("fr", "Français", "../locales/fr/common.ftl"),
    ("ar", "العربية", "../locales/ar/common.ftl"),
    ("pt-BR", "Português (Brasil)", "../locales/pt-BR/common.ftl"),
    ("ru", "Русский", "../locales/ru/common.ftl"),
    ("id", "Bahasa Indonesia", "../locales/id/common.ftl"),
    ("de", "Deutsch", "../locales/de/common.ftl"),
    ("ja", "日本語", "../locales/ja/common.ftl"),
    ("tr", "Türkçe", "../locales/tr/common.ftl"),
    ("ko", "한국어", "../locales/ko/common.ftl"),
    ("it", "Italiano", "../locales/it/common.ftl"),
    ("nl", "Nederlands", "../locales/nl/common.ftl"),
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
        if let Some(script) = langid.script {
            let with_script = format!("{}-{script}", langid.language);
            if let Some(language) = supported(&with_script) {
                return language;
            }
        }
        let base = langid.language.to_string();
        if let Some(language) = supported(&base) {
            return language;
        }
        if let Some(language) = preferred_variant(&langid) {
            return language;
        }
        let same_language = SUPPORTED
            .iter()
            .find(|language| {
                language
                    .tag
                    .split('-')
                    .next()
                    .is_some_and(|tag| tag.eq_ignore_ascii_case(&base))
            })
            .map(|language| language.tag);
        if let Some(language) = same_language {
            return language;
        }
    }
    FALLBACK
}

/// Picks a close variant when a language ships one regional or script
/// catalogue and the request asks for another.
fn preferred_variant(langid: &LanguageIdentifier) -> Option<&'static str> {
    let language = langid.language.to_string().to_ascii_lowercase();
    let script = langid.script.map(|script| script.to_string());
    let region = langid.region.map(|region| region.to_string());
    match language.as_str() {
        "zh" => {
            let traditional = script.as_deref() == Some("Hant")
                || matches!(region.as_deref(), Some("TW" | "HK" | "MO"));
            if traditional {
                supported("zh-Hant").or_else(|| supported("zh-Hans"))
            } else {
                supported("zh-Hans").or_else(|| supported("zh-Hant"))
            }
        }
        "pt" => {
            if region.as_deref() == Some("PT") {
                supported("pt-PT").or_else(|| supported("pt-BR"))
            } else {
                supported("pt-BR").or_else(|| supported("pt-PT"))
            }
        }
        _ => None,
    }
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

/// Raw catalogue source for a shipped language.
///
/// Printed by `i18n-check --template` so a new translation starts from the
/// English keys, comments and plural selectors.
#[must_use]
pub fn catalogue(tag: &str) -> String {
    let tag = negotiate(tag);
    let mut source = String::new();
    for part in resources(&tag) {
        source.push_str(part);
    }
    source
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
        "fr" | "ru" | "uk" | "pl" | "cs" | "sv" | "fi" | "nb" | "nn" => ('\u{202f}', ','),
        "de" | "es" | "it" | "pt" | "nl" | "id" | "tr" | "vi" => ('.', ','),
        _ => (',', '.'),
    }
}

/// CLDR plural categories a counted message must provide.
///
/// A wildcard `*[other]` satisfies `other`; the remaining categories must be
/// present explicitly.
#[must_use]
pub fn plural_categories(tag: &str) -> &'static [&'static str] {
    let language = tag
        .split(['-', '_'])
        .next()
        .unwrap_or(tag)
        .to_ascii_lowercase();
    match language.as_str() {
        "ar" => &["zero", "one", "two", "few", "many", "other"],
        "ru" | "uk" | "pl" | "cs" => &["one", "few", "many", "other"],
        "id" | "ja" | "zh" | "ko" | "th" | "vi" => &["other"],
        _ => &["one", "other"],
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
    use fluent_syntax::ast::{Entry, Expression, InlineExpression, Pattern, PatternElement};
    use std::collections::{BTreeMap, BTreeSet};

    fn ids(tag: &str) -> BTreeSet<String> {
        keys(tag)
    }

    fn collect_inline(expr: &InlineExpression<&str>, out: &mut BTreeSet<String>) {
        match expr {
            InlineExpression::VariableReference { id } => {
                out.insert(id.name.to_string());
            }
            InlineExpression::Placeable { expression } => collect_expression(expression, out),
            InlineExpression::FunctionReference { arguments, .. } => {
                for argument in &arguments.positional {
                    collect_inline(argument, out);
                }
                for argument in &arguments.named {
                    collect_inline(&argument.value, out);
                }
            }
            _ => {}
        }
    }

    fn collect_expression(expr: &Expression<&str>, out: &mut BTreeSet<String>) {
        match expr {
            Expression::Inline(inline) => collect_inline(inline, out),
            Expression::Select { selector, variants } => {
                collect_inline(selector, out);
                for variant in variants {
                    collect_pattern(&variant.value, out);
                }
            }
        }
    }

    fn collect_pattern(pattern: &Pattern<&str>, out: &mut BTreeSet<String>) {
        for element in &pattern.elements {
            if let PatternElement::Placeable { expression } = element {
                collect_expression(expression, out);
            }
        }
    }

    fn variables(tag: &str) -> BTreeMap<String, BTreeSet<String>> {
        let mut found = BTreeMap::new();
        for source in resources(tag) {
            let parsed = fluent_syntax::parser::parse(*source).expect("catalogue parses");
            for entry in parsed.body {
                if let Entry::Message(message) = entry {
                    if let Some(value) = message.value {
                        let mut set = BTreeSet::new();
                        collect_pattern(&value, &mut set);
                        found.insert(message.id.name.to_string(), set);
                    }
                }
            }
        }
        found
    }

    fn select_variants(tag: &str) -> BTreeMap<String, BTreeMap<String, BTreeSet<String>>> {
        let mut found = BTreeMap::new();
        for source in resources(tag) {
            let parsed = fluent_syntax::parser::parse(*source).expect("catalogue parses");
            for entry in parsed.body {
                let Entry::Message(message) = entry else {
                    continue;
                };
                let Some(value) = message.value else {
                    continue;
                };
                for element in &value.elements {
                    let PatternElement::Placeable {
                        expression: Expression::Select { selector, variants },
                    } = element
                    else {
                        continue;
                    };
                    let InlineExpression::VariableReference { id } = selector else {
                        continue;
                    };
                    let keys: BTreeSet<String> = variants
                        .iter()
                        .map(|variant| match &variant.key {
                            fluent_syntax::ast::VariantKey::Identifier { name } => {
                                (*name).to_string()
                            }
                            fluent_syntax::ast::VariantKey::NumberLiteral { value } => {
                                (*value).to_string()
                            }
                        })
                        .collect();
                    found
                        .entry(message.id.name.to_string())
                        .or_insert_with(BTreeMap::new)
                        .insert(id.name.to_string(), keys);
                }
            }
        }
        found
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
    fn translations_keep_every_english_placeholder() {
        let english = variables(FALLBACK);
        for language in SUPPORTED.iter().filter(|language| language.tag != FALLBACK) {
            let translated = variables(language.tag);
            for (id, expected) in &english {
                assert_eq!(
                    translated.get(id),
                    Some(expected),
                    "{}: placeholders differ for {id}",
                    language.tag
                );
            }
        }
    }

    #[test]
    fn counted_messages_provide_required_plural_categories() {
        for language in SUPPORTED {
            let required = plural_categories(language.tag);
            for (id, selects) in select_variants(language.tag) {
                let Some(keys) = selects.get("count") else {
                    continue;
                };
                for category in required {
                    assert!(
                        keys.contains(*category),
                        "{}: {id} is missing the {category} plural category",
                        language.tag
                    );
                }
            }
        }
    }

    #[test]
    fn non_counted_selectors_keep_the_same_variants() {
        let english = select_variants(FALLBACK);
        for language in SUPPORTED.iter().filter(|language| language.tag != FALLBACK) {
            let translated = select_variants(language.tag);
            for (id, selects) in &english {
                let Some(translated_selects) = translated.get(id) else {
                    continue;
                };
                for (variable, expected) in selects {
                    if variable == "count" {
                        continue;
                    }
                    assert_eq!(
                        translated_selects.get(variable),
                        Some(expected),
                        "{}: {id} select on ${variable} differs",
                        language.tag
                    );
                }
            }
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
    fn negotiates_scripts_and_regional_variants() {
        assert_eq!(negotiate("zh-CN"), "zh-Hans");
        assert_eq!(negotiate("zh-Hans-CN"), "zh-Hans");
        assert_eq!(negotiate("zh-TW"), "zh-Hans");
        assert_eq!(negotiate("pt-PT"), "pt-BR");
        assert_eq!(negotiate("pt"), "pt-BR");
        assert_eq!(negotiate("fr-CA"), "fr");
    }

    #[test]
    fn every_language_formats_counts_and_dates() {
        let stamp = chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 11, 10, 14, 5, 0).unwrap();
        for language in SUPPORTED {
            let i18n = Translator::for_tag(language.tag);
            for id in ["chat-unread", "room-members", "nav-item-unread"] {
                for count in [0, 1, 2, 5, 11, 100] {
                    let value = i18n.tn(id, count);
                    assert!(
                        !value.is_empty() && !value.contains("[["),
                        "{}: {id} failed for {count}: {value}",
                        language.tag
                    );
                }
            }
            assert!(i18n.date(stamp).contains("2026"), "{} date", language.tag);
            assert!(!i18n.clock(stamp).is_empty(), "{} clock", language.tag);
            assert!(
                !i18n.date_time(stamp).is_empty(),
                "{} date_time",
                language.tag
            );
            assert!(
                !i18n.number(1_234_567).is_empty(),
                "{} number",
                language.tag
            );
        }
    }

    #[test]
    fn missing_language_falls_back_to_english() {
        assert_eq!(negotiate("de-DE"), "de");
        assert_eq!(negotiate("sw-KE"), "en");
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
    fn catalogue_source_is_exposed_for_templates() {
        assert!(catalogue("en").contains("common-close = Close"));
        assert!(catalogue("fr").contains("common-close = Fermer"));
        assert!(catalogue("de").contains("common-close = Schließen"));
        assert!(catalogue("sw").contains("common-close = Close"));
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
