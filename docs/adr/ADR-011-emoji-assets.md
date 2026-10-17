# ADR-011: Emoji assets and custom packs

## Status

Accepted

## Context

Movim renders Unicode emoji as SVG images, ships custom emoji and sticker packs,
and lets users favorite custom emoji with aliases. The Rust client must provide
the same user-facing surface without copying Movim's AGPL source or assets.

## Decision

- Bundle **Noto Emoji SVG** artwork, licensed under the SIL Open Font License
  1.1, with its license file and attribution.
- Package the SVG set as a single compressed asset and resolve images on demand
  through an in-process cache. This avoids thousands of separate asset files in
  the application bundle.
- Fall back to the Unicode glyph when an SVG asset is unavailable.
- Treat custom emoji and sticker packs as user-imported local data. Support
  Movim-compatible `meta.json` v2 emoji archives where practical.
- Do not copy Movim's AGPL emoji or sticker artwork.

## Consequences

- The application bundle grows by the compressed Noto Emoji archive.
- Emoji rendering is consistent across platforms and does not depend on system
  emoji fonts.
- Custom packs need a local import and cache-management surface.
- Sticker interoperability with Movim requires XEP-0231 Bits of Binary.
