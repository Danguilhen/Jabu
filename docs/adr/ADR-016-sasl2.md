# ADR-016: XEP-0388 SASL2 negotiation

## Status

Proposed. Spike and design only; SASL1 remains the active login path.

## Context

Login currently calls `XMPPStream::start`, which performs SASL1 negotiation via
tokio-xmpp. XEP-0388 SASL2 replaces the SASL1 stream feature with a dedicated
`<authenticate/>` exchange and can carry stream-management, resource-binding
and inline feature negotiation. The tokio-xmpp parser stack does not expose
SASL2 elements, so this cannot be enabled by a flag on the existing call.

SASL2 is also not a drop-in replacement for every server. The client must keep
the SASL1 path working for servers that only offer SASL1 and for accounts whose
server advertises SASL2 but rejects the requested mechanism.

## Decision

Adopt a capability-gated, staged implementation:

1. Read the stream features before calling `XMPPStream::start`. If they contain
   `urn:xmpp:sasl:2`, offer SASL2; otherwise use the existing SASL1 path.
2. Implement the SASL2 stream exchange in `connection/login.rs` on the raw
   `ClientStream`, reusing the existing `sasl` crate mechanisms (PLAIN and
   SCRAM-SHA-256 first, then SCRAM-SHA-1).
3. Parse `<mechanisms/>`, send `<authenticate xmlns='urn:xmpp:sasl:2'/>`,
   handle `<challenge/>`/`<response/>` and `<success/>`, and treat
   `<failure/>` as a fallback trigger only when the mechanism, not the
   credentials, was rejected.
4. Request inline `<bind/>` and, when the server advertises it, inline stream
   management. If inline binding is not offered, continue with the existing
   bind step after SASL2 success.
5. Keep the SASL1 fallback on every SASL2 error that clearly means "not
   supported" and surface authentication failures instead of retrying with a
   different mechanism silently.
6. Do not implement XEP-0388's task/channel-binding extensions until SASL2
   login is stable against Prosody.

## Consequences

- Login gains a second negotiation path and must be tested against both a
  SASL1-only and a SASL2-capable server.
- The client must not send initial presence or bind before the negotiated
  stream state is complete.
- SASL2 inline features can reduce round trips, but the fallback path keeps
  the existing behaviour and test coverage relevant.
- The `sasl` crate remains the mechanism implementation; no new cryptographic
  code is introduced for this step.
- Prosody currently exercises SASL1 in the live suite. A SASL2 fixture or a
  second server configuration is required before this can be marked working.

## Open questions

- Whether tokio-xmpp can be kept for the post-authentication stream or whether
  the SASL2 path needs a small internal stream wrapper.
- Which inline features are safe to accept before the first bind on Android.
- Whether to advertise SASL2 support in service discovery at all; SASL2 is a
  stream feature, not a disco feature.
