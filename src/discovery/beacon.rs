//! Multicast UDP fallback beacon.
//!
//! Some routers block or mangle mDNS (multicast DNS on port 5353), and some
//! networks simply do not forward it. The beacon is the safety net: every node
//! periodically broadcasts a small "I am alive" packet to a well known
//! multicast group and listens for the same packets from other nodes.
//!
//! The packet is plain JSON, so it can be inspected with any packet capture
//! tool - handy when debugging a stubborn router.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;
use tracing::{debug, info, warn};

use crate::config::{NodeConfig, PROTOCOL_VERSION};
use crate::peer::{DiscoverySource, PeerRecord};

use super::registry::PeerRegistry;

/// How often the beacon announces this node.
pub const BEACON_INTERVAL: Duration = Duration::from_secs(3);

/// Largest beacon packet accepted.
pub const MAX_BEACON_SIZE: usize = 1024;

/// Multicast time-to-live: 1 keeps the traffic on the local subnet.
const BEACON_TTL: u32 = 1;

/// The payload broadcast by every node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeaconPacket {
    /// Sender's node id.
    pub node_id: String,
    /// Sender's display name.
    pub display_name: String,
    /// Sender's protocol port.
    pub port: u16,
    /// Protocol version.
    pub version: String,
    /// Sender's wall clock in milliseconds since the Unix epoch.
    pub ts_ms: u64,
}

/// A running fallback beacon.
pub struct Beacon {
    socket: Arc<UdpSocket>,
    group: Ipv4Addr,
    target: SocketAddr,
    own_node_id: String,
}

impl Beacon {
    /// Bind the multicast socket and start announcing this node.
    pub fn start(config: &NodeConfig, registry: PeerRegistry) -> anyhow::Result<Self> {
        let group: Ipv4Addr = config
            .beacon_group
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid beacon group {}: {e}", config.beacon_group))?;

        // socket2 gives us the multicast primitives that Tokio does not
        // expose, plus SO_REUSEADDR so several nodes on one host can share
        // the beacon port.
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
            .map_err(|e| anyhow::anyhow!("failed to create beacon socket: {e}"))?;
        socket.set_reuse_address(true)?;
        socket.set_nonblocking(true)?;
        socket.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, config.beacon_port)).into())?;
        socket.join_multicast_v4(&group, &Ipv4Addr::UNSPECIFIED)?;
        socket.set_multicast_loop_v4(true)?;
        socket.set_multicast_ttl_v4(BEACON_TTL)?;
        socket.set_broadcast(true)?;

        // One socket handles both directions. It is shared through an `Arc`
        // because the multicast group membership belongs to the socket
        // itself - duplicating the handle would lose it on some platforms.
        let socket = Arc::new(
            UdpSocket::from_std(socket.into())
                .map_err(|e| anyhow::anyhow!("failed to adopt beacon socket: {e}"))?,
        );
        let send_socket = Arc::clone(&socket);
        let recv_socket = Arc::clone(&socket);

        let target = SocketAddr::from((group, config.beacon_port));
        let own_node_id = config.node_id.clone();

        let packet = BeaconPacket {
            node_id: config.node_id.clone(),
            display_name: config.display_name.clone(),
            port: config.protocol_port,
            version: PROTOCOL_VERSION.to_string(),
            ts_ms: now_ms(),
        };
        let payload = serde_json::to_vec(&packet)?;

        // --- sender ------------------------------------------------------
        let send_target = target;
        let send_payload = payload.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(BEACON_INTERVAL);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            // Announce immediately so that peers see us without waiting.
            ticker.tick().await;
            loop {
                ticker.tick().await;
                if let Err(e) = send_socket.send_to(&send_payload, send_target).await {
                    warn!(error = %e, "beacon: failed to broadcast");
                }
            }
        });

        // --- receiver ----------------------------------------------------
        let own_node_id_rx = own_node_id.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; MAX_BEACON_SIZE];
            loop {
                let (len, from) = match recv_socket.recv_from(&mut buf).await {
                    Ok(v) => v,
                    Err(e) => {
                        warn!(error = %e, "beacon: receive failed");
                        continue;
                    }
                };

                let packet: BeaconPacket = match serde_json::from_slice(&buf[..len]) {
                    Ok(p) => p,
                    Err(e) => {
                        debug!(error = %e, "beacon: ignoring malformed packet");
                        continue;
                    }
                };

                if packet.node_id == own_node_id_rx {
                    // Our own broadcast coming back to us.
                    continue;
                }

                // The beacon tells us the port but not the address; use the
                // source address of the packet, which is the real one.
                let peer_addr = match from.ip() {
                    std::net::IpAddr::V4(v4) if v4.is_unspecified() => {
                        SocketAddr::from((Ipv4Addr::LOCALHOST, packet.port))
                    }
                    ip => SocketAddr::new(ip, packet.port),
                };

                let record = PeerRecord {
                    node_id: packet.node_id.clone(),
                    display_name: packet.display_name.clone(),
                    addresses: vec![peer_addr],
                    port: packet.port,
                    source: DiscoverySource::Beacon,
                    version: packet.version.clone(),
                    last_seen: std::time::Instant::now(),
                };
                registry.upsert(record);
            }
        });

        info!(
            node_id = %own_node_id,
            group = %group,
            port = config.beacon_port,
            "multicast beacon started"
        );

        Ok(Self {
            socket,
            group,
            target,
            own_node_id,
        })
    }

    /// The multicast group this beacon talks to.
    pub fn group(&self) -> Ipv4Addr {
        self.group
    }

    /// The address packets are sent to.
    pub fn target(&self) -> SocketAddr {
        self.target
    }

    /// This node's id.
    pub fn node_id(&self) -> &str {
        &self.own_node_id
    }

    /// Send one announcement right now.
    pub async fn announce(&self) -> anyhow::Result<()> {
        let packet = BeaconPacket {
            node_id: self.own_node_id.clone(),
            display_name: String::new(),
            port: 0,
            version: PROTOCOL_VERSION.to_string(),
            ts_ms: now_ms(),
        };
        self.socket
            .send_to(&serde_json::to_vec(&packet)?, self.target)
            .await?;
        Ok(())
    }

    /// Stop announcing this node.
    pub fn shutdown(&self) {
        info!(node_id = %self.own_node_id, "multicast beacon stopped");
    }
}

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}
