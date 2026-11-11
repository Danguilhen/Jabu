#!/usr/bin/env bash
# Print the public and local checks for the disposable testbed.
set -euo pipefail

source "$(dirname "${BASH_SOURCE[0]}")/env.sh"

echo "DNS:"
getent hosts "$DOMAIN" || true
echo
echo "HTTPS:"
curl -sS --resolve "$DOMAIN:443:127.0.0.1" -o /dev/null \
    -w '  https://%{url_effective} -> %{http_code}\n' "https://$DOMAIN/" || true
echo
echo "XMPP TLS:"
timeout 30 nix shell nixpkgs#openssl -c openssl s_client \
    -connect "127.0.0.1:$PROSODY_C2S_PORT" \
    -starttls xmpp \
    -xmpphost "$DOMAIN" \
    -servername "$DOMAIN" \
    -brief </dev/null 2>&1 | sed 's/^/  /' || true
echo
echo "Upload endpoint:"
curl -sS --resolve "$DOMAIN:443:127.0.0.1" -o /dev/null \
    -w '  https://%{url_effective} -> %{http_code}\n' "https://$DOMAIN/file_share/" || true
echo
echo "Local listeners:"
ss -ltnp 2>/dev/null | grep -E ":(80|443|5222|5223|5280|8088|18444)\b" || true
echo
echo "Accounts are in: $STATE/credentials.env"
