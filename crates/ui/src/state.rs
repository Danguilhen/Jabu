use dioxus::prelude::{ReadableExt, Signal, WritableExt};

use xmpp_core::{ClientConfig, ConnectionState, XmppClient};

/// Global client state for Dioxus signals.
#[derive(Clone)]
pub struct ClientState {
    pub config: Signal<Option<ClientConfig>>,
    pub client: Signal<Option<XmppClient>>,
    pub connection_state: Signal<ConnectionState>,
    pub is_connected: Signal<bool>,
    pub is_authenticated: Signal<bool>,
    pub last_error: Signal<Option<String>>,
    /// Bumped whenever the underlying XMPP data changes, so components re-read it.
    pub revision: Signal<u64>,
    /// Bare JID of the currently open conversation (1:1 chat or MUC room).
    pub selected_chat: Signal<Option<String>>,
    /// Whether the main area shows the social feed instead of a conversation.
    pub show_feed: Signal<bool>,
    /// Whether the global search dialog is open.
    pub search_open: Signal<bool>,
    /// Whether desktop notifications are shown for background messages.
    pub notifications_enabled: Signal<bool>,
    /// Whether message timestamps are shown under each bubble.
    pub show_timestamps: Signal<bool>,
    /// Bare JID of the contact whose profile drawer is open.
    pub selected_profile: Signal<Option<String>>,
    /// Bare JIDs with saved credentials.
    pub accounts: Signal<Vec<String>>,
    /// Bare JID of the account currently connected.
    pub active_account: Signal<Option<String>>,
}

impl ClientState {
    pub fn new() -> Self {
        let config = Signal::new(None);
        let client = Signal::new(None);
        let connection_state = Signal::new(ConnectionState::Disconnected);
        let is_connected = Signal::new(false);
        let is_authenticated = Signal::new(false);
        let last_error = Signal::new(None);
        let revision = Signal::new(0);
        let selected_chat = Signal::new(None);
        let show_feed = Signal::new(false);
        let search_open = Signal::new(false);
        let notifications_enabled = Signal::new(true);
        let show_timestamps = Signal::new(true);
        let selected_profile = Signal::new(None);
        let accounts = Signal::new(Vec::new());
        let active_account = Signal::new(None);

        Self {
            config,
            client,
            connection_state,
            is_connected,
            is_authenticated,
            last_error,
            revision,
            selected_chat,
            show_feed,
            search_open,
            notifications_enabled,
            show_timestamps,
            selected_profile,
            accounts,
            active_account,
        }
    }

    /// Signal the UI that underlying data changed.
    pub fn bump(&mut self) {
        let next = *self.revision.read() + 1;
        self.revision.set(next);
    }

    /// Open or close the contact profile drawer.
    pub fn show_profile(&mut self, jid: Option<String>) {
        self.selected_profile.set(jid);
    }

    /// Point the UI at a newly active account.
    pub fn set_active_account(&mut self, jid: String, config: ClientConfig, client: XmppClient) {
        self.active_account.set(Some(jid));
        self.config.set(Some(config));
        self.client.set(Some(client.clone()));
        self.connection_state.set(client.connection.current_state());
        self.selected_chat.set(None);
        self.selected_profile.set(None);
        self.show_feed.set(false);
        self.bump();
    }

    /// Select a conversation by bare JID (or clear it).
    pub fn select(&mut self, jid: Option<String>) {
        self.selected_chat.set(jid);
    }
}

impl Default for ClientState {
    fn default() -> Self {
        Self::new()
    }
}
