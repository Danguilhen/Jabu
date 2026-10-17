use std::io::{Cursor, Read};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use storage::StorageBackend;
use zip::ZipArchive;

const PACKS_KEY: &str = "ui.sticker.packs";

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomSticker {
    pub name: String,
    pub mime: String,
    pub data_uri: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomStickerPack {
    pub name: String,
    pub author: String,
    pub license: String,
    pub stickers: Vec<CustomSticker>,
}

#[derive(Deserialize, Default)]
struct Manifest {
    #[serde(default)]
    name: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    license: String,
    #[serde(default)]
    stickers: Vec<ManifestSticker>,
}

#[derive(Deserialize)]
struct ManifestSticker {
    file: String,
    #[serde(default)]
    name: String,
}

#[must_use]
pub fn load_packs(storage: &dyn StorageBackend) -> Vec<CustomStickerPack> {
    storage
        .get_setting(PACKS_KEY)
        .ok()
        .flatten()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

pub fn save_packs(storage: &dyn StorageBackend, packs: &[CustomStickerPack]) -> anyhow::Result<()> {
    storage.set_setting(PACKS_KEY, &serde_json::to_value(packs)?)?;
    Ok(())
}

pub fn add_pack(storage: &dyn StorageBackend, pack: CustomStickerPack) -> anyhow::Result<()> {
    let mut packs = load_packs(storage);
    packs.retain(|existing| existing.name != pack.name);
    packs.push(pack);
    save_packs(storage, &packs)
}

pub fn import_zip(bytes: &[u8], fallback_name: &str) -> Result<CustomStickerPack, String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|error| error.to_string())?;
    let manifest = archive
        .by_name("meta.json")
        .ok()
        .and_then(|mut file| {
            let mut text = String::new();
            file.read_to_string(&mut text).ok()?;
            serde_json::from_str::<Manifest>(&text).ok()
        })
        .unwrap_or_default();

    let mut stickers = Vec::new();
    if manifest.stickers.is_empty() {
        for index in 0..archive.len() {
            let Ok(mut file) = archive.by_index(index) else {
                continue;
            };
            let filename = file.name().to_string();
            let mime = mime_guess::from_path(&filename)
                .first_or_octet_stream()
                .to_string();
            if !mime.starts_with("image/") {
                continue;
            }
            let mut data = Vec::new();
            if file.read_to_end(&mut data).is_err() {
                continue;
            }
            let name = filename.rsplit('/').next().unwrap_or(&filename).to_string();
            let data_uri = format!("data:{mime};base64,{}", STANDARD.encode(data));
            stickers.push(CustomSticker {
                name,
                mime,
                data_uri,
            });
        }
    } else {
        for entry in manifest.stickers {
            let Ok(mut file) = archive.by_name(&entry.file) else {
                continue;
            };
            let mime = mime_guess::from_path(&entry.file)
                .first_or_octet_stream()
                .to_string();
            let mut data = Vec::new();
            if file.read_to_end(&mut data).is_err() {
                continue;
            }
            let data_uri = format!("data:{mime};base64,{}", STANDARD.encode(data));
            stickers.push(CustomSticker {
                name: if entry.name.is_empty() {
                    entry.file.clone()
                } else {
                    entry.name
                },
                mime,
                data_uri,
            });
        }
    }

    Ok(CustomStickerPack {
        name: if manifest.name.is_empty() {
            fallback_name.to_string()
        } else {
            manifest.name
        },
        author: manifest.author,
        license: manifest.license,
        stickers,
    })
}

#[must_use]
pub fn decode_data_uri(data_uri: &str) -> Option<(String, Vec<u8>)> {
    let rest = data_uri.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    let mime = meta.strip_suffix(";base64")?;
    let bytes = STANDARD.decode(data).ok()?;
    Some((mime.to_string(), bytes))
}
