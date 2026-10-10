# ADR-003: Concurrency Model

## Status

Accepted

## Context

Dioxus `Signal<T>` is not `Send` (uses `RefCell` internally), so it cannot be shared across threads. XMPP connection and event processing run on background tokio threads and need to update UI state.

## Decision

Use `Arc<RwLock<T>>` for cross-thread state sharing instead of Dioxus `Signal`.

## Rationale

- `Arc<T>` is `Send` + `Sync`, allowing cross-thread access
- `parking_lot::RwLock<T>` for synchronous contexts (low overhead)
- `tokio::sync::RwLock<T>` for async contexts (non-blocking)
- Dioxus signals are only used on the main/UI thread
- State is extracted from `Arc<RwLock<T>>` before being placed into Dioxus signals

## Consequences

- `Signal<T>` is never `Send`; cross-thread state uses `Arc<RwLock<T>>`
- UI components read from `Arc<RwLock<T>>` and wrap values in `Signal<T>` for Dioxus
- `Connection::set_config()` and `connect()` take `&self` (interior mutability via `RwLock`)

## Alternatives Considered

- **tokio::sync::Mutex<T>**: Async mutex, but unnecessary overhead for sync contexts
- **std::sync::Mutex<T>**: Blocking, less performant than parking_lot::RwLock
- **Arc<tokio::sync::RwLock<T>>**: Mixed sync/async, not needed
