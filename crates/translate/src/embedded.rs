//! Embedded llama.cpp backend.
//!
//! The runtime is linked into Jabu behind the `embedded` feature. Models are
//! ordinary GGUF files installed by [`crate::models::ModelStore`]. Inference
//! runs on a blocking worker thread; the async API never blocks the UI.
//!
//! The context window is deliberately small. Hy-MT2 supports a 262k context,
//! but a chat message only needs a few thousand tokens, and the default
//! context would allocate a very large KV cache.

#![cfg(feature = "embedded")]

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::{LlamaChatMessage, LlamaChatTemplate, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;

use crate::engine::{PromptGenerator, TranslationError};
use crate::models::{find, HardwareProfile};

/// Context window used for chat messages.
pub const DEFAULT_CONTEXT: u32 = 4096;

static BACKEND: OnceLock<LlamaBackend> = OnceLock::new();
static BACKEND_INIT: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn backend() -> Result<&'static LlamaBackend, TranslationError> {
    if let Some(backend) = BACKEND.get() {
        return Ok(backend);
    }
    let _guard = BACKEND_INIT
        .lock()
        .map_err(|_| TranslationError::Generation("llama backend lock poisoned".into()))?;
    if let Some(backend) = BACKEND.get() {
        return Ok(backend);
    }
    let initialized =
        LlamaBackend::init().map_err(|error| TranslationError::Generation(error.to_string()))?;
    match BACKEND.set(initialized) {
        Ok(()) => BACKEND
            .get()
            .ok_or_else(|| TranslationError::Generation("llama backend was not stored".into())),
        Err(unused) => {
            // Another thread won the race. Dropping this handle would clear the
            // global initialization flag, so let it leak instead.
            std::mem::forget(unused);
            BACKEND
                .get()
                .ok_or_else(|| TranslationError::Generation("llama backend is unavailable".into()))
        }
    }
}

/// A loaded llama.cpp model.
pub struct EmbeddedGenerator {
    backend: &'static LlamaBackend,
    model: Arc<LlamaModel>,
    template: Option<LlamaChatTemplate>,
    model_id: String,
    threads: i32,
    context: u32,
}

impl EmbeddedGenerator {
    /// Load a model from disk.
    ///
    /// # Errors
    ///
    /// Returns an error when the model is missing, the device lacks the
    /// configured free memory, or llama.cpp rejects the file.
    pub async fn load(
        path: impl Into<PathBuf>,
        model_id: impl Into<String>,
        threads: i32,
        context: u32,
    ) -> Result<Self, TranslationError> {
        let path = path.into();
        let model_id = model_id.into();
        let hardware = HardwareProfile::detect();
        if let Some(descriptor) = find(&model_id) {
            if !hardware.fits_now(descriptor) {
                return Err(TranslationError::NotConfigured(format!(
                    "not enough free memory for {}; close other applications or use the smaller model",
                    descriptor.name
                )));
            }
        }
        tokio::task::spawn_blocking(move || Self::load_blocking(path, model_id, threads, context))
            .await
            .map_err(|error| TranslationError::Generation(error.to_string()))?
    }

    fn load_blocking(
        path: PathBuf,
        model_id: String,
        threads: i32,
        context: u32,
    ) -> Result<Self, TranslationError> {
        let backend = backend()?;
        let model = LlamaModel::load_from_file(
            backend,
            &path,
            &llama_cpp_2::model::params::LlamaModelParams::default(),
        )
        .map_err(|error| TranslationError::Generation(error.to_string()))?;
        let template = model.chat_template(None).ok();
        Ok(Self {
            backend,
            model: Arc::new(model),
            template,
            model_id,
            threads: threads.max(1),
            context: context.max(512),
        })
    }

    #[must_use]
    pub const fn context_size(&self) -> u32 {
        self.context
    }
}

#[async_trait]
impl PromptGenerator for EmbeddedGenerator {
    fn id(&self) -> &'static str {
        "embedded"
    }

    fn model(&self) -> Option<&str> {
        Some(&self.model_id)
    }

    fn is_available(&self) -> bool {
        true
    }

    async fn generate(&self, prompt: &str, max_tokens: usize) -> Result<String, TranslationError> {
        let backend = self.backend;
        let model = self.model.clone();
        let template = self.template.clone();
        let prompt = prompt.to_string();
        let threads = self.threads;
        let context = self.context;
        tokio::task::spawn_blocking(move || {
            generate_blocking(
                backend,
                &model,
                template.as_ref(),
                &prompt,
                max_tokens,
                threads,
                context,
            )
        })
        .await
        .map_err(|error| TranslationError::Generation(error.to_string()))?
    }
}

#[allow(clippy::too_many_arguments)]
fn generate_blocking(
    backend: &LlamaBackend,
    model: &LlamaModel,
    template: Option<&LlamaChatTemplate>,
    prompt: &str,
    max_tokens: usize,
    threads: i32,
    context: u32,
) -> Result<String, TranslationError> {
    let prompt = apply_template(model, template, prompt);
    let context_params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(context))
        .with_n_threads(threads)
        .with_n_threads_batch(threads);
    let mut context = model
        .new_context(backend, context_params)
        .map_err(|error| TranslationError::Generation(error.to_string()))?;

    let tokens = model.vocab().tokenize(prompt.as_bytes(), true, true);
    if tokens.is_empty() {
        return Err(TranslationError::EmptyInput);
    }
    let capacity = usize::try_from(context.n_ctx()).unwrap_or(0);
    if tokens.len() + 1 >= capacity {
        return Err(TranslationError::InputTooLong {
            max: capacity.saturating_sub(1),
            actual: tokens.len(),
        });
    }
    let budget = max_tokens.min(capacity - tokens.len() - 1).max(1);

    let mut batch = LlamaBatch::new(512, 1);
    let mut position = 0i32;
    let mut logits_index = 0i32;
    for chunk in tokens.chunks(512) {
        batch.clear();
        for (offset, token) in chunk.iter().enumerate() {
            let absolute = position + i32::try_from(offset).unwrap_or(0);
            let is_last = usize::try_from(absolute).unwrap_or(0) == tokens.len() - 1;
            batch
                .add(*token, absolute, &[0], is_last)
                .map_err(|error| TranslationError::Generation(error.to_string()))?;
        }
        context
            .decode(&mut batch)
            .map_err(|error| TranslationError::Generation(error.to_string()))?;
        logits_index = batch.n_tokens() - 1;
        position += i32::try_from(chunk.len()).unwrap_or(0);
    }

    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::penalties(model.n_vocab(), 64, 1.1, 0.0, 0.0),
        LlamaSampler::greedy(),
    ]);
    let mut output = Vec::new();
    for _ in 0..budget {
        let token = sampler.sample(&context, logits_index);
        sampler.accept(token);
        if model.vocab().is_eog(token) {
            break;
        }
        output.push(token);
        batch.clear();
        batch
            .add(token, position, &[0], true)
            .map_err(|error| TranslationError::Generation(error.to_string()))?;
        position += 1;
        context
            .decode(&mut batch)
            .map_err(|error| TranslationError::Generation(error.to_string()))?;
        logits_index = 0;
    }

    let bytes = model.vocab().detokenize(&output, true, false);
    let text = String::from_utf8_lossy(&bytes).trim().to_string();
    if text.is_empty() {
        return Err(TranslationError::EmptyOutput);
    }
    Ok(text)
}

fn apply_template(
    model: &LlamaModel,
    template: Option<&LlamaChatTemplate>,
    prompt: &str,
) -> String {
    let Some(template) = template else {
        return prompt.to_string();
    };
    let Ok(message) = LlamaChatMessage::new("user".to_string(), prompt.to_string()) else {
        return prompt.to_string();
    };
    model
        .apply_chat_template(template, &[message], true)
        .unwrap_or_else(|_| prompt.to_string())
}

/// Load the model on first use instead of at application startup.
pub struct LazyEmbeddedGenerator {
    path: PathBuf,
    model_id: String,
    threads: i32,
    context: u32,
    available: bool,
    inner: tokio::sync::OnceCell<EmbeddedGenerator>,
}

impl LazyEmbeddedGenerator {
    #[must_use]
    pub fn new(
        path: impl Into<PathBuf>,
        model_id: impl Into<String>,
        threads: i32,
        context: u32,
    ) -> Self {
        let path = path.into();
        let available = path.is_file();
        Self {
            path,
            model_id: model_id.into(),
            threads,
            context,
            available,
            inner: tokio::sync::OnceCell::new(),
        }
    }

    #[must_use]
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    async fn loaded(&self) -> Result<&EmbeddedGenerator, TranslationError> {
        self.inner
            .get_or_try_init(|| {
                EmbeddedGenerator::load(
                    self.path.clone(),
                    self.model_id.clone(),
                    self.threads,
                    self.context,
                )
            })
            .await
    }
}

#[async_trait]
impl PromptGenerator for LazyEmbeddedGenerator {
    fn id(&self) -> &'static str {
        "embedded"
    }

    fn model(&self) -> Option<&str> {
        Some(&self.model_id)
    }

    fn is_available(&self) -> bool {
        self.available
    }

    async fn generate(&self, prompt: &str, max_tokens: usize) -> Result<String, TranslationError> {
        self.loaded().await?.generate(prompt, max_tokens).await
    }
}
