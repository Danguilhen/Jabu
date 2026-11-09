//! Helper-process translation test.
//!
//! ```bash
//! JABU_TEST_TRANSLATE_MODEL=/path/to/Hy-MT2-1.8B-Q4_K_M.gguf \
//!   cargo test -p jabu-translate --features vulkan --test sidecar -- --ignored --nocapture
//! ```

#![cfg(feature = "embedded")]

use std::path::PathBuf;

use jabu_translate::{Language, SidecarConfig, SidecarGenerator, TranslationRequest, Translator};

#[tokio::test]
#[ignore = "needs JABU_TEST_TRANSLATE_MODEL and the helper binary"]
async fn translates_through_the_helper_process() {
    let model = std::env::var("JABU_TEST_TRANSLATE_MODEL")
        .expect("set JABU_TEST_TRANSLATE_MODEL to an installed GGUF file");
    let config = SidecarConfig {
        helper: PathBuf::from(env!("CARGO_BIN_EXE_jabu-translate-helper")),
        model: PathBuf::from(model),
        model_id: "hy-mt2-1.8b-q4-k-m".to_string(),
        threads: 4,
        context: 4096,
        gpu_layers: 0,
        sandbox: std::env::var_os("JABU_TEST_SANDBOX").is_some(),
    };
    let generator = SidecarGenerator::new(config);
    let translator = Translator::new(generator);
    let result = translator
        .translate(TranslationRequest::new(
            "안녕하세요! 오늘 날씨가 정말 좋네요.",
            Language::French,
        ))
        .await
        .unwrap();
    assert!(!result.text.is_empty());
    println!("{}", result.text);
}
