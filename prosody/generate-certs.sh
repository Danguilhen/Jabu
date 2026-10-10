#!/bin/bash
# Generate self-signed certificates for Prosody testing

set -e

CERT_DIR="prosody/config/certs"
mkdir -p "$CERT_DIR"

echo "Generating self-signed certificate for localhost..."

# Generate private key
openssl genrsa -out "$CERT_DIR/localhost.key" 2048

# Generate certificate
openssl req -new -x509 -key "$CERT_DIR/localhost.key" -out "$CERT_DIR/localhost.crt" -days 365 -subj "/CN=localhost" \
    -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"

# Set permissions
chmod 644 "$CERT_DIR/localhost.crt"
chmod 600 "$CERT_DIR/localhost.key"

echo "Certificates generated in $CERT_DIR/"
ls -la "$CERT_DIR/"