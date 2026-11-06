//! The pool of active peer connections.
//!
//! A connection is identified by the peer's node id. The pool keeps the
//! routing handle for each peer so that a sender can hand a frame to the
//! right socket without knowing anything about the socket itself.
//!
//! The raw `TcpStream` is owned by the writer task of that peer (see
//! [`crate::protocol::router::spawn_writer`]); the pool owns the *handle* to
//! it. That split keeps a single writer per socket, which is what allows the
//! router to guarantee ordering.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use parking_lot::RwLock;

use crate::peer::PeerId;
use crate::protocol::router::MuxSender;

/// Metadata and routing handle for one connected peer.
#[derive(Debug, Clone)]
pub struct PeerConnection {
    /// The remote node id.
    pub peer_id: PeerId,
    /// Address the connection was established to.
    pub address: SocketAddr,
    /// Handle used to queue outbound frames.
    pub sender: MuxSender,
    /// When the connection was established.
    pub connected_at: Instant,
    /// Identifier of the cryptographic session.
    pub session_id: [u8; 8],
    /// Last measured round trip time.
    pub rtt: Option<Duration>,
    /// Number of frames sent on this connection.
    pub frames_sent: u64,
}

impl PeerConnection {
    /// Create a new connection record.
    pub fn new(
        peer_id: PeerId,
        address: SocketAddr,
        sender: MuxSender,
        session_id: [u8; 8],
    ) -> Self {
        Self {
            peer_id,
            address,
            sender,
            connected_at: Instant::now(),
            session_id,
            rtt: None,
            frames_sent: 0,
        }
    }

    /// How long the connection has been up.
    pub fn uptime(&self) -> Duration {
        self.connected_at.elapsed()
    }
}

/// A registry of live peer connections.
#[derive(Debug, Default)]
pub struct ConnectionPool {
    connections: RwLock<HashMap<PeerId, PeerConnection>>,
}

impl ConnectionPool {
    /// Create an empty pool.
    pub fn new() -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
        }
    }

    /// Insert (or replace) a connection.
    pub fn insert(&self, connection: PeerConnection) -> Option<PeerConnection> {
        self.connections
            .write()
            .insert(connection.peer_id.clone(), connection)
    }

    /// Remove a connection and return it.
    pub fn remove(&self, peer_id: &str) -> Option<PeerConnection> {
        self.connections.write().remove(peer_id)
    }

    /// Look up a connection.
    pub fn get(&self, peer_id: &str) -> Option<PeerConnection> {
        self.connections.read().get(peer_id).cloned()
    }

    /// The routing handle for a peer.
    pub fn sender(&self, peer_id: &str) -> Option<MuxSender> {
        self.connections.read().get(peer_id).map(|c| c.sender.clone())
    }

    /// Whether a peer is connected.
    pub fn contains(&self, peer_id: &str) -> bool {
        self.connections.read().contains_key(peer_id)
    }

    /// Number of live connections.
    pub fn len(&self) -> usize {
        self.connections.read().len()
    }

    /// Whether the pool is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// All connected peer ids.
    pub fn peer_ids(&self) -> Vec<PeerId> {
        let mut ids: Vec<PeerId> = self.connections.read().keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Snapshot of all connections.
    pub fn all(&self) -> Vec<PeerConnection> {
        self.connections.read().values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::frame::{Frame, Priority};
    use crate::protocol::router::{MuxRouter, RouterConfig};

    fn sender() -> MuxSender {
        // The receiver has to stay alive, otherwise the channel is closed and
        // every frame is rejected.
        let router = MuxRouter::new(RouterConfig::default());
        let (sender, receiver) = router.split();
        // Keep the receiver alive for the duration of the test.
        Box::leak(Box::new(receiver));
        sender
    }

    #[test]
    fn connections_are_addressable_by_peer_id() {
        let pool = ConnectionPool::new();
        let conn = PeerConnection::new(
            "peer-a".into(),
            "127.0.0.1:7700".parse().unwrap(),
            sender(),
            [0; 8],
        );
        pool.insert(conn);
        assert!(pool.contains("peer-a"));
        assert_eq!(pool.len(), 1);
        assert!(pool.sender("peer-a").is_some());
        assert!(pool.remove("peer-a").is_some());
        assert!(pool.is_empty());
    }

    #[test]
    fn a_removed_connection_stops_accepting_frames() {
        let pool = ConnectionPool::new();
        let conn = PeerConnection::new(
            "peer-a".into(),
            "127.0.0.1:7700".parse().unwrap(),
            sender(),
            [0; 8],
        );
        pool.insert(conn);
        pool.remove("peer-a");
        let handle = pool.sender("peer-a");
        assert!(handle.is_none());
    }

    #[test]
    fn frame_priority_is_preserved_in_the_sender() {
        let s = sender();
        let env = crate::protocol::frame::Envelope::next(1, Frame::Ping { nonce: 0, sent_ms: 0 });
        assert_eq!(env.priority(), Priority::Control);
        s.try_send(env).unwrap();
    }
}
