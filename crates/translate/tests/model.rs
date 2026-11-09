//! Model-backed translation tests.
//!
//! These are ignored by default and need a local GGUF file:
//!
//! ```bash
//! JABU_TEST_TRANSLATE_MODEL=/path/to/Hy-MT2-1.8B-Q4_K_M.gguf \
//!   cargo test -p jabu-translate --features embedded --test model -- --ignored --nocapture
//! ```

#![cfg(feature = "embedded")]

use jabu_translate::{detect, EmbeddedGenerator, Language, TranslationRequest, Translator};

fn model_path() -> String {
    std::env::var("JABU_TEST_TRANSLATE_MODEL")
        .expect("set JABU_TEST_TRANSLATE_MODEL to an installed GGUF file")
}

#[tokio::test]
#[ignore = "needs JABU_TEST_TRANSLATE_MODEL"]
async fn translates_korean_to_french() {
    let generator = EmbeddedGenerator::load(model_path(), "hy-mt2-1.8b-q4-k-m", 4, 4096, 0)
        .await
        .unwrap();
    let translator = Translator::new(generator);
    let result = translator
        .translate(TranslationRequest::new(
            "안녕하세요! 오늘 날씨가 정말 좋네요.",
            Language::French,
        ))
        .await
        .unwrap();
    assert!(!result.text.is_empty());
    assert_ne!(result.text, "안녕하세요! 오늘 날씨가 정말 좋네요.");
    println!("{}", result.text);
}

#[tokio::test]
#[ignore = "needs JABU_TEST_TRANSLATE_MODEL"]
async fn keeps_links_and_mentions() {
    let generator = EmbeddedGenerator::load(model_path(), "hy-mt2-1.8b-q4-k-m", 4, 4096, 0)
        .await
        .unwrap();
    let translator = Translator::new(generator);
    let text = "Salut @alice, regarde https://example.org/a?b=1 et dis-moi.";
    let result = translator
        .translate(TranslationRequest::new(text, Language::Korean))
        .await
        .unwrap();
    assert!(result.text.contains("@alice"));
    assert!(result.text.contains("https://example.org/a?b=1"));
    println!("{}", result.text);
}

#[test]
fn detects_the_example_pair() {
    assert_eq!(
        detect("안녕하세요 오늘 날씨가 좋네요").unwrap().language,
        Language::Korean
    );
    assert_eq!(
        detect("Bonjour, comment vas-tu aujourd'hui ?")
            .unwrap()
            .language,
        Language::French
    );
}
