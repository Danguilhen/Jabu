-- Prosody 13 test fixture for Jabu.
-- Run through prosody/run.sh so JABU_PROSODY_DIR is set.

local base = ENV_JABU_PROSODY_DIR or "."

admins = { "admin@localhost" }

data_path = base .. "/data"
plugin_paths = { base .. "/modules" }
pidfile = base .. "/data/prosody.pid"

modules_enabled = {
    "roster",
    "saslauth",
    "tls",
    "ping",
    "pep",
    "disco",
    "version",
    "uptime",
    "time",
    "register",
    "mam",
    "carbons",
    "csi_simple",
    "smacks",
    "blocklist",
    "private",
    "vcard",
    "lastactivity",
    "http_file_share",
    "bookmarks",
    "cloud_notify",
    "admin_adhoc",
    "announce",
}

allow_registration = true
registration_invite_only = false
authentication = "internal_hashed"
allow_unencrypted_plain_auth = true

c2s_ports = { 5222 }
c2s_direct_tls_ports = { 5223 }
c2s_interface = "127.0.0.1"

http_ports = { 5280 }
http_interfaces = { "127.0.0.1" }
https_ports = {}

c2s_require_encryption = false
s2s_require_encryption = false
s2s_secure_auth = false

archive_expires_after = "1y"
max_archive_query_results = 1000

http_file_share_size_limit = 104857600
http_file_share_expires_after = "1 week"

log = {
    { levels = { min = "info" }, to = "console" },
    { levels = { min = "debug" }, to = "file", filename = base .. "/data/prosody.log" },
}

VirtualHost "localhost"
    enabled = true
    ssl = {
        key = base .. "/config/certs/localhost.key";
        certificate = base .. "/config/certs/localhost.crt";
    }
    modules_enabled = {
        "mam",
        "carbons",
        "csi_simple",
        "smacks",
        "http_file_share",
        "bookmarks",
        "cloud_notify",
        "pep",
        "blocklist",
        "lastactivity",
    }

Component "pubsub.localhost" "pubsub"
    autocreate_on_publish = true
    modules_enabled = { "pubsub_user_create" }

Component "conference.localhost" "muc"
    name = "Chatrooms"
    restrict_room_creation = false
    muc_room_locking = false
    muc_room_default_public_jids = true
    modules_enabled = { "muc_mam" }

component_ports = { 5347 }
component_interfaces = { "127.0.0.1" }

Component "sfu.example.org"
    component_secret = "jabu-galener-test"
