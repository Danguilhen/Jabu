# ADR-005: E2EE Scope

## Status

Superseded — OMEMO 1:1 is implemented; MUC OMEMO remains in progress.

## Context

End-to-end encryption (E2EE) via OMEMO is a feature in the reference Movim implementation but adds significant complexity to an MVP client.

## Decision

**No E2EE in the original MVP**. This decision has been superseded: OMEMO 1:1
device management, sessions, trust, persistence and encryption are now implemented.
MUC OMEMO is part of the current parity plan.

## Rationale

- **MVP focus**: Core XMPP functionality (chat, roster, MUC) takes priority over encryption
- **Complexity**: OMEMO key management, device trust, and session handling add substantial code
- **Clean-room implementation**: Avoids dependency on external E2EE crates with uncertain maintenance
- **Future consideration**: E2EE can be added post-MVP as a separate effort

## Consequences

These were the consequences of the original decision and are **no longer true**:

- ~~`omemo` module removed from `xmpp-core` and `storage`~~
- ~~No references to omemo in events, roster, chat, MUC, pubsub, processor, or storage~~
- ~~`StorageBackend` trait has no omemo methods~~

Current state: `xmpp-core/src/omemo.rs` implements XEP-0384 for 1:1 chats
(device lists, bundles, trust, sessions and persistence), `storage` keeps the
OMEMO state, and MUC OMEMO is still pending. See `docs/STATUS.md`.

## Alternatives Considered

- **OMEMO via omemo-rust**: Deferred to post-MVP
- **Double Ratchet via signal-protocol**: Deferred to post-MVP
- **No E2EE ever**: Not decided; E2EE may be added in future versions
