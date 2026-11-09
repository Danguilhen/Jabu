//! Optional desktop helper-process backend.
//!
//! This is the client side of the sandbox experiment. The helper is a separate
//! process with inherited stdin/stdout pipes; it opens no network listener.
//! OS-level isolation (bwrap/seccomp/Job Objects) is not applied yet.

#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use crate::engine::{PromptGenerator, TranslationError};

/// How to start the helper process.
#[derive(Debug, Clone)]
pub struct SidecarConfig {
    pub helper: PathBuf,
    pub model: PathBuf,
    pub model_id: String,
    pub threads: i32,
    pub context: u32,
    pub gpu_layers: i32,
}

impl SidecarConfig {
    /// Find the helper next to the running executable.
    #[must_use]
    pub fn default_helper_path() -> Option<PathBuf> {
        let executable = std::env::current_exe().ok()?;
        let directory = executable.parent()?;
        let name = if cfg!(windows) {
            "jabu-translate-helper.exe"
        } else {
            "jabu-translate-helper"
        };
        let path = directory.join(name);
        path.is_file().then_some(path)
    }
}

/// A [`PromptGenerator`] backed by a separate helper process.
pub struct SidecarGenerator {
    config: SidecarConfig,
    child: tokio::sync::Mutex<Option<ChildIo>>,
    next_id: AtomicU64,
}

struct ChildIo {
    _child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: Lines<BufReader<ChildStdout>>,
}

#[derive(Debug, Deserialize)]
struct Response {
    id: u64,
    text: Option<String>,
    error: Option<String>,
}

impl SidecarGenerator {
    #[must_use]
    pub fn new(config: SidecarConfig) -> Self {
        Self {
            config,
            child: tokio::sync::Mutex::new(None),
            next_id: AtomicU64::new(1),
        }
    }

    async fn spawn(config: &SidecarConfig) -> Result<ChildIo, TranslationError> {
        let mut child = Command::new(&config.helper)
            .arg("--model")
            .arg(&config.model)
            .arg("--model-id")
            .arg(&config.model_id)
            .arg("--threads")
            .arg(config.threads.to_string())
            .arg("--gpu-layers")
            .arg(config.gpu_layers.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| TranslationError::Generation(error.to_string()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| TranslationError::Generation("helper stdin unavailable".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| TranslationError::Generation("helper stdout unavailable".into()))?;
        Ok(ChildIo {
            _child: child,
            stdin: BufWriter::new(stdin),
            stdout: BufReader::new(stdout).lines(),
        })
    }

    async fn roundtrip(
        io: &mut ChildIo,
        id: u64,
        prompt: &str,
        max_tokens: usize,
    ) -> Result<String, TranslationError> {
        let request = serde_json::json!({
            "id": id,
            "prompt": prompt,
            "max_tokens": max_tokens,
        });
        io.stdin
            .write_all(format!("{request}\n").as_bytes())
            .await
            .map_err(|error| TranslationError::Generation(error.to_string()))?;
        io.stdin
            .flush()
            .await
            .map_err(|error| TranslationError::Generation(error.to_string()))?;
        let line = io
            .stdout
            .next_line()
            .await
            .map_err(|error| TranslationError::Generation(error.to_string()))?
            .ok_or_else(|| TranslationError::Generation("translation helper exited".into()))?;
        let response: Response = serde_json::from_str(&line)
            .map_err(|error| TranslationError::Generation(error.to_string()))?;
        if response.id != id {
            return Err(TranslationError::Generation(
                "translation helper returned a mismatched response".into(),
            ));
        }
        if let Some(error) = response.error {
            return Err(TranslationError::Generation(error));
        }
        response
            .text
            .filter(|text| !text.trim().is_empty())
            .ok_or(TranslationError::EmptyOutput)
    }

    /// Stop the helper process.
    pub async fn shutdown(&self) {
        let mut child = self.child.lock().await;
        *child = None;
    }

    /// The helper/model pair this backend will use.
    #[must_use]
    pub fn config(&self) -> &SidecarConfig {
        &self.config
    }
}

#[async_trait]
impl PromptGenerator for SidecarGenerator {
    fn id(&self) -> &'static str {
        "sidecar"
    }

    fn model(&self) -> Option<&str> {
        Some(&self.config.model_id)
    }

    fn is_available(&self) -> bool {
        self.config.helper.is_file() && self.config.model.is_file()
    }

    async fn generate(&self, prompt: &str, max_tokens: usize) -> Result<String, TranslationError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut guard = self.child.lock().await;
        if guard.is_none() {
            *guard = Some(Self::spawn(&self.config).await?);
        }
        let result = {
            let io = guard
                .as_mut()
                .ok_or_else(|| TranslationError::Generation("translation helper missing".into()))?;
            Self::roundtrip(io, id, prompt, max_tokens).await
        };
        if result.is_err() {
            *guard = None;
        }
        result
    }
}

/// Whether a helper binary exists at `path`.
#[must_use]
pub fn helper_exists(path: &Path) -> bool {
    path.is_file()
}
