//! Peer identity and connection state.

use std::net::SocketAddr;
use std::time::Instant;

use serde::{Deserialize, Serialize};

/// Unique identifier for a node on the local network.
pub type PeerId = String;

/// Which discovery backend surfaced a peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoverySource {
    /// DNS-SD / mDNS advertisement.
    Mdns,
    /// Multicast UDP fallback beacon.
    Beacon,
    /// Address supplied explicitly by the application.
    Manual,
}

impl DiscoverySource {
    /// Short label used in logs and UI payloads.
    pub fn as_str(&self) -> &'static str {
        match self {
            DiscoverySource::Mdns => "mdns",
            DiscoverySource::Beacon => "beacon",
            DiscoverySource::Manual => "manual",
        }
    }
}

/// Lifecycle state of a peer as tracked by the [`PeerManager`](crate::reliability::manager::PeerManager).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerState {
    /// Known through discovery but not yet dialled.
    Discovered,
    /// TCP connection in progress.
    Connecting,
    /// TCP connected, cryptographic handshake running.
    Handshaking,
    /// Fully established: encrypted channel ready.
    Connected,
    /// Connected but failing the heartbeat check.
    Unhealthy,
    /// Transport dropped; the peer may be re-discovered later.
    Disconnected,
}

impl PeerState {
    /// Whether frames may currently be written to this peer.
    pub fn is_usable(&self) -> bool {
        matches!(self, PeerState::Connected)
    }

    /// Short label used in logs and UI payloads.
    pub fn as_str(&self) -> &'static str {
        match self {
            PeerState::Discovered => "discovered",
            PeerState::Connecting => "connecting",
            PeerState::Handshaking => "handshaking",
            PeerState::Connected => "connected",
            PeerState::Unhealthy => "unhealthy",
            PeerState::Disconnected => "disconnected",
        }
    }
}

/// A peer as seen by the discovery layer.
#[derive(Debug, Clone)]
pub struct PeerRecord {
    /// Stable node id.
    pub node_id: PeerId,
    /// Human readable name.
    pub display_name: String,
    /// Candidate socket addresses, most recent first.
    pub addresses: Vec<SocketAddr>,
    /// Protocol port advertised by the peer.
    pub port: u16,
    /// Which backend found this peer.
    pub source: DiscoverySource,
    /// Protocol version reported by the peer.
    pub version: String,
    /// When this record was last refreshed.
    pub last_seen: Instant,
}

impl PeerRecord {
    /// Best-guess address to dial.
    pub fn best_address(&self) -> Option<SocketAddr> {
        self.addresses.first().copied()
    }
}

/// A compact description of a peer, used in UI and WebSocket payloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerSummary {
    /// Node id.
    pub peer_id: PeerId,
    /// Human readable name.
    pub display_name: String,
    /// Address the peer is reachable at, if known.
    pub address: Option<String>,
    /// Current connection state.
    pub state: String,
    /// Which discovery backend found the peer.
    pub source: String,
    /// Whether the peer is currently connected.
    pub connected: bool,
}

/// Events published by the [`PeerManager`](crate::reliability::manager::PeerManager).
///
/// The transport layer (for example the WebSocket bridge) consumes these to
/// keep user interfaces in sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PeerEvent {
    /// A peer was discovered on the network.
    Discovered {
        /// The peer that was found.
        peer: PeerSummary,
    },
    /// A peer became reachable.
    Connected {
        /// The peer that connected.
        peer_id: PeerId,
    },
    /// A peer went away.
    Disconnected {
        /// The peer that disconnected.
        peer_id: PeerId,
        /// Why the connection ended.
        reason: String,
    },
    /// A text message arrived.
    Message {
        /// Who sent it.
        peer_id: PeerId,
        /// The room it belongs to.
        room: String,
        /// The display name of the sender.
        author: String,
        /// Message body.
        text: String,
        /// The block that committed to this message, when it is known.
        block_hash: Option<String>,
        /// The height of that block.
        height: Option<u64>,
    },
    /// A binary message arrived.
    Binary {
        /// Who sent it.
        peer_id: PeerId,
        /// The room it belongs to.
        room: String,
        /// The display name of the sender.
        author: String,
        /// MIME type.
        mime: String,
        /// Raw bytes.
        payload: Vec<u8>,
        /// The block that committed to this attachment, when it is known.
        block_hash: Option<String>,
        /// The height of that block.
        height: Option<u64>,
    },
    /// A chunk of a bulk stream arrived.
    StreamChunk {
        /// Who sent it.
        peer_id: PeerId,
        /// The room the stream belongs to.
        room: String,
        /// Stream identifier.
        stream_id: u32,
        /// Sequence number within the stream.
        seq: u64,
        /// Raw bytes.
        payload: Vec<u8>,
    },
    /// A live stream was announced.
    StreamMeta {
        /// Who is broadcasting.
        peer_id: PeerId,
        /// The room the stream belongs to.
        room: String,
        /// Stream identifier.
        stream_id: u32,
        /// Title of the broadcast.
        title: String,
        /// MIME type of the chunks.
        mime: String,
    },
    /// A live stream ended.
    StreamEnd {
        /// Who was broadcasting.
        peer_id: PeerId,
        /// The room the stream belongs to.
        room: String,
        /// Stream identifier.
        stream_id: u32,
    },
    /// A block of the chain arrived from a peer.
    LedgerBlock {
        /// The block.
        block: crate::ledger::LedgerBlock,
    },
    /// A peer told us where its chain stands.
    ChainState {
        /// Who told us.
        peer_id: PeerId,
        /// Hash of the peer's newest block.
        head: String,
        /// Height of the peer's chain.
        height: u64,
    },
    /// Something went wrong.
    Error {
        /// Human readable description.
        message: String,
    },
}

/// The identity of the node this browser is attached to.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeIdentity {
    /// Stable node id.
    pub node_id: String,
    /// Human readable name.
    pub display_name: String,
    /// Short fingerprint of the node's Ed25519 identity key.
    pub fingerprint: String,
    /// base64 of the node's Ed25519 public key.
    pub public_key: String,
}

/// Counters describing what the node is doing.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeMetrics {
    /// Bytes sent to peers that are on the same physical network.
    pub bytes_relayed: u64,
    /// Bytes sent to peers that had to be reached over the internet.
    pub bytes_internet: u64,
    /// How many peers are connected.
    pub peers_online: usize,
    /// How long the node has been running, in milliseconds.
    pub uptime_ms: u64,
    /// How many blocks the local chain holds.
    pub ledger_height: u64,
}

/// The state of the local view of the chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerStatus {
    /// Height of the chain head.
    pub height: u64,
    /// Whether the local chain verifies end to end.
    pub valid: bool,
    /// Hash of the chain head.
    pub head: String,
    /// How many peers agree on the head hash.
    pub peers_agree: usize,
    /// How many peers are connected.
    pub peers_total: usize,
}
