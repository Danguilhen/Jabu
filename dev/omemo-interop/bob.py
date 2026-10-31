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

import slixmpp
from omemo.storage import Just, Nothing, Storage
from slixmpp import JID
from slixmpp.xmlstream.handler import Callback
from slixmpp.xmlstream.matcher import MatchXPath
from slixmpp_omemo import TrustLevel, XEP_0384


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
    def __init__(self, jid: str, password: str, room: str | None) -> None:
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
        self.register_plugin("xep_0030")
        self.register_plugin("xep_0045")
        self.register_plugin("xep_0060")
        self.register_plugin("xep_0163")
        self.register_plugin("xep_0384", module=types.SimpleNamespace(xep_0384=RustInteropOmemo))
        self.add_event_handler("omemo_initialized", self._omemo_ready)
        self.add_event_handler("session_start", self._session_start)
        self.add_event_handler("groupchat_message", self._message)
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
        print("READY", flush=True)

    async def _session_start(self, _event) -> None:
        print("SESSION_START", flush=True)
        self.send_presence()
        await self.get_roster()
        if self.room:
            self.plugin["xep_0045"].join_muc(self.room, "bob")

    async def _message(self, msg) -> None:
        if msg["type"] not in ("chat", "groupchat"):
            return
        if msg["body"]:
            print(f"PLAIN {msg['type']} {msg['from']} {msg['body']}", flush=True)
        if not self.plugin["xep_0384"].is_encrypted(msg):
            return
        try:
            decrypted, sender = await self.plugin["xep_0384"].decrypt_message(msg)
        except Exception as error:  # noqa: BLE001 - test helper
            print(f"DECRYPT_ERROR {error!r}", flush=True)
            print(traceback.format_exc(), flush=True)
            return
        body = decrypted["body"]
        print(f"DECRYPTED {sender.bare_jid} {body}", flush=True)
        if not body or not body.startswith("ping"):
            return

        if msg["type"] == "groupchat":
            room = JID(msg["from"].bare)
            recipients = set()
            for nick in self.plugin["xep_0045"].get_roster(room):
                real = self.plugin["xep_0045"].get_jid_property(room, nick, "jid")
                if real:
                    recipients.add(JID(real).bare)
            recipients.add(JID(self.boundjid.bare))
            to = room
        else:
            recipients = JID(msg["from"].bare)
            to = msg["from"]

        reply = self.make_message(mto=to, mbody="pong from slixmpp-omemo")
        reply["type"] = msg["type"]
        reply["id"] = self.new_id()
        encrypted, errors = await self.plugin["xep_0384"].encrypt_message(
            reply, recipients
        )
        if errors:
            print(f"ENCRYPT_WARNINGS {errors!r}", flush=True)
        if encrypted is not None:
            encrypted.send()
            print("SENT_REPLY", flush=True)


async def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--jid", default="bob@localhost")
    parser.add_argument("--password", default="jabu-test")
    parser.add_argument("--room", default=None)
    parser.add_argument("--state", default="/tmp/omemo-interop-bob.json")
    parser.add_argument("--seconds", type=int, default=60)
    args = parser.parse_args()

    RustInteropOmemo.storage_path = Path(args.state)
    if os.environ.get("BOB_DEBUG"):
        logging.basicConfig(level=logging.DEBUG)
    xmpp = Bob(args.jid, args.password, args.room)
    xmpp.connect(host="127.0.0.1", port=5222)
    await asyncio.sleep(args.seconds)
    xmpp.disconnect()


if __name__ == "__main__":
    asyncio.run(main())
