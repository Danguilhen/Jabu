# ADR-018: Target Protocol Coverage

## Status

Accepted as the target set. Per-XEP implementation status is maintained in
`docs/STATUS.md` §4; this record defines scope, not progress.

## Context

Parity needs a defined protocol surface. Without one, features are added
opportunistically and interoperability gaps are discovered late. The target
must also stay separate from the implementation matrix so that completing an
XEP does not rewrite the scope record.

## Decision

Implement and interoperate with the following XEPs, grouped by area. Scope
changes edit this list in place.

### Core

RFC 6120/6121/6122/7590; XEP-0030, 0115, 0198, 0199, 0203, 0215, 0352, 0368,
0388, 0390.

### Identity, presence and contacts

XEP-0012, 0045 (full), 0048, 0050, 0054, 0077, 0080, 0084, 0100, 0107, 0108,
0118, 0153, 0157, 0172, 0191, 0292, 0319, 0392, 0402.

### Messaging and rooms

XEP-0004, 0059, 0085, 0184, 0201, 0203, 0231, 0249, 0280, 0297, 0308, 0313,
0333, 0334, 0359, 0363, 0372, 0380, 0393, 0410, 0424, 0425, 0428, 0433, 0444,
0461, 0469.

### PubSub, social, Spaces, Stories

XEP-0060, 0163, 0223, 0277, 0330, 0402, 0462, 0465, 0472, 0496, 0497, 0499,
0501, 0503.

### Encryption

XEP-0384 (1:1 and MUC), 0380, 0300.

### Calls

XEP-0166, 0167, 0176, 0272, 0320, 0338, 0339, 0353, 0215, 0507; Galener
interoperability and the ProtoXEP conference dialect.

### Push and platform

XEP-0357, 0198 resumption for mobile, 0352 CSI.

## Rationale

- Gives every feature a protocol anchor and a test target.
- Keeps XEP completion notes out of the scope record.
- Makes deliberate omissions (for example Movim-local story view counts)
  visible instead of accidental.

## Consequences

- New features must name the XEP they implement or extend.
- Experimental XEPs stay capability-gated.
- A capability is only claimed in `docs/STATUS.md` when it is implemented and
  exercised against a peer.

## Alternatives considered

- **Ad-hoc coverage**: rejected; it produced late interop gaps.
- **Copy Movim's XEP list verbatim**: rejected; Movim's list includes
  server-side and legacy behaviour that is out of scope here.
