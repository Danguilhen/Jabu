#!/usr/bin/env bash
# Stop the disposable testbed and delete every artifact it created.
#
# This does not remove the router port forwards; remove those manually.
set -euo pipefail

source "$(dirname "${BASH_SOURCE[0]}")/env.sh"

"$LAN_TESTBED_ROOT/stop.sh" || true

if [[ -n "${STATE:-}" && "$STATE" == "$LAN_TESTBED_ROOT/state" ]]; then
    rm -rf "$STATE"
    echo "Removed $STATE"
else
    echo "Refusing to remove unexpected state path: ${STATE:-<unset>}" >&2
    exit 1
fi

cat <<EOF

Cleanup complete.

Still manual, outside this repository:
  1. Remove the router forwards for TCP 80, 443, 5222, 5223, 3478 and UDP
     3478, 50000-50100, 50200-50300.
  2. If you changed the DDNS record only for this test, restore it.
  3. If you added a temporary /etc/hosts entry for hairpin DNS, remove it.

Check that the ports are closed again:
  nixos-firewall-tool show
EOF
