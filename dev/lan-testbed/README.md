# Disposable public LAN testbed

Temporary Prosody + Caddy + Host + Galène stack for testing the 0.1.2
Windows and Android builds from the local network and the public domain.

Everything this stack creates lives under `dev/lan-testbed/state/`, which is
gitignored. Nothing is added to `/etc/nixos`, no systemd unit is enabled, no
package is installed globally, and no DDNS updater is configured.

## Prerequisites

The router must forward these ports to this machine (`192.168.68.56`):

| Public | Local | Purpose |
|---|---|---|
| TCP 80 | 80 | ACME challenge and HTTP redirect |
| TCP 443 | 443 | Caddy: Host, upload proxy, Galène WebSocket |
| TCP 5222 | 5222 | XMPP client-to-server |
| TCP 5223 | 5223 | Optional direct TLS |
| UDP 50000-50100 | 50000-50100 | Galène media |

`organic.ddnsfree.com` must resolve to the current public IP. This test does
not install a DDNS updater; update the record manually if the IP changes.

## Start

If the repository Prosody fixture is already running, stop it first because it
owns the loopback ports the public testbed needs:

```bash
./prosody/stop.sh
```

Then:

```bash
dev/lan-testbed/run.sh
```

The script will ask for sudo twice: once to give the disposable Caddy binary
permission to bind ports 80/443, and once to open the temporary NixOS firewall
ports. It then:

1. Builds `jabu-host`.
2. Obtains a Let's Encrypt certificate through Caddy.
3. Writes the certificate links used by Prosody.
4. Starts Prosody, Galène, the Host and Caddy.
5. Registers `alice`, `bob`, `phone1`, `phone2` and `admin`.

Credentials are printed at the end and stored in
`dev/lan-testbed/state/credentials.env`.

## Check

```bash
dev/lan-testbed/check.sh
```

This prints DNS resolution, the HTTPS status, the XMPP TLS handshake and the
local listeners.

## Client setup

Windows:

- Install the 0.1.2 artifact.
- JID: `alice@organic.ddnsfree.com`
- Password: from `state/credentials.env`
- Voice channel server: `https://organic.ddnsfree.com/group`
- Base group: `public`
- Group password: empty

Android:

- Install the signed 0.1.2 APK.
- JID: `phone1@organic.ddnsfree.com`
- Password: from `state/credentials.env`

The 0.1.3 Android build can test chat, MUC, PubSub, Spaces, Stories, uploads,
OMEMO, reporting and Galène group voice. It still cannot make one-to-one
calls: the Jingle media backend is not compiled in yet.

Group video is not implemented. Android group voice uses the Galène group at
`https://organic.ddnsfree.com/group` with base group `public`. Caddy exposes
`/group*` to Galène and normalises the WebSocket `Origin` header to the public
domain, because Galène rejects the WebView origin `https://dioxus.index.html`.
Use the same rewrite (or configure Galène to allow the app origin) on any other
Galène deployment.

## Host

Caddy serves the Host at:

```text
https://organic.ddnsfree.com/
https://organic.ddnsfree.com/admin
```

The Host uses the `alice` account. That is intentional for this short test:
microblog nodes are published with `access_model=presence`, so a separate
Host account would not see posts unless it had presence subscriptions. Use
`alice` to test the Host pages for Alice's own posts.

The Host is started with a private hosts file through `nss_wrapper`, so it can
reach Prosody on loopback even when the router has no NAT loopback. This only
affects the Host process; it does not modify `/etc/hosts`.

## Hairpin NAT

The Host handles the missing loopback locally, but phones and Windows machines
still need to resolve the public domain to the LAN address. If the router does
not support NAT loopback, use a temporary local DNS override:

- Add `organic.ddnsfree.com 192.168.68.56` to the device's hosts file, or
- Run a local DNS resolver for the LAN and point the devices/router at it.

Do not disable Android Private DNS unless you have set a local resolver; the
certificate still has to match `organic.ddnsfree.com`.

## Stop

```bash
dev/lan-testbed/stop.sh
```

This stops every process and runs `nixos-firewall-tool reset`, restoring the
temporary firewall changes. The state directory is kept for logs and
certificate inspection.

## Reverse everything

Run:

```bash
dev/lan-testbed/clean.sh
```

`clean.sh` stops the services, resets the firewall and deletes
`dev/lan-testbed/state/` including the certificate key, Host database,
Prosody data and logs.

Then finish the external cleanup manually:

1. Remove the router forwards for TCP `80`, `443`, `5222`, `5223` and UDP
   `50000-50100`.
2. If you changed `organic.ddnsfree.com` only for this test, restore the DDNS
   record.
3. If you added a temporary `/etc/hosts` entry for hairpin DNS, remove it.
4. If you stopped the repository Prosody fixture before the test, restart it:
   `./prosody/run.sh`
5. Confirm the firewall is back to its system configuration:

   ```bash
   nixos-firewall-tool show
   ```

No `/etc/nixos` file was edited, no systemd unit was enabled and no permanent
package was installed.
