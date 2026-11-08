//! Optional public Host for Jabu.
//!
//! The Host is a headless server that serves public XMPP content — blogs, and
//! later communities, Spaces and Stories — over HTTP. It reuses `xmpp-core` to
//! fetch public `PubSub` nodes and never exposes chat or account data.

pub mod config;
pub mod content;
pub mod html;
pub mod routes;
pub mod xmpp;

pub use config::HostConfig;
pub use content::{CachedContent, ContentError, ContentSource};

use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use xmpp_core::ClientConfig;

/// Start the Host HTTP server.
///
/// # Errors
///
/// Returns an error when the XMPP account is missing, the connection fails, or
/// the listen address cannot be bound.
pub async fn run(config: HostConfig, xmpp: ClientConfig) -> Result<()> {
    if xmpp.jid.trim().is_empty() {
        bail!("host mode needs an XMPP account in [xmpp].jid");
    }
    let source = xmpp::XmppContent::connect(xmpp.into()).await?;
    let content = Arc::new(CachedContent::new(
        Arc::new(source),
        Duration::from_secs(config.cache_ttl_secs),
    ));
    let app = routes::router(content, config.clone());

    let listener = tokio::net::TcpListener::bind(&config.bind)
        .await
        .with_context(|| format!("could not bind {}", config.bind))?;
    tracing::info!("Jabu Host listening on http://{}", listener.local_addr()?);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("host server failed")?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("Host shutting down");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_a_host_without_an_account() {
        let result = run(HostConfig::default(), ClientConfig::default()).await;
        assert!(result.is_err());
    }
}
