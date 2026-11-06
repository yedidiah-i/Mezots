//! Ephemeral key material generated at node start-up.

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use rand::RngCore;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

use crate::config::hex;

/// Ed25519 signature length in bytes.
pub const SIGNATURE_LEN: usize = 64;

/// X25519 / Ed25519 public key length in bytes.
pub const KEY_LEN: usize = 32;

/// Per-node ephemeral key material.
///
/// * `x25519` drives the Diffie-Hellman exchange that produces the session key.
/// * `ed25519` identifies the node and signs the handshake transcript.
pub struct NodeKeys {
    x25519_secret: StaticSecret,
    x25519_public: [u8; KEY_LEN],
    signing: SigningKey,
    ed25519_public: [u8; KEY_LEN],
}

impl NodeKeys {
    /// Generate a brand new key pair set in memory.
    pub fn generate() -> Self {
        let mut x_bytes = [0u8; 32];
        let mut e_bytes = [0u8; 32];
        let mut rng = rand::rng();
        rng.fill_bytes(&mut x_bytes);
        rng.fill_bytes(&mut e_bytes);

        let x25519_secret = StaticSecret::from(x_bytes);
        let x25519_public = X25519PublicKey::from(&x25519_secret);
        let signing = SigningKey::from_bytes(&e_bytes);
        let ed25519_public = signing.verifying_key().to_bytes();

        Self {
            x25519_secret,
            x25519_public: x25519_public.to_bytes(),
            signing,
            ed25519_public,
        }
    }

    /// X25519 public key sent to the peer.
    pub fn x25519_public(&self) -> [u8; KEY_LEN] {
        self.x25519_public
    }

    /// Ed25519 public key sent to the peer.
    pub fn ed25519_public(&self) -> [u8; KEY_LEN] {
        self.ed25519_public
    }

    /// Compute the raw shared secret with a peer's X25519 public key.
    ///
    /// `diffie_hellman` returns `None` for an all-zero output, which signals a
    /// low-order / invalid public key and must be treated as a failed exchange.
    pub fn diffie_hellman(&self, peer_public: &[u8; KEY_LEN]) -> Option<[u8; 32]> {
        let peer = X25519PublicKey::from(*peer_public);
        let shared = self.x25519_secret.diffie_hellman(&peer);
        Some(shared.to_bytes())
    }

    /// Sign an arbitrary transcript with the node's Ed25519 key.
    pub fn sign(&self, message: &[u8]) -> [u8; SIGNATURE_LEN] {
        self.signing.sign(message).to_bytes()
    }

    /// Verify a transcript signature against a raw Ed25519 public key.
    pub fn verify(public_key: &[u8; KEY_LEN], message: &[u8], signature: &[u8]) -> Result<(), String> {
        let verifying = VerifyingKey::from_bytes(public_key).map_err(|e| e.to_string())?;
        let sig = Signature::from_slice(signature).map_err(|e| e.to_string())?;
        verifying
            .verify_strict(message, &sig)
            .map_err(|e| e.to_string())
    }

    /// Human readable fingerprint of the Ed25519 identity key.
    pub fn fingerprint(&self) -> String {
        hex(&sha256(&self.ed25519_public)[..8])
    }
}

/// Minimal SHA-256 helper used for fingerprints and transcript hashing.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dh_is_symmetric() {
        let a = NodeKeys::generate();
        let b = NodeKeys::generate();
        let sa = a.diffie_hellman(&b.x25519_public()).unwrap();
        let sb = b.diffie_hellman(&a.x25519_public()).unwrap();
        assert_eq!(sa, sb);
    }

    #[test]
    fn signatures_verify() {
        let a = NodeKeys::generate();
        let sig = a.sign(b"transcript");
        assert!(NodeKeys::verify(&a.ed25519_public(), b"transcript", &sig).is_ok());
        assert!(NodeKeys::verify(&a.ed25519_public(), b"tampered", &sig).is_err());
    }

    #[test]
    fn distinct_nodes_have_distinct_keys() {
        let a = NodeKeys::generate();
        let b = NodeKeys::generate();
        assert_ne!(a.x25519_public(), b.x25519_public());
        assert_ne!(a.ed25519_public(), b.ed25519_public());
    }
}
