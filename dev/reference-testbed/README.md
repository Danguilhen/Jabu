# Movim/Galener SFU testbed

Local reference environment for the XMPP SFU conference work. It runs a real
Movim client, Galener and Galène against the repo's Prosody fixture.

## Ports

| Service | Port |
|---|---|
| Prosody c2s / external components | 5222 / 5347 |
| Movim web | 8080 |
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

## Galène 1.x renegotiation caveat

Galène asks the WebSocket client to create a new offer (`renegotiate`) when a
new participant publishes and the existing offer has no spare receive m-line.
Movim 0.35.5's Galener has no `renegotiate` handler, so a late joiner's
downstream stream is closed instead of being forwarded. That is why
`content-add`/`content-remove` fixtures are generated from Movim's own classes
rather than captured live. Live `content-add` can be captured once Galener
(or the future Rust SFU bridge) renegotiates on behalf of the XMPP client.
