#!/usr/bin/env bash
# Generate a local test CA and a server certificate for Prosody.

set -euo pipefail

CERT_DIR="prosody/config/certs"
mkdir -p "$CERT_DIR"

CA_KEY="$CERT_DIR/ca.key"
CA_CERT="$CERT_DIR/ca.crt"
SERVER_KEY="$CERT_DIR/localhost.key"
SERVER_CERT="$CERT_DIR/localhost.crt"
SERVER_CSR="$CERT_DIR/localhost.csr"
EXT_FILE="$CERT_DIR/localhost.ext"

echo "Generating local CA and server certificate for localhost..."

if [ ! -f "$CA_KEY" ] || [ ! -f "$CA_CERT" ]; then
    openssl genrsa -out "$CA_KEY" 2048
    openssl req -x509 -new -nodes -key "$CA_KEY" -sha256 -days 3650 \
        -subj "/CN=Jabu Test CA" -out "$CA_CERT"
fi

openssl genrsa -out "$SERVER_KEY" 2048
openssl req -new -key "$SERVER_KEY" -subj "/CN=localhost" -out "$SERVER_CSR"

cat > "$EXT_FILE" <<'EXT'
subjectAltName = DNS:localhost, IP:127.0.0.1
basicConstraints = CA:FALSE
keyUsage = digitalSignature, keyEncipherment
extendedKeyUsage = serverAuth
EXT

openssl x509 -req -in "$SERVER_CSR" -CA "$CA_CERT" -CAkey "$CA_KEY" \
    -CAcreateserial -out "$SERVER_CERT" -days 3650 -sha256 -extfile "$EXT_FILE"

rm -f "$SERVER_CSR" "$EXT_FILE"
chmod 644 "$CA_CERT" "$SERVER_CERT"
chmod 600 "$CA_KEY" "$SERVER_KEY"

echo "Certificates generated in $CERT_DIR/"
ls -la "$CERT_DIR/"
