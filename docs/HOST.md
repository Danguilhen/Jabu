# Public Host

`crates/host` is an optional, headless HTTP server that publishes **public
XMPP content**: blogs, community nodes, Spaces, Stories and contact profiles.
It reuses `xmpp-core` to fetch those PubSub nodes and never exposes chats,
private posts, bookmarks or account data.

There are two ways to start it:

- `jabu host` — inside the desktop application.
- `jabu-host` — a standalone binary that does not pull in Dioxus, GTK or
  WebKit, intended for servers and containers.

Both read the same `[xmpp]` and `[host]` configuration sections.

## Quick start

```sh
# Build the standalone server
cargo build --release -p host --bin jabu-host

cp deploy/host.toml.example host.toml
$EDITOR host.toml                # JID, password, public URL, admin token
./target/release/jabu-host --config host.toml
```

Put a TLS reverse proxy in front; the server itself speaks plain HTTP and
should bind to loopback.

## Configuration

| Key | Default | Notes |
| --- | --- | --- |
| `bind` | `127.0.0.1:8088` | Listen address. Config-only at runtime. |
| `public_url` | none | Canonical base for links, feeds and social cards. Config-only. |
| `site_name` | `Jabu` | Editable from `/admin/settings`. |
| `description` | — | Editable from `/admin/settings`. |
| `cache_ttl_secs` | `300` | In-memory content cache. Config-only. |
| `blog_page_size` | `20` | Posts fetched per page. Config-only. |
| `featured_blogs` | `[]` | JIDs listed in `sitemap.xml`; editable from `/admin/settings`. |
| `rate_limit_per_minute` | `120` | Per-client requests per minute; `0` disables. Config-only. |
| `trust_proxy` | `false` | Use the first `X-Forwarded-For` value as the client address. Config-only. |
| `db_path` | `jabu-host.sqlite3` | SQLite store for reports, moderation, settings and admin sessions. Config-only. |
| `admin_token` | none | Bootstrap admin token, imported only when no token exists. Config-only. |
| `retention_days` | `90` | Prune dismissed reports and old audit entries; `0` keeps forever. Editable. |
| `accent_color` | `#7dd3fc` | Link/accent colour. Editable. |
| `footer_note` | empty | Optional footer text. Editable. |

The CLI flags `--bind`, `--public-url` and `--admin-token` override the file.
`JABU_`-prefixed environment variables are merged last, like the desktop app.

## Routes

| Route | Purpose |
| --- | --- |
| `/` `/about` | Instance page and privacy note |
| `/blog/{jid}` | Public blog with posts and media |
| `/blog/{jid}/atom.xml` `/blog/{jid}/rss.xml` | Blog feeds |
| `/post/{jid}/{item}` | One blog post |
| `/contact/{jid}` | Contact profile and public subscriptions |
| `/community/{service}/{node}` | Community node |
| `/community/{service}/{node}/{item}` | Community article |
| `/community/{service}/{node}/atom.xml` `/rss.xml` | Community feeds |
| `/space/{service}/{node}` | Space rooms and directories |
| `/story/{jid}/{item}` | Active Story (expired Stories 404) |
| `/sitemap.xml` `/robots.txt` | Discovery and crawler policy |
| `/report` | Public report form |
| `/report/{id}` | Reporter-facing status |
| `/healthz` | Liveness probe (not rate limited) |
| `/admin` | Moderation queue and audit log |
| `/admin/settings` | Instance settings and branding |
| `/admin/tokens` | Admin token management |
| `/admin/api/reports` `/admin/api/audit` | JSON API |
| `/admin/api/backup` | SQLite snapshot download |

Pages carry canonical, OpenGraph/Twitter tags and a `jabu://` deep link, so
both web readers and the app can share the same content. Responses use an
`ETag` and short `Cache-Control`; the content cache is TTL-based.

## Administration

### Bootstrap and login

Set `[host].admin_token` to a long random value for the first start. When the
token table is empty it is stored as the `bootstrap` **admin** token; the
plaintext is not kept. After that, manage tokens from `/admin/tokens`.

The login form exchanges a token for a server-side session. The cookie is
`HttpOnly`, `SameSite=Strict`, `Secure` when `public_url` is HTTPS, and expires
after 24 hours. Logging out deletes the session. Revoking a token also deletes
every session created from it.

The JSON API accepts `Authorization: Bearer <token>` instead of the cookie.

### Roles

| Role | Can do |
| --- | --- |
| `moderator` | Report queue, hide/unhide/dismiss/reopen, audit log, JSON reports/audit |
| `admin` | Everything a moderator can, plus settings, tokens and backups |

Cookie-authenticated POSTs must come from the same origin (checked with
`Origin`/`Referer` and the `Host` header). Bearer requests are exempt.

### Settings and branding

`/admin/settings` edits the site name, description, accent colour, footer note,
featured blogs and retention. Saved values live in SQLite and take effect on
the next request; they override the configuration file on restart. Networking
and cache fields stay config-only because changing them would require
rebuilding the listener or cache.

### Moderation

Every public page has a "Report this page" link. Reports are stored with a
stable target:

| Target | Meaning |
| --- | --- |
| `post/{jid}/{id}` | Blog post |
| `contact/{jid}` | Contact profile (also hides that author's blog) |
| `community/{service}/{node}` | Community node |
| `community/{service}/{node}/{id}` | Community article |
| `space/{service}/{node}` | Space |
| `story/{jid}/{id}` | Story |

Actions are `hide`, `unhide`, `dismiss` and `reopen`. Hiding only affects this
Host — it never changes or deletes the underlying XMPP item. Agents and notes
are written to the audit log. Hidden targets are excluded from lists and
return 404 on their own page.

### Retention

With `retention_days > 0`, dismissed reports and audit entries older than the
cutoff are pruned at startup and after every moderation action. Open and
hidden reports are kept, and hidden targets stay hidden until unhidden. Set
`0` to keep everything.

## Backup and restore

`GET /admin/api/backup` (admin role) creates a consistent snapshot with SQLite
`VACUUM INTO` and streams it as
`jabu-host-YYYYMMDD-HHMMSS.sqlite3`. Nothing is written inside the database
directory.

Restore is deliberately offline:

1. Stop the Host.
2. Keep a copy of the current database.
3. Replace `db_path` with the backup file.
4. Start the Host.

The database contains reports, hidden targets, moderation actions, the audit
log, settings, admin tokens (hashed) and sessions (hashed). Content itself is
fetched from XMPP on demand.

## Deployment

### systemd

```sh
sudo useradd --system --home /var/lib/jabu --shell /usr/sbin/nologin jabu
sudo install -d -o jabu -g jabu /var/lib/jabu /etc/jabu
sudo install -m 600 -o jabu -g jabu host.toml /etc/jabu/host.toml
sudo install -m 755 jabu-host /usr/local/bin/jabu-host
sudo install -m 644 deploy/jabu-host.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now jabu-host
```

The unit runs as an unprivileged user with `ProtectSystem=strict`, an empty
capability set, a system-call filter and `/var/lib/jabu` as the only writable
path.

### Docker

```sh
cd deploy
cp host.toml.example host.toml   # edit it
docker compose up -d --build
```

The compose file binds the Host to `127.0.0.1:8088` and keeps the database in
a named volume. The image contains only the binary and CA certificates.

### Reverse proxy and TLS

Use `deploy/Caddyfile.example` (automatic certificates) or
`deploy/nginx.conf.example`. Forward `Host` and `X-Forwarded-For`, set
`trust_proxy = true`, and keep the Host bound to loopback.

Rate limiting is a fixed one-minute window per client. `/healthz` does not
consume the budget. When a proxy is in front, make sure it forwards the real
client address and that `trust_proxy` is only enabled for a trusted proxy.

## Privacy and security notes

- Only public PubSub nodes and public profiles are served. Chat, private
  messages and account data are never read by the Host.
- Admin tokens and sessions are stored as SHA-256 hashes; token plaintext is
  shown exactly once.
- The Host does not sanitise rich post HTML; bodies are rendered as escaped
  plain text with paragraphs and line breaks.
- Media links are restricted to safe schemes before rendering.
- `/admin` is excluded in `robots.txt`.
- The report form validates lengths; there is no CAPTCHA. Put a proxy-level
  limit in front of a busy instance if abuse is a concern.

## Testing

```sh
cargo test -p host
cargo test --workspace --lib
```

Host tests use an in-memory SQLite store, a fake content source and the same
router as production. They cover pages, feeds, caching, rate limiting, reports,
moderation, roles, sessions, settings, retention, backups and CSRF.
