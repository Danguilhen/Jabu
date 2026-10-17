use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};
use std::sync::{Mutex, OnceLock};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use storage::StorageBackend;
use zip::ZipArchive;

const EMOJI_ZIP: &[u8] = include_bytes!("../assets/noto-emoji.zip");
const RECENT_LIMIT: usize = 24;
const FAVORITES_LIMIT: usize = 48;

pub const SKIN_TONE_LABELS: [&str; 5] = [
    "light skin tone",
    "medium-light skin tone",
    "medium skin tone",
    "medium-dark skin tone",
    "dark skin tone",
];

pub const SKIN_TONE_GLYPHS: [&str; 6] = ["🖐", "🖐🏻", "🖐🏼", "🖐🏽", "🖐🏾", "🖐🏿"];

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EmojiEntry {
    pub glyph: &'static str,
    pub name: &'static str,
    pub group: &'static str,
    pub codepoint: &'static str,
    pub shortcut: String,
}

impl EmojiEntry {
    #[must_use]
    pub fn data_uri(&self) -> Option<String> {
        svg_data_uri(self.codepoint)
    }
}

static ALL_ENTRIES: OnceLock<Vec<EmojiEntry>> = OnceLock::new();
static CATALOG: OnceLock<Vec<EmojiEntry>> = OnceLock::new();
static ARCHIVE: OnceLock<Mutex<ZipArchive<Cursor<&'static [u8]>>>> = OnceLock::new();
static IMAGE_CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();

fn all_entries() -> &'static [EmojiEntry] {
    ALL_ENTRIES.get_or_init(|| {
        let mut seen = HashSet::new();
        emoji::lookup_by_name::iter_emoji()
            .filter(|entry| entry.status == emoji::Status::FullyQualified)
            .filter(|entry| seen.insert(entry.glyph))
            .map(|entry| EmojiEntry {
                glyph: entry.glyph,
                name: entry.name,
                group: entry.group,
                codepoint: entry.codepoint,
                shortcut: shortcut(entry.name),
            })
            .collect()
    })
}

fn is_skin_variant(name: &str) -> bool {
    name.split_once(": ")
        .is_some_and(|(_, rest)| rest.contains("skin tone"))
}

fn base_name(name: &str) -> &str {
    name.split_once(": ").map_or(name, |(base, _)| base)
}

#[must_use]
pub fn catalog() -> &'static [EmojiEntry] {
    CATALOG.get_or_init(|| {
        all_entries()
            .iter()
            .filter(|entry| !is_skin_variant(entry.name))
            .cloned()
            .collect()
    })
}

#[must_use]
pub fn lookup_glyph(glyph: &str) -> Option<&'static EmojiEntry> {
    all_entries().iter().find(|entry| entry.glyph == glyph)
}

#[must_use]
pub fn variants_for(glyph: &str) -> Vec<&'static EmojiEntry> {
    let Some(base) = lookup_glyph(glyph) else {
        return Vec::new();
    };
    all_entries()
        .iter()
        .filter(|entry| is_skin_variant(entry.name) && base_name(entry.name) == base.name)
        .collect()
}

#[must_use]
pub fn entry_for_tone(entry: &'static EmojiEntry, tone: usize) -> &'static EmojiEntry {
    if tone == 0 {
        return entry;
    }
    let Some(label) = SKIN_TONE_LABELS.get(tone - 1) else {
        return entry;
    };
    let wanted = format!("{}: {label}", entry.name);
    variants_for(entry.glyph)
        .into_iter()
        .find(|variant| variant.name == wanted)
        .unwrap_or(entry)
}

#[must_use]
pub fn groups() -> Vec<&'static str> {
    let mut groups = Vec::new();
    for entry in catalog() {
        if !groups.contains(&entry.group) {
            groups.push(entry.group);
        }
    }
    groups
}

#[must_use]
pub fn search(query: &str, limit: usize) -> Vec<&'static EmojiEntry> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let mut matches: Vec<(u8, &'static EmojiEntry)> = catalog()
        .iter()
        .filter_map(|entry| {
            let name = entry.name.to_lowercase();
            let score = if entry.shortcut == query {
                0
            } else if entry.shortcut.starts_with(&query) {
                1
            } else if name.starts_with(&query) {
                2
            } else if entry.shortcut.contains(&query) {
                3
            } else if name.contains(&query) {
                4
            } else {
                return None;
            };
            Some((score, entry))
        })
        .collect();
    matches.sort_by(|(left_score, left), (right_score, right)| {
        left_score
            .cmp(right_score)
            .then_with(|| left.shortcut.cmp(&right.shortcut))
    });
    matches
        .into_iter()
        .take(limit)
        .map(|(_, entry)| entry)
        .collect()
}

#[must_use]
pub fn suggestions(text: &str, limit: usize) -> Vec<&'static EmojiEntry> {
    let mut found = Vec::new();
    for entry in catalog() {
        if text.contains(entry.glyph)
            && !found
                .iter()
                .any(|item: &&EmojiEntry| item.glyph == entry.glyph)
        {
            found.push(entry);
            if found.len() >= limit {
                break;
            }
        }
    }
    found
}

#[must_use]
pub fn shortcut(name: &str) -> String {
    name.chars()
        .map(|character| match character {
            '-' | ' ' => '_',
            '0'..='9' => char::from(b'a' + (character as u8 - b'0')),
            other => other.to_ascii_lowercase(),
        })
        .collect()
}

#[must_use]
pub fn image_for_glyph(glyph: &str) -> Option<String> {
    let entry = emoji::lookup_by_glyph::lookup(glyph)?;
    svg_data_uri(entry.codepoint)
}

/// # Panics
///
/// Panics if the bundled Noto Emoji archive is corrupt.
#[must_use]
pub fn svg_data_uri(codepoint: &str) -> Option<String> {
    let cache = IMAGE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache
        .lock()
        .ok()
        .and_then(|cache| cache.get(codepoint).cloned())
    {
        return cached;
    }

    let filename = format!("emoji_u{}.svg", codepoint_filename(codepoint));
    let uri = ARCHIVE
        .get_or_init(|| {
            Mutex::new(
                ZipArchive::new(Cursor::new(EMOJI_ZIP))
                    .expect("bundled Noto Emoji archive is valid"),
            )
        })
        .lock()
        .ok()
        .and_then(|mut archive| {
            let mut file = archive.by_name(&filename).ok()?;
            let mut svg = Vec::new();
            file.read_to_end(&mut svg).ok()?;
            Some(format!(
                "data:image/svg+xml;base64,{}",
                STANDARD.encode(svg)
            ))
        });

    if let Ok(mut cache) = cache.lock() {
        cache.insert(codepoint.to_string(), uri.clone());
    }
    uri
}

fn codepoint_filename(codepoint: &str) -> String {
    codepoint
        .split_whitespace()
        .filter(|part| !part.eq_ignore_ascii_case("FE0F"))
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join("_")
}

fn account_key(prefix: &str, account: &str) -> String {
    let account = account.split('/').next().unwrap_or(account);
    format!("{prefix}:{account}")
}

fn recent_key(account: &str) -> String {
    account_key("ui.emoji.recent", account)
}

fn favorites_key(account: &str) -> String {
    account_key("ui.emoji.favorites", account)
}

fn skin_tone_key(account: &str) -> String {
    account_key("ui.emoji.skin_tone", account)
}

#[must_use]
pub fn load_recent(storage: &dyn StorageBackend, account: &str) -> Vec<String> {
    storage
        .get_setting(&recent_key(account))
        .ok()
        .flatten()
        .and_then(|value| serde_json::from_value::<Vec<String>>(value).ok())
        .unwrap_or_default()
}

pub fn remember_recent(storage: &dyn StorageBackend, account: &str, glyph: &str) {
    let mut recent = load_recent(storage, account);
    recent.retain(|item| item != glyph);
    recent.insert(0, glyph.to_string());
    recent.truncate(RECENT_LIMIT);
    if let Ok(value) = serde_json::to_value(recent) {
        let _ = storage.set_setting(&recent_key(account), &value);
    }
}

#[must_use]
pub fn load_favorites(storage: &dyn StorageBackend, account: &str) -> Vec<String> {
    storage
        .get_setting(&favorites_key(account))
        .ok()
        .flatten()
        .and_then(|value| serde_json::from_value::<Vec<String>>(value).ok())
        .unwrap_or_default()
}

#[must_use]
pub fn is_favorite(storage: &dyn StorageBackend, account: &str, glyph: &str) -> bool {
    load_favorites(storage, account)
        .iter()
        .any(|item| item == glyph)
}

/// Toggle a favorite and return whether it is now a favorite.
pub fn toggle_favorite(storage: &dyn StorageBackend, account: &str, glyph: &str) -> bool {
    let mut favorites = load_favorites(storage, account);
    let now_favorite = if let Some(index) = favorites.iter().position(|item| item == glyph) {
        favorites.remove(index);
        false
    } else {
        favorites.insert(0, glyph.to_string());
        favorites.truncate(FAVORITES_LIMIT);
        true
    };
    if let Ok(value) = serde_json::to_value(favorites) {
        let _ = storage.set_setting(&favorites_key(account), &value);
    }
    now_favorite
}

#[must_use]
pub fn load_skin_tone(storage: &dyn StorageBackend, account: &str) -> usize {
    storage
        .get_setting(&skin_tone_key(account))
        .ok()
        .flatten()
        .and_then(|value| value.as_u64())
        .map_or(0, |tone| usize::try_from(tone).unwrap_or(0))
        .min(SKIN_TONE_LABELS.len())
}

pub fn save_skin_tone(storage: &dyn StorageBackend, account: &str, tone: usize) {
    let tone = tone.min(SKIN_TONE_LABELS.len());
    let _ = storage.set_setting(&skin_tone_key(account), &serde_json::json!(tone));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_deduplicated_and_searchable() {
        let catalog = catalog();
        assert!(catalog.len() > 1000);
        assert!(catalog.iter().any(|entry| entry.glyph == "😀"));
        let found = search("grinning", 10);
        assert!(!found.is_empty());
    }

    #[test]
    fn keeps_skin_tones_as_variants() {
        let catalog = catalog();
        assert!(!catalog.iter().any(|entry| is_skin_variant(entry.name)));
        let thumbs = lookup_glyph("👍").expect("thumbs up");
        let variants = variants_for(thumbs.glyph);
        assert!(variants.len() >= 5);
        assert_eq!(entry_for_tone(thumbs, 1).glyph, "👍🏻");
        assert!(entry_for_tone(thumbs, 1).data_uri().is_some());
        assert_eq!(entry_for_tone(thumbs, 0).glyph, thumbs.glyph);
        let rower = lookup_glyph("🚣‍♂️").expect("man rowing boat");
        assert_eq!(entry_for_tone(rower, 5).glyph, "🚣🏿‍♂️");
    }

    #[test]
    fn maps_known_codepoints_to_noto_filenames() {
        assert_eq!(codepoint_filename("1F600"), "1f600");
        assert_eq!(
            codepoint_filename("2764 FE0F 200D 1F525"),
            "2764_200d_1f525"
        );
    }

    #[test]
    fn renders_noto_svg() {
        let uri = svg_data_uri("1F600").expect("grinning face asset");
        assert!(uri.starts_with("data:image/svg+xml;base64,"));
    }

    #[test]
    fn favorites_and_skin_tone_are_per_account() {
        let path = std::env::temp_dir().join(format!("jabu-emoji-{}", uuid::Uuid::new_v4()));
        let storage = storage::sqlite_storage::SqliteStorage::new(path.clone()).unwrap();
        assert!(toggle_favorite(&storage, "alice@localhost", "👍"));
        assert!(is_favorite(&storage, "alice@localhost", "👍"));
        assert!(!is_favorite(&storage, "bob@localhost", "👍"));
        assert!(!toggle_favorite(&storage, "alice@localhost", "👍"));
        assert!(!is_favorite(&storage, "alice@localhost", "👍"));

        save_skin_tone(&storage, "alice@localhost", 3);
        assert_eq!(load_skin_tone(&storage, "alice@localhost"), 3);
        assert_eq!(load_skin_tone(&storage, "bob@localhost"), 0);
        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn shortcuts_match_movim_normalisation() {
        assert_eq!(shortcut("grinning face"), "grinning_face");
        assert_eq!(shortcut("keycap 1"), "keycap_b");
    }
}
