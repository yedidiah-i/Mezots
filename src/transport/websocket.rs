//! The browser-facing WebSocket endpoint.
//!
//! Browsers cannot speak the raw peer protocol - they cannot do the
//! Diffie-Hellman handshake in the page, and they cannot open a raw TCP
//! socket at all. This module therefore exposes a JSON bridge:
//!
//! * browser -> node: [`WsCommand`] (rooms, messages, streams, the chain)
//! * node -> browser: [`WsEvent`] (peer state, messages, stream chunks,
//!   chain blocks, counters)
//!
//! The bridge is intentionally thin: it translates and forwards, and all
//! protocol logic stays behind the [`PeerGateway`] interface.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use base64::Engine;
use futures_util::future::BoxFuture;
use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};

use crate::error::Result;
use crate::ledger::{LedgerBlock, LedgerKind};
use crate::peer::{
    LedgerStatus, NodeIdentity, NodeMetrics, PeerEvent, PeerSummary,
};
use crate::reels::{Reel, ReelStore};

/// Maximum number of queued messages per browser client.
const CLIENT_QUEUE: usize = 512;

/// How often the node tells the browsers what it is doing.
const STATUS_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/* -------------------------------------------------------------------------- */
/* The gateway                                                                 */
/* -------------------------------------------------------------------------- */

/// Bridge to the peer manager, implemented by the manager itself.
pub trait PeerGateway: Send + Sync + 'static {
    /// Send a text message to a peer.
    fn send_text<'a>(&'a self, peer_id: &'a str, text: String) -> BoxFuture<'a, Result<()>>;

    /// Send a binary message to a peer.
    fn send_binary<'a>(
        &'a self,
        peer_id: &'a str,
        mime: String,
        payload: Vec<u8>,
    ) -> BoxFuture<'a, Result<()>>;

    /// List the known peers.
    fn peers(&self) -> BoxFuture<'_, Vec<PeerSummary>>;

    /// Send a text message into a room.
    fn send_room_text<'a>(
        &'a self,
        room: &'a str,
        text: String,
    ) -> BoxFuture<'a, Result<LedgerBlock>>;

    /// Send an attachment into a room.
    fn send_room_binary<'a>(
        &'a self,
        room: &'a str,
        mime: String,
        payload: Vec<u8>,
        kind: LedgerKind,
    ) -> BoxFuture<'a, Result<LedgerBlock>>;

    /// Announce a live stream.
    fn start_stream<'a>(
        &'a self,
        room: &'a str,
        title: String,
        mime: String,
    ) -> BoxFuture<'a, Result<u32>>;

    /// Push one chunk of a live stream.
    fn stream_chunk<'a>(
        &'a self,
        room: &'a str,
        stream_id: u32,
        payload: Vec<u8>,
    ) -> BoxFuture<'a, Result<()>>;

    /// Close a live stream.
    fn end_stream<'a>(&'a self, room: &'a str, stream_id: u32) -> BoxFuture<'a, Result<()>>;

    /// The most recent blocks of the chain.
    fn ledger_snapshot(&self, limit: usize) -> BoxFuture<'_, Vec<LedgerBlock>>;

    /// The state of the chain.
    fn ledger_status(&self) -> BoxFuture<'_, LedgerStatus>;

    /// The counters of the node.
    fn metrics(&self) -> BoxFuture<'_, NodeMetrics>;

    /// The identity of the node.
    fn identity(&self) -> BoxFuture<'_, NodeIdentity>;
}

/* -------------------------------------------------------------------------- */
/* Payloads                                                                    */
/* -------------------------------------------------------------------------- */

/// A message delivered into a room.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomMessage {
    /// The room the message belongs to.
    pub room: String,
    /// Who sent it.
    pub peer_id: String,
    /// The display name of the sender.
    pub author: String,
    /// The body.
    pub text: String,
    /// When it arrived.
    pub ts: u64,
    /// The correlation id the browser used.
    pub client_msg_id: Option<String>,
    /// The block that committed to this message.
    pub block_hash: Option<String>,
    /// The height of that block.
    pub height: Option<u64>,
    /// Whether the body was encrypted in the browser.
    pub encrypted: Option<bool>,
}

/// An attachment delivered into a room.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomBinary {
    /// The room the attachment belongs to.
    pub room: String,
    /// Who sent it.
    pub peer_id: String,
    /// The display name of the sender.
    pub author: String,
    /// MIME type of the payload.
    pub mime: String,
    /// Base64 encoded payload.
    pub payload_base64: String,
    /// When it arrived.
    pub ts: u64,
    /// The correlation id the browser used.
    pub client_msg_id: Option<String>,
    /// The block that committed to this attachment.
    pub block_hash: Option<String>,
}

/// Metadata about a live stream.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamMeta {
    /// The room the stream belongs to.
    pub room: String,
    /// The stream id.
    pub stream_id: u32,
    /// The title of the broadcast.
    pub title: String,
    /// MIME type of the chunks.
    pub mime: String,
    /// Who is broadcasting.
    pub publisher: String,
    /// The node id of the broadcaster.
    pub publisher_peer_id: String,
    /// How many people are watching.
    pub viewers: usize,
}

/// One chunk of a live stream.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamChunkEvent {
    /// The room the stream belongs to.
    pub room: String,
    /// The stream id.
    pub stream_id: u32,
    /// The sequence number of the chunk.
    pub seq: u64,
    /// Base64 encoded payload.
    pub payload_base64: String,
    /// Who is broadcasting.
    pub publisher_peer_id: String,
}

/// A room as the node knows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomState {
    /// The room id.
    pub id: String,
    /// The display name.
    pub title: String,
    /// What kind of room it is.
    pub kind: String,
    /// The members that are present.
    pub members: Vec<String>,
    /// Whether direct messages are end to end encrypted.
    pub e2e: bool,
}

/* -------------------------------------------------------------------------- */
/* Commands                                                                    */
/* -------------------------------------------------------------------------- */

/// A command sent by a browser client.
///
/// The names match the TypeScript contract in `lib/protocol.ts` exactly:
/// `rename_all` renames the variants and `rename_all_fields` renames the
/// fields inside them.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum WsCommand {
    /// Send a text message to a peer.
    SendText {
        /// Target peer.
        peer_id: String,
        /// Message body.
        text: String,
    },
    /// Send a binary message to a peer.
    SendBinary {
        /// Target peer.
        peer_id: String,
        /// MIME type.
        mime: String,
        /// Base64 encoded payload.
        payload_base64: String,
    },
    /// Request the current peer list.
    ListPeers,
    /// Subscribe the browser to a room.
    Join {
        /// The room id.
        room: String,
    },
    /// Unsubscribe the browser from a room.
    Leave {
        /// The room id.
        room: String,
    },
    /// Send a text message into a room.
    SendRoomText {
        /// The room id.
        room: String,
        /// Message body.
        text: String,
        /// The correlation id the browser used.
        client_msg_id: Option<String>,
    },
    /// Send an attachment into a room.
    SendRoomBinary {
        /// The room id.
        room: String,
        /// MIME type.
        mime: String,
        /// Base64 encoded payload.
        payload_base64: String,
        /// The correlation id the browser used.
        client_msg_id: Option<String>,
    },
    /// Announce a live stream.
    StreamStart {
        /// The room id.
        room: String,
        /// Title of the broadcast.
        title: String,
        /// MIME type of the chunks.
        mime: String,
    },
    /// Push one chunk of a live stream.
    StreamChunk {
        /// The room id.
        room: String,
        /// The stream id.
        stream_id: u32,
        /// Base64 encoded payload.
        payload_base64: String,
    },
    /// Close a live stream.
    StreamEnd {
        /// The room id.
        room: String,
        /// The stream id.
        stream_id: u32,
    },
    /// Ask for the most recent blocks of the chain.
    LedgerSnapshot {
        /// How many blocks to fetch.
        limit: Option<usize>,
    },
    /// Ask the node to re-verify its chain.
    LedgerVerify,
    /// Ask for the published clips.
    ListReels,
    /// Publish a clip.
    StoreReel {
        /// The caption.
        title: String,
        /// MIME type of the clip.
        mime: String,
        /// Base64 encoded payload.
        payload_base64: String,
    },
}

/* -------------------------------------------------------------------------- */
/* Events                                                                      */
/* -------------------------------------------------------------------------- */

/// A message sent to a browser client.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum WsEvent {
    /// A peer was discovered.
    PeerDiscovered {
        /// The peer.
        peer: PeerSummary,
    },
    /// A peer is connected.
    PeerConnected {
        /// Node id.
        peer_id: String,
    },
    /// A peer disconnected.
    PeerDisconnected {
        /// Node id.
        peer_id: String,
        /// Reason.
        reason: String,
    },
    /// A text message arrived.
    Message {
        /// Sender.
        peer_id: String,
        /// Body.
        text: String,
    },
    /// A binary message arrived.
    Binary {
        /// Sender.
        peer_id: String,
        /// MIME type.
        mime: String,
        /// Base64 encoded payload.
        payload_base64: String,
    },
    /// A bulk stream chunk arrived.
    StreamChunk {
        /// Sender.
        peer_id: String,
        /// Stream id.
        stream_id: u32,
        /// Sequence number.
        seq: u64,
        /// Base64 encoded payload.
        payload_base64: String,
    },
    /// The current peer list.
    Peers {
        /// Known peers.
        peers: Vec<PeerSummary>,
    },
    /// Something went wrong.
    Error {
        /// Human readable message.
        message: String,
    },
    /// The identity of the node.
    Identity {
        /// The identity.
        identity: NodeIdentity,
    },
    /// A message arrived in a room.
    RoomMessage {
        /// The message.
        message: RoomMessage,
    },
    /// An attachment arrived in a room.
    RoomBinary {
        /// The attachment.
        message: RoomBinary,
    },
    /// The rooms the node knows about.
    RoomState {
        /// The rooms.
        rooms: Vec<RoomState>,
    },
    /// A live stream started.
    StreamMeta {
        /// The stream.
        stream: StreamMeta,
    },
    /// A chunk of a live stream arrived.
    RoomStreamChunk {
        /// The chunk.
        chunk: StreamChunkEvent,
    },
    /// A live stream ended.
    RoomStreamEnd {
        /// The room.
        room: String,
        /// The stream id.
        stream_id: u32,
    },
    /// A block of the chain arrived.
    LedgerBlock {
        /// The block.
        block: LedgerBlock,
    },
    /// The state of the chain.
    LedgerStatus {
        /// The state.
        status: LedgerStatus,
    },
    /// A snapshot of the chain.
    LedgerSnapshot {
        /// The blocks, oldest first.
        blocks: Vec<LedgerBlock>,
    },
    /// The counters of the node.
    Metrics {
        /// The counters.
        metrics: NodeMetrics,
    },
    /// The published clips.
    Reels {
        /// The clips.
        reels: Vec<Reel>,
    },
}

/// A message that is on its way to a browser client.
#[derive(Debug, Clone)]
enum WsOutbound {
    /// A JSON event.
    Text(String),
    /// A pong answering a ping.
    Pong(bytes::Bytes),
}

/// Tracks the connected browser clients.
#[derive(Debug, Default)]
pub struct WsHub {
    clients: Mutex<HashMap<u64, mpsc::Sender<WsOutbound>>>,
    next_id: AtomicU64,
}

impl WsHub {
    /// Create an empty hub.
    pub fn new() -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    /// Register a client and return its id.
    fn register(&self, sender: mpsc::Sender<WsOutbound>) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.clients.lock().insert(id, sender);
        id
    }

    /// Remove a client.
    fn unregister(&self, id: u64) {
        self.clients.lock().remove(&id);
    }

    /// Publish an event to every client.
    pub fn publish(&self, event: &WsEvent) {
        let Ok(payload) = serde_json::to_string(event) else {
            return;
        };
        let clients = self.clients.lock();
        for (id, sender) in clients.iter() {
            if sender.try_send(WsOutbound::Text(payload.clone())).is_err() {
                debug!(client = id, "websocket client is too slow, dropping event");
            }
        }
    }

    /// Number of connected clients.
    pub fn client_count(&self) -> usize {
        self.clients.lock().len()
    }
}

/// Shared state of the WebSocket server.
struct WsState {
    hub: Arc<WsHub>,
    gateway: Arc<dyn PeerGateway>,
    reels: ReelStore,
    /// The identity of this node, cached for the status task.
    identity: NodeIdentity,
}

/// The WebSocket server.
pub struct WebSocketServer {
    local_addr: std::net::SocketAddr,
    hub: Arc<WsHub>,
    task: tokio::task::JoinHandle<()>,
}

impl WebSocketServer {
    /// Bind the WebSocket endpoint and start serving.
    pub async fn start(
        port: u16,
        gateway: Arc<dyn PeerGateway>,
        events: broadcast::Receiver<PeerEvent>,
        reels: ReelStore,
    ) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("0.0.0.0", port)).await?;
        let local_addr = listener.local_addr()?;
        let hub = Arc::new(WsHub::new());

        let identity = gateway.identity().await;
        let state = Arc::new(WsState {
            hub: Arc::clone(&hub),
            gateway,
            reels,
            identity,
        });

        let app = Router::new()
            .route("/ws", get(ws_upgrade))
            .route("/health", get(health))
            .route("/reels/{name}", get(serve_reel))
            .with_state(Arc::clone(&state));

        let task = tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                error!(error = %e, "websocket server stopped");
            }
        });

        // Forward node events to the browser clients.
        let hub_events = Arc::clone(&hub);
        let event_state = Arc::clone(&state);
        tokio::spawn(async move {
            let mut events = events;
            while let Ok(event) = events.recv().await {
                for ws_event in convert_event(event, &event_state) {
                    hub_events.publish(&ws_event);
                }
            }
        });

        // Keep the browsers informed about what the node is doing.
        let hub_status = Arc::clone(&hub);
        let status_state = Arc::clone(&state);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(STATUS_INTERVAL);
            loop {
                ticker.tick().await;
                let metrics = status_state.gateway.metrics().await;
                hub_status.publish(&WsEvent::Metrics { metrics });
                let status = status_state.gateway.ledger_status().await;
                hub_status.publish(&WsEvent::LedgerStatus { status });
                let peers = status_state.gateway.peers().await;
                hub_status.publish(&WsEvent::Peers { peers });
            }
        });

        info!(%local_addr, "websocket server listening on /ws");

        Ok(Self {
            local_addr,
            hub,
            task,
        })
    }

    /// The address the server is bound to.
    pub fn local_addr(&self) -> std::net::SocketAddr {
        self.local_addr
    }

    /// The client hub.
    pub fn hub(&self) -> &Arc<WsHub> {
        &self.hub
    }

    /// Stop the server.
    pub fn shutdown(&self) {
        self.task.abort();
    }
}

/// Translate a node event into WebSocket events.
fn convert_event(event: PeerEvent, state: &WsState) -> Vec<WsEvent> {
    match event {
        PeerEvent::Discovered { peer } => vec![WsEvent::PeerDiscovered { peer }],
        PeerEvent::Connected { peer_id } => vec![WsEvent::PeerConnected { peer_id }],
        PeerEvent::Disconnected { peer_id, reason } => {
            vec![WsEvent::PeerDisconnected { peer_id, reason }]
        }
        PeerEvent::Message {
            peer_id,
            room,
            author,
            text,
            block_hash,
            height,
        } => vec![WsEvent::RoomMessage {
            message: RoomMessage {
                room,
                peer_id,
                author,
                text,
                ts: now_ms(),
                client_msg_id: None,
                block_hash,
                height,
                encrypted: None,
            },
        }],
        PeerEvent::Binary {
            peer_id,
            room,
            author,
            mime,
            payload,
            block_hash,
            ..
        } => {
            let payload_base64 =
                base64::engine::general_purpose::STANDARD.encode(payload.clone());

            // A clip that arrives from a peer is stored as a reel, so the
            // feed is shared across the network.
            if room == crate::rooms::REELS_ROOM {
                let reel = state.reels.store(
                    &title_from_mime(&mime),
                    &mime,
                    &payload,
                    &author,
                    &peer_id,
                );
                if let Ok(reel) = reel {
                    if let Some(hash) = block_hash.clone() {
                        state.reels.attach_block(&reel.id, &hash);
                    }
                }
                return vec![WsEvent::Reels {
                    reels: state.reels.list(),
                }];
            }

            vec![WsEvent::RoomBinary {
                message: RoomBinary {
                    room,
                    peer_id,
                    author,
                    mime,
                    payload_base64,
                    ts: now_ms(),
                    client_msg_id: None,
                    block_hash,
                },
            }]
        }
        PeerEvent::StreamMeta {
            peer_id,
            room,
            stream_id,
            title,
            mime,
        } => vec![WsEvent::StreamMeta {
            stream: StreamMeta {
                room,
                stream_id,
                title,
                mime,
                publisher: peer_id.clone(),
                publisher_peer_id: peer_id,
                viewers: 0,
            },
        }],
        PeerEvent::StreamChunk {
            peer_id,
            room,
            stream_id,
            seq,
            payload,
        } => vec![WsEvent::RoomStreamChunk {
            chunk: StreamChunkEvent {
                room,
                stream_id,
                seq,
                payload_base64: base64::engine::general_purpose::STANDARD.encode(payload),
                publisher_peer_id: peer_id,
            },
        }],
        PeerEvent::StreamEnd {
            peer_id: _,
            room,
            stream_id,
        } => vec![WsEvent::RoomStreamEnd { room, stream_id }],
        PeerEvent::LedgerBlock { block } => vec![WsEvent::LedgerBlock { block }],
        PeerEvent::ChainState { .. } => Vec::new(),
        PeerEvent::Error { message } => vec![WsEvent::Error { message }],
    }
}

/// Pull the caption out of a MIME type that carries one.
fn title_from_mime(mime: &str) -> String {
    for part in mime.split(';').skip(1) {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("name=") {
            return percent_decode(value);
        }
    }
    "Clip".to_string()
}

/// Decode a percent-encoded string.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = (bytes[index + 1] as char).to_digit(16);
            let low = (bytes[index + 2] as char).to_digit(16);
            if let (Some(high), Some(low)) = (high, low) {
                out.push(((high << 4) | low) as u8);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Health endpoint.
async fn health() -> Response {
    axum::Json(serde_json::json!({ "status": "ok" })).into_response()
}

/// Serve a published clip.
async fn serve_reel(
    AxumPath(name): AxumPath<String>,
    State(state): State<Arc<WsState>>,
) -> Response {
    let Some(path) = state.reels.file_for(&name) else {
        return (StatusCode::NOT_FOUND, "no such clip").into_response();
    };
    let Ok(bytes) = tokio::fs::read(&path).await else {
        return (StatusCode::NOT_FOUND, "no such clip").into_response();
    };

    let mime = mime_for(&name);
    let mut response = bytes.into_response();
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        mime.parse().unwrap_or(axum::http::HeaderValue::from_static("application/octet-stream")),
    );
    response
}

/// The MIME type that belongs to a clip file name.
fn mime_for(name: &str) -> &'static str {
    match PathBuf::from(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
    {
        "webm" => "video/webm",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        _ => "application/octet-stream",
    }
}

/// Upgrade an HTTP request into a WebSocket session.
async fn ws_upgrade(ws: WebSocketUpgrade, State(state): State<Arc<WsState>>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Run one browser session.
async fn handle_socket(socket: WebSocket, state: Arc<WsState>) {
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::channel::<WsOutbound>(CLIENT_QUEUE);
    let client_id = state.hub.register(tx.clone());

    // The writer owns the socket; every outgoing frame goes through the
    // queue so that only one task ever touches the sink.
    let writer = tokio::spawn(async move {
        while let Some(outbound) = rx.recv().await {
            let message = match outbound {
                WsOutbound::Text(payload) => Message::Text(payload.into()),
                WsOutbound::Pong(payload) => Message::Pong(payload),
            };
            if sink.send(message).await.is_err() {
                break;
            }
        }
    });

    info!(client = client_id, "websocket client connected");

    // Greet the browser with the identity of the node.
    state.hub.publish(&WsEvent::Identity {
        identity: state.identity.clone(),
    });

    while let Some(Ok(message)) = stream.next().await {
        match message {
            Message::Text(text) => {
                handle_command(&state, text.as_str()).await;
            }
            Message::Binary(bytes) => {
                warn!(client = client_id, "ignoring unsolicited binary websocket message");
                let _ = bytes;
            }
            Message::Ping(payload) => {
                let _ = tx.send(WsOutbound::Pong(payload)).await;
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    state.hub.unregister(client_id);
    writer.abort();
    info!(client = client_id, "websocket client disconnected");
}

/// Execute one command coming from a browser client.
async fn handle_command(state: &Arc<WsState>, raw: &str) {
    let command: WsCommand = match serde_json::from_str(raw) {
        Ok(c) => c,
        Err(e) => {
            state.hub.publish(&WsEvent::Error {
                message: format!("invalid command: {e}"),
            });
            return;
        }
    };

    match command {
        WsCommand::SendText { peer_id, text } => {
            if let Err(e) = state.gateway.send_text(&peer_id, text).await {
                state.hub.publish(&WsEvent::Error {
                    message: e.to_string(),
                });
            }
        }
        WsCommand::SendBinary {
            peer_id,
            mime,
            payload_base64,
        } => {
            match base64::engine::general_purpose::STANDARD.decode(payload_base64) {
                Ok(payload) => {
                    if let Err(e) = state.gateway.send_binary(&peer_id, mime, payload).await {
                        state.hub.publish(&WsEvent::Error {
                            message: e.to_string(),
                        });
                    }
                }
                Err(e) => {
                    state.hub.publish(&WsEvent::Error {
                        message: format!("invalid base64 payload: {e}"),
                    });
                }
            }
        }
        WsCommand::ListPeers => {
            let peers = state.gateway.peers().await;
            state.hub.publish(&WsEvent::Peers { peers });
        }
        WsCommand::Join { room: _ } => {
            state
                .hub
                .publish(&WsEvent::RoomState { rooms: room_states(state).await });
        }
        WsCommand::Leave { .. } => {}
        WsCommand::SendRoomText {
            room,
            text,
            client_msg_id,
        } => {
            match state.gateway.send_room_text(&room, text.clone()).await {
                Ok(block) => {
                    // Echo the message back so the browser can confirm the
                    // optimistic copy and show the block it was committed to.
                    state.hub.publish(&WsEvent::RoomMessage {
                        message: RoomMessage {
                            room: crate::rooms::conversation_room(
                                &state.identity.node_id,
                                room.strip_prefix(crate::rooms::DIRECT_PREFIX)
                                    .unwrap_or(&room)
                                    .split(':')
                                    .next()
                                    .unwrap_or(&room),
                            ),
                            peer_id: state.identity.node_id.clone(),
                            author: state.identity.display_name.clone(),
                            text,
                            ts: now_ms(),
                            client_msg_id,
                            block_hash: Some(block.hash),
                            height: Some(block.index),
                            encrypted: None,
                        },
                    });
                }
                Err(e) => {
                    state.hub.publish(&WsEvent::Error {
                        message: e.to_string(),
                    });
                }
            }
        }
        WsCommand::SendRoomBinary {
            room,
            mime,
            payload_base64,
            client_msg_id,
        } => {
            let payload = match base64::engine::general_purpose::STANDARD.decode(payload_base64) {
                Ok(payload) => payload,
                Err(e) => {
                    state.hub.publish(&WsEvent::Error {
                        message: format!("invalid base64 payload: {e}"),
                    });
                    return;
                }
            };

            let kind = if room == crate::rooms::REELS_ROOM {
                LedgerKind::Reel
            } else {
                LedgerKind::Binary
            };

            match state
                .gateway
                .send_room_binary(&room, mime.clone(), payload.clone(), kind)
                .await
            {
                Ok(block) => {
                    if room == crate::rooms::REELS_ROOM {
                        if let Ok(reel) = state.reels.store(
                            &title_from_mime(&mime),
                            &mime,
                            &payload,
                            &state.identity.display_name,
                            &state.identity.node_id,
                        ) {
                            state.reels.attach_block(&reel.id, &block.hash);
                        }
                        state.hub.publish(&WsEvent::Reels {
                            reels: state.reels.list(),
                        });
                    } else {
                        state.hub.publish(&WsEvent::RoomBinary {
                            message: RoomBinary {
                                room,
                                peer_id: state.identity.node_id.clone(),
                                author: state.identity.display_name.clone(),
                                mime,
                                payload_base64: base64::engine::general_purpose::STANDARD
                                    .encode(&payload),
                                ts: now_ms(),
                                client_msg_id,
                                block_hash: Some(block.hash),
                            },
                        });
                    }
                }
                Err(e) => {
                    state.hub.publish(&WsEvent::Error {
                        message: e.to_string(),
                    });
                }
            }
        }
        WsCommand::StreamStart {
            room,
            title,
            mime,
        } => match state.gateway.start_stream(&room, title, mime).await {
            Ok(stream_id) => {
                state.hub.publish(&WsEvent::StreamMeta {
                    stream: StreamMeta {
                        room,
                        stream_id,
                        title: String::new(),
                        mime: String::new(),
                        publisher: state.identity.display_name.clone(),
                        publisher_peer_id: state.identity.node_id.clone(),
                        viewers: 0,
                    },
                });
            }
            Err(e) => {
                state.hub.publish(&WsEvent::Error {
                    message: e.to_string(),
                });
            }
        },
        WsCommand::StreamChunk {
            room,
            stream_id,
            payload_base64,
        } => {
            let payload = match base64::engine::general_purpose::STANDARD.decode(payload_base64) {
                Ok(payload) => payload,
                Err(e) => {
                    state.hub.publish(&WsEvent::Error {
                        message: format!("invalid base64 payload: {e}"),
                    });
                    return;
                }
            };
            if let Err(e) = state
                .gateway
                .stream_chunk(&room, stream_id, payload)
                .await
            {
                state.hub.publish(&WsEvent::Error {
                    message: e.to_string(),
                });
            }
        }
        WsCommand::StreamEnd { room, stream_id } => {
            if let Err(e) = state.gateway.end_stream(&room, stream_id).await {
                state.hub.publish(&WsEvent::Error {
                    message: e.to_string(),
                });
            }
            state.hub.publish(&WsEvent::RoomStreamEnd { room, stream_id });
        }
        WsCommand::LedgerSnapshot { limit } => {
            let blocks = state.gateway.ledger_snapshot(limit.unwrap_or(200)).await;
            state.hub.publish(&WsEvent::LedgerSnapshot { blocks });
        }
        WsCommand::LedgerVerify => {
            let status = state.gateway.ledger_status().await;
            state.hub.publish(&WsEvent::LedgerStatus { status });
        }
        WsCommand::ListReels => {
            state.hub.publish(&WsEvent::Reels {
                reels: state.reels.list(),
            });
        }
        WsCommand::StoreReel {
            title,
            mime,
            payload_base64,
        } => {
            let payload = match base64::engine::general_purpose::STANDARD.decode(payload_base64) {
                Ok(payload) => payload,
                Err(e) => {
                    state.hub.publish(&WsEvent::Error {
                        message: format!("invalid base64 payload: {e}"),
                    });
                    return;
                }
            };

            let reel = state.reels.store(
                &title,
                &mime,
                &payload,
                &state.identity.display_name,
                &state.identity.node_id,
            );

            match reel {
                Ok(reel) => {
                    // Share the clip with the rest of the network.
                    if let Ok(block) = state
                        .gateway
                        .send_room_binary(
                            crate::rooms::REELS_ROOM,
                            mime.clone(),
                            payload,
                            LedgerKind::Reel,
                        )
                        .await
                    {
                        state.reels.attach_block(&reel.id, &block.hash);
                    }
                    state.hub.publish(&WsEvent::Reels {
                        reels: state.reels.list(),
                    });
                }
                Err(e) => {
                    state.hub.publish(&WsEvent::Error {
                        message: format!("could not store the clip: {e}"),
                    });
                }
            }
        }
    }
}

/// The rooms the node knows about.
async fn room_states(state: &WsState) -> Vec<RoomState> {
    let peers = state.gateway.peers().await;
    let mut rooms: Vec<RoomState> = crate::rooms::DEFAULT_ROOMS
        .iter()
        .map(|id| RoomState {
            id: (*id).to_string(),
            title: (*id).to_string(),
            kind: "channel".to_string(),
            members: peers
                .iter()
                .filter(|peer| peer.connected)
                .map(|peer| peer.peer_id.clone())
                .collect(),
            e2e: false,
        })
        .collect();

    for peer in peers.iter().filter(|peer| peer.connected) {
        rooms.push(RoomState {
            id: crate::rooms::conversation_room(&state.identity.node_id, &peer.peer_id),
            title: peer.display_name.clone(),
            kind: "direct".to_string(),
            members: vec![peer.peer_id.clone()],
            e2e: true,
        });
    }

    rooms
}

/// The current time in milliseconds since the epoch.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
