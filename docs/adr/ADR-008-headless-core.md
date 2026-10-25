# ADR-008: Headless Protocol Core and App Service Boundary

## Status

Accepted as the target. The headless-core part is implemented; the
`AppService`/`AccountManager` boundary is **not** enforced yet. The UI still
calls `XmppClient` directly and owns the connection lifecycle. See
`docs/STATUS.md` §6.

## Context

`xmpp-core` previously depended on Dioxus signals for change notification, and
`XmppClient` construction happened before the Dioxus runtime existed. This caused
initialization hazards, made headless tests difficult and coupled protocol state to the UI.

## Decision

Keep `xmpp-core` free of Dioxus. Protocol state uses `Arc<RwLock<T>>`, atomics and the
existing event bus. The presentation layer receives updates through an app-level
`ProcessorNotifier` and service/event boundary. UI components never call protocol methods
directly; they send commands and render events.

## Rationale

- Enables headless integration tests and future gateway/admin binaries.
- Removes startup ordering constraints between protocol state and the Dioxus runtime.
- Keeps platform and UI concerns out of protocol code.

## Consequences

- `ClientState` lives in the UI crate.
- `EventProcessor` takes a notifier implemented by the app.
- New features must expose state changes through events or stores, not through signals in core.
