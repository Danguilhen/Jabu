# ADR-015: OMEMO state encrypted at rest

## Status

Accepted. Implemented for desktop.

## Context

OMEMO state contains the local identity key, signed prekeys, sessions and trust
decisions. It was persisted as plaintext JSON in the SQLite `settings` table.
The database is a file users back up and copy; a stolen database should not
expose the ability to decrypt past or future messages.

The desktop client already uses the OS keyring for account credentials
(ADR-006). The keyring is the natural place for a per-account encryption key,
but headless sessions and some Linux desktops may not have a Secret Service
available.

## Decision

- Keep the OMEMO JSON state in the `settings` table, but encrypt it with
  AES-256-GCM before writing.
- Store a random 32-byte per-account key in the OS keyring under
  `omemo-key:<jid>`.
- Use a 12-byte random nonce per write and store the state as
  `enc:v1:<nonce-b64>:<ciphertext-b64>`.
- On first read of a legacy plaintext value, encrypt and rewrite it when a
  keyring is available.
- Clear a plaintext copy whenever the same secret is written to the keyring, so
  a secret is never left in both stores.
- If no keyring service is available, keep the previous plaintext behaviour and
  log/document the fallback. Do not pretend the data is encrypted.
- Never fall back from an encrypted value to plaintext: if the key is missing or
  decryption fails, skip loading the state and warn instead of overwriting it.
  Without a keyring, saving over an existing encrypted value returns an error.

## Consequences

- A database copy alone is not enough to recover OMEMO state when the keyring is
  available; the keyring entry is required.
- Losing the keyring entry means losing the OMEMO session and device identity;
  the client must re-provision, which is the same outcome as deleting OMEMO
  state.
- Backups need both the database and the keyring entry, or the user must accept
  re-provisioning.
- Android Keystore integration is still pending; the fallback is used there
  until it lands.
- The whole database is still not encrypted; SQLCipher remains a separate
  decision (Gate G).
