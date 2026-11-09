//! Optional desktop helper-process backend.
//!
//! This is the client side of the sandboxed helper. The helper is a separate
//! process with inherited stdin/stdout pipes; it opens no network listener.
//! On Linux, `sandbox` starts it under bubblewrap with no network and a
//! read-only model.

#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    pub sandbox: bool,
    /// Stop the helper after this much inactivity; zero disables shutdown.
    pub idle: Duration,
    /// Optional address-space limit for the helper; zero disables it.
    pub max_memory_mb: u64,
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
    state: Arc<SidecarState>,
    next_id: AtomicU64,
}

struct SidecarState {
    child: tokio::sync::Mutex<Option<ChildIo>>,
    last_used: AtomicU64,
    monitor_started: AtomicBool,
    idle: Duration,
}

impl SidecarState {
    fn new(idle: Duration) -> Self {
        Self {
            child: tokio::sync::Mutex::new(None),
            last_used: AtomicU64::new(epoch_secs()),
            monitor_started: AtomicBool::new(false),
            idle,
        }
    }

    fn touch(&self) {
        self.last_used.store(epoch_secs(), Ordering::Relaxed);
    }

    async fn unload_if_idle(&self) {
        if epoch_secs().saturating_sub(self.last_used.load(Ordering::Relaxed)) < self.idle.as_secs()
        {
            return;
        }
        let mut child = self.child.lock().await;
        if epoch_secs().saturating_sub(self.last_used.load(Ordering::Relaxed))
            >= self.idle.as_secs()
        {
            *child = None;
        }
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
                let Some(state) = weak.upgrade() else {
                    break;
                };
                state.unload_if_idle().await;
                if state.child.lock().await.is_none() {
                    state.monitor_started.store(false, Ordering::SeqCst);
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

fn helper_args(command: &mut Command, model: &Path, config: &SidecarConfig) {
    command
        .arg("--model")
        .arg(model)
        .arg("--model-id")
        .arg(&config.model_id)
        .arg("--threads")
        .arg(config.threads.to_string())
        .arg("--gpu-layers")
        .arg(config.gpu_layers.to_string())
        .arg("--max-memory-mb")
        .arg(config.max_memory_mb.to_string());
}

fn plain_command(config: &SidecarConfig) -> Command {
    let mut command = Command::new(&config.helper);
    helper_args(&mut command, &config.model, config);
    command
}

fn sandbox_command(config: &SidecarConfig) -> Command {
    let mut command = Command::new("bwrap");
    command
        .arg("--die-with-parent")
        .arg("--unshare-all")
        .arg("--new-session")
        .arg("--dev")
        .arg("/dev")
        .arg("--proc")
        .arg("/proc")
        .arg("--tmpfs")
        .arg("/tmp");
    for path in [
        "/nix/store",
        "/usr",
        "/bin",
        "/lib",
        "/lib64",
        "/sys",
        "/etc/ld.so.cache",
        "/etc/vulkan",
        "/run/opengl-driver",
        "/usr/share/vulkan",
    ] {
        if Path::new(path).exists() {
            command.arg("--ro-bind").arg(path).arg(path);
        }
    }
    if Path::new("/dev/dri").exists() {
        command.arg("--dev-bind").arg("/dev/dri").arg("/dev/dri");
    }
    for device in [
        "/dev/nvidiactl",
        "/dev/nvidia0",
        "/dev/nvidia-uvm",
        "/dev/nvidia-modeset",
    ] {
        if Path::new(device).exists() {
            command.arg("--dev-bind").arg(device).arg(device);
        }
    }
    command
        .arg("--ro-bind")
        .arg(&config.helper)
        .arg("/helper")
        .arg("--ro-bind")
        .arg(&config.model)
        .arg("/model.gguf")
        .arg("--chdir")
        .arg("/")
        .arg("/helper");
    helper_args(&mut command, Path::new("/model.gguf"), config);
    command
}

impl SidecarGenerator {
    #[must_use]
    pub fn new(config: SidecarConfig) -> Self {
        let state = Arc::new(SidecarState::new(config.idle));
        Self {
            config,
            state,
            next_id: AtomicU64::new(1),
        }
    }

    async fn spawn(config: &SidecarConfig) -> Result<ChildIo, TranslationError> {
        let mut command = if config.sandbox && cfg!(target_os = "linux") {
            sandbox_command(config)
        } else {
            plain_command(config)
        };
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| {
                TranslationError::NotConfigured(format!(
                    "translation helper could not start: {error}"
                ))
            })?;
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
        let mut child = self.state.child.lock().await;
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
        let mut guard = self.state.child.lock().await;
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
        drop(guard);
        self.state.touch();
        self.state.ensure_monitor();
        result
    }
}

/// Whether a helper binary exists at `path`.
#[must_use]
pub fn helper_exists(path: &Path) -> bool {
    path.is_file()
}
