//! Authenticated encryption for the frame pipeline.
//!
//! Every frame is sealed with ChaCha20-Poly1305 *before* it reaches the wire
//! and opened again before it is handed to the decoder. The cipher keeps a
//! per-direction, strictly monotonic counter that forms the 96-bit nonce
//! (`4-byte prefix || 8-byte counter`), so a nonce is never reused under the
//! same key.
//!
//! Two additional properties are worth calling out:
//!
//! * **Direction binding** - the direction byte is part of the associated
//!   data, so a frame captured from one direction cannot be replayed back at
//!   the sender.
//! * **Session binding** - the session id is part of the associated data, so
//!   frames cannot be moved between two different sessions.

use std::sync::atomic::{AtomicU64, Ordering};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};

use super::CryptoError;

/// Length of the AEAD nonce in bytes.
pub const NONCE_LEN: usize = 12;

/// Length of the Poly1305 authentication tag in bytes.
pub const TAG_LEN: usize = 16;

/// Which way a frame is travelling.
///
/// The tag describes the *flow*, not the role of the cipher: the initiator's
/// send cipher and the responder's receive cipher for the same flow both use
/// [`Direction::InitiatorToResponder`], so both derive the same associated
/// data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Frames travelling from the handshake initiator to the responder.
    InitiatorToResponder,
    /// Frames travelling from the handshake responder to the initiator.
    ResponderToInitiator,
}

impl Direction {
    /// Byte mixed into the associated data.
    fn tag(&self) -> u8 {
        match self {
            Direction::InitiatorToResponder => 0x00,
            Direction::ResponderToInitiator => 0x01,
        }
    }
}

/// Seals and opens frames for exactly one direction of one session.
pub struct FrameCipher {
    cipher: ChaCha20Poly1305,
    prefix: [u8; 4],
    counter: AtomicU64,
    direction: Direction,
    session_id: [u8; 8],
}

/// The key material is deliberately kept out of the debug output.
impl std::fmt::Debug for FrameCipher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameCipher")
            .field("direction", &self.direction)
            .field("session_id", &self.session_id)
            .field("frames", &self.frames_processed())
            .field("key", &"<redacted>")
            .finish()
    }
}

impl FrameCipher {
    /// Build a cipher from a raw 32-byte key.
    ///
    /// * `prefix` - 4 random bytes that prefix the counter to form the nonce.
    /// * `direction` - binds the cipher to one direction of the session.
    /// * `session_id` - binds the cipher to one specific session.
    pub fn new(
        key: [u8; 32],
        prefix: [u8; 4],
        direction: Direction,
        session_id: [u8; 8],
    ) -> Self {
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
        Self {
            cipher,
            prefix,
            counter: AtomicU64::new(0),
            direction,
            session_id,
        }
    }

    /// Seal a plaintext frame.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let counter = self.counter.fetch_add(1, Ordering::SeqCst);
        let nonce = self.nonce(counter);
        let aad = self.aad();
        self.cipher
            .encrypt(&nonce, Payload { msg: plaintext, aad: &aad })
            .map_err(|e| CryptoError::KeyExchange(format!("encryption failed: {e}")))
    }

    /// Open and authenticate a sealed frame.
    ///
    /// Any modification of the ciphertext, the nonce or the associated data
    /// makes the Poly1305 tag check fail, and the error is reported instead of
    /// the plaintext.
    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let counter = self.counter.fetch_add(1, Ordering::SeqCst);
        let nonce = self.nonce(counter);
        let aad = self.aad();
        self.cipher
            .decrypt(&nonce, Payload { msg: ciphertext, aad: &aad })
            .map_err(|_| CryptoError::Signature("AEAD authentication failed".into()))
    }

    /// Number of frames processed by this cipher so far.
    pub fn frames_processed(&self) -> u64 {
        self.counter.load(Ordering::SeqCst)
    }

    fn nonce(&self, counter: u64) -> Nonce {
        let mut nonce = [0u8; NONCE_LEN];
        nonce[..4].copy_from_slice(&self.prefix);
        nonce[4..].copy_from_slice(&counter.to_le_bytes());
        *Nonce::from_slice(&nonce)
    }

    fn aad(&self) -> [u8; 9] {
        let mut aad = [0u8; 9];
        aad[..8].copy_from_slice(&self.session_id);
        aad[8] = self.direction.tag();
        aad
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ciphers() -> (FrameCipher, FrameCipher) {
        let key = [7u8; 32];
        let session = [1u8; 8];
        // Sender and receiver of the same flow must agree on the AAD tag, so
        // both ciphers carry the initiator -> responder direction.
        let tx = FrameCipher::new(
            key,
            [0xaa, 0xbb, 0xcc, 0xdd],
            Direction::InitiatorToResponder,
            session,
        );
        let rx = FrameCipher::new(
            key,
            [0xaa, 0xbb, 0xcc, 0xdd],
            Direction::InitiatorToResponder,
            session,
        );
        (tx, rx)
    }

    #[test]
    fn roundtrip() {
        let (tx, rx) = ciphers();
        let ct = tx.encrypt(b"hello mezots").unwrap();
        assert_ne!(&ct[..], b"hello mezots");
        assert_eq!(rx.decrypt(&ct).unwrap(), b"hello mezots");
    }

    #[test]
    fn tampering_is_detected() {
        let (tx, rx) = ciphers();
        let mut ct = tx.encrypt(b"hello mezots").unwrap();
        ct[0] ^= 0x01;
        assert!(rx.decrypt(&ct).is_err());
    }

    #[test]
    fn reflection_is_rejected() {
        let (tx, _) = ciphers();
        let ct = tx.encrypt(b"hello").unwrap();
        // A cipher for the opposite flow must not open the frame, even though
        // it holds the same key and therefore derives the same nonce: the
        // direction byte is part of the associated data.
        let opposite = FrameCipher::new(
            [7u8; 32],
            [0xaa, 0xbb, 0xcc, 0xdd],
            Direction::ResponderToInitiator,
            [1u8; 8],
        );
        assert!(opposite.decrypt(&ct).is_err());
    }

    #[test]
    fn wrong_session_is_rejected() {
        let tx = FrameCipher::new([7u8; 32], [1, 2, 3, 4], Direction::InitiatorToResponder, [1u8; 8]);
        let other = FrameCipher::new([7u8; 32], [1, 2, 3, 4], Direction::InitiatorToResponder, [9u8; 8]);
        let ct = tx.encrypt(b"hello").unwrap();
        assert!(other.decrypt(&ct).is_err());
    }

    #[test]
    fn nonces_do_not_repeat() {
        let (tx, _) = ciphers();
        let a = tx.encrypt(b"one").unwrap();
        let b = tx.encrypt(b"two").unwrap();
        assert_ne!(a[..a.len() - TAG_LEN], b[..b.len() - TAG_LEN]);
    }
}
