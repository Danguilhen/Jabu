use dioxus::prelude::{ReadableExt, Signal, WritableExt};

use crate::spaces::{initial_spaces, Space, SpaceId};
use xmpp_core::{ClientConfig, ConnectionState, XmppClient};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Theme {
    #[default]
    Dark,
    Light,
    Silver,
    Butter,
}

impl Theme {
    pub const ALL: [(Self, &'static str); 4] = [
        (Self::Light, "Light"),
        (Self::Silver, "Silver"),
        (Self::Dark, "Dark"),
        (Self::Butter, "Butter"),
    ];

    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::Silver => "silver",
            Self::Butter => "butter",
        }
    }

    #[must_use]
    pub fn from_slug(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "light" => Self::Light,
            "silver" => Self::Silver,
            "butter" => Self::Butter,
            _ => Self::Dark,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AppSection {
    #[default]
    Home,
    Contacts,
    Rooms,
    Feed,
    Communities,
}

/// The presence state shown in the user panel and profile popup.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PresenceChoice {
    #[default]
    Online,
    Idle,
    Dnd,
    Xa,
    Invisible,
}

impl PresenceChoice {
    /// The four choices shown in the Discord-style quick switcher.
    pub const QUICK: [Self; 4] = [Self::Online, Self::Idle, Self::Dnd, Self::Invisible];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Online => "Online",
            Self::Idle => "Idle",
            Self::Dnd => "Do not disturb",
            Self::Xa => "Extended away",
            Self::Invisible => "Invisible",
        }
    }

    #[must_use]
    pub const fn show(self) -> Option<&'static str> {
        match self {
            Self::Online => None,
            Self::Idle => Some("away"),
            Self::Dnd => Some("dnd"),
            Self::Xa => Some("xa"),
            Self::Invisible => None,
        }
    }

    #[must_use]
    pub fn from_show(show: Option<&str>) -> Self {
        match show {
            Some("away") => Self::Idle,
            Some("dnd") => Self::Dnd,
            Some("xa") => Self::Xa,
            _ => Self::Online,
        }
    }

    #[must_use]
    pub const fn dot_class(self) -> &'static str {
        match self {
            Self::Online => "bg-green-500",
            Self::Idle => "bg-amber-400",
            Self::Dnd => "bg-red-500",
            Self::Xa => "bg-amber-500",
            Self::Invisible => "bg-gray-500",
        }
    }
}

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
    /// Active top-level section in the space rail.
    pub section: Signal<AppSection>,
    /// Spaces joined by the account.
    pub spaces: Signal<Vec<Space>>,
    /// Space whose rooms are shown in the sidebar.
    pub selected_space: Signal<Option<SpaceId>>,
    /// Community currently open in the Communities section.
    pub selected_community: Signal<Option<xmpp_core::CommunityId>>,
    /// Whether the global search dialog is open.
    pub search_open: Signal<bool>,
    /// Whether desktop notifications are shown for background messages.
    pub notifications_enabled: Signal<bool>,
    /// Whether message timestamps are shown under each bubble.
    pub show_timestamps: Signal<bool>,
    /// Whether the user allows fetching link previews on demand.
    pub link_previews: Signal<bool>,
    /// Base font size in pixels; applied to the document root.
    pub font_size: Signal<u32>,
    /// Whether compact spacing is applied globally.
    pub compact: Signal<bool>,
    /// Active color theme.
    pub theme: Signal<Theme>,
    /// Bare JID of the contact whose profile drawer is open.
    pub selected_profile: Signal<Option<String>>,
    /// Global story viewer snapshot, when open.
    pub story_view: Signal<Option<crate::stories::StoryView>>,
    /// Whether the story composer is open.
    pub story_composer_open: Signal<bool>,
    /// Bare JIDs with saved credentials.
    pub accounts: Signal<Vec<String>>,
    /// Bare JID of the account currently connected.
    pub active_account: Signal<Option<String>>,
    /// Presence currently shown in the user panel.
    pub own_presence: Signal<PresenceChoice>,
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
        let initial_spaces = initial_spaces();
        let selected_chat = Signal::new(std::env::var("JABU_START_CHAT").ok().or_else(|| {
            initial_spaces
                .first()
                .and_then(|space| space.first_room())
                .map(|room| room.conference.clone())
        }));
        let section = Signal::new(AppSection::default());
        let spaces = Signal::new(initial_spaces);
        let selected_space = Signal::new(spaces.read().first().map(|space| space.id.clone()));
        let selected_community = Signal::new(None);
        let search_open = Signal::new(false);
        let notifications_enabled = Signal::new(true);
        let show_timestamps = Signal::new(true);
        let link_previews = Signal::new(false);
        let font_size = Signal::new(16);
        let compact = Signal::new(false);
        let theme = Signal::new(Theme::default());
        let selected_profile = Signal::new(None);
        let story_view = Signal::new(None);
        let story_composer_open = Signal::new(false);
        let accounts = Signal::new(Vec::new());
        let active_account = Signal::new(None);
        let own_presence = Signal::new(PresenceChoice::Online);

        Self {
            config,
            client,
            connection_state,
            is_connected,
            is_authenticated,
            last_error,
            revision,
            selected_chat,
            section,
            spaces,
            selected_space,
            selected_community,
            search_open,
            notifications_enabled,
            show_timestamps,
            link_previews,
            font_size,
            compact,
            theme,
            selected_profile,
            story_view,
            story_composer_open,
            accounts,
            active_account,
            own_presence,
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
        self.section.set(AppSection::Home);
        self.selected_space.set(None);
        self.selected_community.set(None);
        self.story_view.set(None);
        self.story_composer_open.set(false);
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
