//! Peer discovery: mDNS with a multicast UDP fallback beacon.

pub mod beacon;
pub mod mdns;
pub mod registry;

pub use beacon::{Beacon, BEACON_INTERVAL};
pub use mdns::MdnsDiscovery;
pub use registry::{DiscoveryEvent, PeerRegistry};
