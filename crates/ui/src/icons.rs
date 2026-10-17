//! Lucide icons (<https://lucide.dev>), ISC licensed.
//!
//! Icon geometry is derived from the Lucide icon set.

use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Shape {
    Path(&'static str),
    Circle {
        cx: f32,
        cy: f32,
        r: f32,
    },
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        rx: f32,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IconName {
    Archive,
    ArrowLeft,
    BellOff,
    Check,
    CheckCheck,
    ChevronDown,
    CircleX,
    Clock,
    Forward,
    Hash,
    Lock,
    LockKeyhole,
    Minus,
    Mic,
    MicOff,
    Paperclip,
    Pencil,
    Phone,
    PhoneIncoming,
    PhoneMissed,
    PhoneOff,
    PhoneOutgoing,
    Pin,
    Puzzle,
    RefreshCw,
    Reply,
    Search,
    Send,
    Smile,
    Square,
    Star,
    Trash,
    TriangleAlert,
    Users,
    Video,
    VideoOff,
    X,
}

impl IconName {
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Archive => "archive",
            Self::ArrowLeft => "arrow-left",
            Self::BellOff => "bell-off",
            Self::Check => "check",
            Self::CheckCheck => "check-check",
            Self::ChevronDown => "chevron-down",
            Self::CircleX => "circle-x",
            Self::Clock => "clock",
            Self::Forward => "forward",
            Self::Hash => "hash",
            Self::Lock => "lock",
            Self::LockKeyhole => "lock-keyhole",
            Self::Minus => "minus",
            Self::Mic => "mic",
            Self::MicOff => "mic-off",
            Self::Paperclip => "paperclip",
            Self::Pencil => "pencil",
            Self::Phone => "phone",
            Self::PhoneIncoming => "phone-incoming",
            Self::PhoneMissed => "phone-missed",
            Self::PhoneOff => "phone-off",
            Self::PhoneOutgoing => "phone-outgoing",
            Self::Pin => "pin",
            Self::Puzzle => "puzzle",
            Self::RefreshCw => "refresh-cw",
            Self::Reply => "reply",
            Self::Search => "search",
            Self::Send => "send",
            Self::Smile => "smile",
            Self::Square => "square",
            Self::Star => "star",
            Self::Trash => "trash",
            Self::TriangleAlert => "triangle-alert",
            Self::Users => "users",
            Self::Video => "video",
            Self::VideoOff => "video-off",
            Self::X => "x",
        }
    }

    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub const fn shapes(self) -> &'static [Shape] {
        match self {
            Self::Archive => &[
                Shape::Rect {
                    x: 2.0,
                    y: 3.0,
                    width: 20.0,
                    height: 5.0,
                    rx: 1.0,
                },
                Shape::Path("M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8"),
                Shape::Path("M10 12h4"),
            ],
            Self::ArrowLeft => &[Shape::Path("m12 19-7-7 7-7"), Shape::Path("M19 12H5")],
            Self::BellOff => &[
                Shape::Path("M10.268 21a2 2 0 0 0 3.464 0"),
                Shape::Path(
                    "M17 17H4a1 1 0 0 1-.74-1.673C4.59 13.956 6 12.499 6 8a6 6 0 0 1 .258-1.742",
                ),
                Shape::Path("m2 2 20 20"),
                Shape::Path("M8.668 3.01A6 6 0 0 1 18 8c0 2.687.77 4.653 1.707 6.05"),
            ],
            Self::Check => &[Shape::Path("M20 6 9 17l-5-5")],
            Self::CheckCheck => &[
                Shape::Path("M18 6 7 17l-5-5"),
                Shape::Path("m22 10-7.5 7.5L13 16"),
            ],
            Self::ChevronDown => &[Shape::Path("m6 9 6 6 6-6")],
            Self::CircleX => &[
                Shape::Circle {
                    cx: 12.0,
                    cy: 12.0,
                    r: 10.0,
                },
                Shape::Path("m15 9-6 6"),
                Shape::Path("m9 9 6 6"),
            ],
            Self::Clock => &[
                Shape::Circle {
                    cx: 12.0,
                    cy: 12.0,
                    r: 10.0,
                },
                Shape::Path("M12 6v6l4 2"),
            ],
            Self::Forward => &[
                Shape::Path("m15 17 5-5-5-5"),
                Shape::Path("M4 18v-2a4 4 0 0 1 4-4h12"),
            ],
            Self::Hash => &[
                Shape::Path("M4 9h16"),
                Shape::Path("M4 15h16"),
                Shape::Path("M10 3 8 21"),
                Shape::Path("M16 3 14 21"),
            ],
            Self::Lock => &[
                Shape::Rect {
                    x: 3.0,
                    y: 11.0,
                    width: 18.0,
                    height: 11.0,
                    rx: 2.0,
                },
                Shape::Path("M7 11V7a5 5 0 0 1 10 0v4"),
            ],
            Self::LockKeyhole => &[
                Shape::Circle {
                    cx: 12.0,
                    cy: 16.0,
                    r: 1.0,
                },
                Shape::Rect {
                    x: 3.0,
                    y: 10.0,
                    width: 18.0,
                    height: 12.0,
                    rx: 2.0,
                },
                Shape::Path("M7 10V7a5 5 0 0 1 10 0v3"),
            ],
            Self::Minus => &[Shape::Path("M5 12h14")],
            Self::Mic => &[
                Shape::Path("M12 19v3"),
                Shape::Path("M19 10v2a7 7 0 0 1-14 0v-2"),
                Shape::Rect {
                    x: 9.0,
                    y: 2.0,
                    width: 6.0,
                    height: 13.0,
                    rx: 3.0,
                },
            ],
            Self::MicOff => &[
                Shape::Path("M12 19v3"),
                Shape::Path("M15 9.34V5a3 3 0 0 0-5.68-1.33"),
                Shape::Path("M16.95 16.95A7 7 0 0 1 5 12v-2"),
                Shape::Path("M18.89 13.23A7 7 0 0 0 19 12v-2"),
                Shape::Path("m2 2 20 20"),
                Shape::Path("M9 9v3a3 3 0 0 0 5.12 2.12"),
            ],
            Self::Paperclip => &[Shape::Path(
                "m16 6-8.414 8.586a2 2 0 0 0 2.829 2.829l8.414-8.586a4 4 0 1 0-5.657-5.657l-8.379 8.551a6 6 0 1 0 8.485 8.485l8.379-8.551",
            )],
            Self::Pencil => &[
                Shape::Path(
                    "M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z",
                ),
                Shape::Path("m15 5 4 4"),
            ],
            Self::Phone => &[Shape::Path(
                "M13.832 16.568a1 1 0 0 0 1.213-.303l.355-.465A2 2 0 0 1 17 15h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2A18 18 0 0 1 2 4a2 2 0 0 1 2-2h3a2 2 0 0 1 2 2v3a2 2 0 0 1-.8 1.6l-.468.351a1 1 0 0 0-.292 1.233 14 14 0 0 0 6.392 6.384",
            )],
            Self::PhoneIncoming => &[
                Shape::Path("M16 2v6h6"),
                Shape::Path("m22 2-6 6"),
                Shape::Path(
                    "M13.832 16.568a1 1 0 0 0 1.213-.303l.355-.465A2 2 0 0 1 17 15h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2A18 18 0 0 1 2 4a2 2 0 0 1 2-2h3a2 2 0 0 1 2 2v3a2 2 0 0 1-.8 1.6l-.468.351a1 1 0 0 0-.292 1.233 14 14 0 0 0 6.392 6.384",
                ),
            ],
            Self::PhoneMissed => &[
                Shape::Path("m16 2 6 6"),
                Shape::Path("m22 2-6 6"),
                Shape::Path(
                    "M13.832 16.568a1 1 0 0 0 1.213-.303l.355-.465A2 2 0 0 1 17 15h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2A18 18 0 0 1 2 4a2 2 0 0 1 2-2h3a2 2 0 0 1 2 2v3a2 2 0 0 1-.8 1.6l-.468.351a1 1 0 0 0-.292 1.233 14 14 0 0 0 6.392 6.384",
                ),
            ],
            Self::PhoneOff => &[
                Shape::Path(
                    "M10.1 13.9a14 14 0 0 0 3.732 2.668 1 1 0 0 0 1.213-.303l.355-.465A2 2 0 0 1 17 15h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2 18 18 0 0 1-12.728-5.272",
                ),
                Shape::Path("M22 2 2 22"),
                Shape::Path(
                    "M4.76 13.582A18 18 0 0 1 2 4a2 2 0 0 1 2-2h3a2 2 0 0 1 2 2v3a2 2 0 0 1-.8 1.6l-.468.351a1 1 0 0 0-.292 1.233 14 14 0 0 0 .244.473",
                ),
            ],
            Self::PhoneOutgoing => &[
                Shape::Path("m16 8 6-6"),
                Shape::Path("M22 8V2h-6"),
                Shape::Path(
                    "M13.832 16.568a1 1 0 0 0 1.213-.303l.355-.465A2 2 0 0 1 17 15h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2A18 18 0 0 1 2 4a2 2 0 0 1 2-2h3a2 2 0 0 1 2 2v3a2 2 0 0 1-.8 1.6l-.468.351a1 1 0 0 0-.292 1.233 14 14 0 0 0 6.392 6.384",
                ),
            ],
            Self::Pin => &[
                Shape::Path("M12 17v5"),
                Shape::Path(
                    "M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z",
                ),
            ],
            Self::Puzzle => &[Shape::Path(
                "M15.39 4.39a1 1 0 0 0 1.68-.474 2.5 2.5 0 1 1 3.014 3.015 1 1 0 0 0-.474 1.68l1.683 1.682a2.414 2.414 0 0 1 0 3.414L19.61 15.39a1 1 0 0 1-1.68-.474 2.5 2.5 0 1 0-3.014 3.015 1 1 0 0 1 .474 1.68l-1.683 1.682a2.414 2.414 0 0 1-3.414 0L8.61 19.61a1 1 0 0 0-1.68.474 2.5 2.5 0 1 1-3.014-3.015 1 1 0 0 0 .474-1.68l-1.683-1.682a2.414 2.414 0 0 1 0-3.414L4.39 8.61a1 1 0 0 1 1.68.474 2.5 2.5 0 1 0 3.014-3.015 1 1 0 0 1-.474-1.68l1.683-1.682a2.414 2.414 0 0 1 3.414 0z",
            )],
            Self::RefreshCw => &[
                Shape::Path("M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"),
                Shape::Path("M21 3v5h-5"),
                Shape::Path("M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"),
                Shape::Path("M8 16H3v5"),
            ],
            Self::Reply => &[
                Shape::Path("M20 18v-2a4 4 0 0 0-4-4H4"),
                Shape::Path("m9 17-5-5 5-5"),
            ],
            Self::Search => &[
                Shape::Path("m21 21-4.34-4.34"),
                Shape::Circle {
                    cx: 11.0,
                    cy: 11.0,
                    r: 8.0,
                },
            ],
            Self::Send => &[
                Shape::Path(
                    "M14.536 21.686a.5.5 0 0 0 .937-.024l6.5-19a.496.496 0 0 0-.635-.635l-19 6.5a.5.5 0 0 0-.024.937l7.93 3.18a2 2 0 0 1 1.112 1.11z",
                ),
                Shape::Path("m21.854 2.147-10.94 10.939"),
            ],
            Self::Smile => &[
                Shape::Path("M15 10V9"),
                Shape::Path("M16.472 15a6 6 0 01-8.943 0"),
                Shape::Path("M9 10V9"),
                Shape::Circle {
                    cx: 12.0,
                    cy: 12.0,
                    r: 10.0,
                },
            ],
            Self::Square => &[Shape::Rect {
                x: 3.0,
                y: 3.0,
                width: 18.0,
                height: 18.0,
                rx: 2.0,
            }],
            Self::Star => &[Shape::Path(
                "M11.525 2.295a.53.53 0 0 1 .95 0l2.31 4.679a2.123 2.123 0 0 0 1.595 1.16l5.166.756a.53.53 0 0 1 .294.904l-3.736 3.638a2.123 2.123 0 0 0-.611 1.878l.882 5.14a.53.53 0 0 1-.771.56l-4.618-2.428a2.122 2.122 0 0 0-1.973 0L6.396 21.01a.53.53 0 0 1-.77-.56l.881-5.139a2.122 2.122 0 0 0-.611-1.879L2.16 9.795a.53.53 0 0 1 .294-.906l5.165-.755a2.122 2.122 0 0 0 1.597-1.16z",
            )],
            Self::Trash => &[
                Shape::Path("M10 11v6"),
                Shape::Path("M14 11v6"),
                Shape::Path("M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"),
                Shape::Path("M3 6h18"),
                Shape::Path("M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"),
            ],
            Self::TriangleAlert => &[
                Shape::Path(
                    "m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3",
                ),
                Shape::Path("M12 9v4"),
                Shape::Path("M12 17h.01"),
            ],
            Self::Users => &[
                Shape::Path("M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"),
                Shape::Path("M16 3.128a4 4 0 0 1 0 7.744"),
                Shape::Path("M22 21v-2a4 4 0 0 0-3-3.87"),
                Shape::Circle {
                    cx: 9.0,
                    cy: 7.0,
                    r: 4.0,
                },
            ],
            Self::Video => &[
                Shape::Path(
                    "m16 13 5.223 3.482a.5.5 0 0 0 .777-.416V7.87a.5.5 0 0 0-.752-.432L16 10.5",
                ),
                Shape::Rect {
                    x: 2.0,
                    y: 6.0,
                    width: 14.0,
                    height: 12.0,
                    rx: 2.0,
                },
            ],
            Self::VideoOff => &[
                Shape::Path(
                    "M10.66 6H14a2 2 0 0 1 2 2v2.5l5.248-3.062A.5.5 0 0 1 22 7.87v8.196",
                ),
                Shape::Path("M16 16a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h2"),
                Shape::Path("m2 2 20 20"),
            ],
            Self::X => &[Shape::Path("M18 6 6 18"), Shape::Path("m6 6 12 12")],
        }
    }
}

#[component]
pub fn Icon(
    name: IconName,
    #[props(default = 24)] size: u32,
    #[props(default = 2.0)] stroke_width: f32,
    #[props(default = String::from("currentColor"))] color: String,
    #[props(default)] class: String,
) -> Element {
    let class = format!("lucide lucide-{} {}", name.slug(), class);
    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            width: "{size}",
            height: "{size}",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "{color}",
            stroke_width: "{stroke_width}",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            "aria-hidden": "true",
            for shape in name.shapes() {
                {match shape {
                    Shape::Path(d) => rsx! { path { d: *d } },
                    Shape::Circle { cx, cy, r } => rsx! {
                        circle { cx: "{cx}", cy: "{cy}", r: "{r}" }
                    },
                    Shape::Rect { x, y, width, height, rx } => rsx! {
                        rect { x: "{x}", y: "{y}", width: "{width}", height: "{height}", rx: "{rx}" }
                    },
                }}
            }
        }
    }
}
