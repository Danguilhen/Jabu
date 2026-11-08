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

## Conversations.
chat-no-conversations = No conversations yet
chat-no-archived = No archived conversations
chat-unread-filter = Unread
chat-search-placeholder = Search conversations
chat-no-messages = No messages yet
chat-load-older = Load older messages
chat-view-story = View story
chat-pin = Pin
chat-unpin = Unpin
chat-mute-action = Mute
chat-unmute = Unmute
chat-archive = Archive
chat-unarchive = Unarchive
chat-forward = Forward
chat-react = React { $emoji }
chat-saved-to = Saved to { $path }
chat-unread-messages = { $count } unread messages
chat-empty-state = Choose a chat or room from the sidebar, or search across your messages and contacts.
chat-pinned = Pinned
chat-muted = Muted
chat-online = Online
chat-away = Away
chat-dnd = Do not disturb
chat-xa = Extended away
chat-offline = Offline
chat-audio-call = Start an audio call
chat-video-call = Start a video call
chat-search-messages = Search messages
chat-clear-history = Clear this conversation's history
chat-change-subject = Change subject
chat-nudge = Send an attention request
chat-room-subject = Room subject
chat-cancel-reply = Cancel reply
chat-back = Back
chat-sending = Sending…
chat-failed = Failed
chat-edited = edited
chat-encrypted = Encrypted
chat-reply = Reply
chat-reply-thread = Reply in thread
chat-in-thread = In a thread
chat-add-reaction = Add reaction
chat-delete = Delete
chat-delete-everyone = Delete for everyone
chat-download = Download to the attachment cache
chat-message = Message
chat-send-placeholder = Send a message...
chat-attach = Attach a file
chat-send = Send message
chat-send-sticker = Send a sticker
chat-insert-emoji = Insert emoji
chat-stop-upload = Stop uploading the remaining files
chat-uploading = Uploading…
chat-attach-short = Attach
chat-searching = Searching…
chat-type-more = Type at least two characters.
chat-no-results = No results.
chat-results-messages = Messages
chat-results-rooms = Room messages
chat-results-contacts = Contacts
chat-results-rooms-list = Rooms
chat-results-posts = Posts
chat-results-spaces = Spaces
chat-incoming-audio = Incoming audio call
chat-incoming-video = Incoming video call
chat-incoming-call = Incoming call
chat-decline = Decline
chat-accept = Accept
chat-voice-channel = Voice channel
chat-leave = Leave
chat-join = Join
chat-conn-screen = Connecting to the other side's screen…
chat-starting-screen = Starting screen share…
chat-starting-camera = Starting camera…
chat-call-progress = Call in progress
chat-camera-on = Turn camera on
chat-camera-off = Turn camera off
chat-camera-pending = Turning on camera…
chat-switch-camera = Switch camera
chat-share-screen = Share screen
chat-stop-sharing = Stop sharing screen
chat-hang-up = Hang up
chat-voice-connected = Voice connected · { $count } in channel
chat-join-voice = Join the voice channel. Audio is relayed by the room's SFU.
chat-join-voice-action = Join voice
chat-connecting = Connecting…
chat-mute-speaker = Mute speaker
chat-unmute-speaker = Unmute speaker
chat-incoming-from =
    { $kind ->
        [audio] Incoming audio call from { $name }
        [video] Incoming video call from { $name }
       *[other] Incoming call from { $name }
    }
chat-incoming-kind =
    { $kind ->
        [audio] Incoming audio call
        [video] Incoming video call
       *[other] Incoming call
    }
notify-new-story = New story
notify-new-post = New post
notify-story-title = 📝 New story from { $author }
mute-15-minutes = 15 minutes
mute-1-hour = 1 hour
mute-8-hours = 8 hours
mute-24-hours = 24 hours
