//! Static configuration for a local node.

use std::time::Duration;

/// Default DNS-SD service type used to advertise the protocol listener.
pub const DEFAULT_SERVICE_TYPE: &str = "_mezots._tcp.local.";

/// Default multicast group used by the fallback beacon.
pub const DEFAULT_BEACON_GROUP: &str = "239.255.42.99";

/// Version advertised in mDNS TXT records and `NodeInfo` frames.
pub const PROTOCOL_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Everything the transport stack needs to run a node.
#[derive(Debug, Clone)]
pub struct NodeConfig {
    /// Stable identifier for this node, exchanged during discovery + handshake.
    pub node_id: String,
    /// Human readable name shown in the UI layer.
    pub display_name: String,
    /// TCP port for the peer-to-peer protocol listener.
    pub protocol_port: u16,
    /// TCP port for the browser-facing WebSocket endpoint.
    pub websocket_port: u16,
    /// UDP port used for the multicast fallback beacon.
    pub beacon_port: u16,
    /// Multicast group for the fallback beacon.
    pub beacon_group: String,
    /// DNS-SD service type advertised over mDNS.
    pub service_type: String,
    /// Enable the mDNS discovery backend.
    pub enable_mdns: bool,
    /// Enable the multicast UDP fallback beacon.
    pub enable_beacon: bool,
    /// Enable the browser-facing WebSocket server.
    pub enable_websocket: bool,
    /// How often a `Ping` frame is emitted to each connected peer.
    pub heartbeat_interval: Duration,
    /// How long a peer may take to answer a `Ping` before it counts as missed.
    pub heartbeat_timeout: Duration,
    /// Number of consecutive missed pongs before a peer is considered broken.
    pub max_missed_pongs: u32,
    /// Capacity of the per-peer control queue (highest priority).
    pub control_queue_capacity: usize,
    /// Capacity of the per-peer chat queue (medium priority).
    pub chat_queue_capacity: usize,
    /// Capacity of the per-peer stream queue (bulk data, lowest priority).
    pub stream_queue_capacity: usize,
    /// Largest plaintext frame accepted or produced, in bytes.
    pub max_frame_bytes: usize,
    /// Timeout for completing the cryptographic handshake.
    pub handshake_timeout: Duration,
    /// Reject a peer whose Ed25519 identity key differs from the pinned one.
    ///
    /// The identity key is ephemeral by design (a fresh key pair is
    /// generated on every start), so a peer that restarts legitimately
    /// presents a new key. Leave this off unless the peers keep their keys
    /// stable for the lifetime of this node - the warning is still reported
    /// either way.
    pub pin_peer_identity: bool,
    /// Where the node keeps the clips that people publish.
    ///
    /// The clips are served straight off the disk, so a school can point
    /// this at a network share if it wants to keep them somewhere else.
    pub data_dir: std::path::PathBuf,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            node_id: default_node_id(),
            display_name: "Mezots Node".to_string(),
            protocol_port: 7700,
            websocket_port: 7701,
            beacon_port: 7702,
            beacon_group: DEFAULT_BEACON_GROUP.to_string(),
            service_type: DEFAULT_SERVICE_TYPE.to_string(),
            enable_mdns: true,
            enable_beacon: true,
            enable_websocket: true,
            heartbeat_interval: Duration::from_secs(5),
            heartbeat_timeout: Duration::from_secs(10),
            max_missed_pongs: 3,
            control_queue_capacity: 64,
            chat_queue_capacity: 256,
            stream_queue_capacity: 32,
            max_frame_bytes: 16 * 1024 * 1024,
            handshake_timeout: Duration::from_secs(10),
            pin_peer_identity: false,
            data_dir: default_data_dir(),
        }
    }
}

impl NodeConfig {
    /// Generate a random, human-recognisable node id.
    pub fn with_random_id() -> Self {
        Self {
            node_id: default_node_id(),
            ..Self::default()
        }
    }

    /// Set the node id explicitly.
    pub fn with_node_id(mut self, node_id: impl Into<String>) -> Self {
        self.node_id = node_id.into();
        self
    }

    /// Set the advertised display name.
    pub fn with_display_name(mut self, display_name: impl Into<String>) -> Self {
        self.display_name = display_name.into();
        self
    }
}

/// Build a short random node id such as `node-4f9a2c1b`.
pub fn default_node_id() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 4];
    rand::rng().fill_bytes(&mut bytes);
    format!("node-{}", hex(&bytes))
}

/// Where the node keeps its data by default.
///
/// The directory sits next to the executable on a portable install and in
/// the user's data folder everywhere else.
pub fn default_data_dir() -> std::path::PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let candidate = parent.join("data");
            if candidate.is_dir() || parent.join("Cargo.toml").is_file() {
                return candidate;
            }
        }
    }

    if let Ok(dir) = std::env::var("MEZOTS_DATA_DIR") {
        return std::path::PathBuf::from(dir);
    }

    #[cfg(windows)]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            return std::path::PathBuf::from(local).join("mezots");
        }
    }

    #[cfg(not(windows))]
    {
        if let Ok(home) = std::env::var("HOME") {
            return std::path::PathBuf::from(home).join(".local/share/mezots");
        }
    }

    std::path::PathBuf::from("data")
}

/// Lowercase hex encoding helper.
pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(char::from_digit((b >> 4) as u32, 16).unwrap());
        out.push(char::from_digit((b & 0x0f) as u32, 16).unwrap());
    }
    out
}
