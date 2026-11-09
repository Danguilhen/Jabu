//! OpenAI-compatible local endpoint backend.
//!
//! This is an advanced fallback for users who already run Ollama,
//! `llama-server` or a similar server. It is not a Jabu service. The transport
//! is restricted to loopback unless the user explicitly accepts the off-device
//! warning, redirects are disabled, and remote endpoints must use HTTPS.

use std::net::IpAddr;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use url::Url;

use crate::engine::{PromptGenerator, TranslationError};

/// Configuration for an OpenAI-compatible chat-completions endpoint.
#[derive(Debug, Clone)]
pub struct ExternalConfig {
    /// Base URL, for example `http://127.0.0.1:11434/v1`.
    pub base_url: String,
    pub model: String,
    pub token: Option<String>,
    pub allow_remote: bool,
    pub timeout_secs: u64,
}

impl Default for ExternalConfig {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:11434/v1".to_string(),
            model: "hy-mt2:1.8b".to_string(),
            token: None,
            allow_remote: false,
            timeout_secs: 60,
        }
    }
}

/// A chat-completions backend.
pub struct ExternalGenerator {
    config: ExternalConfig,
    endpoint: Url,
    client: reqwest::Client,
}

impl ExternalGenerator {
    /// Validate the endpoint and build a client.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid URL, a non-loopback endpoint without an
    /// explicit override, a plaintext remote endpoint, or a client build
    /// failure.
    pub fn new(config: ExternalConfig) -> Result<Self, TranslationError> {
        let endpoint = endpoint_url(&config.base_url)?;
        let loopback = endpoint.host_str().is_some_and(is_loopback);
        if !loopback && !config.allow_remote {
            return Err(TranslationError::RemoteEndpointDenied(
                "set allow_remote only after accepting that message text leaves this device"
                    .to_string(),
            ));
        }
        if !loopback && endpoint.scheme() != "https" {
            return Err(TranslationError::RemoteEndpointDenied(
                "remote endpoints must use HTTPS".to_string(),
            ));
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs.max(5)))
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .user_agent("Jabu translation")
            .build()
            .map_err(|error| TranslationError::Generation(error.to_string()))?;
        Ok(Self {
            config,
            endpoint,
            client,
        })
    }

    #[must_use]
    pub fn config(&self) -> &ExternalConfig {
        &self.config
    }
}

#[async_trait]
impl PromptGenerator for ExternalGenerator {
    fn id(&self) -> &'static str {
        "external"
    }

    fn model(&self) -> Option<&str> {
        Some(&self.config.model)
    }

    fn is_available(&self) -> bool {
        true
    }

    async fn generate(&self, prompt: &str, max_tokens: usize) -> Result<String, TranslationError> {
        let body = serde_json::json!({
            "model": self.config.model,
            "messages": [{"role": "user", "content": prompt}],
            "temperature": 0.0,
            "max_tokens": max_tokens,
            "stream": false,
        });
        let mut request = self.client.post(self.endpoint.clone()).json(&body);
        if let Some(token) = self
            .config
            .token
            .as_deref()
            .filter(|token| !token.is_empty())
        {
            request = request.bearer_auth(token);
        }
        let response = request.send().await.map_err(|error| {
            if error.is_timeout() {
                TranslationError::Timeout
            } else {
                TranslationError::Generation(error.to_string())
            }
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(TranslationError::Generation(format!(
                "translation server returned HTTP {status}"
            )));
        }
        let parsed: ChatResponse = response
            .json()
            .await
            .map_err(|error| TranslationError::Generation(error.to_string()))?;
        parsed
            .choices
            .into_iter()
            .find_map(|choice| choice.message.content)
            .filter(|content| !content.trim().is_empty())
            .ok_or(TranslationError::EmptyOutput)
    }
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Debug, Deserialize)]
struct Message {
    content: Option<String>,
}

fn endpoint_url(base: &str) -> Result<Url, TranslationError> {
    let trimmed = base.trim().trim_end_matches('/');
    let full = if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/chat/completions")
    } else {
        format!("{trimmed}/v1/chat/completions")
    };
    let url = Url::parse(&full)
        .map_err(|error| TranslationError::NotConfigured(format!("invalid endpoint: {error}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(TranslationError::NotConfigured(
            "endpoint must use http or https".to_string(),
        ));
    }
    if url.host_str().is_none() {
        return Err(TranslationError::NotConfigured(
            "endpoint has no host".to_string(),
        ));
    }
    Ok(url)
}

fn is_loopback(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<IpAddr>()
        .is_ok_and(|address| address.is_loopback())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_v1_endpoint() {
        assert_eq!(
            endpoint_url("http://127.0.0.1:11434").unwrap().as_str(),
            "http://127.0.0.1:11434/v1/chat/completions"
        );
        assert_eq!(
            endpoint_url("http://127.0.0.1:11434/v1/").unwrap().as_str(),
            "http://127.0.0.1:11434/v1/chat/completions"
        );
        assert_eq!(
            endpoint_url("http://127.0.0.1:11434/v1/chat/completions")
                .unwrap()
                .as_str(),
            "http://127.0.0.1:11434/v1/chat/completions"
        );
    }

    #[test]
    fn allows_loopback_without_override() {
        let generator = ExternalGenerator::new(ExternalConfig::default()).unwrap();
        assert!(generator.is_available());
        assert_eq!(generator.endpoint.host_str(), Some("127.0.0.1"));
    }

    #[test]
    fn rejects_remote_without_consent() {
        let config = ExternalConfig {
            base_url: "https://translate.example.org/v1".to_string(),
            ..ExternalConfig::default()
        };
        assert!(matches!(
            ExternalGenerator::new(config),
            Err(TranslationError::RemoteEndpointDenied(_))
        ));
    }

    #[test]
    fn rejects_plaintext_remote_even_with_consent() {
        let config = ExternalConfig {
            base_url: "http://translate.example.org/v1".to_string(),
            allow_remote: true,
            ..ExternalConfig::default()
        };
        assert!(matches!(
            ExternalGenerator::new(config),
            Err(TranslationError::RemoteEndpointDenied(_))
        ));
    }

    #[test]
    fn allows_https_remote_with_consent() {
        let config = ExternalConfig {
            base_url: "https://translate.example.org/v1".to_string(),
            allow_remote: true,
            ..ExternalConfig::default()
        };
        assert!(ExternalGenerator::new(config).is_ok());
    }
}
