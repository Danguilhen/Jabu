//! Accessibility helpers shared by dialogs and the application shell.
//!
//! Dioxus renders through a webview, so the parts that need the browser's
//! focus model (initial focus, Tab containment and focus restoration) are
//! implemented with a small script. Everything that can be expressed as an
//! attribute stays in RSX so it works on every platform.

use dioxus::document;
use dioxus::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_MODAL: AtomicU64 = AtomicU64::new(1);

/// Handle returned by [`use_modal`].
///
/// Apply [`ModalHandle::id`] to the dialog panel and set `onkeydown` to
/// [`ModalHandle::escape`]. The panel also needs `role="dialog"`,
/// `aria-modal="true"`, an accessible name and `tabindex="-1"`.
#[derive(Clone, PartialEq)]
pub struct ModalHandle {
    pub id: String,
    escape: EventHandler<KeyboardEvent>,
}

impl ModalHandle {
    /// Closes the dialog when Escape is pressed inside it.
    #[must_use]
    pub fn escape(&self) -> EventHandler<KeyboardEvent> {
        self.escape
    }
}

/// Sets up modal behaviour for a dialog.
///
/// Moves focus into the dialog (preferring a `data-autofocus` element),
/// wraps Tab and Shift+Tab inside it, and restores the previously focused
/// element when the dialog is unmounted.
#[must_use]
pub fn use_modal(mut onclose: impl FnMut() + 'static) -> ModalHandle {
    let id = use_hook(|| format!("jabu-modal-{}", NEXT_MODAL.fetch_add(1, Ordering::Relaxed)));
    let escape = use_callback(move |event: KeyboardEvent| {
        if event.key() == Key::Escape {
            event.prevent_default();
            event.stop_propagation();
            onclose();
        }
    });

    use_effect({
        let id = id.clone();
        move || {
            #[cfg(debug_assertions)]
            tracing::info!("dialog {id} opened");
            let _ = document::eval(&focus_trap_script(&id));
        }
    });

    use_drop({
        let id = id.clone();
        move || {
            #[cfg(debug_assertions)]
            tracing::info!("dialog {id} closed");
            let _ = document::eval(&restore_focus_script(&id));
        }
    });

    ModalHandle { id, escape }
}

const FOCUSABLE: &str = "a[href],button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex=\"-1\"])";

/// Handle returned by [`use_popup`].
#[derive(Clone, PartialEq)]
pub struct PopupHandle {
    pub id: String,
    escape: EventHandler<KeyboardEvent>,
}

impl PopupHandle {
    /// Closes the popup when Escape is pressed inside it.
    #[must_use]
    pub fn escape(&self) -> EventHandler<KeyboardEvent> {
        self.escape
    }
}

/// Sets up a non-modal popup such as a context menu.
///
/// Moves focus to the first `data-autofocus` element (or the popup itself),
/// closes on Escape and restores focus when it unmounts. Unlike [`use_modal`]
/// it does not trap Tab, so users can leave the menu normally.
#[must_use]
pub fn use_popup(mut onclose: impl FnMut() + 'static) -> PopupHandle {
    let id = use_hook(|| format!("jabu-popup-{}", NEXT_MODAL.fetch_add(1, Ordering::Relaxed)));
    let escape = use_callback(move |event: KeyboardEvent| {
        if event.key() == Key::Escape {
            event.prevent_default();
            event.stop_propagation();
            onclose();
        }
    });

    use_effect({
        let id = id.clone();
        move || {
            let _ = document::eval(&popup_focus_script(&id));
        }
    });

    use_drop({
        let id = id.clone();
        move || {
            let _ = document::eval(&restore_focus_script(&id));
        }
    });

    PopupHandle { id, escape }
}

/// Modal behaviour for a dialog controlled by an `open` signal.
///
/// Use this instead of [`use_modal`] when the dialog is rendered from an
/// inline `if open { ... }` block: focus moves in when `open` becomes true,
/// and focus is restored when it becomes false again.
#[must_use]
pub fn use_modal_when(open: Signal<bool>, mut onclose: impl FnMut() + 'static) -> ModalHandle {
    let id = use_hook(|| format!("jabu-modal-{}", NEXT_MODAL.fetch_add(1, Ordering::Relaxed)));
    let escape = use_callback(move |event: KeyboardEvent| {
        if event.key() == Key::Escape {
            event.prevent_default();
            event.stop_propagation();
            onclose();
        }
    });

    use_effect({
        let id = id.clone();
        move || {
            if *open.read() {
                #[cfg(debug_assertions)]
                tracing::info!("dialog {id} opened");
                let _ = document::eval(&focus_trap_script(&id));
            } else {
                let _ = document::eval(&restore_focus_script(&id));
            }
        }
    });

    use_drop({
        let id = id.clone();
        move || {
            let _ = document::eval(&restore_focus_script(&id));
        }
    });

    ModalHandle { id, escape }
}

fn popup_focus_script(id: &str) -> String {
    format!(
        r#"
(() => {{
  const el = document.getElementById('{id}');
  if (!el) return;
  window.__jabuModalPrev = window.__jabuModalPrev || {{}};
  window.__jabuModalPrev['{id}'] = document.activeElement;
  (el.querySelector('[data-autofocus]') || el).focus();
}})();
"#
    )
}
fn focus_trap_script(id: &str) -> String {
    format!(
        r#"
(() => {{
  const el = document.getElementById('{id}');
  if (!el) return;
  window.__jabuModalPrev = window.__jabuModalPrev || {{}};
  window.__jabuModalPrev['{id}'] = document.activeElement;
  const selector = '{FOCUSABLE}';
  const focusable = () => Array.from(el.querySelectorAll(selector))
    .filter((node) => node.getClientRects().length > 0);
  const preferred = el.querySelector('[data-autofocus]');
  (preferred || focusable()[0] || el).focus();
  el.addEventListener('keydown', (event) => {{
    if (event.key !== 'Tab') return;
    const nodes = focusable();
    if (!nodes.length) {{ event.preventDefault(); el.focus(); return; }}
    const first = nodes[0];
    const last = nodes[nodes.length - 1];
    if (event.shiftKey && document.activeElement === first) {{
      event.preventDefault();
      last.focus();
    }} else if (!event.shiftKey && document.activeElement === last) {{
      event.preventDefault();
      first.focus();
    }}
  }});
}})();
"#
    )
}

fn restore_focus_script(id: &str) -> String {
    format!(
        r#"
(() => {{
  const map = window.__jabuModalPrev;
  if (!map) return;
  const previous = map['{id}'];
  delete map['{id}'];
  if (previous && typeof previous.focus === 'function') previous.focus();
}})();
"#
    )
}

/// A global keyboard shortcut recognised by the application shell.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shortcut {
    /// Close the topmost dialog or overlay.
    CloseOverlay,
    /// Open global search.
    Search,
    /// Open settings.
    Settings,
    /// Switch to a top-level section.
    Section(crate::AppSection),
    /// Focus the conversation pane.
    FocusConversation,
    /// Focus the sidebar or section rail.
    FocusSidebar,
}

/// Maps a key press to the shortcut it triggers.
///
/// `command` is Control on Linux/Windows and Command on macOS. Kept pure so
/// the bindings documented in Settings can be tested without a webview.
#[must_use]
pub fn shortcut_for(key: &Key, command: bool, alt: bool, shift: bool) -> Option<Shortcut> {
    match key {
        Key::Escape => Some(Shortcut::CloseOverlay),
        Key::F6 if shift => Some(Shortcut::FocusSidebar),
        Key::F6 => Some(Shortcut::FocusConversation),
        Key::Character(value) if command && value.eq_ignore_ascii_case("k") => {
            Some(Shortcut::Search)
        }
        Key::Character(value) if command && value == "," => Some(Shortcut::Settings),
        Key::Character(value) if alt => match value.as_str() {
            "1" => Some(Shortcut::Section(crate::AppSection::Home)),
            "2" => Some(Shortcut::Section(crate::AppSection::Contacts)),
            "3" => Some(Shortcut::Section(crate::AppSection::Rooms)),
            "4" => Some(Shortcut::Section(crate::AppSection::Feed)),
            "5" => Some(Shortcut::Section(crate::AppSection::Communities)),
            _ => None,
        },
        _ => None,
    }
}

/// Wording for the polite live region that announces an incoming message.
#[must_use]
pub fn announcement_text(sender: &str, body: &str) -> String {
    format!("New message from {sender}: {body}")
}

/// Builds the accessible name for a list row from its visible state.
///
/// Kept pure so every row (conversations, rooms, spaces) describes itself
/// consistently and the wording can be unit-tested.
#[must_use]
pub fn row_label(name: &str, unread: usize, muted: bool, pinned: bool) -> String {
    let mut label = name.to_string();
    if unread > 0 {
        label.push_str(&format!(", {unread} unread"));
    }
    if pinned {
        label.push_str(", pinned");
    }
    if muted {
        label.push_str(", muted");
    }
    label
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_scripts_target_the_modal_id() {
        let focus = focus_trap_script("jabu-modal-7");
        assert!(focus.contains("getElementById('jabu-modal-7')"));
        assert!(focus.contains("data-autofocus"));
        assert!(focus.contains("addEventListener('keydown'"));

        let restore = restore_focus_script("jabu-modal-7");
        assert!(restore.contains("map['jabu-modal-7']"));
        assert!(restore.contains("previous.focus()"));

        let popup = popup_focus_script("jabu-popup-3");
        assert!(popup.contains("getElementById('jabu-popup-3')"));
        assert!(popup.contains("data-autofocus"));
    }

    #[test]
    fn shortcuts_map_to_actions() {
        let key = |value: &str| Key::Character(value.to_string());
        assert_eq!(
            shortcut_for(&key("k"), true, false, false),
            Some(Shortcut::Search)
        );
        assert_eq!(
            shortcut_for(&key("K"), true, false, false),
            Some(Shortcut::Search)
        );
        assert_eq!(
            shortcut_for(&key(","), true, false, false),
            Some(Shortcut::Settings)
        );
        assert_eq!(
            shortcut_for(&key("3"), false, true, false),
            Some(Shortcut::Section(crate::AppSection::Rooms))
        );
        assert_eq!(
            shortcut_for(&Key::F6, false, false, true),
            Some(Shortcut::FocusSidebar)
        );
        assert_eq!(
            shortcut_for(&Key::F6, false, false, false),
            Some(Shortcut::FocusConversation)
        );
        assert_eq!(
            shortcut_for(&Key::Escape, false, false, false),
            Some(Shortcut::CloseOverlay)
        );
        assert_eq!(shortcut_for(&key("k"), false, false, false), None);
        assert_eq!(shortcut_for(&key("9"), false, true, false), None);
    }

    #[test]
    fn announcements_name_the_sender() {
        assert_eq!(
            announcement_text("Alice", "hello"),
            "New message from Alice: hello"
        );
    }

    #[test]
    fn row_labels_include_state() {
        assert_eq!(row_label("Alice", 0, false, false), "Alice");
        assert_eq!(row_label("Alice", 3, false, false), "Alice, 3 unread");
        assert_eq!(
            row_label("Alice", 3, true, true),
            "Alice, 3 unread, pinned, muted"
        );
    }
}
