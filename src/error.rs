//! Crate-wide error types.

use crate::peer::PeerId;

/// Convenience alias used across the crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors raised by the transport stack.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Underlying socket / filesystem failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Frame (de)serialization failure.
    #[error("codec error: {0}")]
    Codec(#[from] postcard::Error),

    /// Cryptographic operation failed (handshake, AEAD, key derivation).
    #[error("crypto error: {0}")]
    Crypto(#[from] crate::crypto::CryptoError),

    /// The requested peer is not known to the local node.
    #[error("peer not found: {0}")]
    PeerNotFound(PeerId),

    /// The peer exists but has no usable transport (not connected yet).
    #[error("peer {0} is not connected")]
    PeerNotConnected(PeerId),

    /// A bounded queue is saturated; the caller should slow down.
    #[error("backpressure on {queue}: queue is full")]
    Backpressure { queue: &'static str },

    /// The socket was closed while a frame was in flight.
    #[error("transport closed")]
    TransportClosed,

    /// A frame was received that could not be authenticated or decrypted.
    #[error("authentication failed: {0}")]
    Authentication(String),

    /// The peer violated the protocol.
    #[error("protocol violation: {0}")]
    Protocol(String),

    /// The handshake did not complete within the allotted time.
    #[error("handshake timed out")]
    HandshakeTimeout,

    /// Discovery backend could not be started.
    #[error("discovery error: {0}")]
    Discovery(String),
}

impl Error {
    /// Helper for building an authentication failure.
    pub fn auth(msg: impl Into<String>) -> Self {
        Error::Authentication(msg.into())
    }

    /// Helper for building a protocol violation.
    pub fn protocol(msg: impl Into<String>) -> Self {
        Error::Protocol(msg.into())
    }
}
