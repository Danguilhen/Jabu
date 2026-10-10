//! UI components for Jabu

use dioxus::prelude::*;
use xmpp_core::{ClientState, ConnectionState, XmppClient};

/// Main app component
#[component]
pub fn App() -> Element {
    let client_state = use_context::<ClientState>();

    rsx! {
        div { class: "app h-screen w-screen flex flex-col bg-gray-900 text-white",
            // Connection status bar
            ConnectionStatusBar {}

            // Main content area
            div { class: "flex-1 flex overflow-hidden",
                // Sidebar with contacts/chats
                Sidebar {}

                // Main chat area
                MainChatArea {}
            }
        }
    }
}

/// Connection status bar
#[component]
fn ConnectionStatusBar() -> Element {
    let client_state = use_context::<ClientState>();
    let connection_state = client_state.connection_state.read();

    rsx! {
        div { class: "flex items-center justify-between px-4 py-2 bg-gray-800 border-b border-gray-700",
            div { class: "flex items-center gap-3",
                // Connection indicator
                div { class: "flex items-center gap-2",
                    ConnectionIndicator { state: (*connection_state).clone() }
                    span { class: "text-sm font-medium", "{format_connection_state(&connection_state)}" }
                }
            }

            div { class: "flex items-center gap-4",
                // Search
                div { class: "relative",
                    input {
                        class: "w-64 px-3 py-1.5 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-teal-500 focus:border-transparent",
                        placeholder: "Search...",
                        type: "text",
                    }
                }

                // User menu
                UserMenu {}
            }
        }
    }
}

/// Format connection state for display
fn format_connection_state(state: &ConnectionState) -> &str {
    match state {
        ConnectionState::Disconnected => "Disconnected",
        ConnectionState::Connecting => "Connecting",
        ConnectionState::Connected => "Connected",
        ConnectionState::Authenticated => "Authenticated",
        ConnectionState::Reconnecting { .. } => "Reconnecting",
        ConnectionState::Failed { .. } => "Failed",
    }
}

/// Connection indicator dot
#[component]
fn ConnectionIndicator(state: xmpp_core::connection::ConnectionState) -> Element {
    let (color, pulse) = match state {
        xmpp_core::connection::ConnectionState::Connected
        | xmpp_core::connection::ConnectionState::Authenticated => ("bg-green-500", true),
        xmpp_core::connection::ConnectionState::Connecting
        | xmpp_core::connection::ConnectionState::Reconnecting { .. } => ("bg-yellow-500", true),
        xmpp_core::connection::ConnectionState::Failed { .. } => ("bg-red-500", false),
        _ => ("bg-gray-500", false),
    };

    rsx! {
        div { class: "relative",
            div { class: "w-2.5 h-2.5 rounded-full {color}" }
            if pulse {
                div { class: "absolute inset-0 w-2.5 h-2.5 rounded-full {color} animate-ping opacity-75" }
            }
        }
    }
}

/// User menu dropdown
#[component]
fn UserMenu() -> Element {
    rsx! {
        div { class: "relative group",
            button { class: "flex items-center gap-2 p-2 rounded-lg hover:bg-gray-700 transition-colors",
                div { class: "w-8 h-8 rounded-full bg-teal-500 flex items-center justify-center font-medium text-white",
                    "U"
                }
                span { class: "hidden sm:block text-sm", "User" }
                svg { class: "w-4 h-4", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M19 9l-7 7-7-7" }
                }
            }
            div { class: "absolute right-0 mt-2 w-48 bg-gray-800 rounded-lg shadow-lg border border-gray-700 opacity-0 invisible group-hover:opacity-100 group-hover:visible transition-all",
                div { class: "py-1",
                    a { class: "block px-4 py-2 text-sm text-gray-300 hover:bg-gray-700", href: "#", "Profile" }
                    a { class: "block px-4 py-2 text-sm text-gray-300 hover:bg-gray-700", href: "#", "Settings" }
                    hr { class: "my-1 border-gray-700" }
                    button { class: "block w-full text-left px-4 py-2 text-sm text-red-400 hover:bg-gray-700", "Sign Out" }
                }
            }
        }
    }
}

/// Sidebar with contacts, chats, MUCs
#[component]
fn Sidebar() -> Element {
    rsx! {
        aside { class: "w-80 flex flex-col border-r border-gray-700 bg-gray-800",
            // Tabs
            div { class: "flex border-b border-gray-700",
                TabButton { label: "Chats", active: true }
                TabButton { label: "Contacts", active: false }
                TabButton { label: "Rooms", active: false }
            }

            // Content
            div { class: "flex-1 overflow-y-auto",
                ChatList {}
            }
        }
    }
}

/// Tab button
#[component]
fn TabButton(label: &'static str, active: bool) -> Element {
    let class = if active {
        "flex-1 py-3 px-4 text-center text-sm font-medium text-teal-400 border-b-2 border-teal-400"
    } else {
        "flex-1 py-3 px-4 text-center text-sm font-medium text-gray-400 hover:text-white hover:bg-gray-700"
    };

    rsx! {
        button { class: "{class} transition-colors", "{label}" }
    }
}

/// Chat list
#[component]
fn ChatList() -> Element {
    rsx! {
        div { class: "p-2 space-y-1",
            // Placeholder chats
            ChatListItem {
                name: "Alice Johnson",
                last_message: "Hey, how are you?",
                time: "2m ago",
                unread: 3,
                online: true,
                avatar: "A",
            }
            ChatListItem {
                name: "Bob Smith",
                last_message: "Thanks for the update!",
                time: "1h ago",
                unread: 0,
                online: false,
                avatar: "B",
            }
            ChatListItem {
                name: "Team Chat",
                last_message: "Meeting at 3pm",
                time: "3h ago",
                unread: 12,
                online: true,
                avatar: "T",
                is_muc: true,
            }
            ChatListItem {
                name: "Carol Williams",
                last_message: "See you tomorrow",
                time: "Yesterday",
                unread: 0,
                online: true,
                avatar: "C",
            }
        }
    }
}

/// Chat list item
#[component]
fn ChatListItem(
    name: &'static str,
    last_message: &'static str,
    time: &'static str,
    unread: u32,
    online: bool,
    avatar: &'static str,
    #[props(default = false)] is_muc: bool,
) -> Element {
    rsx! {
        button { class: "w-full flex items-center gap-3 p-3 rounded-lg hover:bg-gray-700 transition-colors text-left",
            div { class: "relative",
                div { class: "w-12 h-12 rounded-full bg-teal-500 flex items-center justify-center font-bold text-white text-lg",
                    "{avatar}"
                }
                if online {
                    div { class: "absolute bottom-0 right-0 w-3 h-3 bg-green-500 border-2 border-gray-800 rounded-full" }
                }
            }
            div { class: "flex-1 min-w-0",
                div { class: "flex items-center justify-between",
                    h4 { class: "font-medium truncate", "{name}" }
                    span { class: "text-xs text-gray-400 whitespace-nowrap", "{time}" }
                }
                p { class: "text-sm text-gray-400 truncate", "{last_message}" }
            }
            if unread > 0 {
                span { class: "flex items-center justify-center w-5 h-5 bg-teal-500 text-white text-xs rounded-full",
                    "{unread}"
                }
            }
        }
    }
}

/// Main chat area
#[component]
fn MainChatArea() -> Element {
    rsx! {
        main { class: "flex-1 flex flex-col bg-gray-900",
            // Chat header
            ChatHeader {}

            // Messages
            div { class: "flex-1 overflow-y-auto p-4 space-y-4",
                MessagesList {}
            }

            // Message input
            MessageInput {}
        }
    }
}

/// Chat header
#[component]
fn ChatHeader() -> Element {
    rsx! {
        header { class: "flex items-center justify-between px-4 py-3 bg-gray-800 border-b border-gray-700",
            div { class: "flex items-center gap-3",
                div { class: "w-10 h-10 rounded-full bg-teal-500 flex items-center justify-center font-bold text-white",
                    "A"
                }
                div {
                    h3 { class: "font-medium", "Alice Johnson" }
                    p { class: "text-xs text-green-400", "Online" }
                }
            }
            div { class: "flex items-center gap-2",
                button { class: "p-2 rounded-lg hover:bg-gray-700 transition-colors",
                    svg { class: "w-5 h-5", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z" }
                    }
                }
                button { class: "p-2 rounded-lg hover:bg-gray-700 transition-colors",
                    svg { class: "w-5 h-5", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" }
                    }
                }
                button { class: "p-2 rounded-lg hover:bg-gray-700 transition-colors",
                    svg { class: "w-5 h-5", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M4 6h16M4 12h16M4 18h16" }
                    }
                }
            }
        }
    }
}

/// Messages list
#[component]
fn MessagesList() -> Element {
    rsx! {
        div { class: "space-y-4",
            MessageBubble {
                sender: "Alice Johnson",
                avatar: "A",
                content: "Hey! How are you doing?",
                time: "10:30 AM",
                is_own: false,
                status: "read",
            }
            MessageBubble {
                sender: "You",
                avatar: "U",
                content: "I'm doing great! Thanks for asking.",
                time: "10:31 AM",
                is_own: true,
                status: "delivered",
            }
            MessageBubble {
                sender: "Alice Johnson",
                avatar: "A",
                content: "That's good to hear. Want to grab coffee later?",
                time: "10:32 AM",
                is_own: false,
                status: "read",
            }
            MessageBubble {
                sender: "You",
                avatar: "U",
                content: "Sure! Let's meet at the usual place?",
                time: "10:33 AM",
                is_own: true,
                status: "sent",
            }
        }
    }
}

/// Message bubble
#[component]
fn MessageBubble(
    sender: &'static str,
    avatar: &'static str,
    content: &'static str,
    time: &'static str,
    is_own: bool,
    status: &'static str,
) -> Element {
    let bubble_class = if is_own {
        "bg-teal-600 text-white rounded-2xl rounded-br-md"
    } else {
        "bg-gray-800 text-white rounded-2xl rounded-bl-md"
    };

    let align_class = if is_own {
        "flex-row-reverse"
    } else {
        "flex-row"
    };

    rsx! {
        div { class: "flex items-end gap-2 {align_class}",
            if !is_own {
                div { class: "w-8 h-8 rounded-full bg-teal-500 flex items-center justify-center font-bold text-white text-sm shrink-0",
                    "{avatar}"
                }
            }
            div { class: "max-w-xs lg:max-w-md",
                div { class: "{bubble_class} px-4 py-2",
                    if !is_own {
                        span { class: "text-xs text-gray-400 mb-1 block", "{sender}" }
                    }
                    p { class: "whitespace-pre-wrap", "{content}" }
                }
                div { class: "flex items-center gap-1 mt-1 text-xs text-gray-500",
                    span { "{time}" }
                    if is_own {
                        MessageStatus { status: status }
                    }
                }
            }
            if is_own {
                div { class: "w-8 h-8 rounded-full bg-gray-700 flex items-center justify-center font-bold text-white text-sm shrink-0",
                    "{avatar}"
                }
            }
        }
    }
}

/// Message status indicator
#[component]
fn MessageStatus(status: &'static str) -> Element {
    let (icon, color) = match status {
        "sending" => ("⏳", "text-yellow-400"),
        "sent" => ("✓", "text-gray-400"),
        "delivered" => ("✓✓", "text-gray-400"),
        "read" => ("✓✓", "text-teal-400"),
        "failed" => ("✗", "text-red-400"),
        _ => ("", "text-gray-400"),
    };

    rsx! {
        span { class: "{color}", "{icon}" }
    }
}

/// Message input area
#[component]
fn MessageInput() -> Element {
    let mut message = use_signal(|| String::new());

    rsx! {
        div { class: "p-4 bg-gray-800 border-t border-gray-700",
            div { class: "flex items-end gap-2",
                // Attachment button
                button { class: "p-2 rounded-lg hover:bg-gray-700 transition-colors text-gray-400 hover:text-white",
                    svg { class: "w-5 h-5", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M15.172 7l-6.586 6.586a2 2 0 102.828 2.828l6.414-6.586a4 4 0 00-5.656-5.656l-6.415 6.585a6 6 0 108.486 8.486L20.5 13" }
                    }
                }

                // Text input
                div { class: "flex-1 relative",
                    textarea {
                        class: "w-full px-4 py-3 bg-gray-700 border border-gray-600 rounded-xl text-white placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-teal-500 focus:border-transparent resize-none min-h-[48px] max-h-48 pr-12",
                        placeholder: "Type a message...",
                        value: "{message}",
                        oninput: move |e| message.set(e.value()),
                        rows: 1,
                    }
                }

                // Send button
                button {
                    class: "p-2 rounded-lg bg-teal-500 hover:bg-teal-400 transition-colors text-white disabled:opacity-50 disabled:cursor-not-allowed",
                    disabled: message.read().trim().is_empty(),
                    onclick: move |_| {
                        // Send message logic
                        message.set(String::new());
                    },
                    svg { class: "w-5 h-5", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M12 19l9 2-9-18-9 18 9-2zm0 0v-8" }
                    }
                }
            }
        }
    }
}
