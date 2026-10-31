# Movim/Galener SFU testbed

Local reference environment for the XMPP SFU conference work. It runs a real
Movim client, Galener and Galène against the repo's Prosody fixture.

## Ports

| Service | Port |
|---|---|
| Prosody c2s / external components | 5222 / 5347 |
| Movim web | 8090 |
| Movim daemon | 8081 |
| PostgreSQL | 55432 |
| Galène (internal, launched by Galener) | 18444 |

## Setup

```bash
nix-shell dev/reference-testbed/shell.nix --run ./dev/reference-testbed/setup.sh
```

The first run clones Movim into `dev/reference-testbed/movim/`, installs Composer
dependencies and migrates the database. The clone, database, caches and logs are
gitignored.

## Run

```bash
nix-shell dev/reference-testbed/shell.nix --run ./dev/reference-testbed/run.sh
```

Then open <http://127.0.0.1:8090/> and log in once as:

- JID: `jabu@localhost`
- Password: `jabu-test`

Movim logs in over the daemon WebSocket, so this first login has to happen in a
browser. Afterwards create or join `voice@conference.localhost`, open the room
configuration and invite the SFU component (`sfu.example.org`). Galener joins the
room and creates the Galène group.

## Testing

- Room: `voice@conference.localhost`
- SFU component: `sfu.example.org`
- Galener secret: `jabu-galener-test`
- Galène groups and data: `movim/cache/galener/`

Capture the Galener presence (COIN), disco and Jingle stanzas from the daemon
log; those become the golden fixtures for `xmpp-core` and `media`.

## Stop

```bash
./dev/reference-testbed/stop.sh
```

Prosody is left running because the main development shell uses the same
instance.

## Inviting the SFU without the Movim UI

Once Galener is running and authenticated to Prosody, a room owner can invite
the SFU component directly:

```bash
python3 dev/reference-testbed/invite-sfu.py
```

It joins `voice@conference.localhost` as `alice`, creates an instant room and
sends a mediated invite to `sfu.example.org`. The SFU joins as an occupant
whose real JID is `<hash>@sfu.example.org`; use that JID as the conference
target when implementing the Jingle SFU client.

Override the defaults with `ROOM`, `SFU`, `XMPP_USER`, `XMPP_PASS`,
`XMPP_HOST` and `XMPP_PORT`.

## Notes on the Movim checkout

`setup.sh` applies one testbed-only patch to the cloned Movim source:
`SessionsWorker` normally starts PHP with `-n` and guesses extension `.so`
names, which does not work with Nix's `php84.buildEnv`. The patch lets the
worker inherit the build environment's `php.ini`. The clone is gitignored.

The browser login is still required for the Movim web client: it logs in over
the daemon WebSocket. Nginx proxies `/ws/` to the daemon on port 8081.
The browser session must stay open for the duration of an interop run; closing
the tab disconnects Movim from Prosody.

## OMEMO interop notes

Movim publishes only the legacy `eu.siacs.conversations.axolotl` device list
(no OMEMO 2), so the Rust client falls back to legacy OMEMO 0.3. Enable it under
`/configuration` (`#omemoenabled`) before fetching bundles; until then the PEP
nodes return `item-not-found`.

Two client-side problems currently stop an end-to-end exchange with Movim, and
neither is on the Rust side:

- Movim's legacy OMEMO code throws
  `TypeError: Cannot read properties of undefined (reading 'toString')` from
  `ChatOmemoStorage.isTrustedIdentity` inside `SessionBuilder` as soon as the
  contact's device list is processed, so it never builds a session and never
  sends a reply.
- After a testbed restart the browser login can stay on `/login` while
  Prosody logs `Invalid opening stream header ({jabber:client}presence)`;
  restarting the daemon and logging in again in a fresh browser context
  recovers it.

`sender` and `receiver` paths for legacy OMEMO are covered instead by the
hermetic vectors and the live slixmpp-omemo test in
`crates/xmpp-core/tests/omemo_interop.rs`.

## Capturing the Galener dialect

`capture-stanzas.py` runs the Movim/Galener call flow as a normal XMPP client:
join a room, make the SFU admin, invite it, send `propose` and
`session-initiate`, then start a second participant and terminate. It prints
every stanza and writes the raw transcript to `capture.galener.xml`.

```bash
python3 dev/reference-testbed/capture-stanzas.py
```

`extract-fixtures.py` turns the transcript into the named fixtures under
`crates/xmpp-core/tests/golden/galener/`. Galene 1.0 negotiates new publisher
tracks with `renegotiate`, which this Galener does not implement, so
`content-add`/`content-remove`/`session-info` are generated with the same Movim
classes Galener uses:

```bash
cd dev/reference-testbed/movim
nix-shell ../shell.nix --run 'php ../generate-fixtures.php \
  ../../../crates/xmpp-core/tests/golden/galener'
```

Fixtures produced:

| File | Direction | Meaning |
|---|---|---|
| `disco_info.xml` | SFU -> client | `urn:xmpp:coin:1`, Jingle audio/video features |
| `coin_presence.xml` | SFU -> room | COIN `conference-info` with participants |
| `proceed.xml` | SFU -> client | Jingle Message Initiation proceed |
| `session_accept.xml` | SFU -> client | audio answer, BUNDLE, ICE, DTLS |
| `transport_info.xml` | SFU -> client | ICE candidate |
| `content_add_audio.xml` | SFU -> client | remote speaker track (`category speaker`) |
| `content_add_slides.xml` | SFU -> client | remote screen share (`category slides`) |
| `content_remove_audio.xml` | SFU -> client | participant left / stream removed |
| `content_accept_audio.xml` | client -> SFU | accept a remote content |
| `session_info_mute_*.xml` | SFU -> client | forwarded mute/unmute |

## Capturing a live late joiner

`capture-stanzas.py` drives the signalling with synthetic SDP, so neither
client ever establishes ICE/DTLS and the SFU never publishes a track to the
other participant. The capture therefore stops at `session-accept` and
`transport-info`; `content-add`/`content-remove` are produced from Movim's
`SDPtoJingle`/`Jingle` classes.

To capture them live, join the room with a real WebRTC client (Movim in a
browser, or the Rust app with the GStreamer backend) and run
`capture-stanzas.py` as the second XMPP client. Galener 0.35.5 already turns the
SFU's per-participant `offer` into a Galener `content-add`. Patch `0003` also
routes ICE candidates for those down connections by `jingle-participant`, which
stock Galener 0.35.5 ignores.

## Live conference test

The Rust media crate has an ignored integration test that joins this testbed as
several real GStreamer clients:

```sh
# two clients (alice/bob)
JABU_TEST_XMPP=1 cargo test -p media --test conference_live -- --ignored --nocapture

# ten clients (register the accounts first)
nix-shell dev/reference-testbed/shell.nix --run '
  for i in 1 2 3 4 5 6 7 8 9 10; do
    prosodyctl --config "$PWD/prosody/config/prosody.cfg.lua" \
      register "load$i" localhost jabu-test >/dev/null 2>&1 || true
  done'
JABU_TEST_XMPP=1 JABU_TEST_USERS=load1,load2,load3,load4,load5,load6,load7,load8,load9,load10 \
  cargo test -p media --test conference_live -- --ignored --nocapture
```

The test covers MUC join, SFU discovery, `propose`, Jingle
initiate/accept, per-participant `content-add`, isolated receive transports,
active speaker and screen sharing. It uses a synthetic audio tone and a test
screen source, so no camera, microphone or portal permission is needed.
