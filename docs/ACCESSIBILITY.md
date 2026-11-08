# Accessibility

Target: **WCAG 2.2 AA** for the desktop and web UI. Android uses the same
Dioxus components and inherits the semantics, but screen-reader and
switch-access behaviour on Android has not been verified.

This document records what is implemented, how it is tested, and what is
still missing. Update it with any change that alters focus, naming,
keyboard behaviour or colour tokens.

## Implemented

### Focus and motion

- A single theme-aware `:focus-visible` ring is defined in
  `crates/ui/assets/tailwind.css`; individual controls may add their own ring.
- A skip link ("Skip to content") is the first focusable element in the
  signed-in shell and targets `#main-content`.
- `prefers-reduced-motion: reduce` disables animations and transitions,
  including the web loading spinner.
- `forced-colors: active` swaps the focus ring for `Highlight`.
- The web viewport no longer disables pinch zoom (`index.html`).

### Structure and names

- Landmarks: `nav[aria-label="Sections and spaces"]`, labelled `aside`s,
  and one named `main#main-content` view at a time.
- `aria-current="page"` marks the active section, space and channel;
  `aria-current="true"` marks the selected conversation.
- Icon-only buttons have `aria-label`, form fields have explicit labels, and
  decorative badges, presence dots and icons are `aria-hidden`.
- Unread counts, pinned/muted state and space lock/pending state are exposed
  through visually hidden text (`sr-only`) or the row's accessible name.
- The attachment picker uses a visually hidden but focusable file input, so
  it is reachable by keyboard.

### Dialogs and popups

`crates/ui/src/a11y.rs` provides three helpers:

| Helper | Use |
|---|---|
| `use_modal` | Mounted dialogs: focus in, focus trap, Escape, focus restore |
| `use_modal_when` | Inline `if open { … }` dialogs driven by a signal |
| `use_popup` | Non-modal popups such as context menus (focus in, Escape, restore, no Tab trap) |

A migrated dialog panel carries `role="dialog"`, `aria-modal="true"`, an
accessible name, `tabindex="-1"`, the escape handler and the focus script.
`data-autofocus="true"` picks the initial control (for example global search).

Context menus use a labelled `group` (not `menu`, which would require a
roving-tabindex menu pattern), move focus into the popup, close on Escape
and can be opened from the keyboard with **Shift+F10** or the context-menu
key.

### Live regions

- Connection banner: `role="status"` while connecting and `role="alert"`
  with `aria-live="assertive"` when failed.
- Login, upload and OMEMO errors use `role="alert"`.
- Upload progress uses `role="status"`.
- The incoming-call dialog is an `alertdialog` named with the caller.
- A polite live region announces messages that arrive in the **open**
  conversation only; opening a chat does not read its history aloud.

### Contrast

Themes are defined as CSS variables in `crates/ui/assets/tailwind.css`.
`crates/ui/src/lib.rs::contrast_tests` parses those variables and fails the
suite when any of these pairs drops below AA:

- `gray-400`/`gray-500` on `gray-950`/`gray-900`/`gray-800` (≥ 4.5:1)
- white on `teal-500`/`teal-600`/`red-600`/`red-700` (≥ 4.5:1)
- `teal-400` focus ring on the three surfaces (≥ 3:1)

Dark, silver and butter `--gray-500` were raised and the primary/danger
hover states darkened to meet these thresholds.

### Keyboard shortcuts

| Keys | Action |
|---|---|
| `Ctrl`/`Cmd` + `K` | Global search |
| `Ctrl`/`Cmd` + `,` | Settings |
| `Alt` + `1…5` | Switch section (Chats, Contacts, Rooms, Feed, Communities) |
| `F6` / `Shift`+`F6` | Focus the conversation / sidebar |
| `Escape` | Close the topmost dialog or overlay |
| `Enter` / `Shift`+`Enter` | Send / newline in the composer |
| `Shift`+`F10` | Open the context menu for a focused space or channel |

The mapping lives in `a11y::shortcut_for` and is unit-tested. The same list
is shown in Settings → Keyboard shortcuts.

## Testing

Hermetic:

```sh
cargo test -p ui --lib
```

covers contrast tokens, focus/restore script generation, shortcut mapping,
row-label wording and announcement wording.

Stylesheet: after adding classes or changing tokens, rebuild the committed
bundle:

```sh
tailwindcss -c tailwind.config.js -i crates/ui/assets/tailwind.css \
  -o crates/ui/assets/app.css --minify
```

Manual checklist (not yet automated):

1. **Keyboard only** — Tab through the shell: skip link, rail, sidebar,
   conversation, composer, send. Every control shows a focus ring and every
   action is reachable. Open and close each dialog with the keyboard.
2. **Screen reader** — Orca on Linux (WebKitGTK exposes AT-SPI), NVDA on
   Windows, VoiceOver on macOS. Check landmark list, dialog announcements,
   unread counts, incoming call and new-message announcements.
3. **Zoom and reflow** — 200 % browser zoom and the in-app font size
   setting; no clipped controls or horizontal scrolling at 320 CSS px.
4. **Reduced motion / forced colours** — enable the OS settings and confirm
   animations stop and the focus ring stays visible.
5. **Themes** — check all four themes at the AA pairs above; button hover
   labels must stay legible.

Debug builds accept `JABU_AUTO_SETTINGS=1` to open Settings on start, which
is useful for dialog/focus smoke runs.

## Known gaps

- Keyboard automation under Xvfb is unreliable in this environment: the
  WebKit window is not managed by a window manager reliably enough for
  synthetic key delivery. Keyboard behaviour is covered by unit tests and
  manual runs; an AT-SPI tree dump (`at-spi2-core` + `pyatspi`) is a
  possible future check.
- Android TalkBack, switch access and font scaling are untested.
- Point-and-tap targets smaller than 24×24 CSS px are not audited yet
  (WCAG 2.2 SC 2.5.8).
- Some hover-only controls (message hover actions, emoji reactions) are
  still mouse-first; the main row actions now appear on focus.
- All labels are English; there is no translation layer yet.
- The live region announces only the open conversation and does not queue
  or rate-limit a burst of messages.
- High contrast mode relies on the browser/OS forced-colours support; there
  is no dedicated high-contrast theme.
