#!/usr/bin/env python3
"""Generate a deterministic python-twomemo two-way fixture for the Rust tests.

The fixture is consumed by `crates/xmpp-core/src/omemo.rs`
(`decrypts_python_twomemo_two_way`) and is intentionally small:

  * Alice opens an OMEMO 2 session with a KEX (M0).
  * Bob decrypts it and replies (M1, Bob -> Alice).
  * Bob sends an empty key-transport message (M2, Bob -> Alice).
  * Alice decrypts both, which forces a DH ratchet step on M1.

Every key, nonce and ratchet private key is derived from a fixed seed, so the
output is stable. Run it with the interop virtualenv:

    /tmp/omemo-venv/bin/python dev/omemo-interop/gen_twomemo_fixture.py
"""

from __future__ import annotations

import asyncio
import hashlib
import json
from pathlib import Path

import xeddsa
from doubleratchet import diffie_hellman_ratchet as dhr_mod
from doubleratchet.recommended import (
    HashFunction,
    diffie_hellman_ratchet_curve25519,
)
from x3dh import base_state as base_state_module
from x3dh.base_state import BaseState
from x3dh.crypto_provider import HashFunction as X3dhHash
from x3dh.identity_key_pair import IdentityKeyPairSeed
from x3dh.pre_key_pair import PreKeyPair
from x3dh.signed_pre_key_pair import SignedPreKeyPair
from x3dh.types import IdentityKeyFormat

from twomemo.twomemo import (
    AEADImpl,
    DoubleRatchetImpl,
    MessageChainKDFImpl,
    OMEMOAuthenticatedMessage,
    OMEMOKeyExchange,
    RootChainKDFImpl,
)

OUT = (
    Path(__file__).resolve().parent.parent.parent
    / "crates"
    / "xmpp-core"
    / "tests"
    / "fixtures"
    / "omemo_twomemo_two_way.json"
)


def det(seed: bytes, label: str, length: int) -> bytes:
    out = b""
    counter = 0
    while len(out) < length:
        out += hashlib.sha512(
            seed + label.encode() + counter.to_bytes(4, "big")
        ).digest()
        counter += 1
    return out[:length]


class TwomemoState(BaseState):
    @staticmethod
    def _encode_public_key(_, pub):  # noqa: ANN001 - matches the library hook
        return pub


def make_state(seed: bytes, num_opks: int) -> TwomemoState:
    ik_seed = det(seed, "ik-seed", 32)
    ik_priv = xeddsa.seed_to_priv(ik_seed)
    spk_priv = det(seed, "spk-priv", 32)
    spk_nonce = det(seed, "spk-nonce", 64)
    spk_pub = xeddsa.priv_to_curve25519_pub(spk_priv)
    spk_sig = xeddsa.ed25519_priv_sign(ik_priv, spk_pub, spk_nonce)
    spk = SignedPreKeyPair(priv=spk_priv, sig=spk_sig, timestamp=1234567890)

    state = TwomemoState()
    state._BaseState__identity_key_format = IdentityKeyFormat.ED_25519  # noqa: SLF001
    state._BaseState__hash_function = X3dhHash.SHA_256  # noqa: SLF001
    state._BaseState__info = b"OMEMO X3DH"  # noqa: SLF001
    state._BaseState__identity_key = IdentityKeyPairSeed(ik_seed)  # noqa: SLF001
    state._BaseState__signed_pre_key = spk  # noqa: SLF001
    state._BaseState__old_signed_pre_key = None  # noqa: SLF001
    state._BaseState__pre_keys = {  # noqa: SLF001
        PreKeyPair(priv=det(seed, f"opk-{i}", 32)) for i in range(num_opks)
    }
    state._BaseState__hidden_pre_keys = set()  # noqa: SLF001
    return state


def make_dr_class(priv_queue: list[bytes]):
    queue = list(priv_queue)

    class DR(diffie_hellman_ratchet_curve25519.DiffieHellmanRatchet):
        @staticmethod
        def _generate_priv() -> bytes:
            return queue.pop(0)

    return DR


async def main() -> None:
    seed = det(b"twomemo-two-way-fixture", "scenario", 32)

    alice = make_state(det(seed, "alice", 32), num_opks=0)
    bob = make_state(det(seed, "bob", 32), num_opks=3)
    bob_bundle = bob.bundle
    chosen_opk = sorted(bob_bundle.pre_keys, key=lambda b: b.hex())[0]

    ek_priv = det(seed, "ek-priv", 32)
    old_token = base_state_module.secrets.token_bytes
    old_choice = base_state_module.secrets.choice
    base_state_module.secrets.token_bytes = (
        lambda n: ek_priv if n == 32 else old_token(n)
    )
    base_state_module.secrets.choice = (
        lambda seq: chosen_opk if chosen_opk in seq else old_choice(seq)
    )
    try:
        ss_a, ad_a, x3dh_header = await alice.get_shared_secret_active(
            bob_bundle, b""
        )
        ss_b, ad_b, used_spk = await bob.get_shared_secret_passive(
            x3dh_header, b""
        )
    finally:
        base_state_module.secrets.token_bytes = old_token
        base_state_module.secrets.choice = old_choice
    assert ss_a == ss_b
    assert ad_a == ad_b

    alice_priv_queue = [
        det(seed, "alice-dr-1", 32),
        det(seed, "alice-dr-2", 32),
    ]
    bob_priv_queue = [det(seed, "bob-dr-1", 32)]

    AliceDR = make_dr_class(alice_priv_queue)
    alice_dr, m0 = await DoubleRatchetImpl.encrypt_initial_message(
        diffie_hellman_ratchet_class=AliceDR,
        root_chain_kdf=RootChainKDFImpl,
        message_chain_kdf=MessageChainKDFImpl,
        message_chain_constant=DoubleRatchetImpl.MESSAGE_CHAIN_CONSTANT,
        dos_protection_threshold=100,
        max_num_skipped_message_keys=1000,
        aead=AEADImpl,
        shared_secret=ss_a,
        recipient_ratchet_pub=bob_bundle.signed_pre_key,
        message=b"hello bob",
        associated_data=ad_a,
    )
    auth_m0 = OMEMOAuthenticatedMessage.FromString(m0.ciphertext)
    kex0 = OMEMOKeyExchange(
        pk_id=100,
        spk_id=1,
        ik=alice.bundle.identity_key,
        ek=x3dh_header.ephemeral_key,
        message=auth_m0,
    ).SerializeToString()

    BobDR = make_dr_class(bob_priv_queue)
    bob_dr, recovered = await DoubleRatchetImpl.decrypt_initial_message(
        diffie_hellman_ratchet_class=BobDR,
        root_chain_kdf=RootChainKDFImpl,
        message_chain_kdf=MessageChainKDFImpl,
        message_chain_constant=DoubleRatchetImpl.MESSAGE_CHAIN_CONSTANT,
        dos_protection_threshold=100,
        max_num_skipped_message_keys=1000,
        aead=AEADImpl,
        shared_secret=ss_b,
        own_ratchet_priv=used_spk.priv,
        message=m0,
        associated_data=ad_b,
    )
    assert recovered == b"hello bob"

    reply = await bob_dr.encrypt_message(b"pong from bob", ad_b)
    empty = await bob_dr.encrypt_message(b"", ad_b)

    assert await alice_dr.decrypt_message(reply, ad_a) == b"pong from bob"
    assert await alice_dr.decrypt_message(empty, ad_a) == b""

    fixture = {
        "source": "python-twomemo 2.1 + python-doubleratchet 1.3.0 + python-x3dh 1.3.0",
        "algorithm": "OMEMO 2 (twomemo) two-way",
        "shared_secret_hex": ss_a.hex(),
        "associated_data_hex": ad_a.hex(),
        "bob_spk_priv_hex": used_spk.priv.hex(),
        "bob_spk_pub_hex": bob_bundle.signed_pre_key.hex(),
        "alice_priv_queue_hex": [p.hex() for p in alice_priv_queue],
        "bob_priv_queue_hex": [p.hex() for p in bob_priv_queue],
        "kex0_hex": kex0.hex(),
        "kex0_plaintext_hex": b"hello bob".hex(),
        "messages": [
            {
                "label": "M1 bob -> alice (first reply, forces Alice DH step)",
                "plaintext_hex": b"pong from bob".hex(),
                "ciphertext_hex": reply.ciphertext.hex(),
                "ratchet_pub_hex": reply.header.ratchet_pub.hex(),
                "n": reply.header.sending_chain_length,
                "pn": reply.header.previous_sending_chain_length,
            },
            {
                "label": "M2 bob -> alice (empty key transport)",
                "plaintext_hex": b"".hex(),
                "ciphertext_hex": empty.ciphertext.hex(),
                "ratchet_pub_hex": empty.header.ratchet_pub.hex(),
                "n": empty.header.sending_chain_length,
                "pn": empty.header.previous_sending_chain_length,
            },
        ],
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(fixture, indent=2) + "\n")
    print(f"wrote {OUT}")


if __name__ == "__main__":
    asyncio.run(main())
