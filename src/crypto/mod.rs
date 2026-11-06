//! Cryptographic primitives used by the peer-to-peer transport.
//!
//! The stack is deliberately *ephemeral*: every node start generates a fresh
//! X25519 key pair (for Diffie-Hellman) and a fresh Ed25519 key pair (for
//! identity). Nothing is written to disk, so a reboot invalidates every past
//! session key.

pub mod aead;
pub mod handshake;
pub mod keys;
pub mod trust;

pub use aead::FrameCipher;
pub use handshake::{
    HandshakeFrame, HandshakeOutcome, initiator_handshake, responder_handshake,
};
pub use keys::NodeKeys;
pub use trust::TrustStore;

/// Errors produced by the crypto layer.
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    /// Socket failure while exchanging handshake material.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Handshake frame could not be encoded/decoded.
    #[error("codec error: {0}")]
    Codec(#[from] postcard::Error),

    /// Diffie-Hellman or key derivation failed.
    #[error("key exchange failed: {0}")]
    KeyExchange(String),

    /// Signature verification failed - the peer is not who it claims to be.
    #[error("signature verification failed: {0}")]
    Signature(String),

    /// The peer sent a handshake frame out of order or of the wrong type.
    #[error("unexpected handshake state: {0}")]
    Unexpected(String),

    /// The peer's Ed25519 key does not match the pinned (TOFU) key.
    #[error("peer identity mismatch: pinned key does not match presented key")]
    IdentityMismatch,

    /// The handshake did not complete in time.
    #[error("handshake timed out")]
    Timeout,
}
