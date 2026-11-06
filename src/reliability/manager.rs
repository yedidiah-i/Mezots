//! The unified peer manager.
//!
//! This is the only type the rest of the application talks to. It owns the
//! discovery backends, the TCP listener, the connection pool, the encryption
//! state and the reliability machinery, and it exposes a tiny interface:
//!
//! ```no_run
//! # use mezots::PeerManager;
//! # async fn demo() -> anyhow::Result<()> {
//! let manager = PeerManager::start(mezots::NodeConfig::default()).await?;
//! manager.send_message("node-abc", "hello").await?;
//! # Ok(())
//! # }
//! ```
//!
//! Everything runs inside a single actor task. Commands arrive on an mpsc
//! channel, discovery and connection events arrive on their own channels, and
//! the actor serialises all state changes. That keeps the shared state free of
//! locks and makes the lifecycle of a peer easy to reason about.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use futures_util::future::BoxFuture;
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{debug, info, warn};

use crate::config::NodeConfig;
use crate::crypto::handshake::{initiator_handshake, responder_handshake};
use crate::crypto::trust::IdentityStatus;
use crate::crypto::{NodeKeys, TrustStore};
use crate::discovery::beacon::Beacon;
use crate::discovery::mdns::MdnsDiscovery;
use crate::discovery::registry::{DiscoveryEvent, PeerRegistry, EVENT_BUFFER};
use crate::error::{Error, Result};
use crate::ledger::{Ledger, LedgerBlock, LedgerKind};
use crate::peer::{
    DiscoverySource, LedgerStatus, NodeIdentity, NodeMetrics, PeerEvent, PeerId, PeerRecord,
    PeerState, PeerSummary,
};
use crate::protocol::frame::{Envelope, Frame, Priority};
use crate::protocol::router::{spawn_writer, MuxRouter, MuxSender, RouterConfig};
use crate::transport::pool::{ConnectionPool, PeerConnection};
use crate::transport::secure::SecureChannel;
use crate::transport::tcp::PeerListener;
use crate::transport::websocket::PeerGateway;

use super::heartbeat::{run_heartbeat, HeartbeatConfig, HeartbeatMonitor, Shutdown};
use super::retry::RetryQueue;

/// How long a retry queue keeps an unacknowledged frame.
pub const RETRY_MAX_AGE: Duration = Duration::from_secs(300);
/// How many frames a retry queue holds per peer.
pub const RETRY_MAX_FRAMES: usize = 2048;
/// How often a reconnect is attempted, and how long the backoff grows.
pub const RECONNECT_BACKOFF: Duration = Duration::from_secs(2);
/// Maximum number of consecutive reconnect attempts.
pub const MAX_RECONNECT_ATTEMPTS: u32 = 30;

/// A monotonically increasing id source.
#[derive(Debug, Default)]
pub struct IdCounter {
    next: AtomicU64,
}

impl IdCounter {
    /// Take the next id.
    pub fn next_id(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }
}

/// Commands accepted by the actor.
enum Command {
    SendText {
        peer_id: PeerId,
        text: String,
        reply: oneshot::Sender<Result<u64>>,
    },
    SendBinary {
        peer_id: PeerId,
        mime: String,
        payload: Vec<u8>,
        reply: oneshot::Sender<Result<u64>>,
    },
    SendStreamChunk {
        peer_id: PeerId,
        stream_id: u32,
        payload: Vec<u8>,
        wait: bool,
        reply: oneshot::Sender<Result<u64>>,
    },
    BroadcastText {
        text: String,
        reply: oneshot::Sender<Result<usize>>,
    },
    /// Send a text message into a room.
    SendRoomText {
        room: String,
        text: String,
        reply: oneshot::Sender<Result<LedgerBlock>>,
    },
    /// Send an attachment into a room.
    SendRoomBinary {
        room: String,
        mime: String,
        payload: Vec<u8>,
        reply: oneshot::Sender<Result<LedgerBlock>>,
    },
    /// Announce a live stream.
    StartStream {
        room: String,
        title: String,
        mime: String,
        reply: oneshot::Sender<Result<u32>>,
    },
    /// Push one chunk of a live stream.
    StreamChunk {
        room: String,
        stream_id: u32,
        payload: Vec<u8>,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Close a live stream.
    EndStream {
        room: String,
        stream_id: u32,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Ask for the most recent blocks of the chain.
    LedgerSnapshot {
        limit: usize,
        reply: oneshot::Sender<Vec<LedgerBlock>>,
    },
    /// Ask for the state of the chain.
    LedgerStatus {
        reply: oneshot::Sender<LedgerStatus>,
    },
    /// Ask for the node counters.
    Metrics {
        reply: oneshot::Sender<NodeMetrics>,
    },
    /// Ask for the identity of this node.
    Identity {
        reply: oneshot::Sender<NodeIdentity>,
    },
    Peers {
        reply: oneshot::Sender<Vec<PeerSummary>>,
    },
    ConnectTo {
        peer_id: PeerId,
        address: SocketAddr,
        reply: oneshot::Sender<Result<()>>,
    },
    Disconnect {
        peer_id: PeerId,
    },
    Shutdown,
}

/// Events produced by connection tasks.
enum ManagerEvent {
    Connected {
        peer_id: PeerId,
        address: SocketAddr,
        session_id: [u8; 8],
        sender: MuxSender,
        connection_id: u64,
    },
    Disconnected {
        peer_id: PeerId,
        reason: String,
        connection_id: u64,
    },
    Frame {
        peer_id: PeerId,
        envelope: Envelope,
    },
}

/// Per-peer state kept by the actor.
#[derive(Debug)]
struct PeerEntry {
    record: PeerRecord,
    state: PeerState,
    retry: RetryQueue,
    connection_id: Option<u64>,
    monitor: Option<Arc<HeartbeatMonitor>>,
    streams: HashMap<u32, u64>,
}

impl PeerEntry {
    fn new(record: PeerRecord) -> Self {
        Self {
            record,
            state: PeerState::Discovered,
            retry: RetryQueue::new(RETRY_MAX_FRAMES, RETRY_MAX_AGE),
            connection_id: None,
            monitor: None,
            streams: HashMap::new(),
        }
    }

    fn summary(&self, connected: bool) -> PeerSummary {
        PeerSummary {
            peer_id: self.record.node_id.clone(),
            display_name: self.record.display_name.clone(),
            address: self
                .record
                .best_address()
                .map(|a| a.to_string()),
            state: self.state.as_str().to_string(),
            source: self.record.source.as_str().to_string(),
            connected,
        }
    }
}

/// The public handle to the peer manager.
#[derive(Clone)]
pub struct PeerManager {
    commands: mpsc::Sender<Command>,
    events: broadcast::Sender<PeerEvent>,
    local_addr: SocketAddr,
}

impl PeerManager {
    /// Start a node: bring up discovery, the TCP listener and the actor.
    pub async fn start(config: NodeConfig) -> anyhow::Result<Self> {
        let registry = PeerRegistry::new();
        let pool = ConnectionPool::new();
        let trust = Arc::new(TrustStore::new());
        let keys = Arc::new(NodeKeys::generate());
        let ids = Arc::new(IdCounter::default());
        let (events, _) = broadcast::channel(EVENT_BUFFER);
        let (command_tx, command_rx) = mpsc::channel(512);
        let (manager_tx, manager_rx) = mpsc::channel(512);
        let shutdown = Shutdown::new();

        info!(
            node_id = %config.node_id,
            fingerprint = %keys.fingerprint(),
            protocol_port = config.protocol_port,
            "starting node"
        );

        // --- Phase 1: discovery backends ---------------------------------
        let mut mdns = None;
        if config.enable_mdns {
            match MdnsDiscovery::start(&config, registry.clone()) {
                Ok(service) => mdns = Some(service),
                Err(e) => warn!(error = %e, "mDNS discovery unavailable, relying on the beacon"),
            }
        }

        let mut beacon = None;
        if config.enable_beacon {
            match Beacon::start(&config, registry.clone()) {
                Ok(service) => beacon = Some(service),
                Err(e) => warn!(error = %e, "multicast beacon unavailable"),
            }
        }

        // --- Phase 2: the TCP listener ------------------------------------
        let listener = PeerListener::bind(config.protocol_port)
            .await
            .map_err(|e| anyhow::anyhow!("failed to bind protocol port: {e}"))?;
        let bound_port = listener.local_addr().port();
        // A wildcard bind is not dialable, so report the loopback address
        // instead - that is what a caller on the same host needs.
        let bound_addr = if listener.local_addr().ip().is_unspecified() {
            SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, bound_port))
        } else {
            listener.local_addr()
        };
        let mut effective_config = config.clone();
        effective_config.protocol_port = bound_port;

        let acceptor = {
            let keys = Arc::clone(&keys);
            let trust = Arc::clone(&trust);
            let manager_tx = manager_tx.clone();
            let node_id = effective_config.node_id.clone();
            let timeout = effective_config.handshake_timeout;
            let max_frame = effective_config.max_frame_bytes;
            let config = effective_config.clone();
            let ids = Arc::clone(&ids);
            let shutdown = shutdown.clone();

            listener.spawn(move |stream, address| {
                let keys = Arc::clone(&keys);
                let trust = Arc::clone(&trust);
                let manager_tx = manager_tx.clone();
                let node_id = node_id.clone();
                let config = config.clone();
                let ids = Arc::clone(&ids);
                let shutdown = shutdown.clone();

                async move {
                    // The inbound side of the handshake.
                    let mut stream = stream;
                    let outcome = match tokio::time::timeout(
                        timeout,
                        responder_handshake(&mut stream, &keys, &node_id),
                    )
                    .await
                    {
                        Ok(Ok(outcome)) => outcome,
                        Ok(Err(e)) => {
                            warn!(%address, error = %e, "inbound handshake failed");
                            return;
                        }
                        Err(_) => {
                            warn!(%address, "inbound handshake timed out");
                            return;
                        }
                    };

                    // Zero-trust: the peer's identity key must match the pin.
                    if !accept_identity(
                        &trust,
                        &outcome.peer_node_id,
                        outcome.peer_ed25519,
                        config.pin_peer_identity,
                    ) {
                        return;
                    }

                    let channel =
                        SecureChannel::establish(stream, outcome.tx, outcome.rx, max_frame);
                    let connection_id = ids.next_id();
                    run_connection(
                        outcome.peer_node_id.clone(),
                        address,
                        channel,
                        outcome.session_id,
                        manager_tx,
                        config,
                        ids,
                        shutdown,
                        connection_id,
                    )
                    .await;
                }
            })
        };

        let discovery_events = registry.subscribe();

        let actor = Actor {
            config: effective_config,
            pool,
            trust,
            keys,
            ids,
            events: events.clone(),
            commands: command_rx,
            manager_events: manager_rx,
            discovery_events,
            peers: HashMap::new(),
            shutdown: shutdown.clone(),
            shutdown_rx: shutdown.subscribe(),
            mdns,
            beacon,
            acceptor,
            manager_tx,
            ledger: Ledger::new(),
            bytes_relayed: 0,
            bytes_internet: 0,
            started_at: Instant::now(),
            next_stream_id: 1,
            peer_heads: HashMap::new(),
            block_index: HashMap::new(),
        };

        tokio::spawn(async move {
            actor.run().await;
        });

        Ok(Self {
            commands: command_tx,
            events,
            local_addr: bound_addr,
        })
    }

    /// The address the peer protocol listens on.
    ///
    /// Useful when the configured port was `0` and the operating system
    /// picked a free one.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Send a text message to a peer.
    ///
    /// Returns the envelope id, which the peer acknowledges.
    pub async fn send_message(&self, peer_id: &str, text: impl Into<String>) -> Result<u64> {
        let (reply, rx) = oneshot::channel();
        self.commands
            .send(Command::SendText {
                peer_id: peer_id.to_string(),
                text: text.into(),
                reply,
            })
            .await
            .map_err(|_| Error::TransportClosed)?;
        rx.await.map_err(|_| Error::TransportClosed)?
    }

    /// Send a binary message to a peer.
    pub async fn send_binary(
        &self,
        peer_id: &str,
        mime: impl Into<String>,
        payload: Vec<u8>,
    ) -> Result<u64> {
        let (reply, rx) = oneshot::channel();
        self.commands
            .send(Command::SendBinary {
                peer_id: peer_id.to_string(),
                mime: mime.into(),
                payload,
                reply,
            })
            .await
            .map_err(|_| Error::TransportClosed)?;
        rx.await.map_err(|_| Error::TransportClosed)?
    }

    /// Send one chunk of a bulk stream.
    ///
    /// With `wait = false` the call fails fast with
    /// [`Error::Backpressure`] when the stream queue is full, which lets the
    /// producer throttle itself. With `wait = true` the call blocks until
    /// there is room, so the *caller* is slowed down instead.
    pub async fn send_stream_chunk(
        &self,
        peer_id: &str,
        stream_id: u32,
        payload: Vec<u8>,
        wait: bool,
    ) -> Result<u64> {
        let (reply, rx) = oneshot::channel();
        self.commands
            .send(Command::SendStreamChunk {
                peer_id: peer_id.to_string(),
                stream_id,
                payload,
                wait,
                reply,
            })
            .await
            .map_err(|_| Error::TransportClosed)?;
        rx.await.map_err(|_| Error::TransportClosed)?
    }

    /// Send a text message to every connected peer.
    pub async fn broadcast(&self, text: impl Into<String>) -> Result<usize> {
        let (reply, rx) = oneshot::channel();
        self.commands
            .send(Command::BroadcastText {
                text: text.into(),
                reply,
            })
            .await
            .map_err(|_| Error::TransportClosed)?;
        rx.await.map_err(|_| Error::TransportClosed)?
    }

    /// The current peer table.
    pub async fn peers(&self) -> Vec<PeerSummary> {
        let (reply, rx) = oneshot::channel();
        if self
            .commands
            .send(Command::Peers { reply })
            .await
            .is_err()
        {
            return Vec::new();
        }
        rx.await.unwrap_or_default()
    }

    /// Dial a peer explicitly, bypassing discovery.
    pub async fn connect_to(&self, peer_id: &str, address: SocketAddr) -> Result<()> {
        let (reply, rx) = oneshot::channel();
        self.commands
            .send(Command::ConnectTo {
                peer_id: peer_id.to_string(),
                address,
                reply,
            })
            .await
            .map_err(|_| Error::TransportClosed)?;
        rx.await.map_err(|_| Error::TransportClosed)?
    }

    /// Close the connection to a peer.
    pub async fn disconnect(&self, peer_id: &str) {
        let _ = self
            .commands
            .send(Command::Disconnect {
                peer_id: peer_id.to_string(),
            })
            .await;
    }

    /// Subscribe to peer events.
    pub fn subscribe(&self) -> broadcast::Receiver<PeerEvent> {
        self.events.subscribe()
    }

    /// Stop the node.
    pub async fn shutdown(&self) {
        let _ = self.commands.send(Command::Shutdown).await;
    }

    /* ------------------------------------------------------------------ */
    /* Rooms, streams and the chain                                        */
    /* ------------------------------------------------------------------ */

    /// Send a text message into a room.
    ///
    /// The message is committed to the chain and relayed to the peers that
    /// should see it. A private room (`dm:node-b`) only reaches that peer.
    /// The block that committed to the message is returned.
    pub async fn send_room_text(
        &self,
        room: &str,
        text: impl Into<String>,
    ) -> Result<LedgerBlock> {
        let (reply, rx) = oneshot::channel();
        self.commands
            .send(Command::SendRoomText {
                room: room.to_string(),
                text: text.into(),
                reply,
            })
            .await
            .map_err(|_| Error::TransportClosed)?;
        rx.await.map_err(|_| Error::TransportClosed)?
    }

    /// Send an attachment into a room.
    pub async fn send_room_binary(
        &self,
        room: &str,
        mime: impl Into<String>,
        payload: Vec<u8>,
    ) -> Result<LedgerBlock> {
        let (reply, rx) = oneshot::channel();
        self.commands
            .send(Command::SendRoomBinary {
                room: room.to_string(),
                mime: mime.into(),
                payload,
                reply,
            })
            .await
            .map_err(|_| Error::TransportClosed)?;
        rx.await.map_err(|_| Error::TransportClosed)?
    }

    /// Announce a live stream and return its id.
    pub async fn start_stream(
        &self,
        room: &str,
        title: impl Into<String>,
        mime: impl Into<String>,
    ) -> Result<u32> {
        let (reply, rx) = oneshot::channel();
        self.commands
            .send(Command::StartStream {
                room: room.to_string(),
                title: title.into(),
                mime: mime.into(),
                reply,
            })
            .await
            .map_err(|_| Error::TransportClosed)?;
        rx.await.map_err(|_| Error::TransportClosed)?
    }

    /// Push one chunk of a live stream.
    pub async fn send_room_stream_chunk(
        &self,
        room: &str,
        stream_id: u32,
        payload: Vec<u8>,
    ) -> Result<()> {
        let (reply, rx) = oneshot::channel();
        self.commands
            .send(Command::StreamChunk {
                room: room.to_string(),
                stream_id,
                payload,
                reply,
            })
            .await
            .map_err(|_| Error::TransportClosed)?;
        rx.await.map_err(|_| Error::TransportClosed)?
    }

    /// Close a live stream.
    pub async fn end_room_stream(&self, room: &str, stream_id: u32) -> Result<()> {
        let (reply, rx) = oneshot::channel();
        self.commands
            .send(Command::EndStream {
                room: room.to_string(),
                stream_id,
                reply,
            })
            .await
            .map_err(|_| Error::TransportClosed)?;
        rx.await.map_err(|_| Error::TransportClosed)?
    }

    /// The most recent blocks of the chain, oldest first.
    pub async fn ledger_snapshot(&self, limit: usize) -> Vec<LedgerBlock> {
        let (reply, rx) = oneshot::channel();
        if self
            .commands
            .send(Command::LedgerSnapshot { limit, reply })
            .await
            .is_err()
        {
            return Vec::new();
        }
        rx.await.unwrap_or_default()
    }

    /// The state of the chain.
    pub async fn ledger_status(&self) -> LedgerStatus {
        let (reply, rx) = oneshot::channel();
        if self
            .commands
            .send(Command::LedgerStatus { reply })
            .await
            .is_err()
        {
            return LedgerStatus {
                height: 0,
                valid: true,
                head: crate::ledger::GENESIS_HASH.to_string(),
                peers_agree: 0,
                peers_total: 0,
            };
        }
        rx.await.unwrap_or(LedgerStatus {
            height: 0,
            valid: true,
            head: crate::ledger::GENESIS_HASH.to_string(),
            peers_agree: 0,
            peers_total: 0,
        })
    }

    /// The counters the interface shows.
    pub async fn metrics(&self) -> NodeMetrics {
        let (reply, rx) = oneshot::channel();
        if self.commands.send(Command::Metrics { reply }).await.is_err() {
            return NodeMetrics::default();
        }
        rx.await.unwrap_or_default()
    }

    /// The identity of this node.
    pub async fn identity(&self) -> NodeIdentity {
        let (reply, rx) = oneshot::channel();
        if self.commands.send(Command::Identity { reply }).await.is_err() {
            return NodeIdentity {
                node_id: String::new(),
                display_name: String::new(),
                fingerprint: String::new(),
                public_key: String::new(),
            };
        }
        rx.await.unwrap_or(NodeIdentity {
            node_id: String::new(),
            display_name: String::new(),
            fingerprint: String::new(),
            public_key: String::new(),
        })
    }
}

impl PeerGateway for PeerManager {
    fn send_text<'a>(&'a self, peer_id: &'a str, text: String) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { self.send_message(peer_id, text).await.map(|_| ()) })
    }

    fn send_binary<'a>(
        &'a self,
        peer_id: &'a str,
        mime: String,
        payload: Vec<u8>,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { self.send_binary(peer_id, mime, payload).await.map(|_| ()) })
    }

    fn peers(&self) -> BoxFuture<'_, Vec<PeerSummary>> {
        Box::pin(async move { self.peers().await })
    }

    fn send_room_text<'a>(
        &'a self,
        room: &'a str,
        text: String,
    ) -> BoxFuture<'a, Result<LedgerBlock>> {
        Box::pin(async move { self.send_room_text(room, text).await })
    }

    fn send_room_binary<'a>(
        &'a self,
        room: &'a str,
        mime: String,
        payload: Vec<u8>,
    ) -> BoxFuture<'a, Result<LedgerBlock>> {
        Box::pin(async move { self.send_room_binary(room, mime, payload).await })
    }

    fn start_stream<'a>(
        &'a self,
        room: &'a str,
        title: String,
        mime: String,
    ) -> BoxFuture<'a, Result<u32>> {
        Box::pin(async move { self.start_stream(room, title, mime).await })
    }

    fn stream_chunk<'a>(
        &'a self,
        room: &'a str,
        stream_id: u32,
        payload: Vec<u8>,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { self.send_room_stream_chunk(room, stream_id, payload).await })
    }

    fn end_stream<'a>(&'a self, room: &'a str, stream_id: u32) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { self.end_room_stream(room, stream_id).await })
    }

    fn ledger_snapshot(&self, limit: usize) -> BoxFuture<'_, Vec<LedgerBlock>> {
        Box::pin(async move { self.ledger_snapshot(limit).await })
    }

    fn ledger_status(&self) -> BoxFuture<'_, LedgerStatus> {
        Box::pin(async move { self.ledger_status().await })
    }

    fn metrics(&self) -> BoxFuture<'_, NodeMetrics> {
        Box::pin(async move { self.metrics().await })
    }

    fn identity(&self) -> BoxFuture<'_, NodeIdentity> {
        Box::pin(async move { self.identity().await })
    }
}

/// The actor that owns all mutable state.
struct Actor {
    config: NodeConfig,
    pool: ConnectionPool,
    trust: Arc<TrustStore>,
    keys: Arc<NodeKeys>,
    ids: Arc<IdCounter>,
    events: broadcast::Sender<PeerEvent>,
    commands: mpsc::Receiver<Command>,
    manager_events: mpsc::Receiver<ManagerEvent>,
    discovery_events: broadcast::Receiver<DiscoveryEvent>,
    peers: HashMap<PeerId, PeerEntry>,
    shutdown: Shutdown,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
    mdns: Option<MdnsDiscovery>,
    beacon: Option<Beacon>,
    acceptor: crate::transport::tcp::TcpAcceptor,
    manager_tx: mpsc::Sender<ManagerEvent>,
    /// The school's private chain.
    ledger: Ledger,
    /// Bytes sent to peers on the same physical network.
    bytes_relayed: u64,
    /// Bytes sent to peers that had to be reached over the internet.
    bytes_internet: u64,
    /// When the node started.
    started_at: Instant,
    /// The next stream id this node hands out.
    next_stream_id: u32,
    /// The chain head each peer reported.
    peer_heads: HashMap<PeerId, String>,
    /// Blocks by the hash of what they commit to, so an incoming message can
    /// be matched with the block that recorded it.
    block_index: HashMap<(String, String), (String, u64)>,
}

impl Actor {
    /// The main loop.
    async fn run(mut self) {
        loop {
            tokio::select! {
                command = self.commands.recv() => {
                    match command {
                        Some(command) => {
                            if self.handle_command(command).await {
                                break;
                            }
                        }
                        None => break,
                    }
                }
                Some(event) = self.manager_events.recv() => self.handle_manager_event(event).await,
                Ok(event) = self.discovery_events.recv() => self.handle_discovery(event),
                _ = self.shutdown_rx.changed() => break,
            }
        }

        info!("node shutting down");
        self.acceptor.abort();
        if let Some(mdns) = self.mdns.take() {
            mdns.shutdown();
        }
        if let Some(beacon) = self.beacon.take() {
            beacon.shutdown();
        }
        self.pool
            .all()
            .into_iter()
            .for_each(|connection| connection.sender.close());
    }

    /// Handle one command from the application.
    ///
    /// Returns `true` when the actor should stop.
    async fn handle_command(&mut self, command: Command) -> bool {
        match command {
            Command::SendText { peer_id, text, reply } => {
                let room = crate::rooms::conversation_room(&self.config.node_id, &peer_id);
                let result =
                    self.send_frame(&peer_id, Frame::ChatText { room, text }, Priority::Chat);
                let _ = reply.send(result);
            }
            Command::SendBinary {
                peer_id,
                mime,
                payload,
                reply,
            } => {
                let room = crate::rooms::conversation_room(&self.config.node_id, &peer_id);
                let result = self.send_frame(
                    &peer_id,
                    Frame::ChatBinary { room, mime, payload },
                    Priority::Chat,
                );
                let _ = reply.send(result);
            }
            Command::SendStreamChunk {
                peer_id,
                stream_id,
                payload,
                wait,
                reply,
            } => {
                self.send_stream_frame(&peer_id, stream_id, payload, wait, reply);
            }
            Command::BroadcastText { text, reply } => {
                let peer_ids = self.pool.peer_ids();
                let mut sent = 0;
                for peer_id in peer_ids {
                    if self
                        .send_frame(
                            &peer_id,
                            Frame::ChatText {
                                room: "general".to_string(),
                                text: text.clone(),
                            },
                            Priority::Chat,
                        )
                        .is_ok()
                    {
                        sent += 1;
                    }
                }
                let _ = reply.send(Ok(sent));
            }
            Command::SendRoomText { room, text, reply } => {
                let result = self.publish_room_text(&room, &text);
                let _ = reply.send(result);
            }
            Command::SendRoomBinary {
                room,
                mime,
                payload,
                reply,
            } => {
                let result = self.publish_room_binary(&room, &mime, &payload);
                let _ = reply.send(result);
            }
            Command::StartStream {
                room,
                title,
                mime,
                reply,
            } => {
                let stream_id = self.next_stream_id;
                self.next_stream_id += 1;

                let frame = Frame::StreamMeta {
                    room: room.clone(),
                    stream_id,
                    title: title.clone(),
                    mime: mime.clone(),
                };
                let sent = self.send_to_room(&room, frame, Priority::Stream);
                self.commit(&room, LedgerKind::Stream, title.as_bytes());

                match sent {
                    Ok(_) => {
                        let _ = reply.send(Ok(stream_id));
                    }
                    Err(e) => {
                        let _ = reply.send(Err(e));
                    }
                }
            }
            Command::StreamChunk {
                room,
                stream_id,
                payload,
                reply,
            } => {
                let result = self.send_room_stream_chunk(&room, stream_id, payload);
                let _ = reply.send(result.map(|_| ()));
            }
            Command::EndStream { room, stream_id, reply } => {
                let frame = Frame::StreamEnd {
                    room: room.clone(),
                    stream_id,
                };
                let result = self.send_to_room(&room, frame, Priority::Stream);
                self.commit(&room, LedgerKind::StreamEnd, &stream_id.to_le_bytes());
                let _ = reply.send(result.map(|_| ()));
            }
            Command::LedgerSnapshot { limit, reply } => {
                let _ = reply.send(self.ledger.tail(limit));
            }
            Command::LedgerStatus { reply } => {
                let _ = reply.send(self.ledger_status());
            }
            Command::Metrics { reply } => {
                let _ = reply.send(self.metrics());
            }
            Command::Identity { reply } => {
                let _ = reply.send(self.identity());
            }
            Command::Peers { reply } => {
                let summaries = self.peer_summaries();
                let _ = reply.send(summaries);
            }
            Command::ConnectTo {
                peer_id,
                address,
                reply,
            } => {
                info!(peer_id = %peer_id, %address, "explicit connect requested");
                self.ensure_entry(&peer_id, Some(address), DiscoverySource::Manual);
                self.spawn_dial(peer_id, address);
                let _ = reply.send(Ok(()));
            }
            Command::Disconnect { peer_id } => {
                self.drop_connection(&peer_id, "closed by the application");
            }
            Command::Shutdown => {
                self.shutdown.trigger();
                return true;
            }
        }
        false
    }

    /// Queue a frame for a peer and remember it for retries.
    fn send_frame(&mut self, peer_id: &str, frame: Frame, _priority: Priority) -> Result<u64> {
        let sender = self
            .pool
            .sender(peer_id)
            .ok_or_else(|| Error::PeerNotConnected(peer_id.to_string()))?;

        let id = self.ids.next_id();
        let envelope = Envelope::next(id, frame.clone());

        if let Some(entry) = self.peers.get_mut(peer_id) {
            entry.retry.track(id, frame.clone());
        }

        match sender.try_send(envelope) {
            Ok(()) => Ok(id),
            Err(Error::Backpressure { .. }) => {
                // The queue is full: park the frame in the background so the
                // actor stays responsive and the message is not lost.
                let background = sender.clone();
                let envelope = Envelope::next(id, frame);
                tokio::spawn(async move {
                    if let Err(e) = background.send(envelope).await {
                        warn!(error = %e, "background send failed");
                    }
                });
                Ok(id)
            }
            Err(e) => Err(e),
        }
    }

    /// Queue a bulk stream chunk.
    ///
    /// The outcome is reported through `reply`. With `wait = false` a full
    /// stream queue is reported to the caller immediately so the producer can
    /// throttle itself; with `wait = true` the reply is only sent once the
    /// chunk found room, which slows the caller down instead.
    fn send_stream_frame(
        &mut self,
        peer_id: &str,
        stream_id: u32,
        payload: Vec<u8>,
        wait: bool,
        reply: oneshot::Sender<Result<u64>>,
    ) {
        let sender = match self.pool.sender(peer_id) {
            Some(sender) => sender,
            None => {
                let _ = reply.send(Err(Error::PeerNotConnected(peer_id.to_string())));
                return;
            }
        };

        let id = self.ids.next_id();
        let seq = {
            let entry = match self.peers.get_mut(peer_id) {
                Some(entry) => entry,
                None => {
                    let _ = reply.send(Err(Error::PeerNotFound(peer_id.to_string())));
                    return;
                }
            };
            let counter = entry.streams.entry(stream_id).or_insert(0);
            let seq = *counter;
            *counter += 1;
            seq
        };
        let frame = Frame::StreamChunk {
            room: crate::rooms::conversation_room(&self.config.node_id, &peer_id),
            stream_id,
            seq,
            payload,
        };

        if let Some(entry) = self.peers.get_mut(peer_id) {
            entry.retry.track(id, frame.clone());
        }

        if wait {
            // Slow the *caller* down: the reply is only sent once the chunk
            // found room in the stream queue.
            let background = sender.clone();
            let envelope = Envelope::next(id, frame);
            tokio::spawn(async move {
                let result = background.send_stream(envelope).await.map(|_| id);
                let _ = reply.send(result);
            });
            return;
        }

        let envelope = Envelope::next(id, frame);
        let result = sender.try_send(envelope);
        match result {
            Ok(()) => {
                let _ = reply.send(Ok(id));
            }
            Err(e) => {
                debug!(peer_id = %peer_id, error = %e, "stream chunk rejected");
                let _ = reply.send(Err(e));
            }
        }
    }

    /// Handle a discovery event.
    fn handle_discovery(&mut self, event: DiscoveryEvent) {
        match event {
            DiscoveryEvent::Discovered(record) => {
                let summary = PeerSummary {
                    peer_id: record.node_id.clone(),
                    display_name: record.display_name.clone(),
                    address: record.best_address().map(|a| a.to_string()),
                    state: PeerState::Discovered.as_str().to_string(),
                    source: record.source.as_str().to_string(),
                    connected: false,
                };
                let _ = self.events.send(PeerEvent::Discovered { peer: summary });

                let address = record.best_address();
                self.ensure_entry(&record.node_id, address, record.source);
                if let Some(address) = address {
                    self.maybe_dial(&record.node_id, address);
                }
            }
            DiscoveryEvent::Updated(record) => {
                let address = record.best_address();
                self.ensure_entry(&record.node_id, address, record.source);
                if let Some(address) = address {
                    self.maybe_dial(&record.node_id, address);
                }
            }
            DiscoveryEvent::Removed(peer_id) => {
                self.drop_connection(&peer_id, "peer left the network");
            }
        }
    }

    /// Handle an event produced by a connection task.
    async fn handle_manager_event(&mut self, event: ManagerEvent) {
        match event {
            ManagerEvent::Connected {
                peer_id,
                address,
                session_id,
                sender,
                connection_id,
            } => {
                // A second connection to a peer we already have is redundant.
                if let Some(entry) = self.peers.get(&peer_id) {
                    if let Some(active) = entry.connection_id {
                        if active != connection_id {
                            debug!(peer_id = %peer_id, "ignoring a duplicate connection");
                            sender.close();
                            return;
                        }
                    }
                }

                let entry = self.peers.entry(peer_id.clone()).or_insert_with(|| {
                    PeerEntry::new(PeerRecord {
                        node_id: peer_id.clone(),
                        display_name: peer_id.clone(),
                        addresses: vec![address],
                        port: address.port(),
                        source: DiscoverySource::Manual,
                        version: "unknown".into(),
                        last_seen: Instant::now(),
                    })
                });
                entry.state = PeerState::Connected;
                entry.connection_id = Some(connection_id);
                entry.monitor = Some(Arc::new(HeartbeatMonitor::new(
                    HeartbeatConfig {
                        interval: self.config.heartbeat_interval,
                        timeout: self.config.heartbeat_timeout,
                        max_missed: self.config.max_missed_pongs,
                    },
                )));
                if !entry.record.addresses.contains(&address) {
                    entry.record.addresses.insert(0, address);
                }

                self.pool.insert(PeerConnection::new(
                    peer_id.clone(),
                    address,
                    sender.clone(),
                    session_id,
                ));

                info!(peer_id = %peer_id, %address, "peer connected");
                let _ = self.events.send(PeerEvent::Connected {
                    peer_id: peer_id.clone(),
                });

                // Introduce ourselves to the peer.
                let node_info = Envelope::next(
                    self.ids.next_id(),
                    Frame::NodeInfo {
                        node_id: self.config.node_id.clone(),
                        display_name: self.config.display_name.clone(),
                        version: crate::config::PROTOCOL_VERSION.to_string(),
                    },
                );
                let _ = sender.try_send(node_info);

                // Tell the peer where our chain stands, so the interface can
                // report how much the network agrees.
                let chain_state = Envelope::next(
                    self.ids.next_id(),
                    Frame::ChainState {
                        head: self.ledger.head().to_string(),
                        height: self.ledger.height(),
                    },
                );
                let _ = sender.try_send(chain_state);

                // --- Step 13: replay everything that was not acknowledged --
                let pending = self
                    .peers
                    .get_mut(&peer_id)
                    .map(|entry| entry.retry.take_all())
                    .unwrap_or_default();
                if !pending.is_empty() {
                    info!(
                        peer_id = %peer_id,
                        count = pending.len(),
                        "replaying unacknowledged frames"
                    );
                    for frame in pending {
                        let id = self.ids.next_id();
                        let envelope = Envelope::next(id, frame.frame.clone());
                        if let Some(entry) = self.peers.get_mut(&peer_id) {
                            entry.retry.track(id, frame.frame);
                        }
                        if sender.try_send(envelope).is_err() {
                            warn!(peer_id = %peer_id, "replay queue is full, frame dropped");
                        }
                    }
                }
            }
            ManagerEvent::Disconnected {
                peer_id,
                reason,
                connection_id,
            } => {
                // Ignore events from connections that already superseded.
                if let Some(entry) = self.peers.get(&peer_id) {
                    if let Some(active) = entry.connection_id {
                        if active != connection_id {
                            return;
                        }
                    }
                }
                self.drop_connection(&peer_id, &reason);
                // Step 13: rebuild the session. The reconnect task re-dials
                // the known address with backoff, and a fresh discovery event
                // (with a new IP) updates the address it uses.
                self.schedule_reconnect(&peer_id);
            }
            ManagerEvent::Frame { peer_id, envelope } => {
                self.handle_incoming_frame(&peer_id, envelope);
            }
        }
    }

    /// Process one frame received from a peer.
    fn handle_incoming_frame(&mut self, peer_id: &str, envelope: Envelope) {
        let frame = envelope.frame;

        match frame {
            Frame::Ping { nonce, .. } => {
                // Answer immediately - the peer is waiting for the pong.
                if let Some(sender) = self.pool.sender(peer_id) {
                    let pong = Envelope::next(
                        self.ids.next_id(),
                        Frame::Pong {
                            nonce,
                            sent_ms: crate::discovery::beacon::now_ms(),
                        },
                    );
                    let _ = sender.try_send(pong);
                }
            }
            Frame::Pong { nonce, .. } => {
                if let Some(entry) = self.peers.get(peer_id) {
                    if let Some(monitor) = &entry.monitor {
                        monitor.on_pong(nonce);
                    }
                }
            }
            Frame::Ack { id } => {
                if let Some(entry) = self.peers.get_mut(peer_id) {
                    if entry.retry.acknowledge(id) {
                        debug!(peer_id = %peer_id, id, "frame acknowledged");
                    }
                }
            }
            Frame::ChatText { room, text } => {
                self.acknowledge(peer_id, envelope.id);
                let author = self.author_of(peer_id);
                let payload_hash = crate::ledger::hex(&crate::ledger::sha256_bytes(text.as_bytes()));
                let (block_hash, height) = self
                    .block_index
                    .get(&(room.clone(), payload_hash))
                    .cloned()
                    .map(|(hash, index)| (Some(hash), Some(index)))
                    .unwrap_or((None, None));
                let _ = self.events.send(PeerEvent::Message {
                    peer_id: peer_id.to_string(),
                    room,
                    author,
                    text,
                    block_hash,
                    height,
                });
            }
            Frame::ChatBinary {
                room,
                mime,
                payload,
            } => {
                self.acknowledge(peer_id, envelope.id);
                let author = self.author_of(peer_id);
                let mut committed = mime.as_bytes().to_vec();
                committed.extend_from_slice(&payload);
                let payload_hash =
                    crate::ledger::hex(&crate::ledger::sha256_bytes(&committed));
                let (block_hash, height) = self
                    .block_index
                    .get(&(room.clone(), payload_hash))
                    .cloned()
                    .map(|(hash, index)| (Some(hash), Some(index)))
                    .unwrap_or((None, None));
                let _ = self.events.send(PeerEvent::Binary {
                    peer_id: peer_id.to_string(),
                    room,
                    author,
                    mime,
                    payload,
                    block_hash,
                    height,
                });
            }
            Frame::StreamMeta {
                room,
                stream_id,
                title,
                mime,
            } => {
                self.acknowledge(peer_id, envelope.id);
                let _ = self.events.send(PeerEvent::StreamMeta {
                    peer_id: peer_id.to_string(),
                    room,
                    stream_id,
                    title,
                    mime,
                });
            }
            Frame::StreamChunk {
                room,
                stream_id,
                seq,
                payload,
            } => {
                self.acknowledge(peer_id, envelope.id);
                let _ = self.events.send(PeerEvent::StreamChunk {
                    peer_id: peer_id.to_string(),
                    room,
                    stream_id,
                    seq,
                    payload,
                });
            }
            Frame::StreamEnd { room, stream_id } => {
                self.acknowledge(peer_id, envelope.id);
                let _ = self.events.send(PeerEvent::StreamEnd {
                    peer_id: peer_id.to_string(),
                    room,
                    stream_id,
                });
            }
            Frame::LedgerBlock { block } => {
                self.acknowledge(peer_id, envelope.id);
                let adopted = self.ledger.adopt(block.clone());
                if adopted > 0 {
                    debug!(
                        index = block.index,
                        actor = %block.actor,
                        "adopted a block from a peer"
                    );
                    self.block_index.insert(
                        (block.room.clone(), block.payload_hash.clone()),
                        (block.hash.clone(), block.index),
                    );
                }
                let _ = self.events.send(PeerEvent::LedgerBlock { block });
            }
            Frame::ChainState { head, height } => {
                self.peer_heads.insert(peer_id.to_string(), head.clone());
                let _ = self.events.send(PeerEvent::ChainState {
                    peer_id: peer_id.to_string(),
                    head,
                    height,
                });
            }
            Frame::NodeInfo {
                node_id: _,
                display_name,
                version,
            } => {
                self.acknowledge(peer_id, envelope.id);
                if let Some(entry) = self.peers.get_mut(peer_id) {
                    entry.record.display_name = display_name;
                    entry.record.version = version;
                }
            }
            Frame::Bye { reason } => {
                info!(peer_id = %peer_id, reason = %reason, "peer said goodbye");
                self.drop_connection(peer_id, "peer closed the session");
            }
        }
    }

    /// Acknowledge an incoming frame.
    fn acknowledge(&self, peer_id: &str, id: u64) {
        if let Some(sender) = self.pool.sender(peer_id) {
            let ack = Envelope::next(self.ids.next_id(), Frame::Ack { id });
            let _ = sender.try_send(ack);
        }
    }

    /// Make sure the peer table has an entry for this peer.
    fn ensure_entry(
        &mut self,
        peer_id: &str,
        address: Option<SocketAddr>,
        source: DiscoverySource,
    ) {
        match self.peers.get_mut(peer_id) {
            Some(entry) => {
                if let Some(address) = address {
                    if !entry.record.addresses.contains(&address) {
                        entry.record.addresses.insert(0, address);
                    }
                }
                if entry.state == PeerState::Disconnected {
                    entry.state = PeerState::Discovered;
                }
            }
            None => {
                let record = PeerRecord {
                    node_id: peer_id.to_string(),
                    display_name: peer_id.to_string(),
                    addresses: address.into_iter().collect(),
                    port: address.map(|a| a.port()).unwrap_or(0),
                    source,
                    version: "unknown".into(),
                    last_seen: Instant::now(),
                };
                self.peers.insert(peer_id.to_string(), PeerEntry::new(record));
            }
        }
    }

    /// Decide whether this node should dial the peer.
    ///
    /// Both nodes apply the same rule - the one with the lexicographically
    /// smaller id dials - so they never dial each other at the same time.
    fn should_dial(&self, peer_id: &str) -> bool {
        peer_id > self.config.node_id.as_str()
    }

    /// Dial the peer when we are allowed to and not already connected.
    fn maybe_dial(&mut self, peer_id: &str, address: SocketAddr) {
        if self.pool.contains(peer_id) {
            return;
        }
        if let Some(entry) = self.peers.get(peer_id) {
            if matches!(entry.state, PeerState::Connecting | PeerState::Handshaking) {
                return;
            }
        }
        if !self.should_dial(peer_id) {
            // The other node dials us; we only listen.
            return;
        }
        self.spawn_dial(peer_id.to_string(), address);
    }

    /// Schedule a reconnect attempt for a peer whose connection dropped.
    ///
    /// The dial task retries with backoff. If the peer moved to a new IP
    /// address, the discovery layer refreshes the record and the next attempt
    /// uses the new address.
    fn schedule_reconnect(&mut self, peer_id: &str) {
        let Some(address) = self.peers.get(peer_id).and_then(|e| e.record.best_address())
        else {
            debug!(peer_id = %peer_id, "no known address, cannot reconnect");
            return;
        };

        if let Some(entry) = self.peers.get(peer_id) {
            if matches!(entry.state, PeerState::Connecting | PeerState::Handshaking) {
                // A dial is already running.
                return;
            }
        }

        info!(peer_id = %peer_id, %address, "scheduling a reconnect");
        self.spawn_dial(peer_id.to_string(), address);
    }

    /// Spawn the outbound connection attempt.
    fn spawn_dial(&mut self, peer_id: PeerId, address: SocketAddr) {
        if let Some(entry) = self.peers.get_mut(&peer_id) {
            entry.state = PeerState::Connecting;
        }

        let keys = Arc::clone(&self.keys);
        let trust = Arc::clone(&self.trust);
        let ids = Arc::clone(&self.ids);
        let manager_tx = self.manager_tx.clone();
        let config = self.config.clone();
        let shutdown = self.shutdown.clone();
        let our_node_id = self.config.node_id.clone();
        let timeout = self.config.handshake_timeout;
        let max_frame = self.config.max_frame_bytes;

        tokio::spawn(async move {
            let mut delay = Duration::from_millis(500);
            for attempt in 0..MAX_RECONNECT_ATTEMPTS {
                match TcpStream::connect(address).await {
                    Ok(mut stream) => {
                        let outcome = match tokio::time::timeout(
                            timeout,
                            initiator_handshake(&mut stream, &keys, &our_node_id),
                        )
                        .await
                        {
                            Ok(Ok(outcome)) => Some(outcome),
                            Ok(Err(e)) => {
                                warn!(peer_id = %peer_id, error = %e, "handshake failed");
                                None
                            }
                            Err(_) => {
                                warn!(peer_id = %peer_id, "handshake timed out");
                                None
                            }
                        };

                        if let Some(outcome) = outcome {
                            if !accept_identity(
                                &trust,
                                &outcome.peer_node_id,
                                outcome.peer_ed25519,
                                config.pin_peer_identity,
                            ) {
                                return;
                            }

                            let channel = SecureChannel::establish(
                                stream,
                                outcome.tx,
                                outcome.rx,
                                max_frame,
                            );
                            let connection_id = ids.next_id();
                            run_connection(
                                outcome.peer_node_id.clone(),
                                address,
                                channel,
                                outcome.session_id,
                                manager_tx,
                                config,
                                ids,
                                shutdown,
                                connection_id,
                            )
                            .await;
                            return;
                        }
                    }
                    Err(e) => {
                        debug!(peer_id = %peer_id, error = %e, "connect failed");
                    }
                }

                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(RECONNECT_BACKOFF * 8);
                let _ = attempt;
            }
        });
    }

    /// Drop the connection to a peer.
    fn drop_connection(&mut self, peer_id: &str, reason: &str) {
        let removed = self.pool.remove(peer_id);
        if let Some(connection) = &removed {
            // Closing the router stops the writer task, which drops the
            // socket and tells the peer that we are gone.
            connection.sender.close();
        }
        if let Some(entry) = self.peers.get_mut(peer_id) {
            entry.state = PeerState::Disconnected;
            entry.connection_id = None;
        }
        if removed.is_some() {
            info!(peer_id = %peer_id, reason = %reason, "peer disconnected");
            let _ = self.events.send(PeerEvent::Disconnected {
                peer_id: peer_id.to_string(),
                reason: reason.to_string(),
            });
        }
    }

    /// Snapshot of the peer table.
    fn peer_summaries(&self) -> Vec<PeerSummary> {
        let mut summaries: Vec<PeerSummary> = self
            .peers
            .values()
            .map(|entry| entry.summary(self.pool.contains(&entry.record.node_id)))
            .collect();
        summaries.sort_by(|a, b| a.peer_id.cmp(&b.peer_id));
        summaries
    }

    /* ------------------------------------------------------------------ */
    /* Rooms                                                                */
    /* ------------------------------------------------------------------ */

    /// The peers that should receive a message for this room.
    ///
    /// A private conversation is only delivered to the two nodes involved;
    /// every other room goes to everybody who is connected.
    fn room_targets(&self, room: &str) -> Vec<PeerId> {
        if !crate::rooms::is_direct(room) {
            return self.pool.peer_ids();
        }

        let rest = room.strip_prefix(crate::rooms::DIRECT_PREFIX).unwrap_or("");
        let parts: Vec<&str> = rest.split(':').collect();

        let target = match parts.as_slice() {
            [single] => *single,
            [first, second] => {
                // The canonical form names both parties; the other one is
                // the target.
                if *first == self.config.node_id {
                    *second
                } else if *second == self.config.node_id {
                    *first
                } else {
                    // Neither is us, so the room was written by somebody
                    // else; deliver it to both.
                    return self.pool.peer_ids();
                }
            }
            _ => return Vec::new(),
        };

        if target.is_empty() || target == self.config.node_id {
            Vec::new()
        } else if self.pool.contains(target) {
            vec![target.to_string()]
        } else {
            Vec::new()
        }
    }

    /// Turn the room a browser used into the canonical room.
    ///
    /// A browser says `dm:node-b`; the wire format always carries the
    /// sorted pair, so both sides agree on the room name.
    fn canonical_room(&self, room: &str) -> String {
        match crate::rooms::direct_target(room) {
            Some(target) => crate::rooms::conversation_room(&self.config.node_id, target),
            None => room.to_string(),
        }
    }

    /// Send a frame to every peer that should see it.
    fn send_to_room(
        &mut self,
        room: &str,
        frame: Frame,
        priority: Priority,
    ) -> Result<usize> {
        let targets = self.room_targets(room);
        let size = frame_size(&frame);
        let mut sent = 0;

        for peer_id in targets {
            if self.send_frame(&peer_id, frame.clone(), priority).is_ok() {
                sent += 1;
                self.count_bytes(&peer_id, size);
            }
        }

        Ok(sent)
    }

    /// Send one chunk of a live stream.
    ///
    /// Every peer numbers its own chunks, so the sequence counter lives on
    /// the peer entry.
    fn send_room_stream_chunk(
        &mut self,
        room: &str,
        stream_id: u32,
        payload: Vec<u8>,
    ) -> Result<usize> {
        let targets = self.room_targets(room);
        let size = payload.len();
        let mut sent = 0;

        for peer_id in targets {
            let seq = {
                let Some(entry) = self.peers.get_mut(&peer_id) else {
                    continue;
                };
                let counter = entry.streams.entry(stream_id).or_insert(0);
                let seq = *counter;
                *counter += 1;
                seq
            };

            let frame = Frame::StreamChunk {
                room: room.to_string(),
                stream_id,
                seq,
                payload: payload.clone(),
            };

            if self.send_frame(&peer_id, frame, Priority::Stream).is_ok() {
                sent += 1;
                self.count_bytes(&peer_id, size);
            }
        }

        Ok(sent)
    }

    /// Publish a text message into a room.
    fn publish_room_text(&mut self, room: &str, text: &str) -> Result<LedgerBlock> {
        let canonical = self.canonical_room(room);
        let frame = Frame::ChatText {
            room: canonical.clone(),
            text: text.to_string(),
        };
        self.send_to_room(&canonical, frame, Priority::Chat)?;
        Ok(self.commit(&canonical, LedgerKind::Message, text.as_bytes()))
    }

    /// Publish an attachment into a room.
    fn publish_room_binary(
        &mut self,
        room: &str,
        mime: &str,
        payload: &[u8],
    ) -> Result<LedgerBlock> {
        let canonical = self.canonical_room(room);
        let frame = Frame::ChatBinary {
            room: canonical.clone(),
            mime: mime.to_string(),
            payload: payload.to_vec(),
        };
        self.send_to_room(&canonical, frame, Priority::Chat)?;

        // The chain commits to the hash of the attachment and its type.
        let mut committed = mime.as_bytes().to_vec();
        committed.extend_from_slice(payload);
        Ok(self.commit(&canonical, LedgerKind::Binary, &committed))
    }

    /* ------------------------------------------------------------------ */
    /* The chain                                                            */
    /* ------------------------------------------------------------------ */

    /// Append a block to the chain and tell the peers about it.
    fn commit(&mut self, room: &str, kind: LedgerKind, payload: &[u8]) -> LedgerBlock {
        let block = self.ledger.append(
            &self.keys,
            &self.config.node_id,
            &self.config.display_name,
            room,
            kind,
            payload,
            now_ms(),
        );

        info!(
            index = block.index,
            room = %room,
            kind = block.kind.as_str(),
            "committed a block to the chain"
        );

        self.block_index.insert(
            (room.to_string(), block.payload_hash.clone()),
            (block.hash.clone(), block.index),
        );

        // Gossip the block so every node ends up with the same chain.
        let frame = Frame::LedgerBlock { block: block.clone() };
        let _ = self.send_to_room(room, frame, Priority::Control);

        let _ = self.events.send(PeerEvent::LedgerBlock { block });
        block
    }

    /// The state of the local chain.
    fn ledger_status(&self) -> LedgerStatus {
        let valid = self.ledger.verify().is_ok();
        let head = self.ledger.head().to_string();
        let total = self.pool.peer_ids().len();
        let agree = self
            .peer_heads
            .values()
            .filter(|peer_head| **peer_head == head)
            .count();

        LedgerStatus {
            height: self.ledger.height(),
            valid,
            head,
            peers_agree: agree,
            peers_total: total,
        }
    }

    /// The counters the interface shows.
    fn metrics(&self) -> NodeMetrics {
        NodeMetrics {
            bytes_relayed: self.bytes_relayed,
            bytes_internet: self.bytes_internet,
            peers_online: self.pool.peer_ids().len(),
            uptime_ms: self.started_at.elapsed().as_millis() as u64,
            ledger_height: self.ledger.height(),
        }
    }

    /// The identity of this node.
    fn identity(&self) -> NodeIdentity {
        NodeIdentity {
            node_id: self.config.node_id.clone(),
            display_name: self.config.display_name.clone(),
            fingerprint: self.keys.fingerprint(),
            public_key: crate::ledger::base64_encode(&self.keys.ed25519_public()),
        }
    }

    /// Count the bytes that went out to a peer.
    ///
    /// A peer that was found over mDNS or the multicast beacon is on the
    /// same physical network, so its traffic costs the school nothing.
    fn count_bytes(&mut self, peer_id: &str, bytes: usize) {
            .peers
            .get(peer_id)
            .map(|entry| {
                matches!(
                    entry.record.source,
                    DiscoverySource::Mdns | DiscoverySource::Beacon
                )
            })
            .unwrap_or(false);

        if on_lan {
            self.bytes_relayed += bytes as u64;
        } else {
            self.bytes_internet += bytes as u64;
        }
    }
}

/// The current time in milliseconds since the epoch.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Roughly how many bytes a frame puts on the wire.
fn frame_size(frame: &Frame) -> usize {
    match frame {
        Frame::ChatText { text, .. } => text.len(),
        Frame::ChatBinary { payload, .. } => payload.len(),
        Frame::StreamChunk { payload, .. } => payload.len(),
        Frame::StreamMeta { title, mime, .. } => title.len() + mime.len(),
        Frame::LedgerBlock { .. } => 256,
        _ => 32,
    }
}

/// Decide whether a peer may continue after its identity was checked.
///
/// A peer is accepted the first time it is seen. When it comes back with a
/// different identity key it is only refused when `strict` is set - the
/// identity key is ephemeral by design, so a peer that restarted presents a
/// new one. The change is always reported.
fn accept_identity(
    trust: &TrustStore,
    peer_id: &str,
    key: [u8; 32],
    strict: bool,
) -> bool {
    match trust.inspect(peer_id, key) {
        IdentityStatus::FirstContact => {
            trust.pin(peer_id, key);
            true
        }
        IdentityStatus::Known => true,
        IdentityStatus::Changed => {
            if strict {
                warn!(
                    peer_id = %peer_id,
                    "rejecting peer: the identity key changed since the last contact"
                );
                return false;
            }
            warn!(
                peer_id = %peer_id,
                "the peer's identity key changed (expected after a restart, \
                 since identity keys are ephemeral)"
            );
            trust.repin(peer_id, key);
            true
        }
    }
}

/// Run one established connection until it dies.
///
/// Owns the writer, the reader and the heartbeat of that connection.
async fn run_connection(
    peer_id: PeerId,
    address: SocketAddr,
    channel: SecureChannel,
    session_id: [u8; 8],
    manager_tx: mpsc::Sender<ManagerEvent>,
    config: NodeConfig,
    ids: Arc<IdCounter>,
    shutdown: Shutdown,
    connection_id: u64,
) {
    let router = MuxRouter::new(RouterConfig {
        control_capacity: config.control_queue_capacity,
        chat_capacity: config.chat_queue_capacity,
        stream_capacity: config.stream_queue_capacity,
    });
    let (sender, receiver) = router.split();

    // The writer owns the socket; the reader owns the incoming side.
    let writer = spawn_writer(channel.sender, receiver);

    // Tell the manager that this peer is now reachable.
    let _ = manager_tx
        .send(ManagerEvent::Connected {
            peer_id: peer_id.clone(),
            address,
            session_id,
            sender: sender.clone(),
            connection_id,
        })
        .await;

    let monitor = Arc::new(HeartbeatMonitor::new(HeartbeatConfig {
        interval: config.heartbeat_interval,
        timeout: config.heartbeat_timeout,
        max_missed: config.max_missed_pongs,
    }));

    let mut heartbeat = {
        let sender = sender.clone();
        let monitor = Arc::clone(&monitor);
        let ids = Arc::clone(&ids);
        let peer_id = peer_id.clone();
        let shutdown = shutdown.subscribe();
        tokio::spawn(async move { run_heartbeat(sender, monitor, ids, peer_id, shutdown).await })
    };

    let mut reader = {
        let mut receiver = channel.receiver;
        let manager_tx = manager_tx.clone();
        let peer_id = peer_id.clone();
        tokio::spawn(async move {
            let reason = loop {
                match receiver.recv().await {
                    Ok(Some(envelope)) => {
                        if manager_tx
                            .send(ManagerEvent::Frame {
                                peer_id: peer_id.clone(),
                                envelope,
                            })
                            .await
                            .is_err()
                        {
                            break "manager stopped".to_string();
                        }
                    }
                    Ok(None) => break "peer closed the connection".to_string(),
                    Err(e) => {
                        debug!(peer_id = %peer_id, error = %e, "connection error");
                        break format!("connection error: {e}");
                    }
                }
            };
            let _ = manager_tx
                .send(ManagerEvent::Disconnected {
                    peer_id,
                    reason,
                    connection_id,
                })
                .await;
        })
    };

    tokio::select! {
        _ = &mut reader => {
            heartbeat.abort();
        }
        healthy = &mut heartbeat => {
            if !matches!(healthy, Ok(true)) {
                reader.abort();
                let _ = manager_tx
                    .send(ManagerEvent::Disconnected {
                        peer_id,
                        reason: "heartbeat timeout".to_string(),
                        connection_id,
                    })
                    .await;
            } else {
                reader.abort();
            }
        }
    }

    // Closing the router makes the writer drain and stop.
    sender.close();
    writer.abort();
    let _ = address;
}
