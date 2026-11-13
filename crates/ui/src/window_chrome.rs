//! Desktop window-button layout.
//!
//! Jabu draws its own titlebar, so it has to honour the desktop's button
//! configuration itself. On Linux that configuration is GTK's
//! `gtk-decoration-layout` property, the same value client-side decorated GTK
//! applications follow.
//!
//! The value uses GTK's grammar: comma-separated button names, with a colon
//! separating the buttons on the logical start side from those on the end
//! side, for example `"appmenu:minimize,maximize,close"`. Tokens we do not
//! implement (`menu`, `appmenu`, `icon`, `spacer`) are ignored.

/// One titlebar button Jabu can draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowControl {
    Minimize,
    Maximize,
    Close,
}

/// Buttons requested on each logical side of the titlebar.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WindowControlLayout {
    /// Logical start side, the left side in left-to-right locales.
    pub start: Vec<WindowControl>,
    /// Logical end side, the right side in left-to-right locales.
    pub end: Vec<WindowControl>,
}

impl WindowControlLayout {
    /// The layout used when the desktop gives no usable answer.
    #[must_use]
    pub fn fallback() -> Self {
        Self {
            start: Vec::new(),
            end: vec![
                WindowControl::Minimize,
                WindowControl::Maximize,
                WindowControl::Close,
            ],
        }
    }

    /// No buttons at all.
    #[must_use]
    pub fn hidden() -> Self {
        Self::default()
    }

    /// Whether the titlebar should render no window buttons.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.start.is_empty() && self.end.is_empty()
    }

    /// Parse a GTK `gtk-decoration-layout` value.
    ///
    /// GTK itself treats a value without a colon as belonging to the start
    /// side, so this follows that rule.
    #[must_use]
    pub fn parse_gtk(layout: &str) -> Self {
        Self::parse(layout, Side::Start)
    }

    /// Parse a layout written by hand in the config file.
    ///
    /// People expect `"minimize,maximize,close"` to land on the right, so a
    /// value without a colon belongs to the end side here.
    #[must_use]
    pub fn parse_explicit(layout: &str) -> Self {
        Self::parse(layout, Side::End)
    }

    /// Resolve the configured override into a concrete layout.
    #[must_use]
    pub fn resolve(configured: &str) -> Self {
        match configured.trim() {
            "" | "auto" | "system" => Self::detected(),
            "hidden" | "none" | "off" => Self::hidden(),
            layout => Self::parse_explicit(layout),
        }
    }

    fn detected() -> Self {
        system_layout()
            .map(|layout| Self::parse_gtk(&layout))
            .unwrap_or_else(Self::fallback)
    }

    fn parse(layout: &str, side_without_colon: Side) -> Self {
        let (start_tokens, end_tokens) = match layout.split_once(':') {
            Some((start, end)) => (start, end),
            None => match side_without_colon {
                Side::Start => (layout, ""),
                Side::End => ("", layout),
            },
        };
        Self {
            start: parse_tokens(start_tokens),
            end: parse_tokens(end_tokens),
        }
    }
}

#[derive(Clone, Copy)]
enum Side {
    Start,
    End,
}

fn parse_tokens(tokens: &str) -> Vec<WindowControl> {
    let mut controls = Vec::new();
    for token in tokens.split(',') {
        let control = match token.trim() {
            "minimize" => WindowControl::Minimize,
            "maximize" => WindowControl::Maximize,
            "close" => WindowControl::Close,
            _ => continue,
        };
        // Duplicates are malformed but harmless; GTK would draw two buttons.
        // Draw one, since there is only one window to act on.
        if !controls.contains(&control) {
            controls.push(control);
        }
    }
    controls
}

/// The desktop's requested layout, when it has one.
///
/// GTK must already be initialised; this is called from the titlebar on the
/// main thread, after the window exists.
#[cfg(target_os = "linux")]
pub fn system_layout() -> Option<String> {
    use gtk::prelude::*;

    if !gtk::is_initialized_main_thread() {
        return None;
    }
    let settings = gtk::Settings::default()?;
    let layout = settings.gtk_decoration_layout()?;
    let layout = layout.to_string();
    (!layout.trim().is_empty()).then_some(layout)
}

#[cfg(not(target_os = "linux"))]
pub fn system_layout() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::{WindowControl, WindowControlLayout};
    use WindowControl::{Close, Maximize, Minimize};

    #[test]
    fn parses_the_desktop_default_with_only_close() {
        let layout = WindowControlLayout::parse_gtk("menu:close");
        assert!(layout.start.is_empty());
        assert_eq!(layout.end, vec![Close]);
    }

    #[test]
    fn parses_right_side_buttons_in_order() {
        let layout = WindowControlLayout::parse_gtk("appmenu:minimize,maximize,close");
        assert!(layout.start.is_empty());
        assert_eq!(layout.end, vec![Minimize, Maximize, Close]);
    }

    #[test]
    fn parses_left_side_buttons_in_order() {
        let layout = WindowControlLayout::parse_gtk("close,minimize,maximize:");
        assert_eq!(layout.start, vec![Close, Minimize, Maximize]);
        assert!(layout.end.is_empty());
    }

    #[test]
    fn parses_buttons_on_both_sides() {
        let layout = WindowControlLayout::parse_gtk("close:minimize,maximize");
        assert_eq!(layout.start, vec![Close]);
        assert_eq!(layout.end, vec![Minimize, Maximize]);
    }

    #[test]
    fn hides_everything_for_an_empty_layout() {
        assert!(WindowControlLayout::parse_gtk("").is_empty());
        assert!(WindowControlLayout::parse_gtk(":").is_empty());
        assert!(WindowControlLayout::parse_gtk("appmenu:").is_empty());
        assert_eq!(
            WindowControlLayout::parse_gtk(""),
            WindowControlLayout::hidden()
        );
    }

    #[test]
    fn ignores_tokens_it_cannot_draw() {
        let layout = WindowControlLayout::parse_gtk("icon,menu,spacer:close");
        assert!(layout.start.is_empty());
        assert_eq!(layout.end, vec![Close]);
    }

    #[test]
    fn ignores_duplicates_and_whitespace() {
        let layout = WindowControlLayout::parse_gtk("appmenu: close , close ,minimize ");
        assert_eq!(layout.end, vec![Close, Minimize]);
    }

    #[test]
    fn without_a_colon_gtk_places_buttons_on_the_start_side() {
        let layout = WindowControlLayout::parse_gtk("minimize,maximize,close");
        assert_eq!(layout.start, vec![Minimize, Maximize, Close]);
        assert!(layout.end.is_empty());
    }

    #[test]
    fn without_a_colon_explicit_layouts_land_on_the_end_side() {
        let layout = WindowControlLayout::parse_explicit("minimize,maximize,close");
        assert!(layout.start.is_empty());
        assert_eq!(layout.end, vec![Minimize, Maximize, Close]);
    }

    #[test]
    fn resolves_hidden_spellings() {
        for value in ["hidden", "none", "off", " hidden "] {
            assert!(WindowControlLayout::resolve(value).is_empty(), "{value}");
        }
    }

    #[test]
    fn resolves_explicit_layouts() {
        let layout = WindowControlLayout::resolve("close,minimize,maximize:");
        assert_eq!(layout.start, vec![Close, Minimize, Maximize]);
        assert!(layout.end.is_empty());
    }

    #[test]
    fn fallback_is_the_three_button_layout() {
        let layout = WindowControlLayout::fallback();
        assert!(layout.start.is_empty());
        assert_eq!(layout.end, vec![Minimize, Maximize, Close]);
    }
}
