-- Prosody Configuration for Jabu Testing
-- Copy to prosody/config/prosody.cfg.lua

-- Server identity
admins = { "admin@localhost" }

-- Modules
modules_enabled = {
    -- Standard
    "roster", "saslauth", "tls", "dialback", "disco",
    "private", "vcard", "version", "uptime", "time",
    "ping", "register", "pep",

    -- Modern XMPP
    "mam", "csi", "csi_simple", "throttle_presence",
    "smacks", "carbons", "http_upload", "http_upload_external",
    "bookmarks2", "cloud_notify", "push_notify",

    -- Admin
    "admin_adhoc", "admin_telnet",
}

-- HTTP server for BOSH/WebSocket
http_host = "*"
http_ports = { 5280 }
https_ports = { }
consider_bosh_secure = true
cross_domain_bosh = true
consider_websocket_secure = true

-- SSL/TLS (disabled for local testing)
-- ssl = {
--     key = "/etc/prosody/certs/localhost.key";
--     certificate = "/etc/prosody/certs/localhost.crt";
-- }
c2s_require_encryption = false
s2s_require_encryption = false
s2s_secure_auth = false

-- Authentication
authentication = "internal_hashed"
allow_registration = true
min_password_length = 1

-- MAM (Message Archive Management)
default_archive_policy = true
archive_expires_after = "1y"
max_archive_query_results = 1000

-- CSI (Client State Indication)
csi_max_queue_size = 100

-- HTTP Upload
http_upload_file_size_limit = 100 * 1024 * 1024 -- 100MB
http_upload_external_base_url = "http://localhost:5280/upload/"

-- Push notifications (for mobile testing)
push_notify_vapid_pubkey = "YOUR_VAPID_PUBLIC_KEY"
push_notify_vapid_privkey = "YOUR_VAPID_PRIVATE_KEY"

-- Logging
log = {
    { levels = { min = "info" }, to = "console" },
    { levels = { min = "debug" }, to = "file", filename = "/var/log/prosody/prosody.log" },
}

-- Virtual hosts
VirtualHost "localhost"
    enabled = true
    modules_enabled = {
        "mam", "csi", "smacks", "carbons", "http_upload",
        "bookmarks2", "cloud_notify", "push_notify",
    }

-- Component for testing
Component "conference.localhost" "muc"
    name = "Chatrooms"
    restrict_room_creation = false

Component "upload.localhost" "http_upload"
    http_upload_file_size_limit = 100 * 1024 * 1024

-- Allow local connections without encryption for testing
c2s_require_encryption = false
s2s_require_encryption = false