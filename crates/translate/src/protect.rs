//! Protect chat markup from the translation model.
//!
//! URLs, XMPP mentions, e-mail addresses, code and custom emoji are replaced
//! with `<sn id="jabuN"/>` placeholders before the text reaches the model and
//! restored from the original afterwards. This keeps links and identities
//! exact and prevents the model from inventing new ones.

use std::collections::HashSet;
use std::hash::BuildHasher;
use std::sync::LazyLock;

use regex::Regex;
use thiserror::Error;

static CODE_BLOCK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)```.*?```|~~~.*?~~~").expect("valid code-block regex"));
static INLINE_CODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`[^`\n]+`").expect("valid inline-code regex"));
static URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"https?://[^\s<>"']+"#).expect("valid URL regex"));
static XMPP_URI: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"xmpp:[^\s<>"']+"#).expect("valid XMPP URI regex"));
static EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}").expect("valid e-mail regex")
});
static MENTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"@[A-Za-z0-9._\-]{2,}").expect("valid mention regex"));
static EMOJI_ALIAS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":[A-Za-z0-9_+\-]{2,}:").expect("valid emoji alias regex"));
static PLACEHOLDER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<sn id="jabu([0-9]+)"/>"#).expect("valid placeholder regex"));

const PLACEHOLDER_PREFIX: &str = "<sn id=\"jabu";

/// A piece of a message that is either translated or copied unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    /// Text the model may translate.
    Text(String),
    /// A URL, mention, code span or custom emoji that must be copied exactly.
    Literal(String),
}

/// Split a message around protected spans.
///
/// Translating the text segments separately and copying the literals is more
/// reliable than asking a small model to preserve placeholder tags. Most chat
/// messages contain no literals and still take a single inference call.
#[must_use]
pub fn split<S: BuildHasher>(text: &str, known_emoji: &HashSet<String, S>) -> Vec<Segment> {
    let ranges = protected_ranges(text, known_emoji);
    if ranges.is_empty() {
        return vec![Segment::Text(text.to_string())];
    }
    let mut segments = Vec::new();
    let mut cursor = 0;
    for (start, end) in ranges {
        if start > cursor {
            segments.push(Segment::Text(text[cursor..start].to_string()));
        }
        segments.push(Segment::Literal(text[start..end].to_string()));
        cursor = end;
    }
    if cursor < text.len() {
        segments.push(Segment::Text(text[cursor..].to_string()));
    }
    segments
}

fn protected_ranges<S: BuildHasher>(
    text: &str,
    known_emoji: &HashSet<String, S>,
) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    add_ranges(&CODE_BLOCK, false, text, &mut ranges);
    add_ranges(&INLINE_CODE, false, text, &mut ranges);
    add_ranges(&URL, true, text, &mut ranges);
    add_ranges(&XMPP_URI, false, text, &mut ranges);
    add_ranges(&EMAIL, false, text, &mut ranges);
    add_ranges(&MENTION, false, text, &mut ranges);
    for found in EMOJI_ALIAS.find_iter(text) {
        let alias = found.as_str().trim_matches(':');
        if known_emoji.contains(alias) {
            ranges.push((found.start(), found.end()));
        }
    }
    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        if merged.last().is_some_and(|(_, last_end)| start < *last_end) {
            continue;
        }
        merged.push((start, end));
    }
    merged
}

fn add_ranges(
    regex: &Regex,
    trim_url_punctuation: bool,
    text: &str,
    ranges: &mut Vec<(usize, usize)>,
) {
    for found in regex.find_iter(text) {
        let start = found.start();
        let mut end = found.end();
        if trim_url_punctuation {
            while end > start
                && matches!(
                    text.as_bytes()[end - 1],
                    b'.' | b',' | b';' | b':' | b'!' | b'?' | b')' | b']' | b'}'
                )
            {
                end -= 1;
            }
        }
        if start < end {
            ranges.push((start, end));
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectedText {
    pub masked: String,
    values: Vec<String>,
}

impl ProtectedText {
    /// Restore the original values from a model output.
    ///
    /// # Errors
    ///
    /// Returns an error when the model drops, duplicates or invents a
    /// placeholder. A caller should retry or keep the original message.
    pub fn restore(&self, translated: &str) -> Result<String, ProtectError> {
        let mut output = translated.to_string();
        for (index, value) in self.values.iter().enumerate() {
            let placeholder = format!("{PLACEHOLDER_PREFIX}{index}\"/>");
            let count = output.matches(&placeholder).count();
            match count {
                1 => {
                    output = output.replace(&placeholder, value);
                }
                0 => return Err(ProtectError::Missing(index)),
                _ => return Err(ProtectError::Duplicated(index)),
            }
        }
        if output.contains(PLACEHOLDER_PREFIX) {
            return Err(ProtectError::Unknown);
        }
        Ok(output)
    }

    /// Whether any text was protected.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// Errors from protecting or restoring text.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtectError {
    #[error("the translation dropped placeholder {0}")]
    Missing(usize),
    #[error("the translation duplicated placeholder {0}")]
    Duplicated(usize),
    #[error("the translation introduced an unknown placeholder")]
    Unknown,
}

/// Replace chat markup with placeholders.
#[must_use]
pub fn protect<S: BuildHasher>(text: &str, known_emoji: &HashSet<String, S>) -> ProtectedText {
    let mut values = Vec::new();
    let mut masked = text.to_string();
    masked = replace_all(&CODE_BLOCK, &masked, &mut values);
    masked = replace_all(&INLINE_CODE, &masked, &mut values);
    masked = replace_urls(&masked, &mut values);
    masked = replace_all(&XMPP_URI, &masked, &mut values);
    masked = replace_all(&EMAIL, &masked, &mut values);
    masked = replace_all(&MENTION, &masked, &mut values);
    masked = replace_emoji(&masked, &mut values, known_emoji);
    ProtectedText { masked, values }
}

fn placeholder(index: usize) -> String {
    format!("{PLACEHOLDER_PREFIX}{index}\"/>")
}

fn replace_all(regex: &Regex, input: &str, values: &mut Vec<String>) -> String {
    regex
        .replace_all(input, |captures: &regex::Captures<'_>| {
            let matched = captures.get(0).expect("whole match").as_str();
            let index = values.len();
            values.push(matched.to_string());
            placeholder(index)
        })
        .into_owned()
}

/// URLs usually end at punctuation, but the punctuation belongs to the
/// sentence, not the link.
fn replace_urls(input: &str, values: &mut Vec<String>) -> String {
    URL.replace_all(input, |captures: &regex::Captures<'_>| {
        let matched = captures.get(0).expect("whole match").as_str();
        let mut end = matched.len();
        while end > 0 {
            let last = matched.as_bytes()[end - 1];
            if matches!(
                last,
                b'.' | b',' | b';' | b':' | b'!' | b'?' | b')' | b']' | b'}'
            ) {
                end -= 1;
            } else {
                break;
            }
        }
        let link = &matched[..end];
        let trailing = &matched[end..];
        let index = values.len();
        values.push(link.to_string());
        format!("{}{trailing}", placeholder(index))
    })
    .into_owned()
}

fn replace_emoji<S: BuildHasher>(
    input: &str,
    values: &mut Vec<String>,
    known: &HashSet<String, S>,
) -> String {
    EMOJI_ALIAS
        .replace_all(input, |captures: &regex::Captures<'_>| {
            let matched = captures.get(0).expect("whole match").as_str();
            let alias = matched.trim_matches(':');
            if !known.contains(alias) {
                return matched.to_string();
            }
            let index = values.len();
            values.push(matched.to_string());
            placeholder(index)
        })
        .into_owned()
}

/// Remove markup before language detection so a URL does not look like the
/// message's language.
#[must_use]
pub fn strip_markup(text: &str) -> String {
    let mut stripped = CODE_BLOCK.replace_all(text, " ").into_owned();
    stripped = INLINE_CODE.replace_all(&stripped, " ").into_owned();
    stripped = URL.replace_all(&stripped, " ").into_owned();
    stripped = XMPP_URI.replace_all(&stripped, " ").into_owned();
    stripped = EMAIL.replace_all(&stripped, " ").into_owned();
    stripped = MENTION.replace_all(&stripped, " ").into_owned();
    // `PLACEHOLDER` is only here so a caller can pass already-masked text.
    stripped = PLACEHOLDER.replace_all(&stripped, " ").into_owned();
    stripped.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aliases(values: &[&str]) -> HashSet<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn protects_and_restores_urls_mentions_and_code() {
        let emoji = aliases(&["party"]);
        let text = "Salut @alice, regarde https://example.org/a?b=1, et `cargo test` :party: ok";
        let protected = protect(text, &emoji);
        assert!(!protected.masked.contains("https://"));
        assert!(!protected.masked.contains("@alice"));
        assert!(!protected.masked.contains("cargo test"));
        assert!(!protected.masked.contains(":party:"));
        assert_eq!(protected.restore(&protected.masked).unwrap(), text);
    }

    #[test]
    fn keeps_unknown_colon_aliases() {
        let protected = protect("12:30 is not an emoji :unknown:", &HashSet::new());
        assert!(protected.masked.contains("12:30"));
        assert!(protected.masked.contains(":unknown:"));
    }

    #[test]
    fn reports_dropped_placeholders() {
        let protected = protect("see https://example.org", &HashSet::new());
        assert_eq!(
            protected.restore("see the site"),
            Err(ProtectError::Missing(0))
        );
    }

    #[test]
    fn reports_duplicated_placeholders() {
        let protected = protect("see https://example.org", &HashSet::new());
        let duplicated = format!("{} {}", protected.masked, protected.masked);
        assert_eq!(
            protected.restore(&duplicated),
            Err(ProtectError::Duplicated(0))
        );
    }

    #[test]
    fn strips_markup_for_detection() {
        let stripped = strip_markup("https://example.org @alice `code` bonjour");
        assert!(!stripped.contains("example.org"));
        assert!(!stripped.contains("@alice"));
        assert!(stripped.contains("bonjour"));
    }

    #[test]
    fn splits_literals_from_translatable_text() {
        let segments = split(
            "Salut @alice, lis https://example.org/a?b=1 !",
            &HashSet::new(),
        );
        assert_eq!(segments.len(), 5);
        assert_eq!(segments[1], Segment::Literal("@alice".to_string()));
        assert_eq!(
            segments[3],
            Segment::Literal("https://example.org/a?b=1".to_string())
        );
        let rebuilt: String = segments
            .into_iter()
            .map(|segment| match segment {
                Segment::Text(text) | Segment::Literal(text) => text,
            })
            .collect();
        assert_eq!(rebuilt, "Salut @alice, lis https://example.org/a?b=1 !");
    }
}
