//! Standalone Host server.
//!
//! This binary exists so a Host can be deployed without the desktop stack
//! (`Dioxus`, `GTK`, `WebKit`). It reads the same `[host]` and `[xmpp]` sections as
//! the `jabu host` subcommand.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use serde::Deserialize;

use host::HostConfig;

#[derive(Debug, Parser)]
#[command(name = "jabu-host", version, about = "Serve the public Jabu Host")]
struct Cli {
    /// Configuration file (defaults to ./config.toml when present)
    #[arg(short, long)]
    config: Option<PathBuf>,
    /// Address to listen on, overriding `[host].bind`
    #[arg(long)]
    bind: Option<String>,
    /// Canonical public URL, overriding `[host].public_url`
    #[arg(long)]
    public_url: Option<String>,
    /// Bootstrap admin token, overriding `[host].admin_token`
    #[arg(long)]
    admin_token: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct FileConfig {
    xmpp: xmpp_core::ClientConfig,
    host: HostConfig,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_logging();
    let mut config = load(&cli)?;
    if let Some(bind) = cli.bind {
        config.host.bind = bind;
    }
    if let Some(public_url) = cli.public_url {
        config.host.public_url = Some(public_url);
    }
    if let Some(admin_token) = cli.admin_token {
        config.host.admin_token = Some(admin_token);
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("could not start the Host runtime")?;
    runtime.block_on(host::run(config.host, config.xmpp))
}

fn load(cli: &Cli) -> Result<FileConfig> {
    let path = cli.config.clone().or_else(|| {
        let default = PathBuf::from("config.toml");
        default.exists().then_some(default)
    });
    let mut figment = Figment::new();
    if let Some(path) = path {
        figment = figment.merge(Toml::file(path));
    }
    figment = figment.merge(Env::prefixed("JABU_"));
    figment
        .extract()
        .context("could not read the Host configuration")
}

fn init_logging() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_help_is_wired() {
        use clap::CommandFactory;
        let command = Cli::command();
        assert_eq!(command.get_name(), "jabu-host");
    }

    #[test]
    fn defaults_match_the_library_defaults() {
        let config = FileConfig::default();
        assert_eq!(config.host.bind, HostConfig::default().bind);
    }

    #[test]
    fn loads_a_configuration_file() {
        let path = std::env::temp_dir().join(format!("jabu-host-test-{}.toml", std::process::id()));
        std::fs::write(
            &path,
            "[xmpp]\njid = \"host@example.org\"\n\n[host]\nbind = \"0.0.0.0:9000\"\n\
site_name = \"Example\"\n",
        )
        .unwrap();
        let cli = Cli {
            config: Some(path.clone()),
            bind: None,
            public_url: None,
            admin_token: None,
        };
        let config = load(&cli).unwrap();
        let _ = std::fs::remove_file(path);
        assert_eq!(config.xmpp.jid, "host@example.org");
        assert_eq!(config.host.bind, "0.0.0.0:9000");
        assert_eq!(config.host.site_name, "Example");
    }
}
