# Galène voice-channel fixture

Voice channels use a Galène SFU so a channel can hold more than a handful of
participants. Space voice channels map to Galène groups:

```
<base group>/<space node>/<channel slug>
```

The `public` group in `groups/public.json` enables **automatic subgroups**, so a
new group is created the first time someone joins a channel. No per-channel
server configuration is needed.

## Run the fixture

```bash
./galene/run.sh
```

Then configure the client under **Settings → Voice channels** (the gear menu)
or with **Server settings** directly inside a voice channel:

- **Server**: `http://127.0.0.1:8443/group`
- **Base group**: `public`
- **Group password**: empty

Create a space, add a room with type **Voice channel**, select it, and press
**Join voice**. Speaking participants are highlighted, and each participant has
its own volume slider in the channel view.

## Production notes

- Put Galène behind TLS and use `https://host/group` as the server.
- Keep `auto-subgroups` enabled on the base group.
- Set a real group password (or tokens) instead of the fixture's wildcard user.
- Open the UDP range (or use Galène's multiplexed port) and configure a TURN
  server for participants behind restrictive NATs. Galène advertises its
  ICE servers in the group status; the client applies the first STUN and TURN
  server to each peer connection.
