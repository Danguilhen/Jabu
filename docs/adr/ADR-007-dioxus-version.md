# ADR-007: Dioxus Version Strategy

## Status

Accepted — migrated to Dioxus 0.7.10

## Context

The project used Dioxus 0.6. Dioxus 0.6 is no longer maintained, while Android
and desktop are the priority platforms. Upgrading must not mix incompatible Dioxus crate
versions across the workspace.

## Decision

The workspace was migrated to Dioxus 0.7.10. Desktop builds and runs. An
Android build job exists in CI, but Android runtime, media and platform
integration remain unvalidated; this is not a Dioxus incompatibility. If a later
Android blocker appears, record it here instead of mixing versions.

## Rationale

- Dioxus 0.6 is unmaintained and mobile is a priority.
- Android WebView JavaScript interop will be required for WebRTC and platform bridges.
- A single Dioxus version avoids duplicate runtime and asset systems.

## Consequences

- All Dioxus crates in the workspace must use one version.
- The workspace uses Dioxus 0.7.10 across all crates.
- Android build validation is part of the platform build-infrastructure work.
- `xmpp-core` has no Dioxus dependency after ADR-008, so the upgrade is limited to UI/app/platform.

## Alternatives considered

- Stay on 0.6.3 permanently: rejected; the workspace now uses 0.7.10.
- Mix 0.6 and 0.7 crates: rejected; runtime and component types are incompatible.
