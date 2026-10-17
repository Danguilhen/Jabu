use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::sync::{Mutex, OnceLock};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use storage::StorageBackend;
use zip::ZipArchive;

const PACKS_KEY: &str = "ui.emoji.packs";

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomEmoji {
    pub name: String,
    pub alias: String,
    pub data_uri: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomEmojiPack {
    pub name: String,
    pub description: String,
    pub homepage: String,
    pub license: String,
    pub emojis: Vec<CustomEmoji>,
}

#[derive(Deserialize)]
struct Manifest {
    #[serde(rename = "metaVersion")]
    meta_version: u32,
    emojis: Vec<ManifestEmoji>,
}

#[derive(Deserialize)]
struct ManifestEmoji {
    #[serde(rename = "fileName")]
    file_name: String,
    emoji: ManifestEmojiInfo,
}

#[derive(Deserialize)]
struct ManifestEmojiInfo {
    name: String,
    #[serde(default)]
    aliases: Vec<String>,
}

static ALIAS_CACHE: OnceLock<Mutex<Option<HashMap<String, String>>>> = OnceLock::new();

#[must_use]
pub fn load_packs(storage: &dyn StorageBackend) -> Vec<CustomEmojiPack> {
    storage
        .get_setting(PACKS_KEY)
        .ok()
        .flatten()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

pub fn save_packs(storage: &dyn StorageBackend, packs: &[CustomEmojiPack]) -> anyhow::Result<()> {
    storage.set_setting(PACKS_KEY, &serde_json::to_value(packs)?)?;
    invalidate_alias_cache();
    Ok(())
}

pub fn add_pack(storage: &dyn StorageBackend, pack: CustomEmojiPack) -> anyhow::Result<()> {
    let mut packs = load_packs(storage);
    packs.retain(|existing| existing.name != pack.name);
    packs.push(pack);
    save_packs(storage, &packs)
}

pub fn update_alias(
    storage: &dyn StorageBackend,
    pack_name: &str,
    emoji_name: &str,
    alias: &str,
) -> anyhow::Result<()> {
    let trimmed = alias.trim().trim_matches(':');
    if trimmed.is_empty() {
        return Ok(());
    }
    let mut packs = load_packs(storage);
    if let Some(pack) = packs.iter_mut().find(|pack| pack.name == pack_name) {
        if let Some(emoji) = pack
            .emojis
            .iter_mut()
            .find(|emoji| emoji.name == emoji_name)
        {
            emoji.alias = trimmed.to_string();
        }
    }
    save_packs(storage, &packs)
}

pub fn remove_pack(storage: &dyn StorageBackend, name: &str) -> anyhow::Result<()> {
    let mut packs = load_packs(storage);
    packs.retain(|pack| pack.name != name);
    save_packs(storage, &packs)
}

pub fn invalidate_alias_cache() {
    if let Ok(mut cache) = ALIAS_CACHE.get_or_init(|| Mutex::new(None)).lock() {
        *cache = None;
    }
}

#[must_use]
pub fn alias_map(storage: &dyn StorageBackend) -> HashMap<String, String> {
    let cache = ALIAS_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(mut cache) = cache.lock() {
        if let Some(map) = cache.as_ref() {
            return map.clone();
        }
        let mut map = HashMap::new();
        for pack in load_packs(storage) {
            for emoji in pack.emojis {
                map.insert(emoji.alias, emoji.data_uri);
            }
        }
        *cache = Some(map.clone());
        return map;
    }
    HashMap::new()
}

pub fn import_zip(bytes: &[u8], fallback_name: &str) -> Result<CustomEmojiPack, String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|error| error.to_string())?;
    let manifest: Manifest = {
        let mut file = archive
            .by_name("meta.json")
            .map_err(|_| "meta.json is missing".to_string())?;
        let mut text = String::new();
        file.read_to_string(&mut text)
            .map_err(|error| error.to_string())?;
        serde_json::from_str(&text).map_err(|error| error.to_string())?
    };
    if manifest.meta_version != 2 {
        return Err("only meta.json version 2 is supported".to_string());
    }
    let mut emojis = Vec::new();
    for entry in manifest.emojis {
        let mut file = match archive.by_name(&entry.file_name) {
            Ok(file) => file,
            Err(_) => continue,
        };
        let mut data = Vec::new();
        file.read_to_end(&mut data)
            .map_err(|error| error.to_string())?;
        let mime = mime_guess::from_path(&entry.file_name)
            .first_or_octet_stream()
            .to_string();
        let alias = entry
            .emoji
            .aliases
            .first()
            .cloned()
            .unwrap_or_else(|| slug(&entry.emoji.name));
        emojis.push(CustomEmoji {
            name: entry.emoji.name,
            alias,
            data_uri: format!("data:{mime};base64,{}", STANDARD.encode(data)),
        });
    }
    Ok(CustomEmojiPack {
        name: fallback_name.to_string(),
        description: String::new(),
        homepage: String::new(),
        license: String::new(),
        emojis,
    })
}

fn slug(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugifies_names() {
        assert_eq!(slug("Party Parrot"), "party_parrot");
    }
}
