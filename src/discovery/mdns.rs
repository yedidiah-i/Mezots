//! The mDNS discovery service.
//!
//! Every node advertises itself as a DNS-SD service (`_mezots._tcp.local.`)
//! and browses for the same service type. The peer id, display name and
//! protocol port travel in the TXT records, so a resolved service is enough
//! to dial the peer.
//!
//! The service is registered when the node starts and stays registered for the
//! lifetime of the process. When the daemon reports a peer leaving, the peer
//! is removed from the shared registry.

use std::collections::HashMap;
use std::net::IpAddr;

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use tracing::{debug, info, warn};

use crate::config::{NodeConfig, PROTOCOL_VERSION};
use crate::peer::{DiscoverySource, PeerRecord};

use super::registry::PeerRegistry;

/// TXT key holding the peer id.
pub const TXT_PEER_ID: &str = "peer_id";
/// TXT key holding the display name.
pub const TXT_DISPLAY_NAME: &str = "display_name";
/// TXT key holding the protocol version.
pub const TXT_VERSION: &str = "version";
/// TXT key holding the protocol port.
pub const TXT_PORT: &str = "port";

/// A running mDNS advertiser/browser.
pub struct MdnsDiscovery {
    daemon: ServiceDaemon,
    service_type: String,
    instance_name: String,
}

impl MdnsDiscovery {
    /// Register the service and start browsing for peers.
    ///
    /// The browse loop runs in its own Tokio task and feeds the shared
    /// [`PeerRegistry`].
    pub fn start(config: &NodeConfig, registry: PeerRegistry) -> anyhow::Result<Self> {
        let daemon = ServiceDaemon::new()
            .map_err(|e| anyhow::anyhow!("failed to create mDNS daemon: {e}"))?;

        let service_type = config.service_type.clone();
        let instance_name = config.node_id.clone();
        let host_name = format!("{instance_name}.local.");

        let mut txt_properties = HashMap::new();
        txt_properties.insert(TXT_PEER_ID.to_string(), config.node_id.clone());
        txt_properties.insert(TXT_DISPLAY_NAME.to_string(), config.display_name.clone());
        txt_properties.insert(TXT_VERSION.to_string(), PROTOCOL_VERSION.to_string());
        txt_properties.insert(TXT_PORT.to_string(), config.protocol_port.to_string());

        let service_info = ServiceInfo::new(
            &service_type,
            &instance_name,
            &host_name,
            "",
            config.protocol_port,
            Some(txt_properties),
        )
        .map_err(|e| anyhow::anyhow!("failed to build mDNS service info: {e}"))?;

        daemon
            .register(service_info)
            .map_err(|e| anyhow::anyhow!("failed to register mDNS service: {e}"))?;

        let receiver = daemon
            .browse(&service_type)
            .map_err(|e| anyhow::anyhow!("failed to browse for mDNS services: {e}"))?;

        info!(
            node_id = %config.node_id,
            service_type = %service_type,
            port = config.protocol_port,
            "mDNS service registered"
        );

        let own_node_id = config.node_id.clone();
        tokio::spawn(async move {
            // Maps the DNS-SD full name to the peer id so that a
            // `ServiceRemoved` event can be mapped back to the registry key.
            let mut known: HashMap<String, String> = HashMap::new();

            while let Ok(event) = receiver.recv_async().await {
                match event {
                    ServiceEvent::ServiceFound(_, _) => {
                        debug!("mDNS: potential peer found");
                    }
                    ServiceEvent::ServiceResolved(info) => {
                        let full_name = info.get_fullname().to_string();
                        let txt: HashMap<String, String> = info
                            .get_properties()
                            .iter()
                            .map(|prop| (prop.key().to_string(), prop.val().to_string()))
                            .collect();

                        let peer_id = match txt.get(TXT_PEER_ID) {
                            Some(id) => id.clone(),
                            None => {
                                warn!(full_name = %full_name, "mDNS: service without peer id, ignoring");
                                continue;
                            }
                        };

                        if peer_id == own_node_id {
                            // Our own advertisement.
                            continue;
                        }

                        let port = txt
                            .get(TXT_PORT)
                            .and_then(|p| p.parse::<u16>().ok())
                            .unwrap_or_else(|| info.get_port());

                        let mut addresses: Vec<std::net::SocketAddr> = info
                            .get_addresses()
                            .iter()
                            .copied()
                            .map(|ip| std::net::SocketAddr::new(std::net::IpAddr::V4(ip), port))
                            .collect();

                        // Prefer routable addresses; a loopback address is
                        // only useful when several nodes run on one host.
                        addresses.sort_by_key(|addr| match addr.ip() {
                            IpAddr::V4(v4) if v4.is_loopback() => 1,
                            IpAddr::V6(v6) if v6.is_loopback() => 1,
                            _ => 0,
                        });

                        if addresses.is_empty() {
                            warn!(peer_id = %peer_id, "mDNS: peer without usable address");
                            continue;
                        }

                        let record = PeerRecord {
                            node_id: peer_id.clone(),
                            display_name: txt
                                .get(TXT_DISPLAY_NAME)
                                .cloned()
                                .unwrap_or_else(|| peer_id.clone()),
                            addresses,
                            port,
                            source: DiscoverySource::Mdns,
                            version: txt
                                .get(TXT_VERSION)
                                .cloned()
                                .unwrap_or_else(|| "unknown".to_string()),
                            last_seen: std::time::Instant::now(),
                        };

                        known.insert(full_name, peer_id.clone());
                        registry.upsert(record);
                    }
                    ServiceEvent::ServiceRemoved(_, full_name) => {
                        if let Some(peer_id) = known.remove(&full_name) {
                            debug!(peer_id = %peer_id, "mDNS: peer left the network");
                            registry.remove(&peer_id);
                        }
                    }
                    _ => {}
                }
            }
        });

        Ok(Self {
            daemon,
            service_type,
            instance_name,
        })
    }

    /// Unregister the service.
    pub fn shutdown(&self) {
        let full_name = format!("{}.{}", self.instance_name, self.service_type);
        let _ = self.daemon.unregister(&full_name);
        // Stop the daemon's worker thread as well, otherwise it logs an
        // error when a query is still in flight.
        let _ = self.daemon.shutdown();
        info!(node_id = %self.instance_name, "mDNS service unregistered");
    }
}
