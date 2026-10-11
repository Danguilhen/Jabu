//! Jabu - Cross-platform XMPP Client
//!
//! A modern, cross-platform XMPP client built with Rust and Dioxus.

use anyhow::Result;
use clap::{Parser, Subcommand};
use dioxus::launch;
use dioxus::prelude::*;
use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::{fmt, EnvFilter};

use platform::{init_platform, PlatformContext};
use storage::{SettingsOmemoStore, StorageBackend, StorageFactory};
use ui::App;
use xmpp_core::{
    ChatManager, ClientConfig, ClientState, Connection, ConnectionEvent, DiscoManager, EventBus,
    EventProcessor, MucManager, OmemoManager, PubSubManager, PushManager, Roster, StreamManagement,
    XmppClient, XmppEvent,
};

// Thread-local context for non-Send types
thread_local! {
    static APP_CONTEXT: std::cell::RefCell<Option<AppContext>> = const { std::cell::RefCell::new(None) };
    static EVENT_RX: std::cell::RefCell<Option<tokio::sync::mpsc::UnboundedReceiver<ConnectionEvent>>> = const { std::cell::RefCell::new(None) };
}

#[derive(Clone)]
struct AppContext {
    platform: PlatformContext,
    storage: Arc<dyn StorageBackend>,
    events: Arc<EventBus>,
    xmpp_client: XmppClient,
    initial_config: ClientConfig,
}

fn app_root() -> Element {
    // Extract all needed data while the borrow is held
    let (platform, storage, events, xmpp_client, initial_config) = APP_CONTEXT.with(|cell| {
        let binding = cell.borrow();
        let ctx = binding.as_ref().expect("App context not initialized");
        (
            ctx.platform.clone(),
            ctx.storage.clone(),
            ctx.events.clone(),
            ctx.xmpp_client.clone(),
            ctx.initial_config.clone(),
        )
    });

    // Create ClientState inside the component (non-Send)
    let mut client_state = ClientState::new();
    client_state.config.set(Some(initial_config));
    client_state.client.set(Some(xmpp_client.clone()));
    if let Some(enabled) = storage
        .get_setting("ui.notifications")
        .ok()
        .flatten()
        .and_then(|v| v.as_bool())
    {
        client_state.notifications_enabled.set(enabled);
    }
    if let Some(show) = storage
        .get_setting("ui.timestamps")
        .ok()
        .flatten()
        .and_then(|v| v.as_bool())
    {
        client_state.show_timestamps.set(show);
    }

    // Copy the signals the event processor needs before moving ClientState into context.
    let revision_signal = client_state.revision;
    let connection_state_signal = client_state.connection_state;
    let selected_chat_signal = client_state.selected_chat;
    let notifications_signal = client_state.notifications_enabled;

    let notif_platform = platform.clone();
    let notif_events = events.clone();
    let notif_roster = xmpp_client.roster.clone();

    use_context_provider(|| client_state);
    use_context_provider(|| platform);
    use_context_provider(|| storage);
    use_context_provider(|| events);
    use_context_provider(|| xmpp_client.clone());

    use_future(move || {
        let platform = notif_platform.clone();
        let events = notif_events.clone();
        let roster = notif_roster.clone();
        let selected = selected_chat_signal;
        let notifications = notifications_signal;
        async move {
            let mut rx = events.subscribe();
            while let Ok(event) = rx.recv().await {
                if let XmppEvent::MessageReceived { jid, message } = event {
                    if !*notifications.read() {
                        continue;
                    }
                    let Some(body) = message.body.clone().filter(|b| !b.is_empty()) else {
                        continue;
                    };
                    if message.is_own || *selected.read() == Some(jid.clone()) {
                        continue;
                    }
                    let title = roster
                        .get_by_bare(&jid)
                        .map(|r| r.display_name())
                        .unwrap_or_else(|| jid.clone());
                    let notification = platform::Notification {
                        id: message.id.clone(),
                        title,
                        body,
                        icon: None,
                        tag: Some(jid),
                        data: None,
                        actions: Vec::new(),
                        require_interaction: false,
                        silent: false,
                        timestamp: chrono::Utc::now(),
                    };
                    if let Err(e) = platform.notifications.show_notification(&notification) {
                        tracing::warn!("Failed to show notification: {}", e);
                    }
                }
            }
        }
    });

    // Start event processor on main thread using use_future
    let connection = xmpp_client.connection.clone();
    let roster = xmpp_client.roster.clone();
    let chats = xmpp_client.chats.clone();
    let mucs = xmpp_client.mucs.clone();
    let pubsub = xmpp_client.pubsub.clone();
    let disco = xmpp_client.disco.clone();
    let push = xmpp_client.push.clone();
    let sm = xmpp_client.sm.clone();
    let omemo = xmpp_client.omemo.clone();

    // Use a flag to ensure processor only starts once
    let processor_started = use_signal(|| false);

    use_future(move || {
        let connection = connection.clone();
        let roster = roster.clone();
        let chats = chats.clone();
        let mucs = mucs.clone();
        let pubsub = pubsub.clone();
        let disco = disco.clone();
        let push = push.clone();
        let sm = sm.clone();
        let omemo = omemo.clone();
        let mut processor_started = processor_started;
        async move {
            // Only start processor once
            if *processor_started.read() {
                return;
            }
            processor_started.set(true);

            // Get event_rx from thread-local
            let event_rx = EVENT_RX.with(|cell| cell.borrow_mut().take());

            if let Some(event_rx) = event_rx {
                let processor = EventProcessor::new(
                    connection, roster, chats, mucs, pubsub, disco, push, sm, omemo,
                )
                .with_ui(revision_signal, connection_state_signal);
                if let Err(e) = processor.start(event_rx).await {
                    tracing::error!("Event processor error: {}", e);
                }
            }
        }
    });

    rsx! { App {} }
}

/// Application configuration
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct AppConfig {
    pub xmpp: ClientConfig,
    pub ui: UiConfig,
    pub storage: StorageConfig,
    pub notifications: NotificationConfig,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct UiConfig {
    pub theme: String,
    pub language: String,
    pub font_size: u32,
    pub compact_mode: bool,
    pub show_avatars: bool,
    pub show_timestamps: bool,
    pub animate: bool,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct StorageConfig {
    pub backend: String, // "redb", "sqlite", "indexeddb"
    pub path: Option<String>,
    pub max_message_history: usize,
    pub auto_cleanup: bool,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
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
}

#[derive(Subcommand)]
enum AccountAction {
    /// Add a new account
    Add { jid: String },
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

    // Handle commands
    if let Some(cmd) = cli.command {
        return handle_command(cmd, config);
    }

    // Run the application
    run_app(config)
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
    let mut figment = Figment::new().merge(Toml::file("config.toml"));

    if let Some(path) = config_path {
        figment = figment.merge(Toml::file(path));
    }

    figment = figment.merge(Env::prefixed("JABU_"));

    Ok(figment.extract()?)
}

/// Handle CLI commands
fn handle_command(cmd: Command, config: AppConfig) -> Result<()> {
    match cmd {
        Command::Run => run_app(config),
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
    }
}

fn handle_account_action(action: AccountAction, _config: AppConfig) -> Result<()> {
    match action {
        AccountAction::Add { jid } => {
            println!("Adding account: {}", jid);
        }
        AccountAction::Remove { jid } => {
            println!("Removing account: {}", jid);
        }
        AccountAction::List => {
            println!("Accounts:");
        }
    }
    Ok(())
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

/// Run the Dioxus application
fn run_app(config: AppConfig) -> Result<()> {
    let mut config = config;
    info!("Starting Jabu v{}", env!("CARGO_PKG_VERSION"));

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
        if let Some(credentials) = storage::load_credentials(storage.as_ref()) {
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

    // Create connection and get event receiver
    let (connection_for_processor, event_rx) = Connection::new(config.xmpp.clone().into());
    let connection = Arc::new(connection_for_processor);

    let roster = Arc::new(Roster::new(events.clone()));
    let chats = Arc::new(ChatManager::new(events.clone()));
    let mucs = Arc::new(MucManager::new(events.clone()));
    let pubsub = Arc::new(PubSubManager::new(events.clone()));
    let disco = Arc::new(DiscoManager::new());
    let push = Arc::new(PushManager::new());
    let sm = Arc::new(StreamManagement::new());
    let omemo = Arc::new(OmemoManager::new());
    if has_account {
        omemo.set_store(Arc::new(SettingsOmemoStore::new(
            storage.clone(),
            &config.xmpp.jid,
        )));
        if let Err(e) = omemo.load_persisted() {
            tracing::warn!("Failed to load OMEMO state: {}", e);
        }
        omemo.ensure_local_auto(100);
    }
    connection.set_stream_management(sm.clone());

    // Create XMPP client
    let xmpp_client = XmppClient {
        connection: connection.clone(),
        roster: roster.clone(),
        chats: chats.clone(),
        mucs: mucs.clone(),
        pubsub: pubsub.clone(),
        disco: disco.clone(),
        push: push.clone(),
        sm: sm.clone(),
        omemo: omemo.clone(),
        events: events.clone(),
    };

    // Connect if JID is configured
    let connection_for_connect = connection.clone();

    if has_account {
        tokio::spawn(async move {
            if let Err(e) = connection_for_connect.connect().await {
                tracing::error!("Failed to connect: {}", e);
            }
        });
    }

    // Launch Dioxus app
    let app_ctx = AppContext {
        platform: platform.clone(),
        storage: storage.clone(),
        events: events.clone(),
        xmpp_client: xmpp_client.clone(),
        initial_config: config.xmpp.clone(),
    };
    APP_CONTEXT.with(|cell| {
        *cell.borrow_mut() = Some(app_ctx);
    });
    EVENT_RX.with(|cell| {
        *cell.borrow_mut() = Some(event_rx);
    });

    launch(app_root);

    Ok(())
}
