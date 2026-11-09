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
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
        if let Some(descriptor) = find(&model_id) {
            let metadata = path.metadata().map_err(|error| {
                TranslationError::NotConfigured(format!("{} is missing: {error}", descriptor.name))
            })?;
            if metadata.len() != descriptor.size_bytes {
                return Err(TranslationError::NotConfigured(format!(
                    "{} has the wrong size; remove it and download it again",
                    descriptor.name
                )));
            }
        }
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
struct ModelSlot {
    inner: tokio::sync::Mutex<Option<Arc<EmbeddedGenerator>>>,
    last_used: AtomicU64,
    active: AtomicUsize,
    idle: Duration,
    monitor_started: AtomicBool,
}

impl ModelSlot {
    fn new(idle: Duration) -> Self {
        Self {
            inner: tokio::sync::Mutex::new(None),
            last_used: AtomicU64::new(epoch_secs()),
            active: AtomicUsize::new(0),
            idle,
            monitor_started: AtomicBool::new(false),
        }
    }

    async fn get_or_load(
        &self,
        path: PathBuf,
        model_id: String,
        threads: i32,
        context: u32,
    ) -> Result<Arc<EmbeddedGenerator>, TranslationError> {
        let mut guard = self.inner.lock().await;
        if let Some(model) = guard.as_ref() {
            self.last_used.store(epoch_secs(), Ordering::Relaxed);
            return Ok(model.clone());
        }
        let model = Arc::new(EmbeddedGenerator::load(path, model_id, threads, context).await?);
        *guard = Some(model.clone());
        self.last_used.store(epoch_secs(), Ordering::Relaxed);
        Ok(model)
    }

    fn touch(&self) {
        self.last_used.store(epoch_secs(), Ordering::Relaxed);
    }

    async fn unload_if_idle(&self) {
        if self.active.load(Ordering::Relaxed) != 0 {
            return;
        }
        if epoch_secs().saturating_sub(self.last_used.load(Ordering::Relaxed)) < self.idle.as_secs()
        {
            return;
        }
        let mut guard = self.inner.lock().await;
        *guard = None;
    }

    fn ensure_monitor(self: &Arc<Self>) {
        if self.idle.is_zero() || self.monitor_started.swap(true, Ordering::SeqCst) {
            return;
        }
        let weak = Arc::downgrade(self);
        let interval = (self.idle / 2).max(Duration::from_secs(5));
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                let Some(slot) = weak.upgrade() else {
                    break;
                };
                slot.unload_if_idle().await;
                if slot.inner.lock().await.is_none() {
                    slot.monitor_started.store(false, Ordering::SeqCst);
                    break;
                }
            }
        });
    }
}

fn epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Load the model on first use and unload it after an idle period.
pub struct LazyEmbeddedGenerator {
    path: PathBuf,
    model_id: String,
    threads: i32,
    context: u32,
    available: bool,
    slot: Arc<ModelSlot>,
}

impl LazyEmbeddedGenerator {
    #[must_use]
    pub fn new(
        path: impl Into<PathBuf>,
        model_id: impl Into<String>,
        threads: i32,
        context: u32,
    ) -> Self {
        Self::with_idle_timeout(path, model_id, threads, context, Duration::from_secs(600))
    }

    #[must_use]
    pub fn with_idle_timeout(
        path: impl Into<PathBuf>,
        model_id: impl Into<String>,
        threads: i32,
        context: u32,
        idle: Duration,
    ) -> Self {
        let path = path.into();
        let available = path.is_file();
        Self {
            path,
            model_id: model_id.into(),
            threads,
            context,
            available,
            slot: Arc::new(ModelSlot::new(idle)),
        }
    }

    #[must_use]
    pub fn path(&self) -> &std::path::Path {
        &self.path
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
        let slot = self.slot.clone();
        slot.active.fetch_add(1, Ordering::SeqCst);
        let result = async {
            let model = slot
                .get_or_load(
                    self.path.clone(),
                    self.model_id.clone(),
                    self.threads,
                    self.context,
                )
                .await?;
            let result = model.generate(prompt, max_tokens).await;
            slot.touch();
            result
        }
        .await;
        slot.active.fetch_sub(1, Ordering::SeqCst);
        slot.ensure_monitor();
        result
    }
}
