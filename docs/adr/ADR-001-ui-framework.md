# ADR-001: UI Framework Selection

## Status

Accepted

## Context

Jabu requires a UI framework that supports cross-platform desktop, web, and mobile targets. The framework must integrate well with Rust's async ecosystem (tokio) and support signal-based reactive state management for the XMPP client UI.

## Decision

Use **Dioxus 0.6** as the UI framework.

## Rationale

- **Cross-platform**: Dioxus 0.6 supports web, desktop, and mobile from a single codebase
- **Signal-based state management**: Dioxus 0.6's `Signal<T>` API aligns with reactive UI patterns needed for real-time XMPP chat
- **Rust ecosystem**: Native Rust implementation with good tokio integration
- **Community**: Active development and growing ecosystem
- **Hot reload**: DX serve provides hot reload for rapid UI iteration

## Consequences

- UI code uses Dioxus 0.6 signals API (`Readable`/`Writable` traits, `Signal::set()`)
- Platform-specific code is isolated behind `#[cfg]` attributes
- WebAssembly target required for web deployment (`wasm32-unknown-unknown`)

## Alternatives Considered

- **egui**: Good for desktop but limited web support
- **Iced**: Good cross-platform but less mature ecosystem
- **Tauri + web tech**: Would require HTML/CSS/JS, not pure Rust
- **Prose-ui**: Tied to prose-xmpp, which we decided against
