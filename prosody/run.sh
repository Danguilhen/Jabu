#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export JABU_PROSODY_DIR="$ROOT/prosody"
CONFIG="$JABU_PROSODY_DIR/config/prosody.cfg.lua"
CERT_DIR="$JABU_PROSODY_DIR/config/certs"
PASSWORD="${JABU_PROSODY_PASSWORD:-jabu-test}"

mkdir -p "$JABU_PROSODY_DIR/data"

if [ ! -f "$CERT_DIR/localhost.crt" ] || [ ! -f "$CERT_DIR/localhost.key" ]; then
    (cd "$ROOT" && "$JABU_PROSODY_DIR/generate-certs.sh")
fi

RUNNING=0
if [ -f "$JABU_PROSODY_DIR/data/prosody.pid" ]; then
    PID="$(cat "$JABU_PROSODY_DIR/data/prosody.pid" 2>/dev/null || true)"
    if [ -n "$PID" ] && kill -0 "$PID" 2>/dev/null; then
        RUNNING=1
    fi
fi

if [ "$RUNNING" -eq 0 ]; then
    prosody --config "$CONFIG" --daemonize
fi

for user in alice bob admin; do
    prosodyctl --config "$CONFIG" register "$user" localhost "$PASSWORD" >/dev/null 2>&1 || true
done

for _ in $(seq 1 50); do
    if (exec 3<>/dev/tcp/127.0.0.1/5222) 2>/dev/null; then
        exec 3>&-
        exec 3<&-
        break
    fi
    sleep 0.2
done

if ! (exec 3<>/dev/tcp/127.0.0.1/5222) 2>/dev/null; then
    echo "Prosody did not open 127.0.0.1:5222. See the log above." >&2
    exit 1
fi
exec 3>&-
exec 3<&-

cat <<EOF
Prosody is running.
  C2S:      127.0.0.1:5222
  HTTP:     http://127.0.0.1:5280
  MUC:      conference.localhost
  Accounts: alice@localhost, bob@localhost, admin@localhost
  Password: $PASSWORD
EOF
