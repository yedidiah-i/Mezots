//! Zero-trust LAN peer-to-peer transport stack.
//!
//! The crate is layered so that each phase of the protocol can be reasoned
//! about (and tested) on its own:
//!
//! | Layer | Module | Responsibility |
//! |-------|--------|----------------|
//! | Discovery | [`discovery`] | mDNS service + multicast UDP beacon, peer registry |
//! | Transport | [`transport`] | Tokio TCP listener, connection pool, WebSocket endpoint |
//! | Protocol  | [`protocol`]  | [`protocol::frame::Frame`] enum, postcard codec, priority router |
//! | Crypto    | [`crypto`]    | ephemeral X25519/Ed25519 keys, DH handshake, ChaCha20-Poly1305 AEAD |
//! | Reliability | [`reliability`] | heartbeat, retry/resume queue, [`reliability::manager::PeerManager`] |
//!
//! The public entry point for applications is
//! [`reliability::manager::PeerManager`], which hides discovery, encryption
//! state and socket ownership behind a single actor.

pub mod config;
pub mod crypto;
pub mod discovery;
pub mod error;
pub mod ledger;
pub mod peer;
pub mod protocol;
pub mod reliability;
pub mod reels;
pub mod rooms;
pub mod transport;

pub use config::NodeConfig;
pub use error::{Error, Result};
pub use ledger::{Ledger, LedgerBlock, LedgerKind};
pub use peer::{DiscoverySource, PeerEvent, PeerRecord, PeerState, PeerSummary};
pub use reliability::manager::PeerManager;
