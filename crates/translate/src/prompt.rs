//! Hy-MT2 prompt construction.
//!
//! The model card asks for an English instruction for non-Chinese pairs and a
//! Chinese instruction when either side is Chinese. Keeping this in one place
//! makes the model-specific wording testable without loading a model.

use std::fmt::Write;

use crate::types::Language;

/// Build the user message sent to Hy-MT2.
#[must_use]
pub fn build_prompt(target: Language, source: Option<Language>, text: &str) -> String {
    let chinese = target.uses_chinese_prompt() || source.is_some_and(Language::uses_chinese_prompt);
    let mut prompt = String::new();
    if text.contains("<sn id=\"") {
        if chinese {
            prompt.push_str("请原样保留所有 <sn id=\"...\"/> 标签，不要翻译它们。\n");
        } else {
            prompt
                .push_str("Preserve every <sn id=\"...\"/> tag exactly and do not translate it.\n");
        }
    }
    if chinese {
        let _ = write!(
            prompt,
            "将以下文本翻译为{}，注意只需要输出翻译后的结果，不要额外解释：",
            target.chinese_name()
        );
    } else {
        let _ = write!(
            prompt,
            "Translate the following text into {}.\nNote that you should only output the translated result without any additional explanation:",
            target.english_name()
        );
    }
    prompt.push_str("\n\n");
    prompt.push_str(text);
    prompt
}

/// Conservative output budget for a chat message.
///
/// Chat messages are short. A larger budget only gives a runaway model more
/// room to repeat itself.
#[must_use]
pub fn max_output_tokens(input: &str) -> usize {
    let chars = input.chars().count();
    chars.saturating_mul(2).saturating_add(64).clamp(64, 2048)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_english_prompt_for_korean_to_french() {
        let prompt = build_prompt(Language::French, Some(Language::Korean), "안녕하세요");
        assert!(prompt.starts_with("Translate the following text into French."));
        assert!(prompt.contains("안녕하세요"));
        assert!(!prompt.contains("将以下文本"));
    }

    #[test]
    fn uses_chinese_prompt_for_chinese_target() {
        let prompt = build_prompt(Language::Chinese, Some(Language::French), "Bonjour");
        assert!(prompt.starts_with("将以下文本翻译为中文"));
        assert!(prompt.contains("Bonjour"));
    }

    #[test]
    fn uses_chinese_prompt_when_source_is_chinese() {
        let prompt = build_prompt(Language::French, Some(Language::Chinese), "你好");
        assert!(prompt.starts_with("将以下文本翻译为法语"));
    }

    #[test]
    fn adds_placeholder_instruction_only_when_needed() {
        let plain = build_prompt(Language::French, None, "hello");
        assert!(!plain.contains("<sn id="));
        let protected = build_prompt(Language::French, None, "hello <sn id=\"0\"/>");
        assert!(protected.contains("Preserve every <sn id="));
    }

    #[test]
    fn output_budget_is_bounded() {
        assert_eq!(max_output_tokens(""), 64);
        assert!(max_output_tokens(&"a".repeat(100_000)) <= 2048);
    }
}
