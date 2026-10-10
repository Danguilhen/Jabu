//! XEP-0384 OMEMO: device lists, bundles, trust and encrypted envelopes.
//!
//! The wire format, key bundles and payload encryption live here. Per-device
//! Signal sessions are layered on top by a session backend.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use minidom::{Element, NSChoice};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use x25519_dalek::{PublicKey, StaticSecret};

use dziber_omemo::signal_ratchet::{
    proto::PreKeySignalMessage, IdentityKeyPair, KeyPair, PreKeyBundle, Session,
};

use crate::{Result, XmppError};

pub const NS_OMEMO: &str = "urn:xmpp:omemo:2";
pub const NS_DEVICES: &str = "urn:xmpp:omemo:2:devices";
pub const NS_BUNDLES: &str = "urn:xmpp:omemo:2:bundles";
pub const NS_TRUST: &str = "urn:xmpp:omemo:2:trust";

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

    /// Verify the signed prekey against the bundle's identity key (XEdDSA).
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
        let key = xeddsa::xed25519::PublicKey(identity);
        let mut to_verify = [0u8; 33];
        to_verify[0] = 0x05;
        to_verify[1..].copy_from_slice(&spk);
        use xeddsa::Verify as _;
        key.verify(&to_verify, &sig).is_ok()
    }
}

/// A one-time prekey with its private half, kept only locally.
#[derive(Debug, Clone)]
pub struct PreKeyPair {
    pub id: u32,
    pub private: [u8; 32],
    pub public: [u8; 32],
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
    pub prekeys: Vec<PreKeyPair>,
    pub next_prekey_id: u32,
}

impl LocalDevice {
    /// Generate a fresh identity, signed prekey and `prekey_count` one-time prekeys.
    pub fn generate(device_id: u32, prekey_count: usize) -> Self {
        let identity_private = random_bytes::<32>();
        let identity_secret = StaticSecret::from(identity_private);
        let identity_public = PublicKey::from(&identity_secret).to_bytes();

        let signed_prekey_private = random_bytes::<32>();
        let signed_prekey_secret = StaticSecret::from(signed_prekey_private);
        let signed_prekey_public = PublicKey::from(&signed_prekey_secret).to_bytes();

        use xeddsa::Sign as _;
        let signing_key = xeddsa::xed25519::PrivateKey::from(&identity_secret);
        let mut to_sign = [0u8; 33];
        to_sign[0] = 0x05;
        to_sign[1..].copy_from_slice(&signed_prekey_public);
        let signature = signing_key.sign(&to_sign, rand::thread_rng());

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

    pub fn identity_secret(&self) -> StaticSecret {
        StaticSecret::from(self.identity_private)
    }

    pub fn signed_prekey_secret(&self) -> StaticSecret {
        StaticSecret::from(self.signed_prekey_private)
    }

    pub fn take_prekey(&mut self, id: u32) -> Option<PreKeyPair> {
        let pos = self.prekeys.iter().position(|pk| pk.id == id)?;
        Some(self.prekeys.remove(pos))
    }
}

/// One recipient key inside an OMEMO header.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OmemoKey {
    pub rid: u32,
    pub kex: bool,
    pub data: Vec<u8>,
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
        for key in &self.keys {
            header = header.append(
                Element::builder("key", NS_OMEMO)
                    .attr("rid", key.rid.to_string())
                    .attr("kex", if key.kex { "true" } else { "false" })
                    .append(STANDARD.encode(&key.data)),
            );
        }
        header = header.append(Element::builder("iv", NS_OMEMO).append(STANDARD.encode(&self.iv)));

        Element::builder("encrypted", NS_OMEMO)
            .append(header.build())
            .append(Element::builder("payload", NS_OMEMO).append(STANDARD.encode(&self.payload)))
            .build()
    }

    pub fn from_element(encrypted: &Element) -> Option<Self> {
        let header = encrypted.get_child("header", NSChoice::Any)?;
        let sid = header.attr("sid")?.parse().ok()?;
        let keys = header
            .children()
            .filter(|c| c.name() == "key")
            .filter_map(|k| {
                Some(OmemoKey {
                    rid: k.attr("rid")?.parse().ok()?,
                    kex: k.attr("kex") == Some("true"),
                    data: STANDARD.decode(k.text().trim()).ok()?,
                })
            })
            .collect();
        let iv = header
            .get_child("iv", NSChoice::Any)
            .and_then(|iv| STANDARD.decode(iv.text().trim()).ok())
            .unwrap_or_default();
        let payload = STANDARD
            .decode(encrypted.get_child("payload", NSChoice::Any)?.text().trim())
            .ok()?;

        Some(Self {
            sid,
            keys,
            iv,
            payload,
        })
    }
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

/// Local OMEMO state: our device, peer device lists, bundles and trust.
#[derive(Debug, Default)]
pub struct OmemoManager {
    local: RwLock<Option<LocalDevice>>,
    device_lists: RwLock<HashMap<String, Vec<u32>>>,
    bundles: RwLock<HashMap<String, HashMap<u32, DeviceBundle>>>,
    trust: RwLock<HashMap<String, HashMap<u32, TrustLevel>>>,
    sessions: RwLock<HashMap<String, HashMap<u32, Session>>>,
}

impl OmemoManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn local(&self) -> Option<LocalDevice> {
        self.local.read().clone()
    }

    pub fn set_local(&self, device: LocalDevice) {
        *self.local.write() = Some(device);
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

    pub fn is_enabled(&self) -> bool {
        self.local.read().is_some()
    }

    pub fn set_device_list(&self, jid: &str, devices: Vec<u32>) {
        self.device_lists.write().insert(jid.to_string(), devices);
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
        if bundles.is_empty() {
            return Err(XmppError::CryptoError("no OMEMO recipient devices".into()));
        }

        let (key, iv) = random_payload_key();
        let payload = encrypt_payload(&key, &iv, body.as_bytes())?;
        let mut keys = Vec::with_capacity(bundles.len());
        for bundle in bundles {
            let (data, kex) = self.encrypt_key(jid, bundle, &key)?;
            keys.push(OmemoKey {
                rid: bundle.device_id,
                kex,
                data,
            });
        }

        Ok(OmemoEnvelope {
            sid,
            keys,
            iv: iv.to_vec(),
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
        let key = self.decrypt_key(jid, envelope.sid, &our_key.data, our_key.kex)?;
        let plaintext = decrypt_payload(&key, &envelope.iv, &envelope.payload)?;
        String::from_utf8(plaintext)
            .map_err(|e| XmppError::CryptoError(format!("invalid UTF-8: {}", e)))
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
    /// Returns the ciphertext and whether it is a PreKeySignalMessage
    /// (`kex='true'` in the header).
    pub fn encrypt_key(
        &self,
        jid: &str,
        bundle: &DeviceBundle,
        plaintext: &[u8],
    ) -> Result<(Vec<u8>, bool)> {
        let mut rng = rand::thread_rng();
        let mut sessions = self.sessions.write();
        let map = sessions.entry(jid.to_string()).or_default();

        if let Some(session) = map.get_mut(&bundle.device_id) {
            let message = session
                .encrypt(plaintext, &mut rng)
                .map_err(|e| XmppError::CryptoError(e.to_string()))?;
            let prekey = message.is_prekey();
            return Ok((message.into_bytes(), prekey));
        }

        let local = self
            .local
            .read()
            .clone()
            .ok_or_else(|| XmppError::CryptoError("no local OMEMO device".into()))?;
        let identity = IdentityKeyPair::from_secret(local.identity_private);
        let bundle = to_signal_bundle(bundle)?;
        let mut session = Session::new_alice(&identity, &bundle, &mut rng)
            .map_err(|e| XmppError::CryptoError(e.to_string()))?;
        let message = session
            .encrypt(plaintext, &mut rng)
            .map_err(|e| XmppError::CryptoError(e.to_string()))?;
        let prekey = message.is_prekey();
        map.insert(bundle.device_id, session);
        Ok((message.into_bytes(), prekey))
    }

    /// Decrypt a key encrypted by a peer device, creating an inbound session
    /// when this is the first (prekey) message. `device_id` is the sender's
    /// device id from the envelope header (`sid`).
    pub fn decrypt_key(
        &self,
        jid: &str,
        device_id: u32,
        data: &[u8],
        prekey: bool,
    ) -> Result<Vec<u8>> {
        let mut sessions = self.sessions.write();
        if let Some(session) = sessions.get_mut(jid).and_then(|m| m.get_mut(&device_id)) {
            return session
                .decrypt(data, prekey)
                .map_err(|e| XmppError::CryptoError(e.to_string()));
        }

        if !prekey {
            return Err(XmppError::CryptoError(
                "no session for sender and message is not a prekey message".into(),
            ));
        }
        let body = data
            .get(1..)
            .ok_or_else(|| XmppError::CryptoError("empty prekey message".into()))?;
        let parsed =
            PreKeySignalMessage::decode(body).map_err(|e| XmppError::CryptoError(e.to_string()))?;
        let prekey_id = parsed.pre_key_id;

        let local = self
            .local
            .read()
            .clone()
            .ok_or_else(|| XmppError::CryptoError("no local OMEMO device".into()))?;
        let one_time = local
            .prekeys
            .iter()
            .find(|pk| pk.id == prekey_id)
            .ok_or_else(|| {
                XmppError::CryptoError(format!("unknown one-time prekey {}", prekey_id))
            })?;

        let identity = IdentityKeyPair::from_secret(local.identity_private);
        let signed = KeyPair::from_secret(local.signed_prekey_private);
        let one_time = KeyPair::from_secret(one_time.private);
        let (mut session, plaintext, used) = Session::new_bob(&identity, &signed, &one_time, data)
            .map_err(|e| XmppError::CryptoError(e.to_string()))?;

        if let Some(local) = self.local.write().as_mut() {
            local.take_prekey(used);
        }
        sessions
            .entry(jid.to_string())
            .or_default()
            .insert(device_id, session);
        Ok(plaintext)
    }

    pub fn set_trust(&self, jid: &str, device_id: u32, level: TrustLevel) {
        self.trust
            .write()
            .entry(jid.to_string())
            .or_default()
            .insert(device_id, level);
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

fn random_bytes<const N: usize>() -> [u8; N] {
    use rand::RngCore;
    let mut bytes = [0u8; N];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes
}

fn to_signal_bundle(bundle: &DeviceBundle) -> Result<PreKeyBundle> {
    let identity = key32(&bundle.identity_key)?;
    let signed_prekey = key32(&bundle.signed_prekey)?;
    let signature = <[u8; 64]>::try_from(bundle.signed_prekey_signature.as_slice())
        .map_err(|_| XmppError::CryptoError("invalid signed prekey signature length".into()))?;
    let prekey = bundle
        .prekeys
        .first()
        .ok_or_else(|| XmppError::CryptoError("bundle has no one-time prekey".into()))?;
    Ok(PreKeyBundle {
        registration_id: 0,
        device_id: bundle.device_id,
        signed_pre_key_id: bundle.signed_prekey_id,
        signed_pre_key_public: signed_prekey,
        signed_pre_key_signature: signature,
        identity_key: identity,
        pre_key_id: prekey.id,
        pre_key_public: key32(&prekey.key)?,
    })
}

fn key32(bytes: &[u8]) -> Result<[u8; 32]> {
    <[u8; 32]>::try_from(bytes)
        .map_err(|_| XmppError::CryptoError("invalid Curve25519 key length".into()))
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
                },
                OmemoKey {
                    rid: 21,
                    kex: false,
                    data: vec![4, 5],
                },
            ],
            iv: vec![0; 12],
            payload: vec![9; 32],
        };
        let parsed = OmemoEnvelope::from_element(&envelope.to_element()).unwrap();
        assert_eq!(parsed, envelope);
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
