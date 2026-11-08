//! Reporting content to a public Jabu Host.
//!
//! The Host exposes `POST /api/report`; this module is the small cross-platform
//! client used by the app's report dialog.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// Report sent to a Host.
#[derive(Debug, Clone, Serialize)]
pub struct ReportRequest {
    pub target: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reporter: Option<String>,
}

/// Receipt returned by the Host.
#[derive(Debug, Clone, Deserialize)]
pub struct ReportReceipt {
    pub id: String,
    pub status: String,
}

/// Submit a report to `host`.
///
/// # Errors
///
/// Returns an error when no Host is configured, the request fails, or the Host
/// rejects the report.
pub async fn submit_report(host: &str, request: &ReportRequest) -> Result<ReportReceipt> {
    let host = host.trim().trim_end_matches('/');
    if host.is_empty() {
        bail!("no report Host is configured");
    }
    submit(host, request).await
}

#[cfg(not(target_arch = "wasm32"))]
async fn submit(host: &str, request: &ReportRequest) -> Result<ReportReceipt> {
    use std::time::Duration;

    let client = reqwest::Client::builder()
        .user_agent(concat!("Jabu/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(15))
        .build()?;
    let response = client
        .post(format!("{host}/api/report"))
        .json(request)
        .send()
        .await?;
    if !response.status().is_success() {
        bail!("report Host returned HTTP {}", response.status());
    }
    Ok(response.json().await?)
}

#[cfg(target_arch = "wasm32")]
async fn submit(host: &str, request: &ReportRequest) -> Result<ReportReceipt> {
    let request = gloo_net::http::Request::post(&format!("{host}/api/report"))
        .json(request)?
        .send()
        .await?;
    if !request.ok() {
        bail!("report Host returned HTTP {}", request.status());
    }
    Ok(request.json().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ReportRequest {
        ReportRequest {
            target: "post/alice@example.org/p1".to_string(),
            reason: "spam".to_string(),
            details: None,
            reporter: None,
        }
    }

    #[test]
    fn serializes_optional_fields_only_when_present() {
        let json = serde_json::to_string(&request()).unwrap();
        assert!(json.contains("\"target\""));
        assert!(!json.contains("details"));
        let with = ReportRequest {
            details: Some("bad".to_string()),
            ..request()
        };
        assert!(serde_json::to_string(&with)
            .unwrap()
            .contains("\"details\":\"bad\""));
    }

    #[tokio::test]
    async fn an_empty_host_is_rejected_without_a_request() {
        assert!(submit_report("   ", &request()).await.is_err());
    }
}
