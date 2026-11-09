//! On-device message translation for Jabu.
//!
//! The crate is headless: it has no Dioxus or XMPP dependency. The app owns
//! the UI, storage and worker lifecycle; this crate owns the model prompt, the
//! language detector, markup protection, model installation and the backend
//! interface.
//!
//! # Privacy
//!
//! Translation runs after decryption on the local device. This crate never
//! sends message text over the network. The model download and the optional
//! OpenAI-compatible endpoint are the only network paths; the endpoint is
//! loopback-only unless the caller explicitly accepts the off-device warning.

pub mod detect;
pub mod engine;
pub mod external;
pub mod gguf;
pub mod models;
pub mod prompt;
pub mod protect;
#[cfg(not(target_arch = "wasm32"))]
pub mod sidecar;
pub mod types;

#[cfg(feature = "embedded")]
pub mod embedded;

pub use detect::{detect, Detection};
pub use engine::{
    FakeGenerator, PromptGenerator, TranslationError, Translator, UnavailableGenerator,
};
pub use external::{ExternalConfig, ExternalGenerator};
pub use gguf::{preflight, GgufError, GgufInfo};
pub use models::{
    find, recommend, HardwareProfile, ModelDescriptor, ModelError, ModelStore, Progress,
    Recommendation, MODELS,
};
pub use prompt::{build_prompt, max_output_tokens};
pub use protect::{protect, ProtectError, ProtectedText};
#[cfg(not(target_arch = "wasm32"))]
pub use sidecar::{SidecarConfig, SidecarGenerator};
pub use types::{Language, TranslationRequest, TranslationResult};

/// Stable hash used to invalidate a cached translation after an edit.
#[must_use]
pub fn source_hash(text: &str) -> String {
    use std::fmt::Write;

    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(text.as_bytes());
    let mut output = String::with_capacity(64);
    for byte in digest {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(feature = "embedded")]
pub use embedded::{EmbeddedGenerator, LazyEmbeddedGenerator};
