#!/usr/bin/env python3
"""Capture the Galener Jingle conference dialect from a real SFU.

Joins a room as a normal XMPP client, invites the SFU component, then performs
the Movim/Galener call flow (propose -> session-initiate -> content-add ->
content-accept -> session-info mute -> session-terminate). Every stanza that
arrives is written to a capture file so it can be turned into golden fixtures.

Run against the testbed:

    python3 dev/reference-testbed/capture-stanzas.py

It uses only the Python standard library.
"""

import base64
import hashlib
import os
import re
import secrets
import socket
import sys
import time
import uuid

HOST = os.environ.get("XMPP_HOST", "127.0.0.1")
PORT = int(os.environ.get("XMPP_PORT", "5222"))
DOMAIN = os.environ.get("XMPP_DOMAIN", "localhost")
ROOM = os.environ.get("ROOM") or "voice-{}@conference.localhost".format(secrets.token_hex(3))
SFU = os.environ.get("SFU", "sfu.example.org")
PASSWORD = os.environ.get("XMPP_PASS", "jabu-test")
OUTPUT = os.environ.get(
    "CAPTURE_OUT",
    os.path.join(os.path.dirname(os.path.abspath(__file__)), "capture.galener.xml"),
)


def esc(value):
    return (
        value.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace("'", "&apos;")
        .replace('"', "&quot;")
    )


def fingerprint():
    raw = secrets.token_bytes(32)
    return ":".join(f"{b:02X}" for b in raw)


class Client:
    def __init__(self, user, nick):
        self.user = user
        self.nick = nick
        self.jid = f"{user}@{DOMAIN}/{nick}"
        self.received = []
        self.sock = socket.create_connection((HOST, PORT))
        self.sock.settimeout(0.2)
        self.buffer = ""

    def send(self, xml):
        self.sock.sendall(xml.encode())

    def pump(self):
        try:
            chunk = self.sock.recv(65536)
        except socket.timeout:
            return ""
        if not chunk:
            raise EOFError("connection closed")
        text = chunk.decode(errors="replace")
        self.buffer += text
        return text

    def wait_for(self, needle, timeout=15.0):
        end = time.time() + timeout
        while time.time() < end:
            if needle in self.buffer:
                return True
            self.pump()
        return False

    def read_for(self, seconds=3.0):
        end = time.time() + seconds
        out = ""
        while time.time() < end:
            try:
                chunk = self.sock.recv(65536)
            except socket.timeout:
                continue
            if not chunk:
                break
            text = chunk.decode(errors="replace")
            out += text
            self.buffer += text
        self.received.append(out)
        return out

    def login(self):
        self.send(
            "<?xml version='1.0'?><stream:stream to='{}' xmlns='jabber:client' "
            "xmlns:stream='http://etherx.jabber.org/streams' version='1.0'>".format(DOMAIN)
        )
        time.sleep(0.4)
        self.read_for(1.0)
        auth = base64.b64encode(
            b"\x00" + self.user.encode() + b"\x00" + PASSWORD.encode()
        ).decode()
        self.send(
            "<auth xmlns='urn:ietf:params:xml:ns:xmpp-sasl' mechanism='PLAIN'>{}</auth>".format(auth)
        )
        time.sleep(0.4)
        self.read_for(0.8)
        self.send(
            "<?xml version='1.0'?><stream:stream to='{}' xmlns='jabber:client' "
            "xmlns:stream='http://etherx.jabber.org/streams' version='1.0'>".format(DOMAIN)
        )
        time.sleep(0.4)
        self.read_for(1.0)
        self.send(
            "<iq type='set' id='bind'><bind xmlns='urn:ietf:params:xml:ns:xmpp-bind'>"
            "<resource>{}</resource></bind></iq>".format(esc(self.nick))
        )
        time.sleep(0.4)
        self.read_for(1.0)
        self.send(
            "<iq type='set' id='sess'><session xmlns='urn:ietf:params:xml:ns:xmpp-session'/></iq>"
        )
        time.sleep(0.3)
        self.read_for(0.8)
        self.send("<presence><show>online</show></presence>")
        time.sleep(0.3)
        self.read_for(0.8)

    def join(self):
        self.send(
            "<presence to='{}/{}'><x xmlns='http://jabber.org/protocol/muc'/></presence>".format(
                ROOM, esc(self.nick)
            )
        )
        time.sleep(0.6)
        self.read_for(1.5)
        self.send(
            "<iq type='set' id='cfg' to='{}'><query xmlns='http://jabber.org/protocol/muc#owner'>"
            "<x xmlns='jabber:x:data' type='submit'/></query></iq>".format(ROOM)
        )
        time.sleep(0.4)
        self.read_for(1.0)

    def invite_sfu(self):
        self.send(
            "<message to='{}' id='inv-{}'><x xmlns='http://jabber.org/protocol/muc#user'>"
            "<invite to='{}'><reason>Jabu stanza capture</reason></invite></x></message>".format(
                ROOM, uuid.uuid4(), SFU
            )
        )

    def set_sfu_admin(self):
        # Movim's RoomsUtils::ajaxInviteSFU does exactly this: the component must
        # be admin before it joins, otherwise Galener treats its own affiliation
        # 'none' self-presence as a kick and leaves the room. Galener's own
        # participant JID is sha1(room)@component-domain; Prosody does not always
        # deliver the bare component self-presence, so set both up front.
        sfu_jid = hashlib.sha1(ROOM.encode()).hexdigest() + "@" + SFU
        self.send(
            "<iq type='set' id='admin-{}' to='{}'>"
            "<query xmlns='http://jabber.org/protocol/muc#admin'>"
            "<item affiliation='admin' jid='{}'/>"
            "<item affiliation='admin' jid='{}'/>"
            "</query></iq>".format(uuid.uuid4(), ROOM, SFU, sfu_jid)
        )

    def sfu_jid(self, timeout=20.0):
        end = time.time() + timeout
        pattern = re.compile(r"<item[^>]*jid='([^']+)'")
        while time.time() < end:
            self.read_for(1.0)
            for match in pattern.finditer(self.buffer):
                jid = match.group(1)
                if jid.endswith("@" + SFU):
                    return jid
        return None

    def disco_info(self, to):
        self.send(
            "<iq type='get' id='disco-{}' to='{}'>"
            "<query xmlns='http://jabber.org/protocol/disco#info'/></iq>".format(uuid.uuid4(), esc(to))
        )

    def propose(self, to, sid):
        self.send(
            "<message type='chat' to='{}' id='prop-{}'>"
            "<propose xmlns='urn:xmpp:jingle-message:0' id='{}'>"
            "<description xmlns='urn:xmpp:jingle:apps:rtp:1' media='audio'/>"
            "</propose></message>".format(esc(to), uuid.uuid4(), esc(sid))
        )

    def session_initiate(self, to, sid, video=False):
        media = "video" if video else "audio"
        payloads = (
            "<payload-type id='96' name='VP8' clockrate='90000'>"
            "<rtcp-fb xmlns='urn:xmpp:jingle:apps:rtp:rtcp-fb:0' type='nack'/>"
            "<rtcp-fb xmlns='urn:xmpp:jingle:apps:rtp:rtcp-fb:0' type='nack' subtype='pli'/>"
            "</payload-type>"
            if video
            else "<payload-type id='111' name='opus' clockrate='48000' channels='2'>"
            "<parameter name='minptime' value='10'/>"
            "<parameter name='useinbandfec' value='1'/>"
            "<rtcp-fb xmlns='urn:xmpp:jingle:apps:rtp:rtcp-fb:0' type='transport-cc'/>"
            "</payload-type>"
            "<payload-type id='0' name='PCMU' clockrate='8000'/>"
            "<payload-type id='8' name='PCMA' clockrate='8000'/>"
            "<payload-type id='126' name='telephone-event' clockrate='8000'/>"
        )
        xml = (
            "<iq type='set' id='jingle-{id}' to='{to}'>"
            "<jingle xmlns='urn:xmpp:jingle:1' action='session-initiate' "
            "initiator='{jid}' sid='{sid}'>"
            "<content creator='initiator' name='0' senders='initiator'>"
            "<description xmlns='urn:xmpp:jingle:apps:rtp:1' media='{media}'>"
            "{payloads}"
            "<rtcp-mux/>"
            "</description>"
            "<transport xmlns='urn:xmpp:jingle:transports:ice-udp:1' ufrag='{ufrag}' pwd='{pwd}'>"
            "<fingerprint xmlns='urn:xmpp:jingle:apps:dtls:0' hash='sha-256' setup='actpass'>{fp}</fingerprint>"
            "</transport>"
            "</content>"
            "<group xmlns='urn:xmpp:jingle:1' semantics='BUNDLE'><content name='0'/></group>"
            "</jingle></iq>"
        ).format(
            id=uuid.uuid4(),
            to=esc(to),
            jid=esc(self.jid),
            sid=esc(sid),
            media=media,
            payloads=payloads,
            ufrag=secrets.token_hex(4),
            pwd=secrets.token_hex(12),
            fp=fingerprint(),
        )
        self.send(xml)

    def content_accept(self, to, sid, participant):
        self.send((
            "<iq type='set' id='ca-{id}' to='{to}'>"
            "<jingle xmlns='urn:xmpp:jingle:1' action='content-accept' sid='{sid}' "
            "responder='{jid}'>"
            "<content creator='responder' name='0' senders='both'>"
            "<description xmlns='urn:xmpp:jingle:apps:rtp:1' media='audio'>"
            "<payload-type id='111' name='opus' clockrate='48000' channels='2'/>"
            "<rtcp-mux/>"
            "</description>"
            "<transport xmlns='urn:xmpp:jingle:transports:ice-udp:1' ufrag='{ufrag}' pwd='{pwd}'>"
            "<fingerprint xmlns='urn:xmpp:jingle:apps:dtls:0' hash='sha-256' setup='active'>{fp}</fingerprint>"
            "</transport>"
            "</content>"
            "<jingle-participant xmlns='{{https://movim.eu}}jingle_participant' participant='{participant}'/>"
            "</jingle></iq>"
        ).format(
            id=uuid.uuid4(),
            to=esc(to),
            sid=esc(sid),
            jid=esc(self.jid),
            ufrag=secrets.token_hex(4),
            pwd=secrets.token_hex(12),
            fp=fingerprint(),
            participant=esc(participant),
        ))

    def mute(self, to, sid, name="camera"):
        self.send((
            "<iq type='set' id='mute-{id}' to='{to}'>"
            "<jingle xmlns='urn:xmpp:jingle:1' action='session-info' sid='{sid}'>"
            "<mute xmlns='urn:xmpp:jingle:apps:rtp:info:1' name='{name}'/>"
            "</jingle></iq>"
        ).format(id=uuid.uuid4(), to=esc(to), sid=esc(sid), name=esc(name)))

    def unmute(self, to, sid, name="camera"):
        self.send((
            "<iq type='set' id='unmute-{id}' to='{to}'>"
            "<jingle xmlns='urn:xmpp:jingle:1' action='session-info' sid='{sid}'>"
            "<unmute xmlns='urn:xmpp:jingle:apps:rtp:info:1' name='{name}'/>"
            "</jingle></iq>"
        ).format(id=uuid.uuid4(), to=esc(to), sid=esc(sid), name=esc(name)))

    def terminate(self, to, sid):
        self.send((
            "<iq type='set' id='term-{id}' to='{to}'>"
            "<jingle xmlns='urn:xmpp:jingle:1' action='session-terminate' sid='{sid}'>"
            "<reason><success/></reason></jingle></iq>"
        ).format(id=uuid.uuid4(), to=esc(to), sid=esc(sid)))


def log(title, text):
    if not text.strip():
        return
    print("\n### {}".format(title))
    print(text.strip(), flush=True)


def main():
    alice = Client("alice", "alicecap")
    alice.login()
    alice.join()
    print("Room:", ROOM, flush=True)
    # Own the room first: the component must be admin before it joins.
    alice.set_sfu_admin()
    time.sleep(0.5)
    alice.read_for(1.0)
    alice.invite_sfu()
    sfu = alice.sfu_jid()
    if not sfu:
        print("buffer:", alice.buffer, flush=True)
        raise SystemExit("SFU did not join the room")
    print("SFU virtual JID:", sfu, flush=True)

    alice.disco_info(sfu)
    log("disco#info response", alice.read_for(2.0))

    sid = uuid.uuid4().hex
    alice.propose(sfu, sid)
    log("propose reply", alice.read_for(2.0))

    alice.session_initiate(sfu, sid)
    log("after session-initiate", alice.read_for(6.0))

    # Second participant makes Galener emit a content-add for a remote stream.
    bob = Client("bob", "bobcap")
    bob.login()
    bob.join()
    time.sleep(2.0)
    bob_sid = uuid.uuid4().hex
    bob.propose(sfu, bob_sid)
    time.sleep(1.0)
    bob.session_initiate(sfu, bob_sid)
    log("alice: content-add for bob", alice.read_for(8.0))

    for match in re.finditer(r"participant='([^']+)'", alice.buffer):
        alice.content_accept(sfu, sid, match.group(1))
    log("after content-accept", alice.read_for(3.0))

    alice.mute(sfu, sid)
    alice.unmute(sfu, sid)
    log("after mute/unmute", alice.read_for(3.0))

    bob.terminate(sfu, bob_sid)
    log("alice: content-remove for bob", alice.read_for(5.0))

    alice.terminate(sfu, sid)
    log("after session-terminate", alice.read_for(3.0))

    capture = "".join(alice.received)
    with open(OUTPUT, "w", encoding="utf-8") as handle:
        handle.write(capture)
    print("\nCaptured {} bytes to {}".format(len(capture), OUTPUT), flush=True)


if __name__ == "__main__":
    main()
