# ADR-019: Internationalization with embedded Fluent catalogues

## Status

Accepted, implemented for the desktop UI, notifications and Host pages.

## Context

Jabu shipped English-only strings in Dioxus components, notification workers
and server-rendered Host pages. Translating in place would scatter wording
across crates and make plural rules, fallback and right-to-left direction
inconsistent. Movim's catalogues cannot be reused because Movim is AGPL and
Jabu is MIT.

The desktop, web and Android builds must work without runtime translation
files, and the Host is a server binary that must answer each visitor in a
negotiated language rather than one process-wide language.

## Decision

Use Project Fluent with catalogues embedded at compile time:

1. `crates/i18n` owns the catalogues (`locales/<tag>/common.ftl`), locale
   negotiation through `unic-langid` and `sys-locale`, and a per-thread parsed
   bundle cache. English is the source catalogue; French is the first complete
   translation.
2. `Translator` is a small `Copy` value holding the resolved shipped tag. It
   exposes `t`, `t_args`, `tn` and localized `date`, `clock`, `date_time`,
   `number` and `decimal` helpers.
3. Keys are lowercase hyphenated words (`chat-send-placeholder`); Fluent does
   not accept dots. Messages contain full sentences with `$placeholders`, so
   translators control word order. Protocol identifiers, JIDs, logs and user
   content are never translated.
4. The desktop stores one global `ui.language` preference (`system` or a
   BCP-47 tag). The resolved tag lives in `ClientState.language`;
   `locale::use_i18n()` subscribes components and applies `<html lang>`/`dir`.
5. Notifications resolve the active language from the same state.
6. Host pages negotiate `Accept-Language` per request, emit matching
   `lang`/`dir` attributes and set `Vary: accept-language`. The public and
   admin surfaces share the same translator; form validation returns typed
   errors so messages are localized at the boundary.
7. Catalogues are original MIT contributions. Movim wording, templates and
   assets are not copied.

## Consequences

- Desktop, web and Android need no translation files at runtime.
- Adding a locale means adding a catalogue, an `include_str!` arm and a
  `SUPPORTED` entry; `cargo run -p jabu-i18n --bin i18n-check` reports missing
  or extra keys, and tests enforce key parity and UI key usage.
- Plural categories, date formats and number separators follow the catalogue
  and the small locale table rather than English assumptions.
- An RTL translation still needs a layout audit: direction is wired, but a few
  physical spacing utilities remain and no RTL catalogue ships yet.
- Android's launcher label comes from the generated Gradle project; the app
  cannot localize it through this mechanism, and notification channels are not
  implemented yet.

## Alternatives considered

- **gettext/`.po` files**: mature tooling, but plural and gender selection is
  less expressive than Fluent and the runtime lookup model is a poorer fit for
  Dioxus re-rendering.
- **JSON string maps**: simple, but no plural categories, no translator-facing
  syntax and no compile-time inclusion story across targets.
- **Reusing Movim's catalogues**: rejected on licensing and clean-room grounds.
