//! Trust-on-first-use pinning of peer identities.
//!
//! The handshake proves that a peer holds the private half of the Ed25519 key
//! it presented - but it does not prove that the peer is *the same one* as
//! last time. An attacker who joins the LAN later and publishes a different
//! key for a node id it has already seen would pass the handshake.
//!
//! This store closes that gap: the first time a node id is seen its Ed25519
//! key is pinned, and every later connection must present exactly that key.
//! A changed key is reported as [`CryptoError::IdentityMismatch`] and the
//! connection is refused.

use std::collections::HashMap;
use std::sync::RwLock;

use super::keys::KEY_LEN;
use super::CryptoError;

/// What the trust store knows about a peer identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityStatus {
    /// The peer is seen for the first time.
    FirstContact,
    /// The peer presents the key that was pinned earlier.
    Known,
    /// The peer presents a different key than the one that was pinned.
    Changed,
}

/// In-memory store of pinned peer identities.
#[derive(Debug, Default)]
pub struct TrustStore {
    pins: RwLock<HashMap<String, [u8; KEY_LEN]>>,
}

impl TrustStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self {
            pins: RwLock::new(HashMap::new()),
        }
    }

    /// Look at a presented key without changing the store.
    pub fn inspect(&self, node_id: &str, key: [u8; KEY_LEN]) -> IdentityStatus {
        match self.pins.read().expect("trust store poisoned").get(node_id) {
            Some(pinned) if pinned != &key => IdentityStatus::Changed,
            Some(_) => IdentityStatus::Known,
            None => IdentityStatus::FirstContact,
        }
    }

    /// Pin a key for a node id, if it is not pinned yet.
    ///
    /// Returns `true` when the key was newly pinned and `false` when the node
    /// id was already known. Use [`TrustStore::verify`] afterwards to check a
    /// presented key against the pin.
    pub fn pin(&self, node_id: &str, key: [u8; KEY_LEN]) -> bool {
        let mut pins = self.pins.write().expect("trust store poisoned");
        if pins.contains_key(node_id) {
            return false;
        }
        pins.insert(node_id.to_string(), key);
        true
    }

    /// Replace the pinned key of a node id.
    pub fn repin(&self, node_id: &str, key: [u8; KEY_LEN]) {
        self.pins
            .write()
            .expect("trust store poisoned")
            .insert(node_id.to_string(), key);
    }

    /// Check a presented key against the pin for `node_id`.
    ///
    /// Unknown node ids are accepted and pinned, so the first connection from
    /// a peer always succeeds.
    pub fn verify(&self, node_id: &str, key: [u8; KEY_LEN]) -> Result<(), CryptoError> {
        let mut pins = self.pins.write().expect("trust store poisoned");
        match pins.get(node_id) {
            Some(pinned) if pinned != &key => Err(CryptoError::IdentityMismatch),
            Some(_) => Ok(()),
            None => {
                pins.insert(node_id.to_string(), key);
                Ok(())
            }
        }
    }

    /// Whether the node id has a pinned key.
    pub fn is_pinned(&self, node_id: &str) -> bool {
        self.pins
            .read()
            .expect("trust store poisoned")
            .contains_key(node_id)
    }

    /// Remove a pin, e.g. after the user explicitly accepts a new identity.
    pub fn unpin(&self, node_id: &str) {
        self.pins
            .write()
            .expect("trust store poisoned")
            .remove(node_id);
    }

    /// Number of pinned identities.
    pub fn len(&self) -> usize {
        self.pins.read().expect("trust store poisoned").len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_contact_pins_and_later_contacts_must_match() {
        let store = TrustStore::new();
        assert!(store.verify("peer-a", [1u8; 32]).is_ok());
        assert!(store.is_pinned("peer-a"));
        assert!(store.verify("peer-a", [1u8; 32]).is_ok());
        assert!(store.verify("peer-a", [2u8; 32]).is_err());
    }

    #[test]
    fn inspect_reports_the_identity_status() {
        let store = TrustStore::new();
        assert_eq!(
            store.inspect("peer-a", [1u8; 32]),
            IdentityStatus::FirstContact
        );
        store.pin("peer-a", [1u8; 32]);
        assert_eq!(store.inspect("peer-a", [1u8; 32]), IdentityStatus::Known);
        assert_eq!(store.inspect("peer-a", [2u8; 32]), IdentityStatus::Changed);
    }

    #[test]
    fn unpinning_allows_a_new_identity() {
        let store = TrustStore::new();
        store.pin("peer-b", [3u8; 32]);
        assert!(store.verify("peer-b", [9u8; 32]).is_err());
        store.unpin("peer-b");
        assert!(store.verify("peer-b", [9u8; 32]).is_ok());
    }
}
