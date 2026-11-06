//! The shared registry of discovered peers.
//!
//! Both discovery backends write into the same registry, so the rest of the
//! stack never needs to know which backend found a peer. Changes are published
//! on a broadcast channel that the [`PeerManager`](crate::reliability::manager::PeerManager)
//! subscribes to.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use parking_lot::RwLock;
use tokio::sync::broadcast;

use crate::peer::{PeerId, PeerRecord};

/// Number of discovery events buffered for subscribers.
pub const EVENT_BUFFER: usize = 256;

/// Something that happened to the peer table.
#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    /// A peer was seen for the first time.
    Discovered(PeerRecord),
    /// A known peer was seen again (address or metadata changed).
    Updated(PeerRecord),
    /// A peer left the network.
    Removed(PeerId),
}

/// Thread-safe table of known peers.
#[derive(Debug, Clone)]
pub struct PeerRegistry {
    peers: Arc<RwLock<HashMap<PeerId, PeerRecord>>>,
    events: broadcast::Sender<DiscoveryEvent>,
}

impl Default for PeerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PeerRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        let (events, _) = broadcast::channel(EVENT_BUFFER);
        Self {
            peers: Arc::new(RwLock::new(HashMap::new())),
            events,
        }
    }

    /// Insert or refresh a peer.
    ///
    /// Returns `true` when the peer was new, `false` when an existing record
    /// was updated.
    pub fn upsert(&self, mut record: PeerRecord) -> bool {
        let is_new = {
            let mut peers = self.peers.write();
            match peers.get_mut(&record.node_id) {
                Some(existing) => {
                    // Keep the freshest address list, but never lose the
                    // addresses we already knew about.
                    if record.addresses.is_empty() {
                        record.addresses = existing.addresses.clone();
                    }
                    record.last_seen = Instant::now();
                    *existing = record.clone();
                    false
                }
                None => {
                    record.last_seen = Instant::now();
                    peers.insert(record.node_id.clone(), record.clone());
                    true
                }
            }
        };

        let event = if is_new {
            DiscoveryEvent::Discovered(record)
        } else {
            DiscoveryEvent::Updated(record)
        };
        let _ = self.events.send(event);
        is_new
    }

    /// Remove a peer from the table.
    pub fn remove(&self, peer_id: &str) {
        let removed = self.peers.write().remove(peer_id);
        if removed.is_some() {
            let _ = self.events.send(DiscoveryEvent::Removed(peer_id.to_string()));
        }
    }

    /// Look up a peer.
    pub fn get(&self, peer_id: &str) -> Option<PeerRecord> {
        self.peers.read().get(peer_id).cloned()
    }

    /// All known peers.
    pub fn list(&self) -> Vec<PeerRecord> {
        let mut peers: Vec<PeerRecord> = self.peers.read().values().cloned().collect();
        peers.sort_by(|a, b| a.node_id.cmp(&b.node_id));
        peers
    }

    /// Number of known peers.
    pub fn len(&self) -> usize {
        self.peers.read().len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Subscribe to discovery events.
    pub fn subscribe(&self) -> broadcast::Receiver<DiscoveryEvent> {
        self.events.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peer::DiscoverySource;

    fn record(id: &str) -> PeerRecord {
        PeerRecord {
            node_id: id.to_string(),
            display_name: id.to_string(),
            addresses: vec!["127.0.0.1:7700".parse().unwrap()],
            port: 7700,
            source: DiscoverySource::Mdns,
            version: "0.1.0".into(),
            last_seen: Instant::now(),
        }
    }

    #[test]
    fn upsert_reports_new_and_known_peers() {
        let registry = PeerRegistry::new();
        assert!(registry.upsert(record("a")));
        assert!(!registry.upsert(record("a")));
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn remove_publishes_an_event() {
        let registry = PeerRegistry::new();
        registry.upsert(record("a"));
        let mut events = registry.subscribe();
        registry.remove("a");
        assert!(matches!(
            events.try_recv().unwrap(),
            DiscoveryEvent::Removed(id) if id == "a"
        ));
    }
}
