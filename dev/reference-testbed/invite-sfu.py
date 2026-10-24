import base64
import socket
import time
import uuid

import os

HOST = os.environ.get("XMPP_HOST", "127.0.0.1")
PORT = int(os.environ.get("XMPP_PORT", "5222"))
USER = os.environ.get("XMPP_USER", "alice")
PASS = os.environ.get("XMPP_PASS", "jabu-test")
ROOM = os.environ.get("ROOM", "voice@conference.localhost")
NICK = os.environ.get("ROOM_NICK", USER)
SFU = os.environ.get("SFU", "sfu.example.org")


class XMPP:
    def __init__(self):
        self.s = socket.create_connection((HOST, PORT))
        self.s.settimeout(1)
        self.buf = ""

    def send(self, data):
        self.s.sendall(data.encode())

    def read(self, seconds=2.0):
        end = time.time() + seconds
        out = ""
        while time.time() < end:
            try:
                chunk = self.s.recv(65536)
            except socket.timeout:
                continue
            if not chunk:
                break
            out += chunk.decode(errors="replace")
        self.buf += out
        return out

    def stream(self):
        self.send(
            "<?xml version='1.0'?><stream:stream to='localhost' xmlns='jabber:client' "
            "xmlns:stream='http://etherx.jabber.org/streams' version='1.0'>"
        )
        return self.read(1.5)

    def login(self):
        self.stream()
        auth = base64.b64encode(b"\x00" + USER.encode() + b"\x00" + PASS.encode()).decode()
        self.send(
            f"<auth xmlns='urn:ietf:params:xml:ns:xmpp-sasl' mechanism='PLAIN'>{auth}</auth>"
        )
        time.sleep(0.4)
        self.read(0.5)
        self.stream()
        self.send(
            "<iq type='set' id='bind'><bind xmlns='urn:ietf:params:xml:ns:xmpp-bind'>"
            "<resource>testvoice</resource></bind></iq>"
        )
        time.sleep(0.4)
        print(self.read(1.0))
        self.send(
            "<iq type='set' id='sess'><session xmlns='urn:ietf:params:xml:ns:xmpp-session'/></iq>"
        )
        time.sleep(0.3)
        self.read(0.5)
        self.send("<presence/>")
        time.sleep(0.3)
        self.read(0.5)


x = XMPP()
x.login()
x.send(
    f"<presence to='{ROOM}/{NICK}'><x xmlns='http://jabber.org/protocol/muc'/></presence>"
)
print(x.read(2.0))

# Instant room config, if the room is locked.
x.send(
    f"<iq type='set' id='cfg' to='{ROOM}'><query xmlns='http://jabber.org/protocol/muc#owner'>"
    "<x xmlns='jabber:x:data' type='submit'/></query></iq>"
)
time.sleep(0.5)
print(x.read(1.0))

x.send(
    f"<message to='{ROOM}' id='inv-{uuid.uuid4()}'>"
    "<x xmlns='http://jabber.org/protocol/muc#user'>"
    f"<invite to='{SFU}'><reason>Jabu voice test</reason></invite>"
    "</x></message>"
)
print("invite sent")

end = time.time() + 30
while time.time() < end:
    data = x.read(1.0)
    if data:
        print(data, flush=True)
