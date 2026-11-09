//! Lightweight source-language detection for auto-translation.
//!
//! Chat messages are short, so a general-purpose detector is unreliable on its
//! own. Distinctive scripts are detected directly; Latin text needs a longer
//! sample and a confidence threshold. When the result is uncertain the caller
//! should offer manual translation instead of guessing.

use crate::protect::strip_markup;
use crate::types::Language;

/// A detected language and how much to trust the result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detection {
    pub language: Language,
    pub confidence: f64,
    pub reliable: bool,
}

impl Detection {
    /// Whether automatic translation should act on this detection.
    #[must_use]
    pub fn should_translate(self, target: Language) -> bool {
        self.reliable && !self.language.same_language(target)
    }
}

/// Detect the language of a chat message.
#[must_use]
pub fn detect(text: &str) -> Option<Detection> {
    let cleaned = strip_markup(text);
    if cleaned.trim().is_empty() {
        return None;
    }

    if let Some(hint) = script_hint(&cleaned) {
        return Some(hint);
    }

    let info = whatlang::detect(&cleaned)?;
    let language = map_whatlang(info.lang())?;
    let chars = cleaned.chars().count();
    let mut confidence = info.confidence();
    // Latin-script short messages are the unreliable case. Require both a
    // minimum length and a clear margin before auto-translating.
    if chars < 12 {
        confidence *= 0.5;
    }
    let reliable = confidence >= 0.4 && chars >= 12;
    Some(Detection {
        language,
        confidence,
        reliable,
    })
}

#[allow(clippy::too_many_lines)]
fn script_hint(text: &str) -> Option<Detection> {
    let mut hangul = 0usize;
    let mut kana = 0usize;
    let mut han = 0usize;
    let mut arabic = 0usize;
    let mut cyrillic = 0usize;
    let mut devanagari = 0usize;
    let mut thai = 0usize;
    let mut hebrew = 0usize;
    let mut bengali = 0usize;
    let mut tamil = 0usize;
    let mut telugu = 0usize;
    let mut gujarati = 0usize;
    let mut khmer = 0usize;
    let mut burmese = 0usize;
    let mut tibetan = 0usize;
    let mut letters = 0usize;

    for character in text.chars().filter(|character| character.is_alphabetic()) {
        letters += 1;
        let code = u32::from(character);
        match code {
            0xAC00..=0xD7A3 | 0x1100..=0x11FF | 0x3130..=0x318F => hangul += 1,
            0x3040..=0x30FF => kana += 1,
            0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF => han += 1,
            0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF => arabic += 1,
            0x0400..=0x04FF => cyrillic += 1,
            0x0900..=0x097F => devanagari += 1,
            0x0E00..=0x0E7F => thai += 1,
            0x0590..=0x05FF => hebrew += 1,
            0x0980..=0x09FF => bengali += 1,
            0x0B80..=0x0BFF => tamil += 1,
            0x0C00..=0x0C7F => telugu += 1,
            0x0A80..=0x0AFF => gujarati += 1,
            0x1780..=0x17FF => khmer += 1,
            0x1000..=0x109F => burmese += 1,
            0x0F00..=0x0FFF => tibetan += 1,
            _ => {}
        }
    }
    if letters == 0 {
        return None;
    }

    // These scripts are distinctive enough to identify from a short message.
    let strong = |count: usize, language: Language| {
        (count >= 2 && count * 3 >= letters).then_some(Detection {
            language,
            confidence: 0.9,
            reliable: true,
        })
    };
    if let Some(detection) = strong(hangul, Language::Korean) {
        return Some(detection);
    }
    if let Some(detection) = strong(kana, Language::Japanese) {
        return Some(detection);
    }
    if han >= 2 && han * 3 >= letters {
        // Kana or Hangul mixed with Han wins above; otherwise this is Chinese.
        return Some(Detection {
            language: Language::Chinese,
            confidence: 0.8,
            reliable: true,
        });
    }
    if let Some(detection) = strong(thai, Language::Thai) {
        return Some(detection);
    }
    if let Some(detection) = strong(hebrew, Language::Hebrew) {
        return Some(detection);
    }
    if let Some(detection) = strong(bengali, Language::Bengali) {
        return Some(detection);
    }
    if let Some(detection) = strong(tamil, Language::Tamil) {
        return Some(detection);
    }
    if let Some(detection) = strong(telugu, Language::Telugu) {
        return Some(detection);
    }
    if let Some(detection) = strong(gujarati, Language::Gujarati) {
        return Some(detection);
    }
    if let Some(detection) = strong(khmer, Language::Khmer) {
        return Some(detection);
    }
    if let Some(detection) = strong(burmese, Language::Burmese) {
        return Some(detection);
    }
    if let Some(detection) = strong(tibetan, Language::Tibetan) {
        return Some(detection);
    }

    // Arabic script, Cyrillic and Devanagari cover several languages. Use a
    // hint only for a longer sample and let the confidence mark it as weaker.
    let weak = |count: usize, language: Language| {
        (count >= 6 && count * 3 >= letters).then_some(Detection {
            language,
            confidence: 0.55,
            reliable: true,
        })
    };
    if let Some(detection) = weak(arabic, Language::Arabic) {
        return Some(detection);
    }
    if let Some(detection) = weak(cyrillic, Language::Russian) {
        return Some(detection);
    }
    if let Some(detection) = weak(devanagari, Language::Hindi) {
        return Some(detection);
    }
    None
}

const fn map_whatlang(language: whatlang::Lang) -> Option<Language> {
    use whatlang::Lang;
    Some(match language {
        Lang::Cmn => Language::Chinese,
        Lang::Eng => Language::English,
        Lang::Fra => Language::French,
        Lang::Por => Language::Portuguese,
        Lang::Spa => Language::Spanish,
        Lang::Jpn => Language::Japanese,
        Lang::Tur => Language::Turkish,
        Lang::Rus => Language::Russian,
        Lang::Ara => Language::Arabic,
        Lang::Kor => Language::Korean,
        Lang::Tha => Language::Thai,
        Lang::Ita => Language::Italian,
        Lang::Deu => Language::German,
        Lang::Vie => Language::Vietnamese,
        Lang::Ind => Language::Indonesian,
        Lang::Tgl => Language::Filipino,
        Lang::Hin => Language::Hindi,
        Lang::Pol => Language::Polish,
        Lang::Ces => Language::Czech,
        Lang::Nld => Language::Dutch,
        Lang::Khm => Language::Khmer,
        Lang::Mya => Language::Burmese,
        Lang::Pes => Language::Persian,
        Lang::Guj => Language::Gujarati,
        Lang::Urd => Language::Urdu,
        Lang::Tel => Language::Telugu,
        Lang::Mar => Language::Marathi,
        Lang::Heb => Language::Hebrew,
        Lang::Ben => Language::Bengali,
        Lang::Tam => Language::Tamil,
        Lang::Ukr => Language::Ukrainian,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_korean() {
        let detection = detect("안녕하세요 오늘 날씨가 정말 좋네요").unwrap();
        assert_eq!(detection.language, Language::Korean);
        assert!(detection.reliable);
    }

    #[test]
    fn detects_french() {
        let detection = detect("Bonjour, comment vas-tu aujourd'hui ?").unwrap();
        assert_eq!(detection.language, Language::French);
        assert!(detection.reliable, "{detection:?}");
    }

    #[test]
    fn detects_japanese_with_kana() {
        let detection = detect("こんにちは、元気ですか？").unwrap();
        assert_eq!(detection.language, Language::Japanese);
    }

    #[test]
    fn detects_arabic() {
        let detection = detect("مرحبا كيف حالك اليوم").unwrap();
        assert_eq!(detection.language, Language::Arabic);
    }

    #[test]
    fn ignores_urls_and_short_noise() {
        assert!(detect("https://example.org").is_none());
        assert!(detect("ok").is_none());
        assert!(detect("  ").is_none());
    }

    #[test]
    fn target_comparison_uses_written_language() {
        let detection = detect("你好，今天天气很好").unwrap();
        assert!(!detection.should_translate(Language::Chinese));
        assert!(detection.should_translate(Language::French));
    }
}
