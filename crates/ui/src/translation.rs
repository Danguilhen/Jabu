//! UI-side translation cache types.

/// A translated message ready to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslationView {
    pub text: String,
    /// Detected or declared source language tag.
    pub source: Option<String>,
    /// Target language tag.
    pub target: String,
    /// Model or engine identifier, for diagnostics.
    pub model: Option<String>,
}
