#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct SpaceId {
    pub server: String,
    pub node: String,
}

impl SpaceId {
    #[must_use]
    pub fn new(server: impl Into<String>, node: impl Into<String>) -> Self {
        Self {
            server: server.into(),
            node: node.into(),
        }
    }

    #[must_use]
    pub fn key(&self) -> String {
        format!("{}:{}", self.server, self.node)
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct SpaceDirectory {
    pub id: String,
    pub name: String,
    pub order: usize,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct SpaceRoom {
    pub conference: String,
    pub name: String,
    pub avatar: Option<String>,
    pub unread: usize,
    pub directory_id: Option<String>,
    pub in_call: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SpaceNotify {
    #[default]
    Always,
    Mentions,
    Never,
}

impl SpaceNotify {
    pub const ALL: [Self; 3] = [Self::Always, Self::Mentions, Self::Never];

    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::Mentions => "mentions",
            Self::Never => "never",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Always => "All messages",
            Self::Mentions => "Mentions only",
            Self::Never => "Nothing",
        }
    }

    #[must_use]
    pub fn from_slug(value: &str) -> Self {
        match value {
            "mentions" => Self::Mentions,
            "never" => Self::Never,
            _ => Self::Always,
        }
    }
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct Space {
    pub id: SpaceId,
    pub name: String,
    pub description: String,
    pub avatar: Option<String>,
    pub members: usize,
    pub unread: usize,
    pub pinned: bool,
    pub locked: bool,
    pub pending: bool,
    pub notify: SpaceNotify,
    pub directories: Vec<SpaceDirectory>,
    pub rooms: Vec<SpaceRoom>,
}

impl Space {
    #[must_use]
    pub fn first_room(&self) -> Option<&SpaceRoom> {
        self.rooms.first()
    }
}

#[must_use]
pub fn initial_spaces() -> Vec<Space> {
    if std::env::var_os("JABU_DEMO_SPACES").is_some() {
        demo_spaces()
    } else {
        Vec::new()
    }
}

fn demo_spaces() -> Vec<Space> {
    vec![
        Space {
            id: SpaceId::new("jabu.localhost", "jabu"),
            name: "Jabu".to_string(),
            description: "The Jabu community".to_string(),
            members: 248,
            unread: 7,
            pinned: true,
            notify: SpaceNotify::Mentions,
            directories: vec![
                SpaceDirectory {
                    id: "general".to_string(),
                    name: "General".to_string(),
                    order: 0,
                },
                SpaceDirectory {
                    id: "dev".to_string(),
                    name: "Development".to_string(),
                    order: 1,
                },
            ],
            rooms: vec![
                SpaceRoom {
                    conference: "jabu@conference.localhost".to_string(),
                    name: "jabu".to_string(),
                    unread: 4,
                    directory_id: Some("general".to_string()),
                    ..SpaceRoom::default()
                },
                SpaceRoom {
                    conference: "news@conference.localhost".to_string(),
                    name: "news".to_string(),
                    unread: 3,
                    directory_id: Some("general".to_string()),
                    ..SpaceRoom::default()
                },
                SpaceRoom {
                    conference: "dev@conference.localhost".to_string(),
                    name: "development".to_string(),
                    directory_id: Some("dev".to_string()),
                    in_call: true,
                    ..SpaceRoom::default()
                },
            ],
            ..Space::default()
        },
        Space {
            id: SpaceId::new("conference.localhost", "rust"),
            name: "Rust".to_string(),
            description: "Rust and XMPP".to_string(),
            members: 92,
            unread: 2,
            notify: SpaceNotify::Always,
            rooms: vec![SpaceRoom {
                conference: "rust@conference.localhost".to_string(),
                name: "rust".to_string(),
                unread: 2,
                ..SpaceRoom::default()
            }],
            ..Space::default()
        },
        Space {
            id: SpaceId::new("design.localhost", "design"),
            name: "Design".to_string(),
            description: "Private design space".to_string(),
            members: 12,
            locked: true,
            pending: true,
            notify: SpaceNotify::Never,
            ..Space::default()
        },
    ]
}
