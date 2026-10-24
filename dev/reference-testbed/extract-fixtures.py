#!/usr/bin/env python3
"""Turn a capture-stanzas.py transcript into named golden fixtures."""

import hashlib
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_CAPTURE = os.path.join(HERE, "capture.galener.xml")
DEFAULT_OUT = os.path.join(
    HERE, "..", "..", "crates", "xmpp-core", "tests", "golden", "galener"
)

NORMALIZE = {
    "alicecap": "desktop",
    "bobcap": "mobile",
}


def split_stanzas(text):
    stanzas = []
    i = 0
    n = len(text)
    while i < n:
        start = text.find("<", i)
        if start == -1:
            break
        if text.startswith("<?", start) or text.startswith("<!", start):
            end = text.find(">", start)
            i = end + 1 if end != -1 else n
            continue

        depth = 0
        j = start
        while j < n:
            lt = text.find("<", j)
            if lt == -1:
                return stanzas
            if text.startswith("<?", lt) or text.startswith("<!", lt):
                gt = text.find(">", lt)
                j = gt + 1 if gt != -1 else n
                continue
            gt = text.find(">", lt)
            if gt == -1:
                return stanzas
            tag = text[lt + 1:gt]
            closing = tag.startswith("/")
            self_closing = tag.endswith("/")
            depth += -1 if closing else (0 if self_closing else 1)
            j = gt + 1
            if depth == 0:
                stanzas.append(text[start:j])
                i = j
                break
        else:
            break
    return stanzas


def normalize(stanza):
    room = re.search(r"([a-z0-9-]+)@conference\.localhost", stanza)
    if room:
        stanza = stanza.replace(room.group(0), "voice@conference.localhost")

    fixed = hashlib.sha1(b"voice@conference.localhost").hexdigest()
    stanza = re.sub(r"[0-9a-f]{40}@sfu\.example\.org", fixed + "@sfu.example.org", stanza)
    for old, new in NORMALIZE.items():
        stanza = stanza.replace(old, new)
    return stanza


def section(text, title):
    marker = "### " + title + "\n"
    start = text.find(marker)
    if start == -1:
        return ""
    start += len(marker)
    end = text.find("\n### ", start)
    return text[start:] if end == -1 else text[start:end]


def first(stanzas, needle):
    for stanza in stanzas:
        if needle in stanza:
            return stanza
    return None


def write(path, stanza):
    if stanza is None:
        print("missing", path, file=sys.stderr)
        return
    stanza = normalize(stanza)
    if not re.match(r"<(iq|message|presence)[^>]*xmlns=", stanza):
        stanza = re.sub(r"^<(iq|message|presence)", r"<\1 xmlns='jabber:client'", stanza, count=1)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(stanza + "\n")
    print("wrote", path)


def main():
    capture_path = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_CAPTURE
    out = sys.argv[2] if len(sys.argv) > 2 else DEFAULT_OUT
    text = open(capture_path, encoding="utf-8").read()
    os.makedirs(out, exist_ok=True)

    write(os.path.join(out, "disco_info.xml"),
          first(split_stanzas(section(text, "disco#info response")), "disco#info"))
    write(os.path.join(out, "proceed.xml"),
          first(split_stanzas(section(text, "propose reply")), "<proceed "))
    coin_text = section(text, "alice: content-add for bob")
    if "conference-info" not in coin_text:
        coin_text = section(text, "propose reply")
    write(os.path.join(out, "coin_presence.xml"),
          first(split_stanzas(coin_text), "conference-info"))

    after = split_stanzas(section(text, "after session-initiate"))
    write(os.path.join(out, "session_accept.xml"),
          first(after, "action='session-accept'"))
    write(os.path.join(out, "transport_info.xml"),
          first(after, "action='transport-info'"))


if __name__ == "__main__":
    main()
