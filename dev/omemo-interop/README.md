# OMEMO interop harness

This runs a modern OMEMO peer (`slixmpp` + `slixmpp-omemo`/python-omemo) against
the Rust client over the local Prosody fixture.

## Setup

```bash
python3 -m venv /tmp/omemo-venv
/tmp/omemo-venv/bin/pip install -r dev/omemo-interop/requirements.txt
./prosody/run.sh
```

## Run

```bash
JABU_TEST_XMPP=1 \
JABU_TEST_OMEMO_PYTHON=/tmp/omemo-venv/bin/python \
cargo test -p xmpp-core --test omemo_interop -- --ignored --nocapture
```

The suite contains four ignored tests:

- `interop_with_slixmpp_omemo` publishes a device list and bundle, sends an
  encrypted 1:1 `ping`, and waits for an encrypted `pong`.
- `interop_muc_with_slixmpp_omemo` creates a non-anonymous room, waits for the
  peer's real JID, sends a group-encrypted `ping`, and waits for a group
  `pong`.
- `interop_legacy_with_slixmpp_omemo` starts the peer with `--legacy-only`,
  publishes an empty OMEMO 2 device list plus a legacy OMEMO 0.3 bundle, and
  verifies that `send_omemo_message` falls back to
  `eu.siacs.conversations.axolotl` and decrypts the reply.
- `interop_muc_legacy_with_slixmpp_omemo` does the same for a room, exercising
  outbound legacy group encryption.

## Deterministic fixture

`gen_twomemo_fixture.py` produces
`crates/xmpp-core/tests/fixtures/omemo_twomemo_two_way.json` with fixed keys and
a python-twomemo conversation (Alice KEX, Bob reply, empty key-transport
message). The hermetic Rust test `decrypts_python_twomemo_two_way` replays it, so
associated-data orientation, the KEX format and empty messages are covered
without Prosody or Python. Regenerate with the interop virtualenv:

```bash
/tmp/omemo-venv/bin/python dev/omemo-interop/gen_twomemo_fixture.py
```

## Current result (2026-11-07)

All four ignored tests pass. The Rust client publishes OMEMO 2 and legacy
OMEMO 0.3 bundles, performs X3DH and double-ratchet sessions with the Python
peer, and decrypts in both directions. It falls back to legacy when the peer
publishes no OMEMO 2 devices, and handles 1:1 and MUC messages in both stacks:

```text
LEGACY_ONLY
DECRYPTED alice@localhost ping from rust
SENT_REPLY
test result: ok. 4 passed
```

Notes:

- `slixmpp-omemo` 2.2.0 decrypts the OMEMO 2 ratchet but does not implement the
  XEP-0420 SCE envelope (`NotImplementedError: SCE not supported yet`). The
  harness therefore decrypts at the python-omemo session-manager level and
  unwraps the envelope itself. For MUC it resolves the sender's real JID before
  parsing the message.
- OMEMO 2 keeps the X3DH associated data initiator-first in both directions.
  `TwomemoSession` in `omemo-rs` swaps the AD halves for some directions, which
  desynchronises against `python-twomemo`; the Rust client drives
  `omemo-doubleratchet` directly to keep the AD verbatim.
- Empty OMEMO messages (key transport only, no `<payload>`) must still be
  decrypted so the ratchet advances. They are not added to the chat log.
- MUC group keys include the sender's own device; the room send path now skips
  the self occupant entry before adding its local bundle, so a room with one
  remote peer still counts as a real recipient.
- Legacy OMEMO 0.3 uses AES-128-GCM for the body and flat per-device `<key>`
  elements. Its bundle signs `0x05 || spk` and stuffs the Ed25519 sign bit into
  the signature. The client publishes both stacks on separate PEP nodes and
  prefers OMEMO 2 when both are available.
- MUC sends encrypt for OMEMO 2 or legacy depending on the keys present. The
  room UI warns about present members without keys, and a heartbeat button
  sends a key-only message to establish sessions without posting a visible
  message.
- The Python peer republishes a clean single-device list for both namespaces on
  startup so repeated runs against the persistent Prosody fixture do not
  accumulate stale devices.
- The Python peer is killed by a drop guard even when the test panics, so a
  failed run does not leave a stale connection behind. The four tests share one
  `bob` account and are serialized.
