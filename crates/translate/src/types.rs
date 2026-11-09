//! Language identifiers and translation request/result types.

use serde::{Deserialize, Serialize};

/// A language supported by the Hy-MT2 family.
///
/// The set is intentionally explicit: the prompt needs the model's own
/// language name, and the picker only exposes languages Jabu can actually
/// translate into. Tags follow BCP-47 where a single tag exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    Chinese,
    TraditionalChinese,
    English,
    French,
    Portuguese,
    Spanish,
    Japanese,
    Turkish,
    Russian,
    Arabic,
    Korean,
    Thai,
    Italian,
    German,
    Vietnamese,
    Malay,
    Indonesian,
    Filipino,
    Hindi,
    Polish,
    Czech,
    Dutch,
    Khmer,
    Burmese,
    Persian,
    Gujarati,
    Urdu,
    Telugu,
    Marathi,
    Hebrew,
    Bengali,
    Tamil,
    Ukrainian,
    Tibetan,
    Kazakh,
    Mongolian,
    Uyghur,
    Cantonese,
}

impl Language {
    /// Every language the shipped model understands.
    pub const ALL: [Self; 38] = [
        Self::Chinese,
        Self::TraditionalChinese,
        Self::English,
        Self::French,
        Self::Portuguese,
        Self::Spanish,
        Self::Japanese,
        Self::Turkish,
        Self::Russian,
        Self::Arabic,
        Self::Korean,
        Self::Thai,
        Self::Italian,
        Self::German,
        Self::Vietnamese,
        Self::Malay,
        Self::Indonesian,
        Self::Filipino,
        Self::Hindi,
        Self::Polish,
        Self::Czech,
        Self::Dutch,
        Self::Khmer,
        Self::Burmese,
        Self::Persian,
        Self::Gujarati,
        Self::Urdu,
        Self::Telugu,
        Self::Marathi,
        Self::Hebrew,
        Self::Bengali,
        Self::Tamil,
        Self::Ukrainian,
        Self::Tibetan,
        Self::Kazakh,
        Self::Mongolian,
        Self::Uyghur,
        Self::Cantonese,
    ];

    /// Resolve a BCP-47 tag, a legacy language code or a bare language name.
    ///
    /// Variants that share a written standard (`pt-BR`, `pt-PT`) map to the
    /// model's single language. Chinese script variants stay distinct because
    /// the model can translate between them.
    #[must_use]
    pub fn from_tag(tag: &str) -> Option<Self> {
        let normalized = tag.trim().replace('_', "-").to_ascii_lowercase();
        let base = normalized.split('-').next().unwrap_or(normalized.as_str());
        match base {
            "zh" if normalized.contains("hant")
                || normalized.starts_with("zh-tw")
                || normalized.starts_with("zh-hk")
                || normalized.starts_with("zh-mo") =>
            {
                Some(Self::TraditionalChinese)
            }
            "zh" => Some(Self::Chinese),
            "yue" => Some(Self::Cantonese),
            "en" => Some(Self::English),
            "fr" | "fre" | "fra" => Some(Self::French),
            "pt" => Some(Self::Portuguese),
            "es" | "spa" => Some(Self::Spanish),
            "ja" | "jpn" => Some(Self::Japanese),
            "tr" | "tur" => Some(Self::Turkish),
            "ru" | "rus" => Some(Self::Russian),
            "ar" | "ara" => Some(Self::Arabic),
            "ko" | "kor" => Some(Self::Korean),
            "th" | "tha" => Some(Self::Thai),
            "it" | "ita" => Some(Self::Italian),
            "de" | "ger" | "deu" => Some(Self::German),
            "vi" | "vie" => Some(Self::Vietnamese),
            "ms" | "msa" | "may" => Some(Self::Malay),
            "id" | "ind" => Some(Self::Indonesian),
            "fil" | "tl" | "tgl" => Some(Self::Filipino),
            "hi" | "hin" => Some(Self::Hindi),
            "pl" | "pol" => Some(Self::Polish),
            "cs" | "ces" | "cze" => Some(Self::Czech),
            "nl" | "nld" | "dut" => Some(Self::Dutch),
            "km" | "khm" => Some(Self::Khmer),
            "my" | "mya" | "bur" => Some(Self::Burmese),
            "fa" | "fas" | "per" => Some(Self::Persian),
            "gu" | "guj" => Some(Self::Gujarati),
            "ur" | "urd" => Some(Self::Urdu),
            "te" | "tel" => Some(Self::Telugu),
            "mr" | "mar" => Some(Self::Marathi),
            "he" | "heb" | "iw" => Some(Self::Hebrew),
            "bn" | "ben" => Some(Self::Bengali),
            "ta" | "tam" => Some(Self::Tamil),
            "uk" | "ukr" => Some(Self::Ukrainian),
            "bo" | "bod" | "tib" => Some(Self::Tibetan),
            "kk" | "kaz" => Some(Self::Kazakh),
            "mn" | "mon" => Some(Self::Mongolian),
            "ug" | "uig" => Some(Self::Uyghur),
            _ => Self::from_name(&normalized),
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        let normalized = name.trim().to_ascii_lowercase();
        let normalized = normalized
            .replace(['_', '-'], " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        match normalized.as_str() {
            "chinese" | "simplified chinese" | "mandarin" => Some(Self::Chinese),
            "traditional chinese" => Some(Self::TraditionalChinese),
            "english" => Some(Self::English),
            "french" => Some(Self::French),
            "portuguese" | "brazilian portuguese" | "european portuguese" => Some(Self::Portuguese),
            "spanish" => Some(Self::Spanish),
            "japanese" => Some(Self::Japanese),
            "turkish" => Some(Self::Turkish),
            "russian" => Some(Self::Russian),
            "arabic" => Some(Self::Arabic),
            "korean" => Some(Self::Korean),
            "thai" => Some(Self::Thai),
            "italian" => Some(Self::Italian),
            "german" => Some(Self::German),
            "vietnamese" => Some(Self::Vietnamese),
            "malay" => Some(Self::Malay),
            "indonesian" => Some(Self::Indonesian),
            "filipino" | "tagalog" => Some(Self::Filipino),
            "hindi" => Some(Self::Hindi),
            "polish" => Some(Self::Polish),
            "czech" => Some(Self::Czech),
            "dutch" => Some(Self::Dutch),
            "khmer" => Some(Self::Khmer),
            "burmese" => Some(Self::Burmese),
            "persian" | "farsi" => Some(Self::Persian),
            "gujarati" => Some(Self::Gujarati),
            "urdu" => Some(Self::Urdu),
            "telugu" => Some(Self::Telugu),
            "marathi" => Some(Self::Marathi),
            "hebrew" => Some(Self::Hebrew),
            "bengali" => Some(Self::Bengali),
            "tamil" => Some(Self::Tamil),
            "ukrainian" => Some(Self::Ukrainian),
            "tibetan" => Some(Self::Tibetan),
            "kazakh" => Some(Self::Kazakh),
            "mongolian" => Some(Self::Mongolian),
            "uyghur" => Some(Self::Uyghur),
            "cantonese" => Some(Self::Cantonese),
            _ => None,
        }
    }

    /// Stable tag used for cache keys and settings.
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Chinese => "zh-Hans",
            Self::TraditionalChinese => "zh-Hant",
            Self::English => "en",
            Self::French => "fr",
            Self::Portuguese => "pt",
            Self::Spanish => "es",
            Self::Japanese => "ja",
            Self::Turkish => "tr",
            Self::Russian => "ru",
            Self::Arabic => "ar",
            Self::Korean => "ko",
            Self::Thai => "th",
            Self::Italian => "it",
            Self::German => "de",
            Self::Vietnamese => "vi",
            Self::Malay => "ms",
            Self::Indonesian => "id",
            Self::Filipino => "fil",
            Self::Hindi => "hi",
            Self::Polish => "pl",
            Self::Czech => "cs",
            Self::Dutch => "nl",
            Self::Khmer => "km",
            Self::Burmese => "my",
            Self::Persian => "fa",
            Self::Gujarati => "gu",
            Self::Urdu => "ur",
            Self::Telugu => "te",
            Self::Marathi => "mr",
            Self::Hebrew => "he",
            Self::Bengali => "bn",
            Self::Tamil => "ta",
            Self::Ukrainian => "uk",
            Self::Tibetan => "bo",
            Self::Kazakh => "kk",
            Self::Mongolian => "mn",
            Self::Uyghur => "ug",
            Self::Cantonese => "yue",
        }
    }

    /// Name used in an English Hy-MT2 prompt.
    #[must_use]
    pub const fn english_name(self) -> &'static str {
        match self {
            Self::Chinese => "Chinese",
            Self::TraditionalChinese => "Traditional Chinese",
            Self::English => "English",
            Self::French => "French",
            Self::Portuguese => "Portuguese",
            Self::Spanish => "Spanish",
            Self::Japanese => "Japanese",
            Self::Turkish => "Turkish",
            Self::Russian => "Russian",
            Self::Arabic => "Arabic",
            Self::Korean => "Korean",
            Self::Thai => "Thai",
            Self::Italian => "Italian",
            Self::German => "German",
            Self::Vietnamese => "Vietnamese",
            Self::Malay => "Malay",
            Self::Indonesian => "Indonesian",
            Self::Filipino => "Filipino",
            Self::Hindi => "Hindi",
            Self::Polish => "Polish",
            Self::Czech => "Czech",
            Self::Dutch => "Dutch",
            Self::Khmer => "Khmer",
            Self::Burmese => "Burmese",
            Self::Persian => "Persian",
            Self::Gujarati => "Gujarati",
            Self::Urdu => "Urdu",
            Self::Telugu => "Telugu",
            Self::Marathi => "Marathi",
            Self::Hebrew => "Hebrew",
            Self::Bengali => "Bengali",
            Self::Tamil => "Tamil",
            Self::Ukrainian => "Ukrainian",
            Self::Tibetan => "Tibetan",
            Self::Kazakh => "Kazakh",
            Self::Mongolian => "Mongolian",
            Self::Uyghur => "Uyghur",
            Self::Cantonese => "Cantonese",
        }
    }

    /// Name used in a Chinese Hy-MT2 prompt.
    #[must_use]
    pub const fn chinese_name(self) -> &'static str {
        match self {
            Self::Chinese => "中文",
            Self::TraditionalChinese => "繁体中文",
            Self::English => "英语",
            Self::French => "法语",
            Self::Portuguese => "葡萄牙语",
            Self::Spanish => "西班牙语",
            Self::Japanese => "日语",
            Self::Turkish => "土耳其语",
            Self::Russian => "俄语",
            Self::Arabic => "阿拉伯语",
            Self::Korean => "韩语",
            Self::Thai => "泰语",
            Self::Italian => "意大利语",
            Self::German => "德语",
            Self::Vietnamese => "越南语",
            Self::Malay => "马来语",
            Self::Indonesian => "印尼语",
            Self::Filipino => "菲律宾语",
            Self::Hindi => "印地语",
            Self::Polish => "波兰语",
            Self::Czech => "捷克语",
            Self::Dutch => "荷兰语",
            Self::Khmer => "高棉语",
            Self::Burmese => "缅甸语",
            Self::Persian => "波斯语",
            Self::Gujarati => "古吉拉特语",
            Self::Urdu => "乌尔都语",
            Self::Telugu => "泰卢固语",
            Self::Marathi => "马拉地语",
            Self::Hebrew => "希伯来语",
            Self::Bengali => "孟加拉语",
            Self::Tamil => "泰米尔语",
            Self::Ukrainian => "乌克兰语",
            Self::Tibetan => "藏语",
            Self::Kazakh => "哈萨克语",
            Self::Mongolian => "蒙古语",
            Self::Uyghur => "维吾尔语",
            Self::Cantonese => "粤语",
        }
    }

    /// Whether the model's Chinese instruction template applies.
    #[must_use]
    pub const fn uses_chinese_prompt(self) -> bool {
        matches!(
            self,
            Self::Chinese | Self::TraditionalChinese | Self::Cantonese
        )
    }

    /// Whether two tags describe the same written language.
    #[must_use]
    pub fn same_language(self, other: Self) -> bool {
        self == other || (self.uses_chinese_prompt() && other.uses_chinese_prompt())
    }
}

/// A request to translate one message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslationRequest {
    pub text: String,
    pub source: Option<Language>,
    pub target: Language,
}

impl TranslationRequest {
    #[must_use]
    pub fn new(text: impl Into<String>, target: Language) -> Self {
        Self {
            text: text.into(),
            source: None,
            target,
        }
    }

    #[must_use]
    pub const fn with_source(mut self, source: Language) -> Self {
        self.source = Some(source);
        self
    }
}

/// A successful translation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslationResult {
    pub text: String,
    pub source: Option<Language>,
    pub target: Language,
    pub engine: String,
}

impl TranslationResult {
    /// Cache key fragment for this result.
    #[must_use]
    pub fn cache_key(&self, model: &str) -> String {
        format!(
            "{}:{}:{}:{model}",
            self.target.tag(),
            self.source.map_or("und", Language::tag),
            self.engine
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_bcp47_and_legacy_tags() {
        assert_eq!(Language::from_tag("ko"), Some(Language::Korean));
        assert_eq!(Language::from_tag("fr-FR"), Some(Language::French));
        assert_eq!(Language::from_tag("pt_BR"), Some(Language::Portuguese));
        assert_eq!(Language::from_tag("zh-Hans"), Some(Language::Chinese));
        assert_eq!(
            Language::from_tag("zh-TW"),
            Some(Language::TraditionalChinese)
        );
        assert_eq!(Language::from_tag("fil"), Some(Language::Filipino));
        assert_eq!(Language::from_tag("he"), Some(Language::Hebrew));
        assert_eq!(Language::from_tag("Farsi"), Some(Language::Persian));
        assert_eq!(Language::from_tag("not-a-language"), None);
    }

    #[test]
    fn chinese_variants_share_the_chinese_prompt() {
        assert!(Language::Chinese.uses_chinese_prompt());
        assert!(Language::TraditionalChinese.uses_chinese_prompt());
        assert!(!Language::Japanese.uses_chinese_prompt());
        assert!(Language::Chinese.same_language(Language::TraditionalChinese));
    }

    #[test]
    fn tags_round_trip() {
        for language in Language::ALL {
            assert_eq!(Language::from_tag(language.tag()), Some(language));
            assert!(!language.english_name().is_empty());
            assert!(!language.chinese_name().is_empty());
        }
    }
}
