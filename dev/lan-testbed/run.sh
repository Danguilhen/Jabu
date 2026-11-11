#!/usr/bin/env bash
# Start the disposable public Jabu testbed.
#
# This script expects the router to forward:
#   TCP 80, 443, 5222, 5223
#   UDP 50000-50100
# to this machine. It opens the same ports temporarily in the NixOS firewall.
set -euo pipefail

source "$(dirname "${BASH_SOURCE[0]}")/env.sh"

if [[ "${EUID}" -eq 0 ]]; then
    echo "Run this script as your normal user; it will ask for sudo when needed." >&2
    exit 1
fi

if [[ -f "$STATE/caddy.pid" || -f "$STATE/host.pid" || -f "$STATE/galene.pid" || -f "$STATE/prosody/prosody.pid" ]]; then
    "$LAN_TESTBED_ROOT/stop.sh" || true
fi

for port in "$PROSODY_C2S_PORT" "$PROSODY_DIRECT_TLS_PORT" "$PROSODY_HTTP_PORT"; do
    if (exec 3<>/dev/tcp/127.0.0.1/"$port") 2>/dev/null; then
        exec 3>&-
        exec 3<&-
        echo "Port $port is already in use. Stop the existing Prosody fixture first:" >&2
        echo "  ./prosody/stop.sh" >&2
        exit 1
    fi
done

for command in nix curl getcap sudo; do
    if ! command -v "$command" >/dev/null 2>&1; then
        echo "missing required command: $command" >&2
        exit 1
    fi
done

random_hex() {
    head -c "$1" /dev/urandom | od -An -tx1 | tr -d ' \n'
}

mkdir -p \
    "$STATE/prosody/data" \
    "$STATE/galene/groups" \
    "$STATE/galene/data" \
    "$STATE/galene/static" \
    "$STATE/caddy-data" \
    "$STATE/caddy-config" \
    "$STATE/logs" \
    "$STATE/bin" \
    "$STATE/tls"
chmod 700 "$STATE"

if [[ ! -f "$STATE/credentials.env" ]]; then
    umask 077
    {
        echo "ALICE_PASSWORD=$(random_hex 16)"
        echo "BOB_PASSWORD=$(random_hex 16)"
        echo "PHONE1_PASSWORD=$(random_hex 16)"
        echo "PHONE2_PASSWORD=$(random_hex 16)"
        echo "ADMIN_PASSWORD=$(random_hex 16)"
        echo "HOST_ADMIN_TOKEN=$(random_hex 32)"
    } > "$STATE/credentials.env"
fi
# shellcheck disable=SC1091
source "$STATE/credentials.env"

sed \
    -e "s|@STATE@|$STATE|g" \
    -e "s|@DOMAIN@|$DOMAIN|g" \
    "$LAN_TESTBED_ROOT/prosody.cfg.lua.in" > "$PROSODY_CONFIG"

sed \
    -e "s|@DOMAIN@|$DOMAIN|g" \
    "$LAN_TESTBED_ROOT/Caddyfile.in" > "$STATE/Caddyfile"

cat > "$STATE/host.toml" <<EOF
[xmpp]
jid = "alice@$DOMAIN"
password = "$ALICE_PASSWORD"
resource = "jabu-host"
auto_reconnect = true
tls_config = { ca_file = "/etc/ssl/certs/ca-certificates.crt" }

[host]
bind = "127.0.0.1:$HOST_PORT"
public_url = "https://$DOMAIN"
site_name = "Jabu LAN test"
description = "Disposable public Host for the $DOMAIN LAN test."
db_path = "$STATE/host.sqlite3"
admin_token = "$HOST_ADMIN_TOKEN"
trust_proxy = true
rate_limit_per_minute = 0
EOF

cp "$REPO_ROOT/galene/groups/public.json" "$STATE/galene/groups/public.json"

echo "== Building jabu-host =="
(
    cd "$REPO_ROOT"
    nix develop -c cargo build -p host --bin jabu-host
) > "$STATE/logs/build-host.log" 2>&1

HOST_BIN="$REPO_ROOT/target/x86_64-unknown-linux-gnu/debug/jabu-host"
if [[ ! -x "$HOST_BIN" ]]; then
    HOST_BIN="$REPO_ROOT/target/debug/jabu-host"
fi
if [[ ! -x "$HOST_BIN" ]]; then
    echo "jabu-host binary not found after the build" >&2
    exit 1
fi

echo "== Preparing Caddy =="
if [[ ! -x "$STATE/bin/caddy" ]]; then
    nix shell nixpkgs#caddy -c bash -c \
        'cp "$(command -v caddy)" "$1"' _ "$STATE/bin/caddy"
fi
if ! getcap "$STATE/bin/caddy" 2>/dev/null | grep -q cap_net_bind_service; then
    sudo setcap cap_net_bind_service=+ep "$STATE/bin/caddy"
fi

echo "== Opening temporary firewall ports =="
for port in 80 443 5222 5223; do
    sudo /run/current-system/sw/bin/nixos-firewall-tool open tcp "$port"
done
sudo bash -c \
    'for port in $(seq 50000 50100); do /run/current-system/sw/bin/nixos-firewall-tool open udp "$port"; done'

echo "== Starting Caddy (ACME certificate) =="
XDG_DATA_HOME="$STATE/caddy-data" \
XDG_CONFIG_HOME="$STATE/caddy-config" \
    "$STATE/bin/caddy" run --config "$STATE/Caddyfile" --adapter caddyfile \
    > "$STATE/logs/caddy.log" 2>&1 &
CADDY_PID=$!
echo "$CADDY_PID" > "$STATE/caddy.pid"

CERT=""
for _ in $(seq 1 120); do
    CERT="$(find "$STATE/caddy-data" -type f -name '*.crt' -print -quit 2>/dev/null || true)"
    if [[ -n "$CERT" ]]; then
        break
    fi
    if ! kill -0 "$CADDY_PID" 2>/dev/null; then
        echo "Caddy exited before obtaining a certificate:" >&2
        tail -80 "$STATE/logs/caddy.log" >&2 || true
        exit 1
    fi
    sleep 1
done
if [[ -z "$CERT" ]]; then
    echo "No certificate appeared. Check DNS and TCP 80/443 forwarding." >&2
    tail -80 "$STATE/logs/caddy.log" >&2 || true
    exit 1
fi
KEY="$(dirname "$CERT")/$(basename "$CERT" .crt).key"
if [[ ! -f "$KEY" ]]; then
    KEY="$(find "$(dirname "$CERT")" -type f -name '*.key' -print -quit)"
fi
if [[ -z "$KEY" || ! -f "$KEY" ]]; then
    echo "Certificate key not found next to $CERT" >&2
    exit 1
fi
ln -sf "$CERT" "$STATE/tls/fullchain.pem"
ln -sf "$KEY" "$STATE/tls/privkey.pem"
chmod 600 "$KEY" 2>/dev/null || true

echo "== Starting Prosody =="
(
    cd "$REPO_ROOT"
    nix shell nixpkgs#prosody -c prosody --config "$PROSODY_CONFIG" --daemonize
) > "$STATE/logs/prosody-start.log" 2>&1

for _ in $(seq 1 60); do
    if (exec 3<>/dev/tcp/127.0.0.1/$PROSODY_C2S_PORT) 2>/dev/null; then
        exec 3>&-
        exec 3<&-
        break
    fi
    sleep 0.5
done
if ! (exec 3<>/dev/tcp/127.0.0.1/$PROSODY_C2S_PORT) 2>/dev/null; then
    echo "Prosody did not open port $PROSODY_C2S_PORT" >&2
    tail -80 "$STATE/logs/prosody.log" >&2 2>/dev/null || true
    exit 1
fi

echo "== Registering test accounts =="
for account in alice bob phone1 phone2 admin; do
    case "$account" in
        alice) password="$ALICE_PASSWORD" ;;
        bob) password="$BOB_PASSWORD" ;;
        phone1) password="$PHONE1_PASSWORD" ;;
        phone2) password="$PHONE2_PASSWORD" ;;
        admin) password="$ADMIN_PASSWORD" ;;
    esac
    (
        cd "$REPO_ROOT"
        nix shell nixpkgs#prosody -c prosodyctl --config "$PROSODY_CONFIG" \
            register "$account" "$DOMAIN" "$password"
    ) >/dev/null 2>&1 || true
done

echo "== Starting Galène =="
nix shell nixpkgs#galene -c bash -c \
    "echo \$\$ > '$STATE/galene.pid'; exec galene -insecure -http 127.0.0.1:$GALENE_PORT -groups '$STATE/galene/groups' -data '$STATE/galene/data' -static '$STATE/galene/static' -udp-range $GALENE_UDP_RANGE" \
    > "$STATE/logs/galene.log" 2>&1 &

for _ in $(seq 1 60); do
    if (exec 3<>/dev/tcp/127.0.0.1/$GALENE_PORT) 2>/dev/null; then
        exec 3>&-
        exec 3<&-
        break
    fi
    sleep 0.5
done
if ! (exec 3<>/dev/tcp/127.0.0.1/$GALENE_PORT) 2>/dev/null; then
    echo "Galène did not open port $GALENE_PORT" >&2
    tail -80 "$STATE/logs/galene.log" >&2 || true
    exit 1
fi

echo "== Starting Host =="
NSS_WRAPPER_LIB="$(nix build --no-link --print-out-paths nixpkgs#nss_wrapper 2>/dev/null | tail -1)/lib/libnss_wrapper.so"
HOST_ENV=(env)
if [[ -f "$NSS_WRAPPER_LIB" ]]; then
    cp /etc/hosts "$STATE/hosts"
    printf '127.0.0.1 %s\n' "$DOMAIN" >> "$STATE/hosts"
    HOST_ENV+=(NSS_WRAPPER_HOSTS="$STATE/hosts" LD_PRELOAD="$NSS_WRAPPER_LIB")
    echo "Using a private hosts file for the local Host connection."
fi
"${HOST_ENV[@]}" "$HOST_BIN" --config "$STATE/host.toml" \
    > "$STATE/logs/host.log" 2>&1 &
echo "$!" > "$STATE/host.pid"

for _ in $(seq 1 60); do
    if (exec 3<>/dev/tcp/127.0.0.1/$HOST_PORT) 2>/dev/null; then
        exec 3>&-
        exec 3<&-
        break
    fi
    sleep 0.5
done
if ! (exec 3<>/dev/tcp/127.0.0.1/$HOST_PORT) 2>/dev/null; then
    echo "Host did not open port $HOST_PORT" >&2
    tail -80 "$STATE/logs/host.log" >&2 || true
    exit 1
fi

cat <<EOF

Jabu LAN testbed is running.

  Public URL:  https://$DOMAIN/
  XMPP:        $DOMAIN:5222
  Voice SFU:   https://$DOMAIN/group
  Admin:       https://$DOMAIN/admin

Accounts:
  alice@$DOMAIN  / $ALICE_PASSWORD
  bob@$DOMAIN    / $BOB_PASSWORD
  phone1@$DOMAIN / $PHONE1_PASSWORD
  phone2@$DOMAIN / $PHONE2_PASSWORD
  admin@$DOMAIN  / $ADMIN_PASSWORD

  Host admin token: $HOST_ADMIN_TOKEN

State and logs: $STATE
Stop:  dev/lan-testbed/stop.sh
Clean: dev/lan-testbed/clean.sh
EOF
