## Common actions and labels.
common-close = Close
common-cancel = Cancel
common-save = Save
common-retry = Retry
common-search = Search
common-send = Send message
common-attach = Attach a file
common-remove-attachment = Remove attachment
common-loading = Loading…
common-done = Done
common-delete = Delete
common-edit = Edit

## Navigation and shell sections.
nav-chats = Chats
nav-contacts = Contacts
nav-rooms = Rooms
nav-feed = Feed
nav-communities = Communities
nav-sections = Sections and spaces
nav-accounts = Accounts
nav-add-space = Add a space
notify-always = All messages
notify-mentions = Mentions only
notify-never = Nothing
nav-settings = Settings

## Language settings.
language-title = Language
language-system = System default
language-hint = Applies to the whole app.

## Counted messages.
chat-typing = typing…
chat-unread =
    { $count ->
        [one] { $count } unread message
       *[other] { $count } unread messages
    }

space-members = Members
space-notifications = Notifications
space-address = Address

room-members =
    { $count ->
        [one] { $count } member
       *[other] { $count } members
    }

## Relative time.
time-now = now
time-minutes-ago = { $count }m ago
time-hours-ago = { $count }h ago
time-days-ago = { $count }d ago

## Application shell and authentication.
app-client = A native XMPP client
login-tagline = Your conversations, without the noise.
login-description = A fast, private desktop client for XMPP. Chat, join rooms, share files and keep your history on your own device.
login-direct = Direct messages, rooms and archives
login-encrypted = Encrypted 1:1 chats with OMEMO
login-local = Local storage you control
login-welcome = Welcome back
login-signin = Sign in
login-subtitle = Use your XMPP address and password.
common-jid = JID
common-password = Password
common-resource = Resource
login-insecure = Allow invalid TLS certificates (debug only)
login-remember = Remember this account
login-connecting = Connecting…
login-password-note = Your password is only sent to your XMPP server.
login-password-placeholder = Your password
login-error-jid = Enter a full JID, e.g. user@example.com
login-error-password = Enter your password

connection-connecting = Connecting to server…
connection-connecting-detail = Waiting for the XMPP connection and authentication.
connection-failed = Could not connect
connection-disconnected = Disconnected
connection-disconnected-detail = You are not connected to the XMPP server.

section-home = Direct messages and rooms
section-contacts = People on your roster
section-rooms = Discover and manage rooms
section-feed = Communities and posts
section-communities = Follow and explore communities

nav-item-unread =
    { $label }, { $count ->
        [one] { $count } unread
       *[other] { $count } unread
    }

## Presence and account menus.
presence-title = Presence
presence-status = Status
presence-online = Online
presence-away = Away
presence-idle = Idle
presence-dnd = Do not disturb
presence-xa = Extended away
presence-invisible = Invisible
presence-custom = Custom message
presence-custom-placeholder = What are you up to?
presence-priority = Priority
presence-mark-idle = Mark as idle
presence-invisible-unsupported = Invisible mode is not supported by this server.
presence-invisible-xep = Your server does not support invisible mode (XEP-0186)
presence-set-custom = Set custom status…
presence-edit-profile = Edit profile
presence-manage-accounts = Manage accounts…
presence-gateways = Gateways
presence-sign-out = Sign out

## Accounts.
accounts-title = Accounts
accounts-add = Add account
accounts-add-connect = Add and connect
accounts-active = Active
accounts-online = Online
accounts-offline = Offline
accounts-notify = Notify
accounts-switch = Switch
accounts-remove = Remove
accounts-password-missing = Enter the account password.
accounts-not-connected = This account is not connected.\nRestart Jabu to start it.
accounts-no-config = This account has no saved configuration.

## Settings.
settings-title = Settings
settings-theme = Theme
theme-light = Light
theme-silver = Silver
theme-dark = Dark
theme-butter = Butter
settings-font-size = Font size
settings-compact = Compact spacing
settings-autostart = Start on login
settings-notifications = Desktop notifications
settings-microphone = Microphone
settings-camera = Camera
settings-sfu = Call SFU
settings-timestamps = Show timestamps
settings-link-previews = Link previews
settings-link-previews-hint = Fetches page metadata from linked sites when you ask for a preview.
settings-shortcuts = Keyboard shortcuts
settings-shortcut-search = Search
settings-shortcut-settings = Settings
settings-shortcut-sections = Switch section
settings-shortcut-focus = Focus conversation / sidebar
settings-shortcut-close = Close a dialog or overlay
settings-shortcut-send = Send a message
settings-shortcut-menu = Open a context menu
