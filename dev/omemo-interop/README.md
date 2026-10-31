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

The test starts `bob.py` as `bob@localhost`, lets the Rust client publish its
device list and bundle, sends an encrypted `ping`, and waits for an encrypted
`pong`.

## Current result (2026-11-07)

Passing. The Rust client publishes an OMEMO 2 bundle and device list, performs
an X3DH key exchange and double-ratchet session with the Python peer, and both
directions decrypt:

```text
DECRYPTED alice@localhost ping from rust
SENT_REPLY
test result: ok
```

Notes:

- `slixmpp-omemo` 2.2.0 decrypts the OMEMO 2 ratchet but does not implement the
  XEP-0420 SCE envelope (`NotImplementedError: SCE not supported yet`). The
  harness therefore decrypts at the python-omemo session-manager level and
  unwraps the envelope itself.
- OMEMO 2 keeps the X3DH associated data initiator-first in both directions.
  `TwomemoSession` in `omemo-rs` swaps the AD halves for some directions, which
  desynchronises against `python-twomemo`; the Rust client drives
  `omemo-doubleratchet` directly to keep the AD verbatim.
- Empty OMEMO messages (key transport only, no `<payload>`) must still be
  decrypted so the ratchet advances. They are not added to the chat log.
- The Python peer is killed by a drop guard even when the test panics, so a
  failed run does not leave a stale connection behind.
