//! Translation engine boundary and the protection/restore pipeline.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;

use crate::prompt::{build_prompt, max_output_tokens};
use crate::protect::{split, ProtectError, Segment};
use crate::types::{Language, TranslationRequest, TranslationResult};

/// A completion backend that turns a prompt into text.
///
/// Implementations are deliberately small: model loading, HTTP, sampling and
/// platform details live in the backend, not here.
#[async_trait]
pub trait PromptGenerator: Send + Sync {
    /// Stable identifier used in cache keys and diagnostics.
    fn id(&self) -> &str;

    /// Model name, when the backend knows one.
    fn model(&self) -> Option<&str> {
        None
    }

    /// Whether the backend can currently answer.
    fn is_available(&self) -> bool;

    /// Generate a completion for `prompt`.
    async fn generate(&self, prompt: &str, max_tokens: usize) -> Result<String, TranslationError>;
}

#[async_trait]
impl PromptGenerator for Arc<dyn PromptGenerator> {
    fn id(&self) -> &str {
        self.as_ref().id()
    }

    fn model(&self) -> Option<&str> {
        self.as_ref().model()
    }

    fn is_available(&self) -> bool {
        self.as_ref().is_available()
    }

    async fn generate(&self, prompt: &str, max_tokens: usize) -> Result<String, TranslationError> {
        self.as_ref().generate(prompt, max_tokens).await
    }
}

#[async_trait]
impl PromptGenerator for Box<dyn PromptGenerator> {
    fn id(&self) -> &str {
        self.as_ref().id()
    }

    fn model(&self) -> Option<&str> {
        self.as_ref().model()
    }

    fn is_available(&self) -> bool {
        self.as_ref().is_available()
    }

    async fn generate(&self, prompt: &str, max_tokens: usize) -> Result<String, TranslationError> {
        self.as_ref().generate(prompt, max_tokens).await
    }
}

/// Errors surfaced by translation backends.
#[derive(Debug, Error)]
pub enum TranslationError {
    #[error("the message is empty")]
    EmptyInput,
    #[error("the message is too long to translate ({actual} characters, limit {max})")]
    InputTooLong { max: usize, actual: usize },
    #[error("translation is not configured: {0}")]
    NotConfigured(String),
    #[error("translation engine failed: {0}")]
    Generation(String),
    #[error("translation timed out")]
    Timeout,
    #[error(transparent)]
    Protection(#[from] ProtectError),
    #[error("the translation was empty")]
    EmptyOutput,
    #[error("remote translation endpoint is not allowed: {0}")]
    RemoteEndpointDenied(String),
}

/// The high-level translation pipeline.
///
/// It protects chat markup, builds the model prompt, calls the backend, cleans
/// the response and restores the protected values. The original text is never
/// mutated.
pub struct Translator<G> {
    generator: G,
    max_input_chars: usize,
    known_emoji: HashSet<String>,
}

impl<G: PromptGenerator> Translator<G> {
    #[must_use]
    pub fn new(generator: G) -> Self {
        Self {
            generator,
            max_input_chars: 4000,
            known_emoji: HashSet::new(),
        }
    }

    #[must_use]
    pub const fn with_max_input_chars(mut self, max: usize) -> Self {
        self.max_input_chars = max;
        self
    }

    #[must_use]
    pub fn with_known_emoji(mut self, aliases: impl IntoIterator<Item = String>) -> Self {
        self.known_emoji = aliases.into_iter().collect();
        self
    }

    #[must_use]
    pub const fn generator(&self) -> &G {
        &self.generator
    }

    /// Translate one message.
    ///
    /// # Errors
    ///
    /// Returns an error for empty/oversized input, an unavailable backend, a
    /// backend failure, or a response that lost a protected placeholder.
    pub async fn translate(
        &self,
        request: TranslationRequest,
    ) -> Result<TranslationResult, TranslationError> {
        let text = request.text.trim();
        if text.is_empty() {
            return Err(TranslationError::EmptyInput);
        }
        let length = text.chars().count();
        if length > self.max_input_chars {
            return Err(TranslationError::InputTooLong {
                max: self.max_input_chars,
                actual: length,
            });
        }
        if !self.generator.is_available() {
            return Err(TranslationError::NotConfigured(
                "no translation model is installed".to_string(),
            ));
        }

        let segments = split(text, &self.known_emoji);
        if segments.len() == 1 && matches!(segments[0], Segment::Text(_)) {
            let translated = self
                .translate_text(text, request.source, request.target)
                .await?;
            return Ok(TranslationResult {
                text: translated,
                source: request.source,
                target: request.target,
                engine: self.generator.id().to_string(),
            });
        }

        let mut output = String::new();
        for segment in segments {
            match segment {
                Segment::Literal(literal) => output.push_str(&literal),
                Segment::Text(segment) => {
                    let leading = segment.len() - segment.trim_start().len();
                    let trailing = segment.len() - segment.trim_end().len();
                    let core = &segment[leading..segment.len() - trailing];
                    if core.is_empty() || !core.chars().any(char::is_alphanumeric) {
                        output.push_str(&segment);
                        continue;
                    }
                    let translated = self
                        .translate_text(core, request.source, request.target)
                        .await?;
                    output.push_str(&segment[..leading]);
                    output.push_str(translated.trim());
                    output.push_str(&segment[segment.len() - trailing..]);
                }
            }
        }
        Ok(TranslationResult {
            text: output,
            source: request.source,
            target: request.target,
            engine: self.generator.id().to_string(),
        })
    }

    async fn translate_text(
        &self,
        text: &str,
        source: Option<Language>,
        target: Language,
    ) -> Result<String, TranslationError> {
        let prompt = build_prompt(target, source, text);
        let raw = self
            .generator
            .generate(&prompt, max_output_tokens(text))
            .await?;
        let cleaned = clean_output(&raw).trim();
        if cleaned.is_empty() {
            return Err(TranslationError::EmptyOutput);
        }
        Ok(cleaned.to_string())
    }
}

fn clean_output(output: &str) -> &str {
    let mut text = output.trim();
    for prefix in ["Translation:", "translation:", "翻译：", "翻译:"] {
        if let Some(rest) = text.strip_prefix(prefix) {
            text = rest.trim();
            break;
        }
    }
    text
}

/// A backend that cannot translate, used when no model is installed.
#[derive(Debug, Default)]
pub struct UnavailableGenerator;

#[async_trait]
impl PromptGenerator for UnavailableGenerator {
    fn id(&self) -> &'static str {
        "unavailable"
    }

    fn is_available(&self) -> bool {
        false
    }

    async fn generate(
        &self,
        _prompt: &str,
        _max_tokens: usize,
    ) -> Result<String, TranslationError> {
        Err(TranslationError::NotConfigured(
            "no translation model is installed".to_string(),
        ))
    }
}

/// A deterministic backend for tests and for exercising the pipeline without
/// downloading a model.
pub struct FakeGenerator {
    id: String,
    response: Box<dyn Fn(&str) -> String + Send + Sync>,
}

impl FakeGenerator {
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        response: impl Fn(&str) -> String + Send + Sync + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            response: Box::new(response),
        }
    }

    /// Return the source text (the part after the prompt's blank line).
    #[must_use]
    pub fn echoing(id: impl Into<String>) -> Self {
        Self::new(id, |prompt| {
            prompt
                .rsplit_once("\n\n")
                .map_or_else(|| prompt.to_string(), |(_, text)| text.to_string())
        })
    }

    /// Always return the same text.
    #[must_use]
    pub fn fixed(id: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        Self::new(id, move |_| text.clone())
    }
}

#[async_trait]
impl PromptGenerator for FakeGenerator {
    fn id(&self) -> &str {
        &self.id
    }

    fn model(&self) -> Option<&str> {
        Some("fake")
    }

    fn is_available(&self) -> bool {
        true
    }

    async fn generate(&self, prompt: &str, _max_tokens: usize) -> Result<String, TranslationError> {
        Ok((self.response)(prompt))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[tokio::test]
    async fn translates_and_keeps_the_engine_id() {
        let translator = Translator::new(FakeGenerator::fixed("fake", "Bonjour"));
        let result = translator
            .translate(TranslationRequest::new("안녕하세요", Language::French))
            .await
            .unwrap();
        assert_eq!(result.text, "Bonjour");
        assert_eq!(result.target, Language::French);
        assert_eq!(result.engine, "fake");
    }

    #[tokio::test]
    async fn protects_links_and_mentions_through_the_pipeline() {
        let translator = Translator::new(FakeGenerator::echoing("echo"));
        let text = "Salut @alice, va sur https://example.org/a?b=1";
        let result = translator
            .translate(TranslationRequest::new(text, Language::French))
            .await
            .unwrap();
        assert_eq!(result.text, text);
    }

    #[tokio::test]
    async fn segment_translation_keeps_literals_exact() {
        let translator = Translator::new(FakeGenerator::new("upper", |prompt| {
            prompt
                .rsplit_once("\n\n")
                .map_or_else(String::new, |(_, text)| text.to_uppercase())
        }));
        let text = "salut @alice, lis https://example.org/a?b=1";
        let result = translator
            .translate(TranslationRequest::new(text, Language::French))
            .await
            .unwrap();
        assert_eq!(result.text, "SALUT @alice, LIS https://example.org/a?b=1");
    }

    #[tokio::test]
    async fn prompt_contains_the_target_language() {
        let prompts = Arc::new(Mutex::new(Vec::new()));
        let captured = prompts.clone();
        let generator = FakeGenerator::new("capture", move |prompt| {
            captured.lock().unwrap().push(prompt.to_string());
            "bonjour".to_string()
        });
        let translator = Translator::new(generator);
        translator
            .translate(TranslationRequest::new("안녕하세요", Language::French))
            .await
            .unwrap();
        let prompts = prompts.lock().unwrap();
        assert!(prompts[0].contains("into French"));
    }

    #[tokio::test]
    async fn rejects_empty_and_oversized_input() {
        let translator = Translator::new(FakeGenerator::echoing("echo")).with_max_input_chars(4);
        assert!(matches!(
            translator
                .translate(TranslationRequest::new("   ", Language::French))
                .await,
            Err(TranslationError::EmptyInput)
        ));
        assert!(matches!(
            translator
                .translate(TranslationRequest::new("abcde", Language::French))
                .await,
            Err(TranslationError::InputTooLong { .. })
        ));
    }

    #[tokio::test]
    async fn unavailable_backend_is_reported() {
        let translator = Translator::new(UnavailableGenerator);
        assert!(matches!(
            translator
                .translate(TranslationRequest::new("hello", Language::French))
                .await,
            Err(TranslationError::NotConfigured(_))
        ));
    }

    #[tokio::test]
    async fn cleans_common_output_prefixes() {
        let translator = Translator::new(FakeGenerator::fixed("fake", "  Translation:  bonjour  "));
        let result = translator
            .translate(TranslationRequest::new("hello", Language::French))
            .await
            .unwrap();
        assert_eq!(result.text, "bonjour");
    }
}
