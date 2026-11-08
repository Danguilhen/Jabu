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

## Space, social and picker surfaces.
space-category-name = Category name
space-channel-name = Channel name
space-channel-type = Channel type
space-category = Category
space-change-avatar = Change avatar
space-change-banner = Change banner
space-invite = Invite
space-configure = Configure
space-members-todo = Member management and configuration are not connected yet.
space-invite-link = Invite link
space-affiliation = Affiliation
space-invite-help = Anyone with this link can join the space. Only space managers can grant an affiliation.
space-send-invite = Send invitation
space-approve = Approve
space-deny = Deny
space-save-config = Save configuration
common-add = Add
space-save-directories = Save directories
space-link-room = Link room
space-room-type = Room type
space-room-address = Room address
space-display-name = Display name
space-join-auto = Join automatically
space-add-to = Add to space
space-visibility = Visibility
space-service-invalid = The spaces service address is invalid.
common-create = Create
profile-full-name = Full name
profile-nickname = Nickname
profile-organisation = Organisation
profile-title = Title
profile-email = Email
profile-phone = Phone
profile-website = Website
profile-note = Note
profile-avatar = Avatar
space-confirm-destroy = Confirm destroy
space-destroy = Destroy
chat-clear-help = This removes the messages from this device and local storage. Messages on the server are not deleted.
chat-clear = Clear
commands-title = Commands
common-next = Next
common-finish = Finish
common-submit = Submit
contact-open-chat = Open chat
common-refresh = Refresh
report-action = Report
contact-presence-sent = Presence request sent. Waiting for approval.
contact-request-presence = Request presence
gateway-register = Register
contacts-requests = Contact requests
contacts-none = No contacts
contacts-awaiting = Awaiting approval
contact-info = Info
gateway-search-service = Search service
common-load-more = Load more
common-send-action = Send
common-new = New
feed-publish = Publish post
feed-new-post = New post
community-no-posts = No posts in this community yet.
community-save = Save settings
community-delete = Delete community
feed-tags-placeholder = Tags (comma separated)
feed-load-older = Load older posts
story-share = Share
story-open-media = Open story media
story-encrypt-comment = Encrypt comment with OMEMO
chat-encrypt = Encrypt with OMEMO
story-retake = Retake
story-replace = Replace
story-take-photo = Take photo
story-cancel-camera = Cancel camera
story-choose = Choose an image, a video, or take a photo
story-gallery = Gallery
members-kick = Kick
members-ban = Ban
omemo-hide = Hide encryption devices
omemo-devices = Encryption devices
omemo-fetch = Fetch keys
omemo-heartbeat = Heartbeat
omemo-no-jids = This room does not expose real JIDs or has no known members.
omemo-none = No OMEMO devices known yet. Enable encryption in the composer to fetch them.
omemo-trust = Trust
omemo-distrust = Distrust
sticker-load = Load sticker
emoji-manage = Manage
emoji-help = Tap an emoji to add or remove it from Favorites.
emoji-remove-pack = Remove pack
emoji-no-match = No matching emoji.
emoji-no-favorites = No favorites yet. Press Favorites to add some.
emoji-none = No emoji here yet.

## Reporting and stickers.
report-reason = Reason
report-spam = Spam
report-abuse = Abuse or harassment
report-illegal = Illegal content
report-other = Other
report-details = Details (optional)
report-send = Send report
report-sending = Sending…
report-stored = Report stored. Id { $id }, status { $status }.
stickers-title = Stickers
stickers-import-hint = Import a sticker pack (.zip)
stickers-import-action = Import
stickers-no-packs = No sticker packs yet.
stickers-add-hint = Add a ZIP of images or a Movim sticker pack with meta.json.
stickers-import-pack = Import sticker pack

## Remaining UI labels.
space-invitee-jid = Invitee JID
profile-menu-title = Profile
contact-jid = Contact JID
contact-nickname-placeholder = nickname (optional)
rooms-search-title = Search rooms
rooms-join = Join room
post-like = Like
post-comment = Comment
post-comment-placeholder = Write a comment…
community-pubsub = PubSub service
community-create = Create a community
community-node = Node id
community-title = Community title
feed-post = Post
feed-post-title = Post title
feed-post-body = Post body
feed-post-placeholder = Write something…
feed-tags = Tags
community-label = Community
community-description = Description
community-members = Community members
report-this-post = Report this post
feed-edit = Edit post
feed-delete = Delete post
story-title = Story
story-comment-placeholder = Send a comment…
story-delete = Delete this story
story-share-title = Share story
story-caption = Caption
story-caption-placeholder = Add a caption…
main-no-conversation = No conversation selected
main-conversation = Conversation
chat-nudge-action = Nudge
chat-preview-link = Preview link
emoji-favorite = Add or remove favorite emoji
emoji-import = Import a Movim emoji pack (.zip)
common-import = Import
emoji-search = Search emoji
community-pubsub-hint = PubSub service, e.g. pubsub.example.org
community-node-hint = Node id, e.g. rust
emoji-alias-hint = Alias used in messages, e.g. :party:

feed-filter-all = All
feed-post-hint = Share something with your contacts...

## Dialog labels.
accounts-account-jid = Account JID
call-connected = Connected
space-menu = Space menu
channel-actions = Channel actions
space-create-category = Create category
space-create-channel = Create channel
space-members-tab = Members
space-invite-title = Invite to space
space-members-title = Space members
space-settings-title = Space settings
space-rooms-title = Space rooms
space-directory-placeholder = New directory name
space-add-room-title = Add room
accounts-menu = Account menu
space-add-title = Add space
space-address-placeholder = Space address or invite
space-service-label = Spaces service
space-service-placeholder = Spaces service (automatic if empty)
space-name = Space name
room-settings-title = Room settings
report-reason-placeholder = Reason (optional)
chat-clear-title = Clear history
rooms-search-public = Search public rooms
rooms-search-placeholder = Room name or keyword

call-authenticated = Authenticated
call-reconnecting = Reconnecting

report-title = Report content

## Dates and numbers.
date-month-1 = Jan
date-month-2 = Feb
date-month-3 = Mar
date-month-4 = Apr
date-month-5 = May
date-month-6 = Jun
date-month-7 = Jul
date-month-8 = Aug
date-month-9 = Sep
date-month-10 = Oct
date-month-11 = Nov
date-month-12 = Dec
date-medium = { $month } { $day }, { $year }
date-time = { $date }, { $time }
time-hm = { $hour }:{ $minute }

profile-listening = Listening
profile-mood = Mood
profile-activity = Activity
profile-location = Location
profile-idle-since = Idle since
profile-capabilities = Capabilities
profile-subscriptions = Public subscriptions

## Calls and shell chrome.
call-calling = Calling…
call-ringing = Ringing…
call-active = In call
call-ended = Ended
call-declined = Declined
call-missed = Missed
call-failed = Failed
call-incoming = Incoming
call-shared-screen = Shared screen
call-remote-screen = Remote screen
call-remote-video = Remote video
call-screen-preview = Screen preview
call-camera-preview = Camera preview
tray-show = Show Jabu
tray-quit = Quit
shell-skip-content = Skip to content
titlebar-minimize = Minimize
titlebar-maximize = Maximize
titlebar-close = Close
common-yes = Yes
common-no = No
search-placeholder = Search messages, contacts and rooms
feed-open-hint = Your social feed is open in the main area.

## Space rail and menus.
a11y-unread = { $count } unread
a11y-muted = muted
a11y-pending-invite = pending invite
a11y-locked = locked
space-actions = Space actions
space-label = Space
space-channel = Channel
space-mark-read = Mark as read
space-pin = Pin space
space-unpin = Unpin space
space-unmute = Unmute
space-muted-for = Muted for { $duration }
space-muted-forever = Muted until you unmute
space-until-unmute = Until I turn it back on
space-leave = Leave space
space-leave-confirm = Leave space?
space-leave-warning = You will no longer see { $name } in your rail. Messages stay on the server.
space-leaving = Leaving…
space-info = Space information
space-hide-muted = Hide muted channels
space-invite-menu = Invite to space…
space-share-link = Share space link
space-share-link-menu = Share space link…
space-create-category-menu = Create category…
space-create-channel-menu = Create channel…
space-link-room-menu = Link existing room…
space-mute-channel = Mute channel
space-unmute-channel = Unmute channel
space-no-rooms = No rooms yet
space-all-hidden = All channels are hidden
space-unhide-hint = Unhide muted channels to see them again.
space-empty-hint = Rooms added to this space will appear here.
time-in-minutes = { $count } min
time-in-hours = { $count } h
time-in-days = { $count } d

## Space and room dialogs.
common-creating = Creating…
common-subscribing = Subscribing…
common-subscribe = Subscribe
common-discovering = Discovering…
common-saving = Saving…
common-optional = Optional
common-error-jid = Enter a valid JID.
space-error-category-name = Enter a category name.
space-category-placeholder = e.g. Development
space-error-managers-only = Only space managers can create channels.
space-error-channel-name = Enter a channel name.
space-error-room-exists = A room with that address already exists in this space.
space-error-create-room = Could not create the room: { $error }
space-error-add-room = The room was created, but it could not be added to the space: { $error }
space-channel-placeholder = e.g. general
space-text-channel = Text channel
space-voice-channel = Voice channel
space-no-category = No category
space-parent = Parent
space-linked-nodes = Linked nodes
space-related-nodes = Related nodes
space-related-hint = Child or linked nodes on this service
pubsub-invalid-service = invalid PubSub service
space-no-related = No related nodes found
space-hierarchy = Hierarchy subscription
space-hierarchy-hint = Follow this node, its children or the whole subtree
space-scope-node = This node
space-scope-children = Children
space-scope-all = All descendants
space-scope-node-lower = this node
space-scope-children-lower = child nodes
space-scope-all-lower = all descendants
space-subscribed = Subscribed to { $scope }
space-role-member = Member
space-role-admin = Admin
space-role-owner = Owner
space-role-outcast = Outcast
space-role-publisher = Publisher
space-role-none = None
space-affiliation-none = No affiliation
space-loading-members = Loading members…
space-no-members = No members returned by the service.
members-pending = Pending
space-config-title = Space configuration
common-loading-config = Loading configuration…
space-rooms-dirs-title = Space rooms and directories
space-directories = Directories
space-kind-text = Text
space-kind-voice = Voice
space-no-directory = No directory
space-link-room-title = Link an existing room
space-voice-name-placeholder = Voice channel name
space-error-room-address = Enter a room address.
space-error-room-address-invalid = Enter a valid room address.
space-join-existing = Join an existing space
space-error-invite = Enter a valid space invitation link.
space-create-new = Create a new space
space-public = Public
space-private = Private
space-error-name = Enter a space name.

## Chat labels.
chat-you = You
chat-attachment-fallback = [attachment]
chat-message-yourself = Message yourself
chat-note-to-self = Note to Self
chat-back-to-chats = Back to chats
chat-forward-title = Forward message
chat-forward-failed = Could not forward this message.
chat-forward-none = No other conversations to forward to.
room-error-jid = Invalid room JID
room-saved = Room configuration saved.
room-destroyed = Room destroyed.
room-config-title = Room configuration
room-error-invite-jid = Enter a valid JID to invite
room-invite-title = Invite to room
story-yours = Your story

## Space roles shown in member lists.
space-role-none-label = None

## Contacts, gateways and rooms.
presence-invisible-error = This server does not support invisible mode (XEP-0186).
calls-automatic = Automatic
contact-idle = idle
contact-block = Block
contact-unblock = Unblock
contact-presence-wants = This contact wants to see your presence.
contact-presence-mutual = Presence is not shared in both directions.
contact-not-found = Contact not found
contact-groups = Groups (comma separated)
contacts-all = All contacts
profile-name = Name
profile-role = Role
profile-no-subscriptions = No public subscriptions
profile-shared-media = Shared media
profile-no-media = No shared media yet
profile-resources = Resources
profile-no-presence = No presence information
common-invalid-jid = Invalid JID
common-removing = Removing…
common-publishing = Publishing…
commands-adhoc-title = Ad-hoc commands
commands-still-running = The command is still running.
commands-loading = Loading commands…
commands-none = No commands advertised
gateway-none = This server advertises no gateways.
gateway-removed = Registration removed.
gateway-submitted = Registration submitted.
gateway-looking = Looking for gateways…
gateway-registered = This account is registered.
gateway-loading-form = Loading form…
chat-attachment-name = Attachment
rooms-none = No rooms joined
room-pin = Pin room
room-unpin = Unpin room
rooms-error-service-jid = Enter a valid search service JID.
rooms-error-term = Enter a search term.
rooms-none-found = No public rooms found.
rooms-find = Find public rooms
rooms-not-open = Not open

## Communities and posts.
communities-explore = Explore communities
communities-following = Following
communities-none-following = You do not follow any communities yet.
communities-cached = Cached
communities-discover-hint = Discover public PubSub nodes and follow them.
communities-suggestions = Suggestions
communities-suggested = Suggested by contacts
communities-none-cached = No communities cached yet. Search a PubSub server above.
community-error-required = Server, node and title are required
community-articles = Articles
community-create-action = Create community
community-follow = Follow
community-unfollow = Unfollow
community-settings-title = Community settings
community-policy-open = Anyone can publish
community-policy-subscribers = Subscribers can publish
community-policy-publishers = Publishers only
community-delete-confirm = Delete this community and all its posts?
community-no-affiliation = No affiliation information returned.
feed-error-title = Add a title for the post
feed-new-community-post = New community post
feed-editing = Editing post
feed-tags-hint = rust, xmpp
feed-none = No posts yet
post-no-comments = No comments yet
post-edited = · edited
story-play = Play
story-pause = Pause
story-unavailable = Story unavailable
story-unavailable-short = This story is no longer available.
story-unavailable-detail = This story is no longer available. It may have expired or you may not have access.
story-delete-confirm = Delete this story?
story-delete-hint = It will be retracted from your stories node immediately.
story-no-share-targets = No contacts or rooms to share with.
story-error-image = Could not read that image
story-error-media = A story needs an image or a video
story-error-no-camera = No camera was found
story-error-camera-image = The camera returned an unreadable image
story-error-crop = Could not crop that image
story-new = New story
story-crop-preview-alt = story crop preview
story-zoom = Zoom
story-horizontal = Horizontal
story-vertical = Vertical
story-camera-preview-alt = camera preview
story-default-camera = Default camera
story-publish = Publish story

## Chat message details and OMEMO.
chat-private-notes = Private notes
room-subject = Subject
room-no-occupants = No occupants visible yet.
room-kicked = Kicked
room-banned = Banned
omemo-bundle-missing = bundle not downloaded
omemo-trusted = Trusted
omemo-untrusted = Not trusted
omemo-unverified = Not verified
omemo-signature-invalid = signature invalid
chat-sticker = Sticker
chat-forwarded = Forwarded message
chat-mentioned-you = mentioned you
chat-attention = Attention
chat-edited-suffix = (edited)
chat-upload-cancelled = Upload cancelled
omemo-toggle = Toggle OMEMO encryption
omemo-toggle-room = Toggle room OMEMO encryption

## Emoji picker and report dialog.
emoji-title = Emoji
emoji-recent = Recent
emoji-favorites = Favorites
emoji-custom = Custom
emoji-tone = Tone
emoji-default-tone = Default
emoji-toggle-favorite = Toggle :{ $shortcut }: favorite
emoji-tone-light = light skin tone
emoji-tone-medium-light = medium-light skin tone
emoji-tone-medium = medium skin tone
emoji-tone-medium-dark = medium-dark skin tone
emoji-tone-dark = dark skin tone
emoji-group-smileys = Smileys & Emotion
emoji-group-people = People & Body
emoji-group-animals = Animals & Nature
emoji-group-food = Food & Drink
emoji-group-travel = Travel & Places
emoji-group-activities = Activities
emoji-group-objects = Objects
emoji-group-symbols = Symbols
emoji-group-flags = Flags
emoji-group-component = Component
emoji-group-skin-tones = Skin Tones
report-target = Target
report-no-host = No reporting Host is configured. Add [reporting] host = "https://…" to config.toml.
stickers-invalid = Invalid sticker data
call-connecting = Connecting…
