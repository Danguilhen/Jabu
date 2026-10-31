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

The peers complete device-list and bundle exchange, and the SCE payload is
accepted. The test still fails during key exchange:

```text
DECRYPT_ERROR ValueError()
twomemo.twomemo.OMEMOKeyExchange ... Wire format was corrupt
```

Cause: `dziber-omemo` emits libsignal-v3 `PreKeySignalMessage`/`SignalMessage`
bytes (8-byte MAC, libsignal KDF labels), while OMEMO 2 (`urn:xmpp:omemo:2`)
uses the Doubleratchet/OMEMO `OMEMOKeyExchange` and
`OMEMOAuthenticatedMessage` protobufs with a 16-byte truncated MAC and the
`"OMEMO Message Key Material"` HKDF label. The SCE/payload layer is modern, but
the ratchet wire format is not interoperable.

Fixing this requires replacing or adapting the ratchet layer (for example the
`omemo-rs` OMEMO 2 stack), not just changing the stanza XML.
