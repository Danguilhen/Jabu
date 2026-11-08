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
nav-settings = Settings

## Language settings.
language-title = Language
language-system = System default
language-hint = Applies to the whole app.

## Counted messages.
chat-unread =
    { $count ->
        [one] { $count } unread message
       *[other] { $count } unread messages
    }

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
