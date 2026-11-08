//! Jabu - Cross-platform XMPP Client
//!
//! A modern, cross-platform XMPP client built with Rust and Dioxus.

use anyhow::Result;
use clap::{Parser, Subcommand};
use dioxus::prelude::*;
use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::{fmt, EnvFilter};

use jid::Jid;
use platform::{init_platform, PlatformContext};
use storage::{load_session, SettingsOmemoStore, StorageBackend, StorageFactory};
use ui::{App, ClientState, SessionStarter, Theme};
mod session_workers;

use xmpp_core::{
    AccountSession, CallKind, CallState, ClientConfig, ConnectionEvent, ConnectionState,
    DiscoManager, EventBus, ProcessorNotifier, SessionRegistry, XmppClient, XmppEvent,
};

// Thread-local context for non-Send types
thread_local! {
    static APP_CONTEXT: std::cell::RefCell<Option<AppContext>> = const { std::cell::RefCell::new(None) };
    static EVENT_RX: std::cell::RefCell<Option<tokio::sync::mpsc::UnboundedReceiver<ConnectionEvent>>> = const { std::cell::RefCell::new(None) };
}

struct UiNotifier {
    revision: Signal<u64>,
    connection_state: Signal<ConnectionState>,
}

impl ProcessorNotifier for UiNotifier {
    fn bump(&self) {
        let mut revision = self.revision;
        let next = *revision.read() + 1;
        revision.set(next);
    }

    fn set_connection_state(&self, state: ConnectionState) {
        let mut connection_state = self.connection_state;
        connection_state.set(state);
    }
}

#[derive(Clone)]
struct AppContext {
    platform: PlatformContext,
    storage: Arc<dyn StorageBackend>,
    events: Arc<EventBus>,
    xmpp_client: XmppClient,
    initial_config: ClientConfig,
    initial_theme: Theme,
    report_host: Option<String>,
    deep_link: Option<String>,
    sessions: Arc<SessionRegistry>,
}

fn app_root() -> Element {
    // Extract all needed data while the borrow is held
    let (
        platform,
        storage,
        events,
        xmpp_client,
        initial_config,
        initial_theme,
        report_host,
        deep_link,
        sessions,
    ) = APP_CONTEXT.with(|cell| {
        let binding = cell.borrow();
        let ctx = binding.as_ref().expect("App context not initialized");
        (
            ctx.platform.clone(),
            ctx.storage.clone(),
            ctx.events.clone(),
            ctx.xmpp_client.clone(),
            ctx.initial_config.clone(),
            ctx.initial_theme,
            ctx.report_host.clone(),
            ctx.deep_link.clone(),
            ctx.sessions.clone(),
        )
    });

    // Create ClientState inside the component (non-Send)
    let account_bare = initial_config
        .jid
        .split('/')
        .next()
        .unwrap_or_default()
        .to_string();

    let mut client_state = ClientState::new();
    let mut account_list: Vec<String> = storage
        .list_accounts()
        .unwrap_or_default()
        .into_iter()
        .map(|account| account.jid)
        .collect();
    if !account_bare.is_empty() && !account_list.contains(&account_bare) {
        account_list.push(account_bare.clone());
    }
    account_list.sort();
    client_state.accounts.set(account_list);
    client_state
        .active_account
        .set((!account_bare.is_empty()).then(|| account_bare.clone()));
    client_state.config.set(Some(initial_config.clone()));
    let starter_base_config = initial_config;
    client_state.client.set(Some(xmpp_client.clone()));
    client_state.own_presence.set(ui::PresenceChoice::from_show(
        client_state
            .config
            .read()
            .as_ref()
            .and_then(|config| config.presence_show.as_deref()),
    ));
    client_state
        .report_host
        .set(report_host.filter(|host| !host.trim().is_empty()));
    if let Some(enabled) = storage
        .get_setting("ui.notifications")
        .ok()
        .flatten()
        .and_then(|v| v.as_bool())
    {
        client_state.notifications_enabled.set(enabled);
    }
    if let Some(preferences) = storage::load_account_preferences(storage.as_ref(), &account_bare) {
        client_state
            .notifications_enabled
            .set(preferences.notifications);
    }
    if let Some(show) = storage
        .get_setting("ui.timestamps")
        .ok()
        .flatten()
        .and_then(|v| v.as_bool())
    {
        client_state.show_timestamps.set(show);
    }
    if let Some(enabled) = storage
        .get_setting("ui.link_previews")
        .ok()
        .flatten()
        .and_then(|v| v.as_bool())
    {
        client_state.link_previews.set(enabled);
    }
    if let Some(size) = storage
        .get_setting("ui.font_size")
        .ok()
        .flatten()
        .and_then(|v| v.as_u64())
    {
        client_state.font_size.set(size.clamp(12, 24) as u32);
    }
    if let Some(compact) = storage
        .get_setting("ui.compact")
        .ok()
        .flatten()
        .and_then(|v| v.as_bool())
    {
        client_state.compact.set(compact);
    }
    let theme = storage
        .get_setting("ui.theme")
        .ok()
        .flatten()
        .and_then(|v| v.as_str().map(Theme::from_slug))
        .unwrap_or(initial_theme);
    client_state.theme.set(theme);

    let initial_spaces: Vec<ui::Space> = xmpp_client
        .spaces
        .list()
        .into_iter()
        .map(Into::into)
        .collect();
    if !initial_spaces.is_empty() {
        client_state.spaces.set(initial_spaces);
    }

    if let Some(link) = deep_link.as_deref().and_then(ui::deeplink::DeepLink::parse) {
        client_state.open_deep_link(&link);
        if let ui::deeplink::DeepLink::Story { jid, item } = link {
            client_state.pending_story.set(Some((jid, item)));
        }
    }

    // Copy the signals the event processor needs before moving ClientState into context.
    let revision_signal = client_state.revision;
    let connection_state_signal = client_state.connection_state;
    let notifier = Arc::new(UiNotifier {
        revision: revision_signal,
        connection_state: connection_state_signal,
    });
    let media_client = xmpp_client.clone();
    use_future(move || {
        let client = media_client.clone();
        async move {
            client.start_call_media_listener();
        }
    });

    #[cfg(debug_assertions)]
    {
        let auto_client = xmpp_client.clone();
        use_future(move || {
            let client = auto_client.clone();
            async move {
                if let Ok(target) = std::env::var("JABU_AUTO_CALL") {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    if let Ok(jid) = target.parse::<Jid>() {
                        let kind = match std::env::var("JABU_AUTO_CALL_KIND").as_deref() {
                            Ok("video") => CallKind::Video,
                            _ => CallKind::Audio,
                        };
                        let _ = client.start_call(&jid, kind).await;
                    }
                }
                if std::env::var("JABU_AUTO_ANSWER").is_ok() {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        for session in client.call_sessions() {
                            if session.state == CallState::Incoming {
                                let _ = client.accept_call(&session.sid).await;
                            }
                        }
                    }
                }
                if let Ok(secs) = std::env::var("JABU_AUTO_HANGUP") {
                    let secs = secs.parse::<u64>().unwrap_or(5);
                    tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
                    for session in client.call_sessions() {
                        if !session.state.is_final() {
                            let _ = client.end_call(&session.sid, Some("success")).await;
                        }
                    }
                }
            }
        });
    }

    let call_client = xmpp_client.clone();
    let call_events = events.clone();
    let call_notifier = notifier.clone();
    use_future(move || {
        let client = call_client.clone();
        let mut rx = call_events.subscribe();
        let notifier = call_notifier.clone();
        async move {
            while let Ok(event) = rx.recv().await {
                if let XmppEvent::CallEnded { sid, .. } = event {
                    let Some(session) = client.call_session(&sid) else {
                        continue;
                    };
                    let peer = session.peer.to_bare().to_string();
                    let is_room =
                        client.mucs.get_by_bare(&peer).is_some()
                            || client.spaces.list().iter().any(|space| {
                                space.rooms.iter().any(|room| room.conference == peer)
                            });
                    if is_room {
                        continue;
                    }
                    let ended = session.ended_at.unwrap_or_else(chrono::Utc::now);
                    let started = session.connected_at.unwrap_or(session.created_at);
                    let call = xmpp_core::CallEventInfo {
                        direction: match session.direction {
                            xmpp_core::CallDirection::Incoming => "incoming".to_string(),
                            xmpp_core::CallDirection::Outgoing => "outgoing".to_string(),
                        },
                        kind: session.kind.as_str().to_string(),
                        state: session.state.as_str().to_string(),
                        duration_secs: (ended - started).num_seconds().max(0),
                        reason: session.reason.clone(),
                    };
                    if let Err(error) = client.add_call_event(&session.peer, call) {
                        tracing::warn!("Failed to record call event: {error}");
                    } else {
                        notifier.bump();
                    }
                }
            }
        }
    });

    let worker_state = client_state.clone();
    let worker_platform = platform.clone();
    use_context_provider(|| client_state);
    use_context_provider(|| platform);
    use_context_provider(|| storage.clone());
    use_context_provider(|| events.clone());
    use_context_provider(|| sessions.clone());

    let starter_registry = sessions.clone();
    let starter_storage = storage.clone();
    let starter_platform = worker_platform.clone();
    let starter_state = worker_state.clone();
    let starter_base = starter_base_config;
    use_context_provider(|| {
        SessionStarter(Arc::new(move |account: String| {
            let bare = account.split('/').next().unwrap_or(&account).to_string();
            if starter_registry.client(&bare).is_some() {
                return Ok(());
            }
            let Some(credentials) =
                storage::load_account_credentials(starter_storage.as_ref(), &bare)
            else {
                return Err("No saved password for this account.".to_string());
            };
            let Some(password) = credentials.password.clone() else {
                return Err("No saved password for this account.".to_string());
            };
            let mut config = starter_base.clone();
            config.jid = credentials.jid;
            config.password = Some(password);
            config.resource = credentials.resource;
            if let Some(preferences) =
                storage::load_account_preferences(starter_storage.as_ref(), &bare)
            {
                config.presence_show = preferences.presence_show;
                config.presence_status = preferences.presence_status;
            }
            let session = AccountSession::new(config.clone());
            prepare_session_client(&session.client, &starter_storage, &config.jid);
            starter_registry.insert(session);
            session_workers::start_one(
                &starter_registry,
                &starter_storage,
                &starter_platform,
                &starter_state,
                None,
                &bare,
            );
            Ok(())
        }))
    });

    // Start the processor, persistence and notification workers for every
    // session. Only the active session updates the visible UI; the others keep
    // their connections and buckets in memory and persist events.
    use_hook(move || {
        session_workers::start(
            &sessions,
            &storage,
            &worker_platform,
            &worker_state,
            deep_link.clone(),
        );
    });

    rsx! {
        ui::AppStyles {}
        App {}
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn install_call_media(
    client: &XmppClient,
    disco: Arc<DiscoManager>,
    preferred_camera: Option<String>,
    preferred_microphone: Option<String>,
    preferred_sfu: Option<String>,
) {
    client
        .voice
        .set_port(Arc::new(media::ConferenceVoicePort::new(
            client.clone(),
            preferred_sfu
                .as_deref()
                .and_then(|value| value.parse().ok()),
        )));
    let port = match media::GStreamerMediaPort::new(Vec::new()) {
        Ok(port) => port,
        Err(error) => {
            tracing::warn!("Call media backend unavailable: {error}");
            return;
        }
    };
    client.set_call_media(port.clone());
    client.set_call_camera_preference(preferred_camera.as_deref());
    client.set_call_audio_preference(preferred_microphone.as_deref());
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(10));
        let servers = disco
            .stun_turn_servers()
            .into_iter()
            .map(|service| {
                let scheme = if service.kind == "turn" {
                    if service.transport.as_deref() == Some("tcp") {
                        "turns"
                    } else {
                        "turn"
                    }
                } else {
                    "stun"
                };
                media::IceServer {
                    urls: vec![format!("{scheme}:{}:{}", service.host, service.port)],
                    username: service.username,
                    credential: service.password,
                }
            })
            .collect::<Vec<_>>();
        port.set_ice_servers(servers);
    });
}

/// Application configuration
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct AppConfig {
    pub xmpp: ClientConfig,
    pub ui: UiConfig,
    pub storage: StorageConfig,
    pub notifications: NotificationConfig,
    pub host: host::HostConfig,
    pub reporting: ReportingConfig,
}

/// Where in-app reports are sent.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct ReportingConfig {
    /// Base URL of a public Jabu Host, e.g. `https://jabu.example.org`.
    pub host: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct UiConfig {
    pub theme: String,
    pub language: String,
    pub font_size: u32,
    pub compact_mode: bool,
    pub show_avatars: bool,
    pub show_timestamps: bool,
    pub animate: bool,
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct StorageConfig {
    pub backend: String, // "redb", "sqlite", "indexeddb"
    pub path: Option<String>,
    pub max_message_history: usize,
    pub auto_cleanup: bool,
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct NotificationConfig {
    pub enabled: bool,
    pub sound: bool,
    pub desktop: bool,
    pub push: bool,
    pub per_chat: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            xmpp: ClientConfig::default(),
            ui: UiConfig {
                theme: "dark".to_string(),
                language: "en".to_string(),
                font_size: 14,
                compact_mode: false,
                show_avatars: true,
                show_timestamps: true,
                animate: true,
            },
            storage: StorageConfig {
                backend: "redb".to_string(),
                path: None,
                max_message_history: 10000,
                auto_cleanup: true,
            },
            notifications: NotificationConfig {
                enabled: true,
                sound: true,
                desktop: true,
                push: true,
                per_chat: true,
            },
            host: host::HostConfig::default(),
            reporting: ReportingConfig::default(),
        }
    }
}

/// CLI arguments
#[derive(Parser)]
#[command(name = "jabu", version, about = "Cross-platform XMPP Client")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Configuration file path
    #[arg(short, long)]
    config: Option<String>,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,

    /// JID to connect to
    #[arg(short, long)]
    jid: Option<String>,

    /// Password (will prompt if not provided)
    #[arg(short, long)]
    password: Option<String>,

    /// Open a jabu:// deep link after starting
    #[arg(long, value_name = "URL")]
    open: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the application (default)
    Run,
    /// Show version information
    Version,
    /// Manage accounts
    Account {
        #[command(subcommand)]
        action: AccountAction,
    },
    /// Import/export data
    Data {
        #[command(subcommand)]
        action: DataAction,
    },
    /// Serve the public Host pages
    Host,
}

#[derive(Subcommand)]
enum AccountAction {
    /// Add a new account
    Add {
        jid: String,
        /// Password (prompted when omitted)
        #[arg(long)]
        password: Option<String>,
        /// Resource to bind
        #[arg(long)]
        resource: Option<String>,
    },
    /// Remove an account
    Remove { jid: String },
    /// List accounts
    List,
}

#[derive(Subcommand)]
enum DataAction {
    /// Export all data
    Export { path: String },
    /// Import data
    Import { path: String },
}

/// Main entry point
fn main() -> Result<()> {
    // Android's NativeActivity does not provide a usable argv; clap's
    // std::env::args() path panics with a capacity overflow there.
    #[cfg(target_os = "android")]
    let cli = Cli::parse_from(["jabu"]);
    #[cfg(not(target_os = "android"))]
    let cli = Cli::parse();

    // Initialize logging
    init_logging(&cli.log_level)?;

    // Load configuration
    let config = load_config(cli.config.as_deref())?;

    // Override with CLI arguments
    let mut config = config;
    if let Some(jid) = cli.jid {
        config.xmpp.jid = jid;
    }
    if let Some(password) = cli.password {
        config.xmpp.password = Some(password);
    }
    let deep_link = cli.open;

    // Handle commands
    if let Some(cmd) = cli.command {
        return handle_command(cmd, config, deep_link);
    }

    // Run the application
    run_app(config, deep_link)
}

/// Initialize logging
fn init_logging(level: &str) -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    Ok(())
}

/// Load configuration from file
fn load_config(config_path: Option<&str>) -> Result<AppConfig> {
    let mut figment = Figment::new();
    if std::path::Path::new("config.toml").exists() {
        figment = figment.merge(Toml::file("config.toml"));
    }

    if let Some(path) = config_path {
        figment = figment.merge(Toml::file(path));
    }

    figment = figment.merge(Env::prefixed("JABU_"));

    Ok(figment.extract()?)
}

/// Handle CLI commands
fn handle_command(cmd: Command, config: AppConfig, deep_link: Option<String>) -> Result<()> {
    match cmd {
        Command::Run => run_app(config, deep_link),
        Command::Version => {
            println!("jabu {}", env!("CARGO_PKG_VERSION"));
            println!(
                "Built with Rust {}",
                option_env!("RUSTC_VERSION").unwrap_or("unknown")
            );
            Ok(())
        }
        Command::Account { action } => handle_account_action(action, config),
        Command::Data { action } => handle_data_action(action, config),
        Command::Host => run_host(config),
    }
}

/// Run the optional public Host server.
fn run_host(config: AppConfig) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("could not start the Host runtime")?;
    runtime.block_on(host::run(config.host, config.xmpp))
}

fn handle_account_action(action: AccountAction, config: AppConfig) -> Result<()> {
    let storage = open_storage(&config)?;
    match action {
        AccountAction::Add {
            jid,
            password,
            resource,
        } => {
            let bare = jid.split('/').next().unwrap_or(&jid).to_string();
            let password = match password {
                Some(password) => password,
                None => {
                    print!("Password for {bare}: ");
                    std::io::Write::flush(&mut std::io::stdout())?;
                    let mut line = String::new();
                    std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut line)?;
                    line.trim_end_matches(['\r', '\n']).to_string()
                }
            };
            let resource = resource.unwrap_or_else(|| "jabu".to_string());
            let account = storage::StoredAccount {
                jid: bare.clone(),
                password_hash: None,
                resource: resource.clone(),
                priority: 0,
                auto_reconnect: true,
                created_at: chrono::Utc::now(),
                last_login: None,
            };
            storage.save_account(&account)?;
            let credentials = storage::StoredCredentials {
                jid: bare.clone(),
                password: Some(password),
                resource: Some(resource),
            };
            storage::save_account_credentials(storage.as_ref(), &credentials)?;
            storage::save_credentials(storage.as_ref(), &credentials)?;
            storage.set_setting("ui.active_account", &serde_json::json!(bare))?;
            println!("Added {bare}");
        }
        AccountAction::Remove { jid } => {
            let bare = jid.split('/').next().unwrap_or(&jid).to_string();
            storage.delete_account(&bare)?;
            storage::delete_account_credentials(storage.as_ref(), &bare)?;
            if storage
                .get_setting("ui.active_account")?
                .and_then(|value| value.as_str().map(str::to_string))
                .as_deref()
                == Some(bare.as_str())
            {
                let _ = storage::clear_credentials(storage.as_ref());
                let _ = storage.set_setting("ui.active_account", &serde_json::Value::Null);
            }
            println!("Removed {bare}");
        }
        AccountAction::List => {
            let accounts = storage.list_accounts()?;
            if accounts.is_empty() {
                println!("No accounts configured.");
            } else {
                for account in accounts {
                    println!("{}\t{}", account.jid, account.resource);
                }
            }
        }
    }
    Ok(())
}

fn open_storage(config: &AppConfig) -> Result<Arc<dyn StorageBackend>> {
    let platform = init_platform(Arc::new(EventBus::new()))?;
    let path = config
        .storage
        .path
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(|| platform.storage.get_data_dir().unwrap());
    Ok(Arc::new(StorageFactory::create(path)?))
}

fn handle_data_action(action: DataAction, _config: AppConfig) -> Result<()> {
    match action {
        DataAction::Export { path } => {
            println!("Exporting data to: {}", path);
        }
        DataAction::Import { path } => {
            println!("Importing data from: {}", path);
        }
    }
    Ok(())
}

/// Load the per-account state that does not depend on a live connection.
fn prepare_session_client(client: &XmppClient, storage: &Arc<dyn StorageBackend>, jid: &str) {
    let bare = jid.split('/').next().unwrap_or(jid);
    client
        .omemo
        .set_store(Arc::new(SettingsOmemoStore::new(storage.clone(), bare)));
    if let Err(error) = client.omemo.load_persisted() {
        tracing::warn!("Failed to load OMEMO state for {bare}: {error}");
    }
    client.omemo.ensure_local_auto(100);
    client.omemo.set_own_jid(bare);
    load_session(storage.as_ref(), client, jid);
    if let Ok(spaces) = storage.get_spaces(bare) {
        for space in spaces {
            client.spaces.upsert(space);
        }
    }
}

/// Run the Dioxus application
fn run_app(config: AppConfig, deep_link: Option<String>) -> Result<()> {
    let mut config = config;
    info!("Starting Jabu v{}", env!("CARGO_PKG_VERSION"));

    let _instance = match platform::single_instance::acquire("jabu")? {
        Some(guard) => guard,
        None => {
            info!("Another Jabu instance is already running");
            return Ok(());
        }
    };

    // Create event bus
    let events = Arc::new(EventBus::new());

    // Initialize platform
    let platform = init_platform(events.clone())?;
    info!("Platform initialized: {:?}", platform.platform);

    // Initialize storage
    let storage_path = config
        .storage
        .path
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(|| platform.storage.get_data_dir().unwrap());

    let storage = Arc::new(StorageFactory::create(storage_path)?);
    info!("Storage initialized");

    if config.xmpp.jid.trim().is_empty() {
        let preferred = storage
            .get_setting("ui.active_account")
            .ok()
            .flatten()
            .and_then(|value| value.as_str().map(str::to_string));
        let credentials = preferred
            .as_deref()
            .and_then(|jid| storage::load_account_credentials(storage.as_ref(), jid))
            .or_else(|| {
                storage
                    .list_accounts()
                    .ok()
                    .and_then(|accounts| accounts.first().cloned())
                    .and_then(|account| {
                        storage::load_account_credentials(storage.as_ref(), &account.jid)
                    })
            })
            .or_else(|| storage::load_credentials(storage.as_ref()));
        if let Some(credentials) = credentials {
            config.xmpp.jid = credentials.jid;
            if config.xmpp.password.is_none() {
                config.xmpp.password = credentials.password;
            }
            if config.xmpp.resource.is_none() {
                config.xmpp.resource = credentials.resource;
            }
        }
    }
    let has_account = !config.xmpp.jid.trim().is_empty();
    if has_account {
        let bare = config
            .xmpp
            .jid
            .split('/')
            .next()
            .unwrap_or(&config.xmpp.jid)
            .to_string();
        let credentials = storage::StoredCredentials {
            jid: bare.clone(),
            password: config.xmpp.password.clone(),
            resource: config.xmpp.resource.clone(),
        };
        let account = storage::StoredAccount {
            jid: bare.clone(),
            password_hash: None,
            resource: config.xmpp.resource.clone().unwrap_or_default(),
            priority: 0,
            auto_reconnect: config.xmpp.auto_reconnect,
            created_at: chrono::Utc::now(),
            last_login: Some(chrono::Utc::now()),
        };
        let _ = storage.save_account(&account);
        if credentials.password.is_some() {
            let _ = storage::save_account_credentials(storage.as_ref(), &credentials);
        }
        let _ = storage.set_setting("ui.active_account", &serde_json::json!(bare));
        if let Some(preferences) = storage::load_account_preferences(storage.as_ref(), &bare) {
            config.xmpp.presence_show = preferences.presence_show;
            config.xmpp.presence_status = preferences.presence_status;
        }
    }

    // Create the active account session. The session owns the connection,
    // managers and the receiver consumed by the event processor.
    let mut session = AccountSession::with_events(config.xmpp.clone(), events.clone());
    let xmpp_client = session.client.clone();
    let event_rx = session
        .take_receiver()
        .expect("a new session owns its connection receiver");
    if has_account {
        prepare_session_client(&xmpp_client, &storage, &config.xmpp.jid);
    }

    // Register the active session and connect the other saved accounts in the
    // background, up to a small cap so a large account list cannot exhaust the
    // server's stream limit.
    let sessions = Arc::new(SessionRegistry::new());
    sessions.insert(session);
    let active_session = config
        .xmpp
        .jid
        .split('/')
        .next()
        .unwrap_or(&config.xmpp.jid)
        .to_string();
    let base_config = config.xmpp.clone();
    let mut background_sessions = 0;
    if let Ok(accounts) = storage.list_accounts() {
        for account in accounts {
            let bare = account.jid;
            if bare == active_session || sessions.client(&bare).is_some() {
                continue;
            }
            if background_sessions >= 4 {
                break;
            }
            let Some(credentials) = storage::load_account_credentials(storage.as_ref(), &bare)
            else {
                continue;
            };
            if credentials.password.is_none() {
                continue;
            }
            let mut background_config = base_config.clone();
            background_config.jid = credentials.jid;
            background_config.password = credentials.password;
            background_config.resource = credentials.resource;
            if let Some(preferences) = storage::load_account_preferences(storage.as_ref(), &bare) {
                background_config.presence_show = preferences.presence_show;
                background_config.presence_status = preferences.presence_status;
            }
            let background = AccountSession::new(background_config.clone());
            prepare_session_client(&background.client, &storage, &background_config.jid);
            sessions.insert(background);
            background_sessions += 1;
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    let preferred_camera = storage
        .get_setting("calls.camera")
        .ok()
        .flatten()
        .and_then(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        });
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    let preferred_microphone = storage
        .get_setting("calls.microphone")
        .ok()
        .flatten()
        .and_then(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        });
    // The SFU component cannot be discovered from the server (Prosody does not
    // list it under disco#items), so it is configured, as it is in Movim.
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    let preferred_sfu = storage
        .get_setting("calls.sfu")
        .ok()
        .flatten()
        .and_then(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        });
    #[cfg(not(target_arch = "wasm32"))]
    let preferred_spaces_service = storage
        .get_setting("spaces.service")
        .ok()
        .flatten()
        .and_then(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        });
    xmpp_client.set_spaces_service(preferred_spaces_service.as_deref());
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    install_call_media(
        &xmpp_client,
        xmpp_client.disco.clone(),
        preferred_camera,
        preferred_microphone,
        preferred_sfu,
    );

    // Launch Dioxus app
    let app_ctx = AppContext {
        platform: platform.clone(),
        storage: storage.clone(),
        events: events.clone(),
        xmpp_client: xmpp_client.clone(),
        initial_config: config.xmpp.clone(),
        initial_theme: Theme::from_slug(&config.ui.theme),
        report_host: config
            .reporting
            .host
            .clone()
            .filter(|host| !host.trim().is_empty()),
        deep_link,
        sessions,
    };
    APP_CONTEXT.with(|cell| {
        *cell.borrow_mut() = Some(app_ctx);
    });
    EVENT_RX.with(|cell| {
        *cell.borrow_mut() = Some(event_rx);
    });

    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        use dioxus::desktop::{Config, LogicalSize, WindowBuilder};

        dioxus::LaunchBuilder::new()
            .with_cfg(
                Config::new().with_disable_context_menu(true).with_window(
                    WindowBuilder::new()
                        .with_title("Jabu")
                        .with_decorations(false)
                        .with_inner_size(LogicalSize::new(1280.0, 800.0))
                        .with_min_inner_size(LogicalSize::new(860.0, 560.0)),
                ),
            )
            .launch(app_root);
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    dioxus::launch(app_root);

    Ok(())
}
