#!/usr/bin/env python3
"""Minimal slixmpp OMEMO peer for interop testing the Rust client.

Run with the dependencies from dev/omemo-interop/requirements.txt, for example:

    python3 -m venv /tmp/omemo-venv
    /tmp/omemo-venv/bin/pip install -r dev/omemo-interop/requirements.txt
    /tmp/omemo-venv/bin/python dev/omemo-interop/bob.py --room room@conference.localhost
"""

from __future__ import annotations

import argparse
import asyncio
import copy
import json
import logging
import os
import ssl
import sys
import traceback
import types
from pathlib import Path
from typing import Any, Dict
from xml.etree import ElementTree

import slixmpp
import twomemo
import twomemo.etree
import twomemo.twomemo
from omemo.storage import Just, Nothing, Storage
from slixmpp import JID
from slixmpp.xmlstream.handler import Callback
from slixmpp.xmlstream.matcher import MatchXPath
from slixmpp_omemo import TrustLevel, XEP_0384

SCE_NS = "urn:xmpp:sce:1"
TWOMEMO_NS = twomemo.twomemo.NAMESPACE


def sce_body(plaintext: bytes) -> str:
    """Extract the body from a XEP-0420 SCE envelope."""
    try:
        root = ElementTree.fromstring(plaintext)
    except ElementTree.ParseError:
        return plaintext.decode("utf-8")
    if root.tag != f"{{{SCE_NS}}}envelope":
        return plaintext.decode("utf-8")
    body = root.find(f".//{{jabber:client}}body")
    if body is None:
        body = root.find(f".//{{{SCE_NS}}}body")
    return body.text or "" if body is not None else ""


def sce_envelope(body: str, to: str) -> bytes:
    """Build the XEP-0420 SCE envelope sent by the Python peer."""
    envelope = ElementTree.Element(f"{{{SCE_NS}}}envelope")
    content = ElementTree.SubElement(envelope, f"{{{SCE_NS}}}content")
    body_elt = ElementTree.SubElement(content, "{jabber:client}body")
    body_elt.text = body
    ElementTree.SubElement(envelope, f"{{{SCE_NS}}}rpad").text = "~"
    ElementTree.SubElement(envelope, f"{{{SCE_NS}}}to", {"jid": to})
    return ElementTree.tostring(envelope, encoding="utf-8")


class FileStorage(Storage):
    def __init__(self, path: Path) -> None:
        super().__init__()
        self.path = path
        if path.exists():
            self.data: Dict[str, Any] = json.loads(path.read_text())
        else:
            self.data = {}

    def _save(self) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.path.write_text(json.dumps(self.data))

    async def _load(self, key: str):
        if key in self.data:
            return Just(self.data[key])
        return Nothing()

    async def _store(self, key: str, value: Any) -> None:
        self.data[key] = copy.deepcopy(value)
        self._save()

    async def _delete(self, key: str) -> None:
        self.data.pop(key, None)
        self._save()


class RustInteropOmemo(XEP_0384):
    storage_path = Path("/tmp/omemo-interop-bob.json")

    @property
    def storage(self) -> Storage:
        return FileStorage(self.storage_path)

    @property
    def _btbv_enabled(self) -> bool:
        return False

    async def _prompt_manual_trust(self, manually_trusted, identifier):
        session_manager = await self.get_session_manager()
        for device in manually_trusted:
            await session_manager.set_trust(
                device.bare_jid,
                device.identity_key,
                TrustLevel.TRUSTED.value,
            )


class Bob(slixmpp.ClientXMPP):
    def __init__(
        self,
        jid: str,
        password: str,
        room: str | None,
        legacy_only: bool = False,
    ) -> None:
        context = ssl.create_default_context()
        context.check_hostname = False
        context.verify_mode = ssl.CERT_NONE
        super().__init__(
            jid,
            password,
            plugin_config={"use_message_ids": True},
            ssl_context=context,
        )
        self.room = room
        self.legacy_only = legacy_only
        self.register_plugin("xep_0030")
        self.register_plugin("xep_0045")
        self.register_plugin("xep_0060")
        self.register_plugin("xep_0163")
        self.register_plugin("xep_0384", module=types.SimpleNamespace(xep_0384=RustInteropOmemo))
        self.add_event_handler("omemo_initialized", self._omemo_ready)
        self.add_event_handler("session_start", self._session_start)
        self.register_handler(
            Callback(
                "RustInteropMessage",
                MatchXPath("{jabber:client}message"),
                self._message_callback,
            )
        )

    def _message_callback(self, msg) -> None:
        asyncio.create_task(self._message(msg))

    async def _omemo_ready(self, _event=None) -> None:
        if self.legacy_only:
            try:
                await self["xep_0060"].publish(
                    JID(self.boundjid.bare),
                    "urn:xmpp:omemo:2:devices",
                    id="current",
                    payload=ElementTree.Element("{urn:xmpp:omemo:2}devices"),
                )
                print("LEGACY_ONLY", flush=True)
            except Exception as error:  # noqa: BLE001 - test helper
                print(f"LEGACY_ONLY_ERROR {error!r}", flush=True)
        print("READY", flush=True)

    async def _session_start(self, _event) -> None:
        print("SESSION_START", flush=True)
        self.send_presence()
        await self.get_roster()
        if self.room:
            self.plugin["xep_0045"].join_muc(self.room, "bob")

    async def _decrypt_twomemo(self, msg):
        """slixmpp-omemo 2.2.0 decrypts the OMEMO 2 ratchet but does not
        implement the XEP-0420 SCE envelope. Decrypt at the session-manager
        level and unwrap the envelope here so the rest of the harness can test
        the ratchet and payload layers end to end."""
        plugin = self.plugin["xep_0384"]
        session_manager = await plugin.get_session_manager()
        if msg["type"] == "groupchat":
            room = JID(msg["from"]).bare
            nick = JID(msg["from"]).resource
            real = self.plugin["xep_0045"].get_jid_property(JID(room), nick, "jid")
            if real is None:
                raise ValueError(f"no real JID for {msg['from']}")
            sender = JID(real).bare
        else:
            sender = JID(msg["from"]).bare
        element = msg.xml.find(f"{{{TWOMEMO_NS}}}encrypted")
        message = twomemo.etree.parse_message(element, sender)
        plaintext, device_info, _ = await session_manager.decrypt(message)
        return sce_body(plaintext or b""), device_info

    async def _send_twomemo_reply(self, msg, body, recipients, to) -> None:
        plugin = self.plugin["xep_0384"]
        session_manager = await plugin.get_session_manager()
        await plugin.refresh_device_lists(recipients)
        plaintext = sce_envelope(body, JID(to).bare)
        messages, errors = await session_manager.encrypt(
            frozenset(recipient.bare for recipient in recipients),
            {TWOMEMO_NS: plaintext},
            backend_priority_order=[TWOMEMO_NS],
        )
        if errors:
            print(f"ENCRYPT_WARNINGS {errors!r}", flush=True)
        reply = self.make_message(mto=to, mbody=plugin.fallback_message)
        reply["type"] = msg["type"]
        reply["id"] = self.new_id()
        reply.enable("store")
        for message in messages:
            reply.append(twomemo.etree.serialize_message(message))
        reply.send()
        print("SENT_REPLY", flush=True)

    async def _send_legacy_reply(self, msg, body, recipients, to) -> None:
        plugin = self.plugin["xep_0384"]
        reply = self.make_message(mto=to, mbody=body)
        reply["type"] = msg["type"]
        reply["id"] = self.new_id()
        encrypted, errors = await plugin.encrypt_message(reply, recipients)
        if errors:
            print(f"ENCRYPT_WARNINGS {errors!r}", flush=True)
        if encrypted is not None:
            encrypted.send()
            print("SENT_REPLY", flush=True)

    async def _message(self, msg) -> None:
        if msg["type"] not in ("chat", "groupchat"):
            return
        if msg["body"]:
            print(f"PLAIN {msg['type']} {msg['from']} {msg['body']}", flush=True)
        if not self.plugin["xep_0384"].is_encrypted(msg):
            return
        try:
            if msg.xml.find(f"{{{TWOMEMO_NS}}}encrypted") is not None:
                body, sender = await self._decrypt_twomemo(msg)
            else:
                decrypted, sender = await self.plugin["xep_0384"].decrypt_message(msg)
                body = decrypted["body"]
        except Exception as error:  # noqa: BLE001 - test helper
            print(f"DECRYPT_ERROR {error!r}", flush=True)
            print(traceback.format_exc(), flush=True)
            return
        print(f"DECRYPTED {sender.bare_jid} {body}", flush=True)
        if not body or not body.startswith("ping"):
            return

        if msg["type"] == "groupchat":
            room = JID(msg["from"].bare)
            recipients = set()
            for nick in self.plugin["xep_0045"].get_roster(room):
                real = self.plugin["xep_0045"].get_jid_property(room, nick, "jid")
                if real:
                    recipients.add(JID(real))
            recipients.add(JID(self.boundjid.bare))
            to = room
        else:
            recipients = {JID(msg["from"])}
            to = msg["from"]

        if msg.xml.find(f"{{{TWOMEMO_NS}}}encrypted") is not None:
            await self._send_twomemo_reply(
                msg, "pong from slixmpp-omemo", recipients, to
            )
        else:
            await self._send_legacy_reply(
                msg, "pong from slixmpp-omemo", recipients, to
            )


async def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--jid", default="bob@localhost")
    parser.add_argument("--password", default="jabu-test")
    parser.add_argument("--room", default=None)
    parser.add_argument("--legacy-only", action="store_true")
    parser.add_argument("--state", default="/tmp/omemo-interop-bob.json")
    parser.add_argument("--seconds", type=int, default=60)
    args = parser.parse_args()

    RustInteropOmemo.storage_path = Path(args.state)
    if os.environ.get("BOB_DEBUG"):
        logging.basicConfig(level=logging.DEBUG)
    xmpp = Bob(args.jid, args.password, args.room, args.legacy_only)
    xmpp.connect(host="127.0.0.1", port=5222)
    await asyncio.sleep(args.seconds)
    xmpp.disconnect()


if __name__ == "__main__":
    asyncio.run(main())
