//! Model manifest, hardware policy, download and installation.
//!
//! Weights are not shipped in the installer. The manifest pins a revision and
//! a SHA-256; downloads are verified before they become loadable. The store is
//! a normal directory under the app's data directory so a model survives cache
//! eviction.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::StreamExt;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::AsyncWriteExt;

/// One pinned model file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelDescriptor {
    pub id: &'static str,
    pub name: &'static str,
    pub file_name: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size_bytes: u64,
    pub license: &'static str,
    pub quantization: &'static str,
    /// Minimum free memory before a load is attempted.
    pub min_ram_mb: u64,
    /// Memory at which the model is the recommended choice.
    pub recommended_ram_mb: u64,
}

/// Models Jabu knows how to install.
pub const MODELS: &[ModelDescriptor] = &[
    ModelDescriptor {
        id: "hy-mt2-1.8b-q4-k-m",
        name: "Hy-MT2 1.8B",
        file_name: "Hy-MT2-1.8B-Q4_K_M.gguf",
        url: "https://huggingface.co/tencent/Hy-MT2-1.8B-GGUF/resolve/a0c709d9fac510f2c807aa3af52872340dc37a4a/Hy-MT2-1.8B-Q4_K_M.gguf",
        sha256: "dc5f44fcf1fa496ee7ad725982c0c8c553a4de00259b53af84c4b89fb0c06699",
        size_bytes: 1_133_080_448,
        license: "Apache-2.0",
        quantization: "Q4_K_M",
        min_ram_mb: 4096,
        recommended_ram_mb: 8192,
    },
    ModelDescriptor {
        id: "hy-mt2-7b-q4-k-m",
        name: "Hy-MT2 7B",
        file_name: "Hy-MT2-7B-Q4_K_M.gguf",
        url: "https://huggingface.co/tencent/Hy-MT2-7B-GGUF/resolve/ab8472660ac61fac25f1af43fac2599d52a8a775/Hy-MT2-7B-Q4_K_M.gguf",
        sha256: "9f96256500f3fc1ab4d64336b58f52a949a95ad7516b0c229476eef782f9f77b",
        size_bytes: 4_624_648_896,
        license: "Apache-2.0",
        quantization: "Q4_K_M",
        min_ram_mb: 12_288,
        recommended_ram_mb: 16_384,
    },
];

/// Find a model by its stable id.
#[must_use]
pub fn find(id: &str) -> Option<&'static ModelDescriptor> {
    MODELS.iter().find(|model| model.id == id)
}

/// Errors from model installation and verification.
#[derive(Debug, Error)]
pub enum ModelError {
    #[error("unknown translation model: {0}")]
    UnknownModel(String),
    #[error("model file is missing: {0}")]
    NotInstalled(String),
    #[error("model file has {actual} bytes, expected {expected}")]
    Size { expected: u64, actual: u64 },
    #[error("model checksum mismatch: expected {expected}, got {actual}")]
    Checksum { expected: String, actual: String },
    #[error("model download failed: {0}")]
    Http(String),
    #[error("model I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("model worker failed: {0}")]
    Join(String),
}

/// Optional progress callback: `(downloaded, total)` in bytes.
pub type Progress = Arc<dyn Fn(u64, u64) + Send + Sync>;

/// A directory of installed GGUF files.
#[derive(Debug, Clone)]
pub struct ModelStore {
    root: PathBuf,
}

impl ModelStore {
    /// Create a store rooted at `root`, creating it with private permissions.
    ///
    /// # Errors
    ///
    /// Returns an error when the directory cannot be created.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, ModelError> {
        let root = root.into();
        std::fs::create_dir_all(&root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700));
        }
        Ok(Self { root })
    }

    /// The platform's normal model directory.
    ///
    /// # Errors
    ///
    /// Returns an error when no data directory is available or it cannot be
    /// created.
    pub fn default_path() -> Result<Self, ModelError> {
        let root = dirs::data_dir()
            .ok_or_else(|| ModelError::Io(std::io::Error::other("no data directory")))?
            .join("jabu")
            .join("models");
        Self::new(root)
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn path(&self, model: &ModelDescriptor) -> PathBuf {
        self.root.join(model.file_name)
    }

    /// A cheap installed check: the file exists and has the pinned size.
    ///
    /// This does not hash multi-gigabyte files on every launch. A full check is
    /// performed when a model is installed and can be requested explicitly.
    #[must_use]
    pub fn is_installed(&self, model: &ModelDescriptor) -> bool {
        self.path(model)
            .metadata()
            .is_ok_and(|metadata| metadata.len() == model.size_bytes)
    }

    /// Installed models, in manifest order.
    #[must_use]
    pub fn installed(&self) -> Vec<&'static ModelDescriptor> {
        MODELS
            .iter()
            .filter(|model| self.is_installed(model))
            .collect()
    }

    /// Verify a model's size and checksum.
    ///
    /// # Errors
    ///
    /// Returns an error when the file is missing, the wrong size, or has a
    /// different SHA-256.
    pub fn verify(&self, model: &ModelDescriptor) -> Result<(), ModelError> {
        let path = self.path(model);
        let metadata = path
            .metadata()
            .map_err(|_| ModelError::NotInstalled(path.display().to_string()))?;
        if metadata.len() != model.size_bytes {
            return Err(ModelError::Size {
                expected: model.size_bytes,
                actual: metadata.len(),
            });
        }
        let actual = sha256_file(&path)?;
        if actual != model.sha256 {
            return Err(ModelError::Checksum {
                expected: model.sha256.to_string(),
                actual,
            });
        }
        Ok(())
    }

    /// Download and install a model.
    ///
    /// # Errors
    ///
    /// Returns an error for a network failure, a size/checksum mismatch or an
    /// I/O failure. A partial download is removed.
    pub async fn download(
        &self,
        model: &ModelDescriptor,
        progress: Option<Progress>,
    ) -> Result<PathBuf, ModelError> {
        let destination = self.path(model);
        if self.is_installed(model) {
            return Ok(destination);
        }
        let temporary = destination.with_extension("gguf.part");
        let _ = tokio::fs::remove_file(&temporary).await;
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::limited(5))
            .user_agent("Jabu model download")
            .build()
            .map_err(|error| ModelError::Http(error.to_string()))?;
        let response = client
            .get(model.url)
            .send()
            .await
            .map_err(|error| ModelError::Http(error.to_string()))?;
        if !response.status().is_success() {
            return Err(ModelError::Http(format!(
                "download returned HTTP {}",
                response.status()
            )));
        }
        if let Some(length) = response.content_length() {
            if length != model.size_bytes {
                return Err(ModelError::Size {
                    expected: model.size_bytes,
                    actual: length,
                });
            }
        }

        let mut file = tokio::fs::File::create(&temporary).await?;
        let mut hasher = Sha256::new();
        let mut downloaded = 0u64;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| ModelError::Http(error.to_string()))?;
            hasher.update(&chunk);
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            if downloaded > model.size_bytes {
                drop(file);
                let _ = tokio::fs::remove_file(&temporary).await;
                return Err(ModelError::Size {
                    expected: model.size_bytes,
                    actual: downloaded,
                });
            }
            if let Some(progress) = &progress {
                progress(downloaded, model.size_bytes);
            }
        }
        file.flush().await?;
        file.sync_all().await?;
        drop(file);

        if downloaded != model.size_bytes {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(ModelError::Size {
                expected: model.size_bytes,
                actual: downloaded,
            });
        }
        let actual = format!("{:x}", hasher.finalize());
        if actual != model.sha256 {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(ModelError::Checksum {
                expected: model.sha256.to_string(),
                actual,
            });
        }
        let _ = tokio::fs::remove_file(&destination).await;
        tokio::fs::rename(&temporary, &destination).await?;
        set_private_permissions(&destination);
        Ok(destination)
    }

    /// Import a user-supplied GGUF file.
    ///
    /// The file is treated as untrusted: its size and checksum must match the
    /// manifest before it is installed.
    ///
    /// # Errors
    ///
    /// Returns an error when the source is unreadable, the size/checksum does
    /// not match, or the file cannot be copied.
    pub async fn install_from_file(
        &self,
        model: &ModelDescriptor,
        source: impl Into<PathBuf>,
    ) -> Result<PathBuf, ModelError> {
        let source = source.into();
        let destination = self.path(model);
        let temporary = destination.with_extension("gguf.part");
        let source_for_worker = source.clone();
        let temporary_for_worker = temporary.clone();
        let expected_size = model.size_bytes;
        let expected_hash = model.sha256.to_string();
        tokio::task::spawn_blocking(move || {
            let metadata = std::fs::metadata(&source_for_worker)?;
            if metadata.len() != expected_size {
                return Err(ModelError::Size {
                    expected: expected_size,
                    actual: metadata.len(),
                });
            }
            let mut input = std::fs::File::open(&source_for_worker)?;
            let mut output = std::fs::File::create(&temporary_for_worker)?;
            let mut hasher = Sha256::new();
            let mut buffer = vec![0u8; 1024 * 1024];
            loop {
                let read = input.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
                output.write_all(&buffer[..read])?;
            }
            output.sync_all()?;
            let actual = format!("{:x}", hasher.finalize());
            if actual != expected_hash {
                let _ = std::fs::remove_file(&temporary_for_worker);
                return Err(ModelError::Checksum {
                    expected: expected_hash,
                    actual,
                });
            }
            Ok::<_, ModelError>(())
        })
        .await
        .map_err(|error| ModelError::Join(error.to_string()))??;
        let _ = tokio::fs::remove_file(&destination).await;
        tokio::fs::rename(&temporary, &destination).await?;
        set_private_permissions(&destination);
        Ok(destination)
    }

    /// Remove an installed model.
    ///
    /// # Errors
    ///
    /// Returns an error when the file exists but cannot be removed.
    pub fn remove(&self, model: &ModelDescriptor) -> Result<(), ModelError> {
        let path = self.path(model);
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(ModelError::Io(error)),
        }
    }
}

fn sha256_file(path: &Path) -> Result<String, ModelError> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn set_private_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
}

/// Memory, cores and CPU features relevant to model selection.
#[derive(Debug, Clone)]
pub struct HardwareProfile {
    pub total_ram_mb: u64,
    pub available_ram_mb: u64,
    pub physical_cores: usize,
    pub os: &'static str,
    pub arch: &'static str,
    pub mobile: bool,
    pub cpu_features: Vec<&'static str>,
}

impl HardwareProfile {
    /// Detect the current device.
    #[must_use]
    pub fn detect() -> Self {
        let mut system = sysinfo::System::new();
        system.refresh_memory();
        let total_ram_mb = system.total_memory() / 1024 / 1024;
        let available_ram_mb = system.available_memory() / 1024 / 1024;
        let physical_cores = system.physical_core_count().unwrap_or_else(|| {
            std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
        });
        let mut cpu_features = Vec::new();
        #[cfg(target_arch = "x86_64")]
        {
            if std::arch::is_x86_feature_detected!("sse4.2") {
                cpu_features.push("sse4.2");
            }
            if std::arch::is_x86_feature_detected!("avx2") {
                cpu_features.push("avx2");
            }
            if std::arch::is_x86_feature_detected!("fma") {
                cpu_features.push("fma");
            }
            if std::arch::is_x86_feature_detected!("avx512f") {
                cpu_features.push("avx512f");
            }
        }
        #[cfg(target_arch = "aarch64")]
        {
            if cfg!(target_feature = "neon") {
                cpu_features.push("neon");
            }
        }
        Self {
            total_ram_mb,
            available_ram_mb,
            physical_cores,
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            mobile: cfg!(any(target_os = "android", target_os = "ios")),
            cpu_features,
        }
    }

    /// Whether the model fits the currently free memory.
    #[must_use]
    pub const fn fits_now(&self, model: &ModelDescriptor) -> bool {
        self.available_ram_mb >= model.min_ram_mb
    }
}

/// The model Jabu suggests for this device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recommendation {
    pub model: Option<&'static ModelDescriptor>,
    pub reason: String,
}

/// Pick a default model for the hardware.
#[must_use]
pub fn recommend(hardware: &HardwareProfile) -> Recommendation {
    if hardware.mobile {
        if hardware.total_ram_mb >= 7000 {
            return Recommendation {
                model: find("hy-mt2-1.8b-q4-k-m"),
                reason: "on-device translation uses the smaller model on phones".to_string(),
            };
        }
        return Recommendation {
            model: None,
            reason: "this device has less than 8 GB of RAM; use a local translation server or keep translation off".to_string(),
        };
    }
    if hardware.total_ram_mb >= 16_000 {
        Recommendation {
            model: find("hy-mt2-7b-q4-k-m"),
            reason: "this desktop has enough memory for the higher-quality 7B model; CPU-only translation is slower than the 1.8B model".to_string(),
        }
    } else if hardware.total_ram_mb >= 7000 {
        Recommendation {
            model: find("hy-mt2-1.8b-q4-k-m"),
            reason: "this device is a good fit for the 1.8B model; the 7B model may be too large"
                .to_string(),
        }
    } else {
        Recommendation {
            model: None,
            reason: "this device has less than 8 GB of RAM; use a local translation server or keep translation off".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(total_ram_mb: u64, mobile: bool) -> HardwareProfile {
        HardwareProfile {
            total_ram_mb,
            available_ram_mb: total_ram_mb,
            physical_cores: 8,
            os: "linux",
            arch: "x86_64",
            mobile,
            cpu_features: vec!["avx2"],
        }
    }

    #[test]
    fn manifest_has_pinned_hashes_and_sizes() {
        assert_eq!(MODELS.len(), 2);
        for model in MODELS {
            assert_eq!(model.sha256.len(), 64);
            assert!(model.url.contains(&model.sha256[..8]) || model.url.contains("/resolve/"));
            assert!(model.size_bytes > 1_000_000_000);
            assert!(model.min_ram_mb <= model.recommended_ram_mb);
        }
    }

    #[test]
    fn recommends_by_memory() {
        assert_eq!(
            recommend(&profile(32_000, false))
                .model
                .map(|model| model.id),
            Some("hy-mt2-7b-q4-k-m")
        );
        assert_eq!(
            recommend(&profile(8_000, false))
                .model
                .map(|model| model.id),
            Some("hy-mt2-1.8b-q4-k-m")
        );
        assert!(recommend(&profile(4_000, false)).model.is_none());
        assert_eq!(
            recommend(&profile(8_000, true)).model.map(|model| model.id),
            Some("hy-mt2-1.8b-q4-k-m")
        );
        assert!(recommend(&profile(4_000, true)).model.is_none());
    }

    #[test]
    fn store_reports_installed_models() {
        let directory = tempfile::tempdir().unwrap();
        let store = ModelStore::new(directory.path()).unwrap();
        assert!(store.installed().is_empty());
        let model = find("hy-mt2-1.8b-q4-k-m").unwrap();
        std::fs::write(store.path(model), vec![0u8; 8]).unwrap();
        assert!(!store.is_installed(model));
        assert!(store.verify(model).is_err());
    }

    #[test]
    fn detects_a_plausible_profile() {
        let hardware = HardwareProfile::detect();
        assert!(hardware.physical_cores >= 1);
        assert!(!hardware.os.is_empty());
    }
}
