#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GROUP_DIR="$ROOT/galene/groups"
DATA="$ROOT/galene/data"
STATIC="$ROOT/galene/static"

mkdir -p "$DATA" "$STATIC"

cat <<EOF
Galène SFU listening on http://127.0.0.1:8443/group

Configure the client in Settings -> Voice channels:
  Server:      http://127.0.0.1:8443/group
  Base group:  public
  Password:    (leave empty)

The native client talks to the WebSocket endpoint directly, so the browser
frontend in ./galene/static is not required.
EOF

exec nix shell nixpkgs#galene -c galene \
    -insecure \
    -http 127.0.0.1:8443 \
    -groups "$GROUP_DIR" \
    -data "$DATA" \
    -static "$STATIC" \
    -udp-range 50000-50100
