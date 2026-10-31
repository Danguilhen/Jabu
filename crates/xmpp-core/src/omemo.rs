//! XEP-0384 OMEMO: device lists, bundles, trust and encrypted envelopes.
//!
//! The wire format, key bundles and payload encryption live here. Per-device
//! Signal sessions are layered on top by a session backend.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use minidom::{Element, NSChoice};
use parking_lot::RwLock;
use prost::Message as _;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use x25519_dalek::{PublicKey, StaticSecret};

use omemo_doubleratchet::dh_ratchet::{
    DhPrivProvider, DhRatchetSnapshot, DiffieHellmanRatchet, Header as DrHeader,
    OsRngDhPrivProvider,
};
use omemo_oldmemo::{
    build_key_exchange as build_legacy_key_exchange,
    parse_key_exchange as parse_legacy_key_exchange, peek_dh_pub as peek_legacy_dh_pub,
    OldmemoSession, OldmemoSessionSnapshot,
};
use omemo_stanza::axolotl_aead::{open_payload as open_legacy_payload, seal_payload};
use omemo_stanza::axolotl_stanza::{
    Bundle as LegacyStanzaBundle, Encrypted as LegacyStanzaEncrypted, KeyEntry as LegacyStanzaKey,
};
use omemo_twomemo::{
    aead_decrypt, aead_encrypt, build_associated_data, build_key_exchange, parse_key_exchange,
    peek_dh_pub, MsgKdf, OmemoAuthenticatedMessage, OmemoMessage, RootKdf, SessionRole,
    TwomemoSessionSnapshot, DEFAULT_DOS_THRESHOLD, DEFAULT_MAX_SKIPPED, MESSAGE_CHAIN_CONSTANT,
};
use omemo_x3dh::{
    get_shared_secret_active, get_shared_secret_active_oldmemo, get_shared_secret_passive,
    get_shared_secret_passive_oldmemo, verify_bundle_oldmemo, Bundle as X3dhBundle,
    Header as X3dhHeader, IdentityKeyPair, PreKeyPair as X3dhPreKeyPair,
    SignedPreKeyPair as X3dhSignedPreKeyPair, X3dhState,
};

use crate::{Result, XmppError};

pub const NS_OMEMO: &str = "urn:xmpp:omemo:2";
pub const NS_DEVICES: &str = "urn:xmpp:omemo:2:devices";
pub const NS_BUNDLES: &str = "urn:xmpp:omemo:2:bundles";
pub const NS_TRUST: &str = "urn:xmpp:omemo:2:trust";

/// Legacy OMEMO 0.3 (XEP-0384 v0.3) namespaces.
pub const NS_LEGACY_OMEMO: &str = "eu.siacs.conversations.axolotl";
pub const NS_LEGACY_DEVICES: &str = "eu.siacs.conversations.axolotl.devicelist";
pub const NS_LEGACY_BUNDLES_PREFIX: &str = "eu.siacs.conversations.axolotl.bundles:";

/// Identity trust for a peer device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustLevel {
    Undecided,
    Trusted,
    Untrusted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreKeyPublic {
    pub id: u32,
    pub key: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceBundle {
    pub device_id: u32,
    pub identity_key: Vec<u8>,
    pub signed_prekey_id: u32,
    pub signed_prekey: Vec<u8>,
    pub signed_prekey_signature: Vec<u8>,
    pub prekeys: Vec<PreKeyPublic>,
}

impl DeviceBundle {
    pub fn to_element(&self) -> Element {
        let mut prekeys = Element::builder("prekeys", NS_OMEMO);
        for pk in &self.prekeys {
            prekeys = prekeys.append(
                Element::builder("pk", NS_OMEMO)
                    .attr("id", pk.id.to_string())
                    .append(STANDARD.encode(&pk.key)),
            );
        }
        Element::builder("bundle", NS_OMEMO)
            .append(
                Element::builder("spk", NS_OMEMO)
                    .attr("id", self.signed_prekey_id.to_string())
                    .append(STANDARD.encode(&self.signed_prekey)),
            )
            .append(
                Element::builder("spks", NS_OMEMO)
                    .append(STANDARD.encode(&self.signed_prekey_signature)),
            )
            .append(Element::builder("ik", NS_OMEMO).append(STANDARD.encode(&self.identity_key)))
            .append(prekeys.build())
            .build()
    }

    pub fn from_element(bundle: &Element, device_id: u32) -> Option<Self> {
        let identity_key = decode_child(bundle, "ik")?;
        let spk = bundle.get_child("spk", NSChoice::Any)?;
        let signed_prekey_id = spk.attr("id")?.parse().ok()?;
        let signed_prekey = STANDARD.decode(spk.text().trim()).ok()?;
        let signed_prekey_signature = decode_child(bundle, "spks")?;
        let prekeys = bundle
            .get_child("prekeys", NSChoice::Any)
            .map(|list| {
                list.children()
                    .filter(|c| c.name() == "pk")
                    .filter_map(|pk| {
                        Some(PreKeyPublic {
                            id: pk.attr("id")?.parse().ok()?,
                            key: STANDARD.decode(pk.text().trim()).ok()?,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Some(Self {
            device_id,
            identity_key,
            signed_prekey_id,
            signed_prekey,
            signed_prekey_signature,
            prekeys,
        })
    }

    /// Human-readable SHA-256 fingerprint of the identity key.
    pub fn fingerprint(&self) -> String {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(&self.identity_key);
        let hex: String = digest.iter().map(|b| format!("{:02x}", b)).collect();
        hex.as_bytes()
            .chunks(8)
            .map(|c| String::from_utf8_lossy(c).to_string())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Verify the signed prekey against the bundle's identity key (Ed25519).
    pub fn verify(&self) -> bool {
        let Ok(spk) = <[u8; 32]>::try_from(self.signed_prekey.as_slice()) else {
            return false;
        };
        let Ok(sig) = <[u8; 64]>::try_from(self.signed_prekey_signature.as_slice()) else {
            return false;
        };
        let Ok(identity) = <[u8; 32]>::try_from(self.identity_key.as_slice()) else {
            return false;
        };
        omemo_xeddsa::ed25519_verify(&sig, &identity, &spk)
    }
}

/// Legacy OMEMO 0.3 bundle (`eu.siacs.conversations.axolotl`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegacyDeviceBundle {
    pub device_id: u32,
    /// Ed25519 identity key, recovered from the wire sign-bit stuffing.
    pub identity_key: Vec<u8>,
    pub signed_prekey_id: u32,
    pub signed_prekey: Vec<u8>,
    pub signed_prekey_signature: Vec<u8>,
    pub prekeys: Vec<PreKeyPublic>,
}

impl LegacyDeviceBundle {
    /// # Errors
    ///
    /// Returns an error when a key has the wrong length or the bundle cannot
    /// be encoded.
    pub fn to_element(&self) -> Result<Element> {
        let bundle = LegacyStanzaBundle {
            signed_prekey_id: self.signed_prekey_id,
            signed_prekey_pub: array32(&self.signed_prekey)?,
            signed_prekey_sig: <[u8; 64]>::try_from(self.signed_prekey_signature.as_slice())
                .map_err(|_| {
                    XmppError::CryptoError("invalid legacy signed prekey signature length".into())
                })?,
            identity_key_ed: array32(&self.identity_key)?,
            prekeys: self
                .prekeys
                .iter()
                .map(|pk| {
                    Ok(omemo_stanza::axolotl_stanza::PreKey {
                        id: pk.id,
                        pub_key: array32(&pk.key)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        };
        let xml = bundle
            .encode()
            .map_err(|e| XmppError::CryptoError(e.to_string()))?;
        Element::from_reader(xml.as_bytes())
            .map_err(|e| XmppError::DeserializationError(e.to_string()))
    }

    pub fn from_element(bundle: &Element, device_id: u32) -> Option<Self> {
        let parsed = LegacyStanzaBundle::parse(&crate::vcard::to_xml(bundle)).ok()?;
        Some(Self {
            device_id,
            identity_key: parsed.identity_key_ed.to_vec(),
            signed_prekey_id: parsed.signed_prekey_id,
            signed_prekey: parsed.signed_prekey_pub.to_vec(),
            signed_prekey_signature: parsed.signed_prekey_sig.to_vec(),
            prekeys: parsed
                .prekeys
                .iter()
                .map(|pk| PreKeyPublic {
                    id: pk.id,
                    key: pk.pub_key.to_vec(),
                })
                .collect(),
        })
    }

    pub fn verify(&self) -> bool {
        self.to_x3dh()
            .and_then(|bundle| {
                verify_bundle_oldmemo(&bundle).map_err(|e| XmppError::CryptoError(e.to_string()))
            })
            .is_ok()
    }

    fn to_x3dh(&self) -> Result<X3dhBundle> {
        Ok(X3dhBundle {
            identity_key: array32(&self.identity_key)?,
            signed_pre_key: array32(&self.signed_prekey)?,
            signed_pre_key_sig: <[u8; 64]>::try_from(self.signed_prekey_signature.as_slice())
                .map_err(|_| {
                    XmppError::CryptoError("invalid legacy signed prekey signature length".into())
                })?,
            pre_keys: self
                .prekeys
                .iter()
                .map(|pk| array32(&pk.key))
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

/// A one-time prekey with its private half, kept only locally.
#[derive(Debug, Clone)]
pub struct PreKeyPair {
    pub id: u32,
    pub private: [u8; 32],
    pub public: [u8; 32],
}

/// A rotated-out signed prekey kept for delayed messages.
#[derive(Debug, Clone)]
pub struct SignedPreKey {
    pub id: u32,
    pub private: [u8; 32],
    pub public: [u8; 32],
    pub signature: [u8; 64],
}

/// This device's OMEMO identity and unpublished prekeys.
#[derive(Debug, Clone)]
pub struct LocalDevice {
    pub device_id: u32,
    pub identity_private: [u8; 32],
    pub identity_public: [u8; 32],
    pub signed_prekey_id: u32,
    pub signed_prekey_private: [u8; 32],
    pub signed_prekey_public: [u8; 32],
    pub signed_prekey_signature: [u8; 64],
    pub signed_prekey_created_at: i64,
    pub old_signed_prekeys: Vec<SignedPreKey>,
    pub prekeys: Vec<PreKeyPair>,
    pub next_prekey_id: u32,
}

impl LocalDevice {
    /// Generate a fresh identity, signed prekey and `prekey_count` one-time prekeys.
    pub fn generate(device_id: u32, prekey_count: usize) -> Self {
        let identity_private = random_bytes::<32>();
        let identity_public = omemo_xeddsa::priv_to_ed25519_pub(&identity_private);

        let signed_prekey_private = random_bytes::<32>();
        let signed_prekey_secret = StaticSecret::from(signed_prekey_private);
        let signed_prekey_public = PublicKey::from(&signed_prekey_secret).to_bytes();
        let signature = sign_prekey(&identity_private, &signed_prekey_public);

        let mut prekeys = Vec::with_capacity(prekey_count);
        for i in 0..prekey_count {
            let private = random_bytes::<32>();
            let secret = StaticSecret::from(private);
            prekeys.push(PreKeyPair {
                id: 1 + i as u32,
                private,
                public: PublicKey::from(&secret).to_bytes(),
            });
        }

        Self {
            device_id,
            identity_private,
            identity_public,
            signed_prekey_id: 1,
            signed_prekey_private,
            signed_prekey_public,
            signed_prekey_signature: signature,
            signed_prekey_created_at: now_seconds(),
            old_signed_prekeys: Vec::new(),
            next_prekey_id: 1 + prekey_count as u32,
            prekeys,
        }
    }

    pub fn bundle(&self) -> DeviceBundle {
        DeviceBundle {
            device_id: self.device_id,
            identity_key: self.identity_public.to_vec(),
            signed_prekey_id: self.signed_prekey_id,
            signed_prekey: self.signed_prekey_public.to_vec(),
            signed_prekey_signature: self.signed_prekey_signature.to_vec(),
            prekeys: self
                .prekeys
                .iter()
                .map(|pk| PreKeyPublic {
                    id: pk.id,
                    key: pk.public.to_vec(),
                })
                .collect(),
        }
    }

    /// Legacy OMEMO 0.3 bundle for this device.
    ///
    /// The 0.3 signature covers the 33-byte `0x05 || spk` form, unlike the
    /// OMEMO 2 signature which covers the raw 32-byte key.
    pub fn legacy_bundle(&self) -> LegacyDeviceBundle {
        LegacyDeviceBundle {
            device_id: self.device_id,
            identity_key: self.identity_public.to_vec(),
            signed_prekey_id: self.signed_prekey_id,
            signed_prekey: self.signed_prekey_public.to_vec(),
            signed_prekey_signature: sign_prekey_oldmemo(
                &self.identity_private,
                &self.signed_prekey_public,
            )
            .to_vec(),
            prekeys: self
                .prekeys
                .iter()
                .map(|pk| PreKeyPublic {
                    id: pk.id,
                    key: pk.public.to_vec(),
                })
                .collect(),
        }
    }

    /// X3DH state for the current key set.
    #[must_use]
    pub fn x3dh_state(&self) -> X3dhState {
        X3dhState {
            identity_key: IdentityKeyPair::Priv(self.identity_private),
            signed_pre_key: X3dhSignedPreKeyPair {
                priv_key: self.signed_prekey_private,
                sig: self.signed_prekey_signature,
                timestamp: self.signed_prekey_created_at.max(0) as u64,
            },
            old_signed_pre_key: None,
            pre_keys: self
                .prekeys
                .iter()
                .map(|prekey| X3dhPreKeyPair {
                    priv_key: prekey.private,
                })
                .collect(),
        }
    }

    /// X3DH state with the signed prekey matching `id` as the current one.
    ///
    /// This lets delayed key exchanges reference a retained old signed prekey.
    #[must_use]
    pub fn x3dh_state_for_signed_prekey(&self, id: u32) -> Option<X3dhState> {
        if id == self.signed_prekey_id {
            return Some(self.x3dh_state());
        }
        let old = self
            .old_signed_prekeys
            .iter()
            .find(|prekey| prekey.id == id)?;
        let mut state = self.x3dh_state();
        state.signed_pre_key = X3dhSignedPreKeyPair {
            priv_key: old.private,
            sig: old.signature,
            timestamp: 0,
        };
        Some(state)
    }

    #[must_use]
    pub fn signed_prekey_public(&self, id: u32) -> Option<[u8; 32]> {
        if id == self.signed_prekey_id {
            return Some(self.signed_prekey_public);
        }
        self.old_signed_prekeys
            .iter()
            .find(|prekey| prekey.id == id)
            .map(|prekey| prekey.public)
    }

    pub fn take_prekey(&mut self, id: u32) -> Option<PreKeyPair> {
        let pos = self.prekeys.iter().position(|pk| pk.id == id)?;
        Some(self.prekeys.remove(pos))
    }

    /// Replace the current signed prekey, keeping the old private key.
    pub fn rotate_signed_prekey(&mut self) -> DeviceBundle {
        let old = SignedPreKey {
            id: self.signed_prekey_id,
            private: self.signed_prekey_private,
            public: self.signed_prekey_public,
            signature: self.signed_prekey_signature,
        };
        self.old_signed_prekeys.push(old);
        if self.old_signed_prekeys.len() > 2 {
            self.old_signed_prekeys.remove(0);
        }

        let private = random_bytes::<32>();
        let secret = StaticSecret::from(private);
        let public = PublicKey::from(&secret).to_bytes();
        let signature = sign_prekey(&self.identity_private, &public);

        self.signed_prekey_id = self.signed_prekey_id.saturating_add(1);
        self.signed_prekey_private = private;
        self.signed_prekey_public = public;
        self.signed_prekey_signature = signature;
        self.signed_prekey_created_at = now_seconds();
        self.bundle()
    }

    /// Private half of the current or a retained signed prekey.
    #[must_use]
    pub fn signed_prekey_private(&self, id: u32) -> Option<[u8; 32]> {
        if id == self.signed_prekey_id {
            return Some(self.signed_prekey_private);
        }
        self.old_signed_prekeys
            .iter()
            .find(|key| key.id == id)
            .map(|key| key.private)
    }
}

fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

/// One recipient key inside an OMEMO header.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OmemoKey {
    pub rid: u32,
    pub kex: bool,
    pub data: Vec<u8>,
    /// Recipient bare JID for group messages; `None` in the flat 1:1 format.
    #[serde(default)]
    pub jid: Option<String>,
}

/// The `<encrypted xmlns='urn:xmpp:omemo:2'/>` payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OmemoEnvelope {
    pub sid: u32,
    pub keys: Vec<OmemoKey>,
    pub iv: Vec<u8>,
    pub payload: Vec<u8>,
}

impl OmemoEnvelope {
    pub fn to_element(&self) -> Element {
        let mut header = Element::builder("header", NS_OMEMO).attr("sid", self.sid.to_string());
        if self.keys.iter().any(|key| key.jid.is_some()) {
            let mut groups: std::collections::BTreeMap<String, Vec<&OmemoKey>> =
                std::collections::BTreeMap::new();
            for key in &self.keys {
                if let Some(jid) = key.jid.as_ref() {
                    groups.entry(jid.clone()).or_default().push(key);
                } else {
                    header = header.append(key_element(key, None));
                }
            }
            for (jid, keys) in groups {
                let mut element = Element::builder("keys", NS_OMEMO).attr("jid", jid);
                for key in keys {
                    element = element.append(key_element(key, None));
                }
                header = header.append(element);
            }
        } else {
            for key in &self.keys {
                header = header.append(key_element(key, None));
            }
        }
        header = if self.iv.is_empty() {
            header
        } else {
            header.append(Element::builder("iv", NS_OMEMO).append(STANDARD.encode(&self.iv)))
        };

        Element::builder("encrypted", NS_OMEMO)
            .append(header.build())
            .append(Element::builder("payload", NS_OMEMO).append(STANDARD.encode(&self.payload)))
            .build()
    }

    pub fn from_element(encrypted: &Element) -> Option<Self> {
        let header = encrypted.get_child("header", NSChoice::Any)?;
        let sid = header.attr("sid")?.parse().ok()?;
        let mut keys = Vec::new();
        for child in header.children() {
            match child.name() {
                "key" => {
                    if let Some(key) = parse_key(child, None) {
                        keys.push(key);
                    }
                }
                "keys" => {
                    let jid = child.attr("jid").map(str::to_string);
                    for key in child.children().filter(|key| key.name() == "key") {
                        if let Some(key) = parse_key(key, jid.clone()) {
                            keys.push(key);
                        }
                    }
                }
                _ => {}
            }
        }
        let iv = header
            .get_child("iv", NSChoice::Any)
            .and_then(|iv| STANDARD.decode(iv.text().trim()).ok())
            .unwrap_or_default();
        // OMEMO 2 empty messages (key transport only) omit the payload.
        let payload = encrypted
            .get_child("payload", NSChoice::Any)
            .and_then(|payload| STANDARD.decode(payload.text().trim()).ok())
            .unwrap_or_default();

        Some(Self {
            sid,
            keys,
            iv,
            payload,
        })
    }

    #[must_use]
    pub fn has_grouped_keys(&self) -> bool {
        self.keys.iter().any(|key| key.jid.is_some())
    }

    /// Modern XEP-0384 payloads have no header IV and carry a 32-byte key plus
    /// 16-byte tag in each recipient key.
    #[must_use]
    pub fn is_modern(&self) -> bool {
        self.iv.is_empty()
    }
}

/// One recipient key inside a legacy OMEMO 0.3 header.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegacyOmemoKey {
    pub rid: u32,
    pub prekey: bool,
    pub data: Vec<u8>,
}

/// The `<encrypted xmlns='eu.siacs.conversations.axolotl'/>` payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegacyOmemoEnvelope {
    pub sid: u32,
    pub keys: Vec<LegacyOmemoKey>,
    pub iv: Vec<u8>,
    pub payload: Option<Vec<u8>>,
}

impl LegacyOmemoEnvelope {
    /// # Errors
    ///
    /// Returns an error when the envelope cannot be encoded.
    pub fn to_element(&self) -> Result<Element> {
        let encoded = LegacyStanzaEncrypted {
            sid: self.sid,
            keys: self
                .keys
                .iter()
                .map(|key| LegacyStanzaKey {
                    rid: key.rid,
                    prekey: key.prekey,
                    data: key.data.clone(),
                })
                .collect(),
            iv: self.iv.clone(),
            payload: self.payload.clone(),
        }
        .encode()
        .map_err(|e| XmppError::CryptoError(e.to_string()))?;
        Element::from_reader(encoded.as_bytes())
            .map_err(|e| XmppError::DeserializationError(e.to_string()))
    }

    pub fn from_element(encrypted: &Element) -> Option<Self> {
        let parsed = LegacyStanzaEncrypted::parse(&crate::vcard::to_xml(encrypted)).ok()?;
        Some(Self {
            sid: parsed.sid,
            keys: parsed
                .keys
                .into_iter()
                .map(|key| LegacyOmemoKey {
                    rid: key.rid,
                    prekey: key.prekey,
                    data: key.data,
                })
                .collect(),
            iv: parsed.iv,
            payload: parsed.payload,
        })
    }
}

fn key_element(key: &OmemoKey, jid: Option<String>) -> Element {
    let mut element = Element::builder("key", NS_OMEMO)
        .attr("rid", key.rid.to_string())
        .attr("kex", if key.kex { "true" } else { "false" });
    if let Some(jid) = jid {
        element = element.attr("jid", jid);
    }
    element.append(STANDARD.encode(&key.data)).build()
}

fn parse_key(element: &Element, jid: Option<String>) -> Option<OmemoKey> {
    Some(OmemoKey {
        rid: element.attr("rid")?.parse().ok()?,
        kex: element.attr("kex") == Some("true"),
        data: STANDARD.decode(element.text().trim()).ok()?,
        jid,
    })
}

/// Encrypt the payload key with AES-128-GCM (the OMEMO message key is random).
pub fn encrypt_payload(key: &[u8], iv: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
    use aes_gcm::aead::Aead;
    use aes_gcm::{Aes128Gcm, KeyInit};

    let cipher = Aes128Gcm::new_from_slice(key)
        .map_err(|e| XmppError::CryptoError(format!("invalid OMEMO key: {}", e)))?;
    let nonce = aes_gcm::Nonce::from_slice(iv);
    cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| XmppError::CryptoError(format!("payload encryption failed: {}", e)))
}

pub fn decrypt_payload(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
    use aes_gcm::aead::Aead;
    use aes_gcm::{Aes128Gcm, KeyInit};

    let cipher = Aes128Gcm::new_from_slice(key)
        .map_err(|e| XmppError::CryptoError(format!("invalid OMEMO key: {}", e)))?;
    let nonce = aes_gcm::Nonce::from_slice(iv);
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| XmppError::CryptoError(format!("payload decryption failed: {}", e)))
}

/// Random 16-byte OMEMO message key and 12-byte IV.
pub fn random_payload_key() -> ([u8; 16], [u8; 12]) {
    (random_bytes::<16>(), random_bytes::<12>())
}

/// Build a XEP-0420 SCE envelope around a body.
fn sce_envelope(body: &str, to: Option<&str>) -> Vec<u8> {
    let mut envelope = Element::builder("envelope", "urn:xmpp:sce:1")
        .append(
            Element::builder("content", "urn:xmpp:sce:1")
                .append(Element::builder("body", "jabber:client").append(body.to_string())),
        )
        .append(Element::builder("rpad", "urn:xmpp:sce:1").append("~"));
    if let Some(to) = to {
        envelope =
            envelope.append(Element::builder("to", "urn:xmpp:sce:1").attr("jid", to.to_string()));
    }
    crate::vcard::to_xml(&envelope.build()).into_bytes()
}

fn sce_body(plaintext: &[u8]) -> Result<String> {
    let Ok(envelope) = Element::from_reader(plaintext) else {
        return String::from_utf8(plaintext.to_vec())
            .map_err(|e| XmppError::CryptoError(format!("invalid UTF-8: {e}")));
    };
    if envelope.name() != "envelope" {
        return String::from_utf8(plaintext.to_vec())
            .map_err(|e| XmppError::CryptoError(format!("invalid UTF-8: {e}")));
    }
    envelope
        .get_child("content", NSChoice::Any)
        .and_then(|content| content.get_child("body", NSChoice::Any))
        .map(|body| body.text())
        .ok_or_else(|| XmppError::CryptoError("SCE envelope has no body".into()))
}

fn derive_payload_keys(key: &[u8]) -> Result<([u8; 32], [u8; 32], [u8; 16])> {
    let hkdf = hkdf::Hkdf::<sha2::Sha256>::new(Some(&[0u8; 32]), key);
    let mut output = [0u8; 80];
    hkdf.expand(b"OMEMO Payload", &mut output)
        .map_err(|_| XmppError::CryptoError("OMEMO payload key derivation failed".into()))?;
    let mut enc = [0u8; 32];
    let mut auth = [0u8; 32];
    let mut iv = [0u8; 16];
    enc.copy_from_slice(&output[..32]);
    auth.copy_from_slice(&output[32..64]);
    iv.copy_from_slice(&output[64..80]);
    Ok((enc, auth, iv))
}

fn modern_payload_encrypt(key: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
    use cbc::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
    let (enc, _, iv) = derive_payload_keys(key)?;
    let encryptor = cbc::Encryptor::<aes::Aes256>::new((&enc).into(), (&iv).into());
    Ok(encryptor.encrypt_padded_vec_mut::<Pkcs7>(plaintext))
}

fn modern_payload_tag(key: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
    use hmac::{Hmac, Mac};
    let (_, auth, _) = derive_payload_keys(key)?;
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(&auth)
        .map_err(|e| XmppError::CryptoError(e.to_string()))?;
    mac.update(ciphertext);
    Ok(mac.finalize().into_bytes()[..16].to_vec())
}

fn modern_payload_decrypt(key: &[u8], ciphertext: &[u8], tag: &[u8]) -> Result<Vec<u8>> {
    use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
    use hmac::{Hmac, Mac};
    let (enc, auth, iv) = derive_payload_keys(key)?;
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(&auth)
        .map_err(|e| XmppError::CryptoError(e.to_string()))?;
    mac.update(ciphertext);
    mac.verify_truncated_left(tag)
        .map_err(|_| XmppError::CryptoError("OMEMO payload authentication failed".into()))?;
    let decryptor = cbc::Decryptor::<aes::Aes256>::new((&enc).into(), (&iv).into());
    decryptor
        .decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
        .map_err(|_| XmppError::CryptoError("OMEMO payload decryption failed".into()))
}

/// Persistence for OMEMO state (local keys, device lists, trust, sessions).
pub trait OmemoStore: Send + Sync {
    fn load(&self) -> Option<String>;
    fn save(&self, data: &str) -> Result<()>;
}

/// Per-device OMEMO 2 double-ratchet session.
///
/// This drives the `omemo-doubleratchet` primitives directly so the X3DH
/// associated data is used verbatim for both sending and receiving.
/// `python-twomemo` derives the AD once, initiator-first, and never swaps it;
/// the `TwomemoSession` helper in `omemo-twomemo` swaps halves for some
/// directions, which only works against another peer using the same swap.
///
/// The ratchet is stored as a plain snapshot and rebuilt with an OS-RNG priv
/// provider around each operation. That keeps the session `Send + Sync` (the
/// `Box<dyn DhPrivProvider>` inside the live ratchet is not) and makes
/// persistence a direct clone.
struct OmemoSession {
    dh: DhRatchetSnapshot,
    ad_x3dh: Vec<u8>,
    skipped: VecDeque<(SkippedKey, Vec<u8>)>,
    max_skipped: usize,
}

type SkippedKey = ([u8; 32], u64);

impl OmemoSession {
    fn create_active(
        ad_x3dh: Vec<u8>,
        root_chain_key: Vec<u8>,
        remote_ratchet_pub: [u8; 32],
        priv_provider: Box<dyn DhPrivProvider>,
    ) -> Result<Self> {
        let dh: DiffieHellmanRatchet<RootKdf, MsgKdf> = DiffieHellmanRatchet::create_active(
            remote_ratchet_pub,
            root_chain_key,
            MESSAGE_CHAIN_CONSTANT.to_vec(),
            DEFAULT_DOS_THRESHOLD,
            priv_provider,
        )
        .map_err(|e| XmppError::CryptoError(e.to_string()))?;
        Ok(Self {
            dh: dh.snapshot(),
            ad_x3dh,
            skipped: VecDeque::default(),
            max_skipped: DEFAULT_MAX_SKIPPED,
        })
    }

    fn create_passive(
        ad_x3dh: Vec<u8>,
        root_chain_key: Vec<u8>,
        own_ratchet_priv: [u8; 32],
        remote_ratchet_pub: [u8; 32],
        priv_provider: Box<dyn DhPrivProvider>,
    ) -> Result<Self> {
        let dh: DiffieHellmanRatchet<RootKdf, MsgKdf> = DiffieHellmanRatchet::create_passive(
            own_ratchet_priv,
            remote_ratchet_pub,
            root_chain_key,
            MESSAGE_CHAIN_CONSTANT.to_vec(),
            DEFAULT_DOS_THRESHOLD,
            priv_provider,
        )
        .map_err(|e| XmppError::CryptoError(e.to_string()))?;
        Ok(Self {
            dh: dh.snapshot(),
            ad_x3dh,
            skipped: VecDeque::default(),
            max_skipped: DEFAULT_MAX_SKIPPED,
        })
    }

    fn ratchet(&self) -> DiffieHellmanRatchet<RootKdf, MsgKdf> {
        DiffieHellmanRatchet::from_snapshot(self.dh.clone(), Box::new(OsRngDhPrivProvider::new()))
    }

    fn encrypt_message(&mut self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let mut dh = self.ratchet();
        let (message_key, header) = dh
            .next_encryption_key()
            .map_err(|e| XmppError::CryptoError(e.to_string()))?;
        let ad = build_associated_data(&self.ad_x3dh, &header);
        let encrypted = aead_encrypt(&ad, &message_key, plaintext);
        self.dh = dh.snapshot();
        Ok(encrypted)
    }

    fn decrypt_message(&mut self, auth_message: &[u8]) -> Result<Vec<u8>> {
        let auth = OmemoAuthenticatedMessage::decode(auth_message)
            .map_err(|e| XmppError::CryptoError(e.to_string()))?;
        let inner = OmemoMessage::decode(auth.message.as_slice())
            .map_err(|e| XmppError::CryptoError(e.to_string()))?;
        if inner.dh_pub.len() != 32 {
            return Err(XmppError::CryptoError(
                "OMEMO ratchet header has an invalid key length".into(),
            ));
        }
        let mut ratchet_pub = [0u8; 32];
        ratchet_pub.copy_from_slice(&inner.dh_pub);
        let header = DrHeader {
            ratchet_pub,
            previous_sending_chain_length: u64::from(inner.pn),
            sending_chain_length: u64::from(inner.n),
        };
        let key_index = (header.ratchet_pub, header.sending_chain_length);

        if let Some(pos) = self.skipped.iter().position(|(key, _)| *key == key_index) {
            let message_key = self.skipped[pos].1.clone();
            let ad = build_associated_data(&self.ad_x3dh, &header);
            let plaintext = aead_decrypt(&ad, &message_key, auth_message)
                .map_err(|e| XmppError::CryptoError(e.to_string()))?;
            self.skipped.remove(pos);
            return Ok(plaintext);
        }

        let mut dh = self.ratchet();
        let (message_key, new_skipped) = dh
            .next_decryption_key(&header)
            .map_err(|e| XmppError::CryptoError(e.to_string()))?;
        let ad = build_associated_data(&self.ad_x3dh, &header);
        let plaintext = aead_decrypt(&ad, &message_key, auth_message)
            .map_err(|e| XmppError::CryptoError(e.to_string()))?;

        self.dh = dh.snapshot();
        self.skipped.extend(new_skipped);
        while self.skipped.len() > self.max_skipped {
            self.skipped.pop_front();
        }
        Ok(plaintext)
    }

    fn snapshot_bytes(&self) -> Vec<u8> {
        TwomemoSessionSnapshot {
            dh: self.dh.clone(),
            ad_x3dh: self.ad_x3dh.clone(),
            max_skipped: self.max_skipped,
            skipped: self.skipped.iter().cloned().collect(),
            // The role only matters to `TwomemoSession`; this wrapper ignores it.
            role: SessionRole::Active,
        }
        .encode()
    }

    fn from_snapshot_bytes(bytes: &[u8]) -> Result<Self> {
        let snapshot = TwomemoSessionSnapshot::decode(bytes)
            .map_err(|e| XmppError::CryptoError(e.to_string()))?;
        Ok(Self {
            dh: snapshot.dh,
            ad_x3dh: snapshot.ad_x3dh,
            skipped: snapshot.skipped.into_iter().collect(),
            max_skipped: snapshot.max_skipped,
        })
    }
}

/// Legacy OMEMO 0.3 session stored as a plain snapshot.
///
/// `OldmemoSession` holds a `Box<dyn DhPrivProvider>` and is not
/// `Send + Sync`; rebuilding it around each operation keeps `OmemoManager`
/// shareable across threads.
struct LegacySession {
    snapshot: OldmemoSessionSnapshot,
}

impl LegacySession {
    fn from_session(session: OldmemoSession) -> Self {
        Self {
            snapshot: session.snapshot(),
        }
    }

    fn session(&self) -> OldmemoSession {
        OldmemoSession::from_snapshot(self.snapshot.clone(), Box::new(OsRngDhPrivProvider::new()))
    }

    fn encrypt_message(&mut self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let mut session = self.session();
        let encrypted = session
            .encrypt_message(plaintext)
            .map_err(|e| XmppError::CryptoError(e.to_string()))?;
        self.snapshot = session.snapshot();
        Ok(encrypted)
    }

    fn decrypt_message(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        let mut session = self.session();
        let plaintext = session
            .decrypt_message(ciphertext)
            .map_err(|e| XmppError::CryptoError(e.to_string()))?;
        self.snapshot = session.snapshot();
        Ok(plaintext)
    }

    fn snapshot_bytes(&self) -> Vec<u8> {
        self.snapshot.encode()
    }

    fn from_snapshot_bytes(bytes: &[u8]) -> Result<Self> {
        let snapshot = OldmemoSessionSnapshot::decode(bytes)
            .map_err(|e| XmppError::CryptoError(e.to_string()))?;
        Ok(Self { snapshot })
    }
}

/// Local OMEMO state: our device, peer device lists, bundles and trust.
#[derive(Default)]
pub struct OmemoManager {
    store: RwLock<Option<Arc<dyn OmemoStore>>>,
    local: RwLock<Option<LocalDevice>>,
    device_lists: RwLock<HashMap<String, Vec<u32>>>,
    bundles: RwLock<HashMap<String, HashMap<u32, DeviceBundle>>>,
    trust: RwLock<HashMap<String, HashMap<u32, TrustLevel>>>,
    sessions: RwLock<HashMap<String, HashMap<u32, OmemoSession>>>,
    legacy_device_lists: RwLock<HashMap<String, Vec<u32>>>,
    legacy_bundles: RwLock<HashMap<String, HashMap<u32, LegacyDeviceBundle>>>,
    legacy_sessions: RwLock<HashMap<String, HashMap<u32, LegacySession>>>,
}

impl OmemoManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop all in-memory state (used when switching accounts).
    pub fn reset(&self) {
        *self.local.write() = None;
        self.device_lists.write().clear();
        self.bundles.write().clear();
        self.trust.write().clear();
        self.sessions.write().clear();
    }

    pub fn set_store(&self, store: Arc<dyn OmemoStore>) {
        *self.store.write() = Some(store);
    }

    /// Load previously persisted state, if any.
    pub fn load_persisted(&self) -> Result<bool> {
        let data = match self.store.read().as_ref().and_then(|s| s.load()) {
            Some(data) => data,
            None => return Ok(false),
        };
        self.import_state(&data)?;
        Ok(true)
    }

    fn persist(&self) {
        let Some(store) = self.store.read().clone() else {
            return;
        };
        if let Err(e) = store.save(&self.export_state()) {
            tracing::warn!("Failed to persist OMEMO state: {}", e);
        }
    }

    pub fn export_state(&self) -> String {
        let local = self.local.read().as_ref().map(StoredLocal::from_device);
        let device_lists = self.device_lists.read().clone();
        let trust = self.trust.read().clone();
        let sessions = self
            .sessions
            .read()
            .iter()
            .map(|(jid, devices)| {
                let map = devices
                    .iter()
                    .map(|(id, session)| (*id, STANDARD.encode(session.snapshot_bytes())))
                    .collect();
                (jid.clone(), map)
            })
            .collect();
        let legacy_device_lists = self.legacy_device_lists.read().clone();
        let legacy_bundles = self.legacy_bundles.read().clone();
        let legacy_sessions = self
            .legacy_sessions
            .read()
            .iter()
            .map(|(jid, devices)| {
                let map = devices
                    .iter()
                    .map(|(id, session)| (*id, STANDARD.encode(session.snapshot_bytes())))
                    .collect();
                (jid.clone(), map)
            })
            .collect();

        let stored = StoredOmemo {
            version: 1,
            local,
            device_lists,
            trust,
            sessions,
            legacy_device_lists,
            legacy_bundles,
            legacy_sessions,
        };
        serde_json::to_string(&stored).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn import_state(&self, data: &str) -> Result<()> {
        let stored: StoredOmemo = serde_json::from_str(data)
            .map_err(|e| XmppError::DeserializationError(e.to_string()))?;
        if let Some(local) = stored.local {
            *self.local.write() = Some(local.into_device()?);
        }
        *self.device_lists.write() = stored.device_lists;
        *self.trust.write() = stored.trust;
        let mut sessions = HashMap::new();
        for (jid, devices) in stored.sessions {
            let mut map = HashMap::new();
            for (id, encoded) in devices {
                match STANDARD.decode(&encoded) {
                    Ok(bytes) => match OmemoSession::from_snapshot_bytes(&bytes) {
                        Ok(session) => {
                            map.insert(id, session);
                        }
                        Err(e) => tracing::warn!("Skipping unreadable OMEMO session: {}", e),
                    },
                    Err(e) => tracing::warn!("Skipping invalid OMEMO session encoding: {}", e),
                }
            }
            sessions.insert(jid, map);
        }
        *self.sessions.write() = sessions;

        *self.legacy_device_lists.write() = stored.legacy_device_lists;
        *self.legacy_bundles.write() = stored.legacy_bundles;
        let mut legacy_sessions = HashMap::new();
        for (jid, devices) in stored.legacy_sessions {
            let mut map = HashMap::new();
            for (id, encoded) in devices {
                match STANDARD.decode(&encoded) {
                    Ok(bytes) => match LegacySession::from_snapshot_bytes(&bytes) {
                        Ok(session) => {
                            map.insert(id, session);
                        }
                        Err(e) => {
                            tracing::warn!("Skipping unreadable legacy OMEMO session: {}", e)
                        }
                    },
                    Err(e) => {
                        tracing::warn!("Skipping invalid legacy OMEMO session encoding: {}", e)
                    }
                }
            }
            legacy_sessions.insert(jid, map);
        }
        *self.legacy_sessions.write() = legacy_sessions;
        Ok(())
    }

    pub fn local(&self) -> Option<LocalDevice> {
        self.local.read().clone()
    }

    pub fn set_local(&self, device: LocalDevice) {
        *self.local.write() = Some(device);
        self.persist();
    }

    /// Create a local device if none exists yet.
    pub fn ensure_local(&self, device_id: u32, prekey_count: usize) -> LocalDevice {
        if let Some(local) = self.local() {
            return local;
        }
        let device = LocalDevice::generate(device_id, prekey_count);
        self.set_local(device.clone());
        device
    }

    /// Create a local device with a random 31-bit device id if none exists.
    pub fn ensure_local_auto(&self, prekey_count: usize) -> LocalDevice {
        if let Some(local) = self.local() {
            return local;
        }
        let mut device_id = rand::random::<u32>() & 0x7fff_ffff;
        if device_id == 0 {
            device_id = 1;
        }
        self.ensure_local(device_id, prekey_count)
    }

    /// Top up one-time prekeys when fewer than `min` remain, adding `add` new
    /// ones. Returns the new bundle when prekeys were generated.
    pub fn replenish_prekeys(&self, min: usize, add: usize) -> Option<DeviceBundle> {
        let mut local = self.local.write();
        let device = local.as_mut()?;
        if device.prekeys.len() >= min {
            return None;
        }
        let start = device.next_prekey_id;
        for i in 0..add as u32 {
            let private = random_bytes::<32>();
            let secret = StaticSecret::from(private);
            device.prekeys.push(PreKeyPair {
                id: start + i,
                private,
                public: PublicKey::from(&secret).to_bytes(),
            });
        }
        device.next_prekey_id = start + add as u32;
        let bundle = device.bundle();
        drop(local);
        self.persist();
        Some(bundle)
    }

    pub fn is_enabled(&self) -> bool {
        self.local.read().is_some()
    }

    /// Rotate the signed prekey when it is older than `max_age_secs`.
    ///
    /// Returns the new bundle so the caller can republish it.
    pub fn rotate_signed_prekey_if_due(&self, max_age_secs: i64) -> Option<DeviceBundle> {
        let mut local = self.local.write();
        let device = local.as_mut()?;
        let age = now_seconds().saturating_sub(device.signed_prekey_created_at);
        if age < max_age_secs {
            return None;
        }
        let bundle = device.rotate_signed_prekey();
        drop(local);
        self.persist();
        Some(bundle)
    }

    pub fn set_device_list(&self, jid: &str, devices: Vec<u32>) {
        self.device_lists.write().insert(jid.to_string(), devices);
        self.persist();
    }

    pub fn device_list(&self, jid: &str) -> Vec<u32> {
        self.device_lists
            .read()
            .get(jid)
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_bundle(&self, jid: &str, bundle: DeviceBundle) {
        self.bundles
            .write()
            .entry(jid.to_string())
            .or_default()
            .insert(bundle.device_id, bundle);
    }

    pub fn bundle(&self, jid: &str, device_id: u32) -> Option<DeviceBundle> {
        self.bundles
            .read()
            .get(jid)
            .and_then(|m| m.get(&device_id))
            .cloned()
    }

    pub fn set_legacy_device_list(&self, jid: &str, devices: Vec<u32>) {
        self.legacy_device_lists
            .write()
            .insert(jid.to_string(), devices);
        self.persist();
    }

    pub fn legacy_device_list(&self, jid: &str) -> Vec<u32> {
        self.legacy_device_lists
            .read()
            .get(jid)
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_legacy_bundle(&self, jid: &str, bundle: LegacyDeviceBundle) {
        self.legacy_bundles
            .write()
            .entry(jid.to_string())
            .or_default()
            .insert(bundle.device_id, bundle);
    }

    pub fn legacy_bundle(&self, jid: &str, device_id: u32) -> Option<LegacyDeviceBundle> {
        self.legacy_bundles
            .read()
            .get(jid)
            .and_then(|m| m.get(&device_id))
            .cloned()
    }

    pub fn legacy_bundles(&self, jid: &str) -> Vec<LegacyDeviceBundle> {
        let ids = self.legacy_device_list(jid);
        ids.into_iter()
            .filter_map(|id| self.legacy_bundle(jid, id))
            .filter(LegacyDeviceBundle::verify)
            .collect()
    }

    /// Union of OMEMO 2 and legacy device IDs, for trust UI display.
    pub fn all_device_ids(&self, jid: &str) -> Vec<u32> {
        let mut ids = self.device_list(jid);
        for id in self.legacy_device_list(jid) {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        ids
    }

    /// Encrypt a chat body for a set of legacy OMEMO 0.3 devices.
    ///
    /// # Errors
    ///
    /// Returns an error when there is no local device, no usable bundle, or a
    /// session cannot encrypt the key material.
    pub fn encrypt_legacy_message(
        &self,
        jid: &str,
        body: &str,
        bundles: &[LegacyDeviceBundle],
    ) -> Result<LegacyOmemoEnvelope> {
        let sid = self
            .local
            .read()
            .as_ref()
            .map(|d| d.device_id)
            .ok_or_else(|| XmppError::CryptoError("no local OMEMO device".into()))?;
        let bundles: Vec<&LegacyDeviceBundle> = bundles
            .iter()
            .filter(|bundle| {
                self.trust(jid, bundle.device_id) != TrustLevel::Untrusted && bundle.verify()
            })
            .collect();
        if bundles.is_empty() {
            return Err(XmppError::CryptoError(
                "no legacy OMEMO recipient devices".into(),
            ));
        }

        let (payload, iv, key_blob) = seal_payload(body.as_bytes());
        let mut keys = Vec::with_capacity(bundles.len());
        for bundle in bundles {
            let (data, prekey) = self.encrypt_legacy_key(jid, bundle, &key_blob)?;
            keys.push(LegacyOmemoKey {
                rid: bundle.device_id,
                prekey,
                data,
            });
        }

        Ok(LegacyOmemoEnvelope {
            sid,
            keys,
            iv: iv.to_vec(),
            payload: Some(payload),
        })
    }

    /// Decrypt a legacy OMEMO 0.3 envelope addressed to this device.
    ///
    /// # Errors
    ///
    /// Returns an error when the envelope has no key for us, the session cannot
    /// decrypt the key material, or the payload fails authentication.
    pub fn decrypt_legacy_message(
        &self,
        jid: &str,
        envelope: &LegacyOmemoEnvelope,
    ) -> Result<String> {
        let device_id = self
            .local
            .read()
            .as_ref()
            .map(|d| d.device_id)
            .ok_or_else(|| XmppError::CryptoError("no local OMEMO device".into()))?;
        let our_key = envelope
            .keys
            .iter()
            .find(|key| key.rid == device_id)
            .ok_or_else(|| XmppError::CryptoError("message is not encrypted for us".into()))?;
        let secret = self.decrypt_legacy_key(jid, envelope.sid, &our_key.data, our_key.prekey)?;
        let Some(payload) = envelope.payload.as_ref() else {
            // Key-transport-only message: decrypting advances the ratchet.
            return Ok(String::new());
        };
        let plaintext = open_legacy_payload(payload, &envelope.iv, &secret)
            .map_err(|e| XmppError::CryptoError(e.to_string()))?;
        String::from_utf8(plaintext)
            .map_err(|e| XmppError::CryptoError(format!("invalid UTF-8: {e}")))
    }

    /// Encrypt one key blob for a legacy OMEMO 0.3 device.
    ///
    /// # Errors
    ///
    /// Returns an error when there is no local device, the bundle has no
    /// one-time prekey, or the key exchange fails.
    pub fn encrypt_legacy_key(
        &self,
        jid: &str,
        bundle: &LegacyDeviceBundle,
        plaintext: &[u8],
    ) -> Result<(Vec<u8>, bool)> {
        let mut sessions = self.legacy_sessions.write();
        let map = sessions.entry(jid.to_string()).or_default();

        if let Some(session) = map.get_mut(&bundle.device_id) {
            let data = session
                .encrypt_message(plaintext)
                .map_err(|e| XmppError::CryptoError(e.to_string()))?;
            drop(sessions);
            self.persist();
            return Ok((data, false));
        }

        let local = self
            .local
            .read()
            .clone()
            .ok_or_else(|| XmppError::CryptoError("no local OMEMO device".into()))?;
        let Some(chosen) = bundle
            .prekeys
            .get(rand::random::<usize>() % bundle.prekeys.len().max(1))
        else {
            return Err(XmppError::CryptoError(
                "legacy bundle has no one-time prekey".into(),
            ));
        };
        let chosen_key = array32(&chosen.key)?;
        let x3dh_bundle = bundle.to_x3dh()?;
        let (output, header) = get_shared_secret_active_oldmemo(
            &local.x3dh_state(),
            &x3dh_bundle,
            &[],
            random_bytes::<32>(),
            Some(chosen_key),
            true,
        )
        .map_err(|e| XmppError::CryptoError(e.to_string()))?;
        let mut session = OldmemoSession::create_active(
            output.associated_data,
            output.shared_secret.to_vec(),
            array32(&bundle.signed_prekey)?,
            Box::new(OsRngDhPrivProvider::new()),
        )
        .map_err(|e| XmppError::CryptoError(e.to_string()))?;
        let auth = session
            .encrypt_message(plaintext)
            .map_err(|e| XmppError::CryptoError(e.to_string()))?;
        let own_identity_curve = omemo_xeddsa::priv_to_curve25519_pub(&local.identity_private);
        let data = build_legacy_key_exchange(
            chosen.id,
            bundle.signed_prekey_id,
            own_identity_curve,
            header.ephemeral_key,
            &auth,
        );
        map.insert(bundle.device_id, LegacySession::from_session(session));
        drop(sessions);
        self.persist();
        Ok((data, true))
    }

    /// Decrypt one legacy OMEMO 0.3 key blob.
    ///
    /// # Errors
    ///
    /// Returns an error when the key exchange or ratchet message cannot be
    /// decrypted or the referenced prekey is unknown.
    pub fn decrypt_legacy_key(
        &self,
        jid: &str,
        device_id: u32,
        data: &[u8],
        prekey: bool,
    ) -> Result<Vec<u8>> {
        let mut sessions = self.legacy_sessions.write();
        if !prekey {
            let session = sessions
                .get_mut(jid)
                .and_then(|map| map.get_mut(&device_id))
                .ok_or_else(|| {
                    XmppError::CryptoError(
                        "no legacy session for sender and message is not a key exchange".into(),
                    )
                })?;
            let plaintext = session
                .decrypt_message(data)
                .map_err(|e| XmppError::CryptoError(e.to_string()))?;
            drop(sessions);
            self.persist();
            return Ok(plaintext);
        }

        let (prekey_id, signed_prekey_id, identity_key, ephemeral_key, auth) =
            parse_legacy_key_exchange(data).map_err(|e| XmppError::CryptoError(e.to_string()))?;
        let local = self
            .local
            .read()
            .clone()
            .ok_or_else(|| XmppError::CryptoError("no local OMEMO device".into()))?;
        let signed_public = local
            .signed_prekey_public(signed_prekey_id)
            .ok_or_else(|| {
                XmppError::CryptoError(format!("unknown signed prekey {signed_prekey_id}"))
            })?;
        let one_time = local
            .prekeys
            .iter()
            .find(|prekey| prekey.id == prekey_id)
            .ok_or_else(|| {
                XmppError::CryptoError(format!("unknown one-time prekey {prekey_id}"))
            })?;
        let state = local
            .x3dh_state_for_signed_prekey(signed_prekey_id)
            .ok_or_else(|| {
                XmppError::CryptoError(format!("unknown signed prekey {signed_prekey_id}"))
            })?;
        let sender_identity = curve_to_ed25519(&identity_key)?;
        let header = X3dhHeader {
            identity_key: sender_identity,
            ephemeral_key,
            signed_pre_key: signed_public,
            pre_key: Some(array32(&one_time.public)?),
        };
        let (output, _) = get_shared_secret_passive_oldmemo(&state, &header, &[], true)
            .map_err(|e| XmppError::CryptoError(e.to_string()))?;
        let alice_ratchet =
            peek_legacy_dh_pub(&auth).map_err(|e| XmppError::CryptoError(e.to_string()))?;
        let mut session = OldmemoSession::create_passive(
            output.associated_data,
            output.shared_secret.to_vec(),
            state.signed_pre_key.priv_key,
            alice_ratchet,
            Box::new(OsRngDhPrivProvider::new()),
        )
        .map_err(|e| XmppError::CryptoError(e.to_string()))?;
        let plaintext = session
            .decrypt_message(&auth)
            .map_err(|e| XmppError::CryptoError(e.to_string()))?;

        if let Some(local) = self.local.write().as_mut() {
            local.take_prekey(prekey_id);
        }
        sessions
            .entry(jid.to_string())
            .or_default()
            .insert(device_id, LegacySession::from_session(session));
        drop(sessions);
        self.persist();
        Ok(plaintext)
    }

    /// Encrypt a chat body for a set of peer devices.
    pub fn encrypt_message(
        &self,
        jid: &str,
        body: &str,
        bundles: &[DeviceBundle],
    ) -> Result<OmemoEnvelope> {
        let sid = self
            .local
            .read()
            .as_ref()
            .map(|d| d.device_id)
            .ok_or_else(|| XmppError::CryptoError("no local OMEMO device".into()))?;
        let bundles: Vec<&DeviceBundle> = bundles
            .iter()
            .filter(|b| self.trust(jid, b.device_id) != TrustLevel::Untrusted && b.verify())
            .collect();
        if bundles.is_empty() {
            return Err(XmppError::CryptoError("no OMEMO recipient devices".into()));
        }

        let key = random_bytes::<32>();
        let plaintext = sce_envelope(body, Some(jid));
        let payload = modern_payload_encrypt(&key, &plaintext)?;
        let tag = modern_payload_tag(&key, &payload)?;
        let mut secret = key.to_vec();
        secret.extend_from_slice(&tag);
        let mut keys = Vec::with_capacity(bundles.len());
        for bundle in bundles {
            let (data, kex) = self.encrypt_key(jid, bundle, &secret)?;
            keys.push(OmemoKey {
                rid: bundle.device_id,
                kex,
                data,
                jid: Some(jid.to_string()),
            });
        }

        Ok(OmemoEnvelope {
            sid,
            keys,
            iv: Vec::new(),
            payload,
        })
    }

    /// Decrypt an envelope addressed to this device.
    pub fn decrypt_message(&self, jid: &str, envelope: &OmemoEnvelope) -> Result<String> {
        let device_id = self
            .local
            .read()
            .as_ref()
            .map(|d| d.device_id)
            .ok_or_else(|| XmppError::CryptoError("no local OMEMO device".into()))?;
        let our_key = envelope
            .keys
            .iter()
            .find(|k| k.rid == device_id)
            .ok_or_else(|| XmppError::CryptoError("message is not encrypted for us".into()))?;
        let secret = self.decrypt_key(jid, envelope.sid, &our_key.data, our_key.kex)?;
        if envelope.is_modern() {
            // Empty OMEMO messages only carry key material; decrypting them
            // advances the ratchet but there is no body to display.
            if envelope.payload.is_empty() {
                return Ok(String::new());
            }
            if secret.len() != 48 {
                return Err(XmppError::CryptoError(
                    "modern OMEMO key has an unexpected length".into(),
                ));
            }
            let plaintext =
                modern_payload_decrypt(&secret[..32], &envelope.payload, &secret[32..48])?;
            sce_body(&plaintext)
        } else {
            let plaintext = decrypt_payload(&secret, &envelope.iv, &envelope.payload)?;
            String::from_utf8(plaintext)
                .map_err(|e| XmppError::CryptoError(format!("invalid UTF-8: {}", e)))
        }
    }

    /// Encrypt one group-chat body once for every recipient device.
    ///
    /// Each element of `recipients` is a bare JID and that account's bundles.
    /// Untrusted devices are skipped per recipient.
    pub fn encrypt_group_message(
        &self,
        body: &str,
        room: &str,
        recipients: &[(String, Vec<DeviceBundle>)],
    ) -> Result<OmemoEnvelope> {
        let sid = self
            .local
            .read()
            .as_ref()
            .map(|d| d.device_id)
            .ok_or_else(|| XmppError::CryptoError("no local OMEMO device".into()))?;
        let key = random_bytes::<32>();
        let plaintext = sce_envelope(body, Some(room));
        let payload = modern_payload_encrypt(&key, &plaintext)?;
        let tag = modern_payload_tag(&key, &payload)?;
        let mut secret = key.to_vec();
        secret.extend_from_slice(&tag);
        let mut keys = Vec::new();
        for (jid, bundles) in recipients {
            for bundle in bundles {
                if self.trust(jid, bundle.device_id) == TrustLevel::Untrusted || !bundle.verify() {
                    continue;
                }
                let (data, kex) = self.encrypt_key(jid, bundle, &secret)?;
                keys.push(OmemoKey {
                    rid: bundle.device_id,
                    kex,
                    data,
                    jid: Some(jid.clone()),
                });
            }
        }
        if keys.is_empty() {
            return Err(XmppError::CryptoError("no OMEMO recipient devices".into()));
        }
        Ok(OmemoEnvelope {
            sid,
            keys,
            iv: Vec::new(),
            payload,
        })
    }

    /// Decrypt a group message addressed to our device and bare JID.
    pub fn decrypt_group_message(
        &self,
        sender: &str,
        recipient: &str,
        envelope: &OmemoEnvelope,
    ) -> Result<String> {
        let device_id = self
            .local
            .read()
            .as_ref()
            .map(|d| d.device_id)
            .ok_or_else(|| XmppError::CryptoError("no local OMEMO device".into()))?;
        let our_key = envelope
            .keys
            .iter()
            .find(|key| {
                key.rid == device_id && (key.jid.as_deref() == Some(recipient) || key.jid.is_none())
            })
            .ok_or_else(|| XmppError::CryptoError("message is not encrypted for us".into()))?;
        let secret = self.decrypt_key(sender, envelope.sid, &our_key.data, our_key.kex)?;
        if envelope.is_modern() {
            if envelope.payload.is_empty() {
                return Ok(String::new());
            }
            if secret.len() != 48 {
                return Err(XmppError::CryptoError(
                    "modern OMEMO key has an unexpected length".into(),
                ));
            }
            let plaintext =
                modern_payload_decrypt(&secret[..32], &envelope.payload, &secret[32..48])?;
            sce_body(&plaintext)
        } else {
            let plaintext = decrypt_payload(&secret, &envelope.iv, &envelope.payload)?;
            String::from_utf8(plaintext)
                .map_err(|e| XmppError::CryptoError(format!("invalid UTF-8: {}", e)))
        }
    }

    /// Whether a Signal session already exists for this peer device.
    pub fn has_session(&self, jid: &str, device_id: u32) -> bool {
        self.sessions
            .read()
            .get(jid)
            .map(|m| m.contains_key(&device_id))
            .unwrap_or(false)
    }

    /// Encrypt a small payload (the OMEMO message key) for one device.
    ///
    /// Returns the ciphertext and whether it is an OMEMOKeyExchange
    /// (`kex='true'` in the header).
    pub fn encrypt_key(
        &self,
        jid: &str,
        bundle: &DeviceBundle,
        plaintext: &[u8],
    ) -> Result<(Vec<u8>, bool)> {
        let mut sessions = self.sessions.write();
        let map = sessions.entry(jid.to_string()).or_default();

        if let Some(session) = map.get_mut(&bundle.device_id) {
            let data = session
                .encrypt_message(plaintext)
                .map_err(|e| XmppError::CryptoError(e.to_string()))?;
            drop(sessions);
            self.persist();
            return Ok((data, false));
        }

        let local = self
            .local
            .read()
            .clone()
            .ok_or_else(|| XmppError::CryptoError("no local OMEMO device".into()))?;
        let Some(chosen) = bundle
            .prekeys
            .get(rand::random::<usize>() % bundle.prekeys.len().max(1))
        else {
            return Err(XmppError::CryptoError(
                "bundle has no one-time prekey".into(),
            ));
        };
        let chosen_key = array32(&chosen.key)?;
        let x3dh_bundle = device_bundle_to_x3dh(bundle)?;
        let (output, header) = get_shared_secret_active(
            &local.x3dh_state(),
            &x3dh_bundle,
            &[],
            random_bytes::<32>(),
            Some(chosen_key),
            true,
        )
        .map_err(|e| XmppError::CryptoError(e.to_string()))?;
        let mut session = OmemoSession::create_active(
            output.associated_data,
            output.shared_secret.to_vec(),
            array32(&bundle.signed_prekey)?,
            Box::new(OsRngDhPrivProvider::new()),
        )?;
        let auth = session
            .encrypt_message(plaintext)
            .map_err(|e| XmppError::CryptoError(e.to_string()))?;
        let data = build_key_exchange(
            chosen.id,
            bundle.signed_prekey_id,
            header.identity_key,
            header.ephemeral_key,
            &auth,
        )
        .map_err(|e| XmppError::CryptoError(e.to_string()))?;
        map.insert(bundle.device_id, session);
        drop(sessions);
        self.persist();
        Ok((data, true))
    }

    /// Decrypt a key encrypted by a peer device, creating an inbound session
    /// when this is the first (key exchange) message. `device_id` is the
    /// sender's device id from the envelope header (`sid`).
    pub fn decrypt_key(
        &self,
        jid: &str,
        device_id: u32,
        data: &[u8],
        prekey: bool,
    ) -> Result<Vec<u8>> {
        let mut sessions = self.sessions.write();
        if !prekey {
            let session = sessions
                .get_mut(jid)
                .and_then(|map| map.get_mut(&device_id))
                .ok_or_else(|| {
                    XmppError::CryptoError(
                        "no session for sender and message is not a key exchange".into(),
                    )
                })?;
            let plaintext = session
                .decrypt_message(data)
                .map_err(|e| XmppError::CryptoError(e.to_string()))?;
            drop(sessions);
            self.persist();
            return Ok(plaintext);
        }

        let (prekey_id, signed_prekey_id, identity_key, ephemeral_key, auth) =
            parse_key_exchange(data).map_err(|e| XmppError::CryptoError(e.to_string()))?;
        let local = self
            .local
            .read()
            .clone()
            .ok_or_else(|| XmppError::CryptoError("no local OMEMO device".into()))?;
        let signed_public = local
            .signed_prekey_public(signed_prekey_id)
            .ok_or_else(|| {
                XmppError::CryptoError(format!("unknown signed prekey {signed_prekey_id}"))
            })?;
        let one_time = local
            .prekeys
            .iter()
            .find(|prekey| prekey.id == prekey_id)
            .ok_or_else(|| {
                XmppError::CryptoError(format!("unknown one-time prekey {prekey_id}"))
            })?;
        let state = local
            .x3dh_state_for_signed_prekey(signed_prekey_id)
            .ok_or_else(|| {
                XmppError::CryptoError(format!("unknown signed prekey {signed_prekey_id}"))
            })?;
        let header = X3dhHeader {
            identity_key,
            ephemeral_key,
            signed_pre_key: signed_public,
            pre_key: Some(array32(&one_time.public)?),
        };
        let (output, _) = get_shared_secret_passive(&state, &header, &[], true)
            .map_err(|e| XmppError::CryptoError(e.to_string()))?;
        let alice_ratchet =
            peek_dh_pub(&auth).map_err(|e| XmppError::CryptoError(e.to_string()))?;
        let mut session = OmemoSession::create_passive(
            output.associated_data,
            output.shared_secret.to_vec(),
            state.signed_pre_key.priv_key,
            alice_ratchet,
            Box::new(OsRngDhPrivProvider::new()),
        )?;
        let plaintext = session
            .decrypt_message(&auth)
            .map_err(|e| XmppError::CryptoError(e.to_string()))?;

        if let Some(local) = self.local.write().as_mut() {
            local.take_prekey(prekey_id);
        }
        sessions
            .entry(jid.to_string())
            .or_default()
            .insert(device_id, session);
        drop(sessions);
        self.persist();
        Ok(plaintext)
    }

    pub fn set_trust(&self, jid: &str, device_id: u32, level: TrustLevel) {
        self.trust
            .write()
            .entry(jid.to_string())
            .or_default()
            .insert(device_id, level);
        self.persist();
    }

    pub fn trust(&self, jid: &str, device_id: u32) -> TrustLevel {
        self.trust
            .read()
            .get(jid)
            .and_then(|m| m.get(&device_id))
            .copied()
            .unwrap_or(TrustLevel::Undecided)
    }
}

#[derive(Serialize, Deserialize)]
struct StoredPreKey {
    id: u32,
    private: String,
    public: String,
}

#[derive(Serialize, Deserialize)]
struct StoredSignedPreKey {
    id: u32,
    private: String,
    public: String,
    signature: String,
}

#[derive(Serialize, Deserialize)]
struct StoredLocal {
    device_id: u32,
    identity_private: String,
    identity_public: String,
    signed_prekey_id: u32,
    signed_prekey_private: String,
    signed_prekey_public: String,
    signed_prekey_signature: String,
    #[serde(default)]
    signed_prekey_created_at: i64,
    #[serde(default)]
    old_signed_prekeys: Vec<StoredSignedPreKey>,
    prekeys: Vec<StoredPreKey>,
    next_prekey_id: u32,
}

impl StoredLocal {
    fn from_device(device: &LocalDevice) -> Self {
        Self {
            device_id: device.device_id,
            identity_private: STANDARD.encode(device.identity_private),
            identity_public: STANDARD.encode(device.identity_public),
            signed_prekey_id: device.signed_prekey_id,
            signed_prekey_private: STANDARD.encode(device.signed_prekey_private),
            signed_prekey_public: STANDARD.encode(device.signed_prekey_public),
            signed_prekey_signature: STANDARD.encode(device.signed_prekey_signature),
            signed_prekey_created_at: device.signed_prekey_created_at,
            old_signed_prekeys: device
                .old_signed_prekeys
                .iter()
                .map(|key| StoredSignedPreKey {
                    id: key.id,
                    private: STANDARD.encode(key.private),
                    public: STANDARD.encode(key.public),
                    signature: STANDARD.encode(key.signature),
                })
                .collect(),
            prekeys: device
                .prekeys
                .iter()
                .map(|pk| StoredPreKey {
                    id: pk.id,
                    private: STANDARD.encode(pk.private),
                    public: STANDARD.encode(pk.public),
                })
                .collect(),
            next_prekey_id: device.next_prekey_id,
        }
    }

    fn into_device(self) -> Result<LocalDevice> {
        let mut prekeys = Vec::with_capacity(self.prekeys.len());
        for pk in self.prekeys {
            prekeys.push(PreKeyPair {
                id: pk.id,
                private: key32_bytes(&STANDARD.decode(pk.private).map_err(decoding)?)?,
                public: key32_bytes(&STANDARD.decode(pk.public).map_err(decoding)?)?,
            });
        }
        let mut old_signed_prekeys = Vec::with_capacity(self.old_signed_prekeys.len());
        for key in self.old_signed_prekeys {
            old_signed_prekeys.push(SignedPreKey {
                id: key.id,
                private: key32_bytes(&STANDARD.decode(key.private).map_err(decoding)?)?,
                public: key32_bytes(&STANDARD.decode(key.public).map_err(decoding)?)?,
                signature: <[u8; 64]>::try_from(
                    STANDARD.decode(key.signature).map_err(decoding)?.as_slice(),
                )
                .map_err(|_| XmppError::DeserializationError("invalid OMEMO signature".into()))?,
            });
        }
        Ok(LocalDevice {
            device_id: self.device_id,
            identity_private: key32_bytes(
                &STANDARD.decode(self.identity_private).map_err(decoding)?,
            )?,
            identity_public: key32_bytes(
                &STANDARD.decode(self.identity_public).map_err(decoding)?,
            )?,
            signed_prekey_id: self.signed_prekey_id,
            signed_prekey_private: key32_bytes(
                &STANDARD
                    .decode(self.signed_prekey_private)
                    .map_err(decoding)?,
            )?,
            signed_prekey_public: key32_bytes(
                &STANDARD
                    .decode(self.signed_prekey_public)
                    .map_err(decoding)?,
            )?,
            signed_prekey_signature: <[u8; 64]>::try_from(
                STANDARD
                    .decode(self.signed_prekey_signature)
                    .map_err(decoding)?
                    .as_slice(),
            )
            .map_err(|_| XmppError::DeserializationError("invalid OMEMO signature".into()))?,
            signed_prekey_created_at: self.signed_prekey_created_at,
            old_signed_prekeys,
            prekeys,
            next_prekey_id: self.next_prekey_id,
        })
    }
}

#[derive(Serialize, Deserialize)]
struct StoredOmemo {
    version: u32,
    local: Option<StoredLocal>,
    device_lists: HashMap<String, Vec<u32>>,
    trust: HashMap<String, HashMap<u32, TrustLevel>>,
    sessions: HashMap<String, HashMap<u32, String>>,
    #[serde(default)]
    legacy_device_lists: HashMap<String, Vec<u32>>,
    #[serde(default)]
    legacy_bundles: HashMap<String, HashMap<u32, LegacyDeviceBundle>>,
    #[serde(default)]
    legacy_sessions: HashMap<String, HashMap<u32, String>>,
}

fn decoding(e: base64::DecodeError) -> XmppError {
    XmppError::DeserializationError(e.to_string())
}

fn key32_bytes(bytes: &[u8]) -> Result<[u8; 32]> {
    <[u8; 32]>::try_from(bytes)
        .map_err(|_| XmppError::DeserializationError("invalid Curve25519 key length".into()))
}

fn random_bytes<const N: usize>() -> [u8; N] {
    use rand::RngCore;
    let mut bytes = [0u8; N];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes
}

/// Sign a signed-prekey public key with the device identity key.
///
/// OMEMO 2 publishes the identity key in Ed25519 form and signs the raw
/// 32-byte Curve25519 signed prekey. The private identity scalar is clamped
/// before signing so the signature verifies against the same Edwards point
/// that `priv_to_ed25519_pub` publishes.
fn sign_prekey(identity_private: &[u8; 32], signed_prekey_public: &[u8; 32]) -> [u8; 64] {
    sign_with_identity(identity_private, signed_prekey_public)
}

/// Legacy OMEMO 0.3 signs the 33-byte `0x05 || spk` form.
fn sign_prekey_oldmemo(identity_private: &[u8; 32], signed_prekey_public: &[u8; 32]) -> [u8; 64] {
    let mut encoded = [0u8; 33];
    encoded[0] = 0x05;
    encoded[1..].copy_from_slice(signed_prekey_public);
    sign_with_identity(identity_private, &encoded)
}

fn sign_with_identity(identity_private: &[u8; 32], message: &[u8]) -> [u8; 64] {
    use rand::RngCore;
    let mut nonce = [0u8; 64];
    rand::thread_rng().fill_bytes(&mut nonce);
    let mut identity = *identity_private;
    identity[0] &= 0xF8;
    identity[31] &= 0x7F;
    identity[31] |= 0x40;
    omemo_xeddsa::ed25519_priv_sign(&identity, message, &nonce)
}

fn device_bundle_to_x3dh(bundle: &DeviceBundle) -> Result<X3dhBundle> {
    let mut prekeys = Vec::with_capacity(bundle.prekeys.len());
    for prekey in &bundle.prekeys {
        prekeys.push(array32(&prekey.key)?);
    }
    Ok(X3dhBundle {
        identity_key: array32(&bundle.identity_key)?,
        signed_pre_key: array32(&bundle.signed_prekey)?,
        signed_pre_key_sig: <[u8; 64]>::try_from(bundle.signed_prekey_signature.as_slice())
            .map_err(|_| XmppError::CryptoError("invalid signed prekey signature length".into()))?,
        pre_keys: prekeys,
    })
}

fn array32(bytes: &[u8]) -> Result<[u8; 32]> {
    <[u8; 32]>::try_from(bytes)
        .map_err(|_| XmppError::CryptoError("invalid Curve25519 key length".into()))
}

/// Lift a Curve25519 public key to the Ed25519 form used by oldmemo's X3DH
/// header. The sign bit is not transmitted in a key exchange; both choices
/// share the same Montgomery point, so pick the one that round-trips.
fn curve_to_ed25519(curve: &[u8; 32]) -> Result<[u8; 32]> {
    for set_sign_bit in [false, true] {
        let candidate = omemo_xeddsa::curve25519_pub_to_ed25519_pub(curve, set_sign_bit);
        if omemo_xeddsa::ed25519_pub_to_curve25519_pub(&candidate)
            .ok()
            .as_ref()
            == Some(curve)
        {
            return Ok(candidate);
        }
    }
    Err(XmppError::CryptoError("invalid legacy identity key".into()))
}

fn decode_child(el: &Element, name: &str) -> Option<Vec<u8>> {
    STANDARD
        .decode(el.get_child(name, NSChoice::Any)?.text().trim())
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_verifiable_bundle() {
        let local = LocalDevice::generate(1234, 5);
        let bundle = local.bundle();
        assert_eq!(bundle.device_id, 1234);
        assert_eq!(bundle.prekeys.len(), 5);
        assert!(
            bundle.verify(),
            "signed prekey must verify against identity"
        );

        let element = bundle.to_element();
        let parsed = DeviceBundle::from_element(&element, 1234).unwrap();
        assert_eq!(parsed, bundle);
        assert!(parsed.verify());
    }

    #[test]
    fn skips_untrusted_devices() {
        let alice = OmemoManager::new();
        alice.set_local(LocalDevice::generate(1, 2));
        let bob = LocalDevice::generate(2, 2);
        alice.set_trust("bob@example.com", 2, TrustLevel::Untrusted);
        assert!(alice
            .encrypt_message("bob@example.com", "hi", &[bob.bundle()])
            .is_err());
        alice.set_trust("bob@example.com", 2, TrustLevel::Trusted);
        assert!(alice
            .encrypt_message("bob@example.com", "hi", &[bob.bundle()])
            .is_ok());
    }

    #[test]
    fn signed_prekey_rotation_keeps_the_old_key() {
        let manager = OmemoManager::new();
        let mut device = LocalDevice::generate(7, 2);
        let old_id = device.signed_prekey_id;
        let old_private = device.signed_prekey_private;
        let new_bundle = device.rotate_signed_prekey();
        assert_ne!(new_bundle.signed_prekey_id, old_id);
        assert_eq!(device.signed_prekey_private(old_id), Some(old_private));
        assert!(new_bundle.verify());

        manager.set_local(device);
        let exported = manager.export_state();
        let restored = OmemoManager::new();
        restored.import_state(&exported).unwrap();
        let restored = restored.local().expect("local");
        assert_eq!(restored.signed_prekey_private(old_id), Some(old_private));
    }

    #[test]
    fn delayed_message_uses_a_rotated_signed_prekey() {
        let bob = OmemoManager::new();
        let mut bob_device = LocalDevice::generate(2, 2);
        let old_bundle = bob_device.bundle();
        bob_device.rotate_signed_prekey();
        bob.set_local(bob_device);

        let alice = OmemoManager::new();
        alice.set_local(LocalDevice::generate(1, 2));
        let envelope = alice
            .encrypt_message("bob@example.com", "delayed hello", &[old_bundle])
            .unwrap();
        assert_eq!(
            bob.decrypt_message("alice@example.com", &envelope).unwrap(),
            "delayed hello"
        );
    }

    #[derive(Deserialize)]
    struct TwoWayFixture {
        shared_secret_hex: String,
        associated_data_hex: String,
        bob_spk_priv_hex: String,
        alice_priv_queue_hex: Vec<String>,
        bob_priv_queue_hex: Vec<String>,
        kex0_hex: String,
        kex0_plaintext_hex: String,
        messages: Vec<TwoWayMessage>,
    }

    #[derive(Deserialize)]
    struct TwoWayMessage {
        label: String,
        plaintext_hex: String,
        ciphertext_hex: String,
    }

    fn hex_bytes(value: &str) -> Vec<u8> {
        hex::decode(value).expect("valid fixture hex")
    }

    fn hex32(value: &str) -> [u8; 32] {
        hex_bytes(value).try_into().expect("32-byte fixture key")
    }

    /// Replay a deterministic python-twomemo conversation.
    ///
    /// Bob's first reply is the critical case: it is authenticated with the
    /// same initiator-first X3DH associated data as Alice's outgoing message.
    /// A swapped AD (the old `TwomemoSession` behaviour) fails here even though
    /// an in-process round-trip still passes. The second message is empty and
    /// must still advance Alice's receiving chain.
    #[test]
    fn decrypts_python_twomemo_two_way() {
        let fixture: TwoWayFixture =
            serde_json::from_str(include_str!("../tests/fixtures/omemo_twomemo_two_way.json"))
                .expect("fixture parses");
        let ad = hex_bytes(&fixture.associated_data_hex);
        let shared_secret = hex_bytes(&fixture.shared_secret_hex);

        let bob_spk_priv = hex32(&fixture.bob_spk_priv_hex);
        let bob_spk_pub = omemo_xeddsa::priv_to_curve25519_pub(&bob_spk_priv);
        let alice_privs = fixture
            .alice_priv_queue_hex
            .iter()
            .map(|value| hex32(value))
            .collect();
        let bob_privs = fixture
            .bob_priv_queue_hex
            .iter()
            .map(|value| hex32(value))
            .collect();

        let mut alice = OmemoSession::create_active(
            ad.clone(),
            shared_secret.clone(),
            bob_spk_pub,
            omemo_twomemo::fixed_priv_provider(alice_privs),
        )
        .unwrap();

        let (pk_id, spk_id, _ik, _ek, auth) =
            parse_key_exchange(&hex_bytes(&fixture.kex0_hex)).unwrap();
        assert_eq!((pk_id, spk_id), (100, 1));
        let alice_first_ratchet_pub = peek_dh_pub(&auth).unwrap();
        let mut bob = OmemoSession::create_passive(
            ad,
            shared_secret,
            bob_spk_priv,
            alice_first_ratchet_pub,
            omemo_twomemo::fixed_priv_provider(bob_privs),
        )
        .unwrap();

        assert_eq!(
            bob.decrypt_message(&auth).unwrap(),
            hex_bytes(&fixture.kex0_plaintext_hex)
        );

        for message in &fixture.messages {
            let decrypted = alice
                .decrypt_message(&hex_bytes(&message.ciphertext_hex))
                .unwrap();
            assert_eq!(
                decrypted,
                hex_bytes(&message.plaintext_hex),
                "{}",
                message.label
            );
        }
        assert_eq!(alice.skipped.len(), 0);
    }

    #[test]
    fn replies_round_trip_through_the_same_session() {
        let alice = OmemoManager::new();
        let alice_device = LocalDevice::generate(1, 5);
        alice.set_local(alice_device.clone());
        let bob = OmemoManager::new();
        let bob_device = LocalDevice::generate(2, 5);
        bob.set_local(bob_device.clone());

        let to_bob = alice
            .encrypt_message("bob@example.com", "ping", &[bob_device.bundle()])
            .unwrap();
        assert_eq!(
            bob.decrypt_message("alice@example.com", &to_bob).unwrap(),
            "ping"
        );

        // An empty OMEMO message must advance the ratchet without a body.
        let (data, kex) = bob
            .encrypt_key("alice@example.com", &alice_device.bundle(), &[0u8; 32])
            .unwrap();
        let empty = OmemoEnvelope {
            sid: bob_device.device_id,
            keys: vec![OmemoKey {
                rid: alice_device.device_id,
                kex,
                data,
                jid: None,
            }],
            iv: Vec::new(),
            payload: Vec::new(),
        };
        assert_eq!(
            alice.decrypt_message("bob@example.com", &empty).unwrap(),
            ""
        );

        let to_alice = bob
            .encrypt_message("alice@example.com", "pong", &[alice_device.bundle()])
            .unwrap();
        assert_eq!(
            alice.decrypt_message("bob@example.com", &to_alice).unwrap(),
            "pong"
        );
    }

    #[test]
    fn legacy_replies_round_trip_and_persist() {
        let alice = OmemoManager::new();
        let alice_device = LocalDevice::generate(11, 5);
        alice.set_local(alice_device.clone());
        let bob = OmemoManager::new();
        let bob_device = LocalDevice::generate(22, 5);
        bob.set_local(bob_device.clone());
        assert!(bob_device.legacy_bundle().verify());

        let to_bob = alice
            .encrypt_legacy_message("bob@example.com", "ping", &[bob_device.legacy_bundle()])
            .unwrap();
        assert_eq!(
            bob.decrypt_legacy_message("alice@example.com", &to_bob)
                .unwrap(),
            "ping"
        );

        let to_alice = bob
            .encrypt_legacy_message("alice@example.com", "pong", &[alice_device.legacy_bundle()])
            .unwrap();
        assert_eq!(
            alice
                .decrypt_legacy_message("bob@example.com", &to_alice)
                .unwrap(),
            "pong"
        );

        let exported = alice.export_state();
        let restored = OmemoManager::new();
        restored.import_state(&exported).unwrap();
        let next = bob
            .encrypt_legacy_message(
                "alice@example.com",
                "again",
                &[alice_device.legacy_bundle()],
            )
            .unwrap();
        assert_eq!(
            restored
                .decrypt_legacy_message("bob@example.com", &next)
                .unwrap(),
            "again"
        );
    }

    #[test]
    fn replenishes_low_prekeys() {
        let manager = OmemoManager::new();
        manager.set_local(LocalDevice::generate(9, 2));
        assert!(manager.replenish_prekeys(1, 80).is_none());
        let bundle = manager.replenish_prekeys(50, 5).unwrap();
        assert_eq!(bundle.prekeys.len(), 7);
        let ids: Vec<u32> = bundle.prekeys.iter().map(|p| p.id).collect();
        assert_eq!(ids, vec![1, 2, 3, 4, 5, 6, 7]);
        assert!(bundle.verify());
    }

    #[test]
    fn persists_and_restores_state() {
        let manager = OmemoManager::new();
        let device = LocalDevice::generate(42, 3);
        manager.set_local(device.clone());
        manager.set_device_list("bob@example.com", vec![7]);
        manager.set_trust("bob@example.com", 7, TrustLevel::Trusted);

        let exported = manager.export_state();
        let restored = OmemoManager::new();
        restored.import_state(&exported).unwrap();

        let local = restored.local().unwrap();
        assert_eq!(local.device_id, 42);
        assert_eq!(local.identity_public, device.identity_public);
        assert_eq!(local.prekeys.len(), 3);
        assert!(local.bundle().verify());
        assert_eq!(restored.device_list("bob@example.com"), vec![7]);
        assert_eq!(restored.trust("bob@example.com", 7), TrustLevel::Trusted);
    }

    #[test]
    fn persisted_state_keeps_session() {
        let alice = OmemoManager::new();
        alice.set_local(LocalDevice::generate(1, 2));
        let bob_local = LocalDevice::generate(2, 2);
        let bob = OmemoManager::new();
        bob.set_local(bob_local.clone());

        let envelope = alice
            .encrypt_message("bob@example.com", "hello", &[bob_local.bundle()])
            .unwrap();

        let restored = OmemoManager::new();
        restored.import_state(&alice.export_state()).unwrap();
        let envelope2 = restored
            .encrypt_message("bob@example.com", "again", &[bob_local.bundle()])
            .unwrap();
        assert_eq!(envelope.sid, envelope2.sid);
        assert!(!envelope2.keys[0].kex, "session must survive a restore");
    }

    #[test]
    fn tampered_bundle_fails_verification() {
        let local = LocalDevice::generate(1, 1);
        let mut bundle = local.bundle();
        bundle.signed_prekey[0] ^= 0xff;
        assert!(!bundle.verify());
    }

    #[test]
    fn envelope_roundtrip() {
        let envelope = OmemoEnvelope {
            sid: 10,
            keys: vec![
                OmemoKey {
                    rid: 20,
                    kex: true,
                    data: vec![1, 2, 3],
                    jid: None,
                },
                OmemoKey {
                    rid: 21,
                    kex: false,
                    data: vec![4, 5],
                    jid: None,
                },
            ],
            iv: vec![0; 12],
            payload: vec![9; 32],
        };
        let parsed = OmemoEnvelope::from_element(&envelope.to_element()).unwrap();
        assert_eq!(parsed, envelope);
    }

    #[test]
    fn grouped_envelope_roundtrip() {
        let envelope = OmemoEnvelope {
            sid: 10,
            keys: vec![
                OmemoKey {
                    rid: 21,
                    kex: false,
                    data: vec![4, 5],
                    jid: Some("alice@example.com".to_string()),
                },
                OmemoKey {
                    rid: 20,
                    kex: true,
                    data: vec![1, 2, 3],
                    jid: Some("bob@example.com".to_string()),
                },
            ],
            iv: vec![0; 12],
            payload: vec![9; 32],
        };
        let element = envelope.to_element();
        assert!(element
            .get_child("header", NS_OMEMO)
            .and_then(|header| header.get_child("keys", NS_OMEMO))
            .is_some());
        let parsed = OmemoEnvelope::from_element(&element).unwrap();
        assert_eq!(parsed, envelope);
        assert!(parsed.has_grouped_keys());
    }

    #[test]
    fn group_message_roundtrip() {
        let alice = OmemoManager::new();
        let alice_local = LocalDevice::generate(1, 2);
        alice.set_local(alice_local.clone());
        let bob = OmemoManager::new();
        let bob_local = LocalDevice::generate(2, 2);
        bob.set_local(bob_local.clone());

        let envelope = alice
            .encrypt_group_message(
                "hello room",
                "room@conference.example.com",
                &[
                    ("bob@example.com".to_string(), vec![bob_local.bundle()]),
                    ("alice@example.com".to_string(), vec![alice_local.bundle()]),
                ],
            )
            .unwrap();
        assert!(envelope.has_grouped_keys());
        assert!(envelope.is_modern());
        let element = envelope.to_element();
        let header = element.get_child("header", NS_OMEMO).expect("header");
        assert!(header.get_child("iv", NS_OMEMO).is_none());
        let keys = header.get_child("keys", NS_OMEMO).expect("keys");
        let key = keys.get_child("key", NS_OMEMO).expect("key");
        assert!(key.attr("jid").is_none());
        assert_eq!(
            bob.decrypt_group_message("alice@example.com", "bob@example.com", &envelope)
                .unwrap(),
            "hello room"
        );
    }

    #[test]
    fn payload_encrypt_decrypt() {
        let (key, iv) = random_payload_key();
        let ciphertext = encrypt_payload(&key, &iv, b"hello omemo").unwrap();
        assert_ne!(&ciphertext, b"hello omemo");
        let plain = decrypt_payload(&key, &iv, &ciphertext).unwrap();
        assert_eq!(plain, b"hello omemo");

        let (other, _) = random_payload_key();
        assert!(decrypt_payload(&other, &iv, &ciphertext).is_err());
    }

    #[test]
    fn signal_session_roundtrip() {
        let alice = OmemoManager::new();
        let bob = OmemoManager::new();
        let alice_local = alice.ensure_local(1, 5);
        let bob_local = bob.ensure_local(2, 5);
        let bob_bundle = bob_local.bundle();
        assert!(bob_bundle.verify());

        let message_key = b"0123456789abcdef";
        let (cipher, prekey) = alice
            .encrypt_key("bob@example.com", &bob_bundle, message_key)
            .unwrap();
        assert!(prekey, "first message must be a prekey message");
        let plain = bob
            .decrypt_key("alice@example.com", 1, &cipher, true)
            .unwrap();
        assert_eq!(plain, message_key);

        // Bob can reply over the session created while decrypting.
        let (reply, reply_prekey) = bob
            .encrypt_key("alice@example.com", &alice_local.bundle(), b"reply")
            .unwrap();
        assert!(!reply_prekey);
        let plain = alice
            .decrypt_key("bob@example.com", 2, &reply, false)
            .unwrap();
        assert_eq!(plain, b"reply");
    }

    #[test]
    fn manager_stores_state() {
        let manager = OmemoManager::new();
        let local = manager.ensure_local(42, 3);
        assert_eq!(local.device_id, 42);
        assert!(manager.is_enabled());

        manager.set_device_list("alice@example.com", vec![7, 8]);
        assert_eq!(manager.device_list("alice@example.com"), vec![7, 8]);

        manager.set_bundle("alice@example.com", local.bundle());
        assert!(manager.bundle("alice@example.com", 42).is_some());

        assert_eq!(
            manager.trust("alice@example.com", 42),
            TrustLevel::Undecided
        );
        manager.set_trust("alice@example.com", 42, TrustLevel::Trusted);
        assert_eq!(manager.trust("alice@example.com", 42), TrustLevel::Trusted);
    }
}
