//! The browser-facing WebSocket endpoint.
//!
//! Browsers cannot speak the raw peer protocol - they cannot do the
//! Diffie-Hellman handshake in the page, and they cannot open a raw TCP
//! socket at all. This module therefore exposes a JSON bridge:
//!
//! * browser -> node: [`WsCommand`] (`sendText`, `sendBinary`, `listPeers`)
//! * node -> browser: [`WsEvent`] (peer state, messages, stream chunks)
//!
//! The bridge is intentionally thin: it translates and forwards, and all
//! protocol logic stays behind the [`PeerGateway`] interface.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
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
use crate::peer::{PeerEvent, PeerSummary};

/// Maximum number of queued messages per browser client.
const CLIENT_QUEUE: usize = 256;

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
}

/// A message sent to a browser client.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
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
}

/// A command sent by a browser client.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
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
    ) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("0.0.0.0", port)).await?;
        let local_addr = listener.local_addr()?;
        let hub = Arc::new(WsHub::new());
        let state = Arc::new(WsState {
            hub: Arc::clone(&hub),
            gateway,
        });

        let app = Router::new()
            .route("/ws", get(ws_upgrade))
            .route("/health", get(health))
            .with_state(Arc::clone(&state));

        let task = tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                error!(error = %e, "websocket server stopped");
            }
        });

        // Forward node events to the browser clients.
        let hub_events = Arc::clone(&hub);
        tokio::spawn(async move {
            let mut events = events;
            while let Ok(event) = events.recv().await {
                let ws_event = convert_event(event);
                hub_events.publish(&ws_event);
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

/// Translate a node event into a WebSocket event.
fn convert_event(event: PeerEvent) -> WsEvent {
    match event {
        PeerEvent::Discovered { peer } => WsEvent::PeerDiscovered { peer },
        PeerEvent::Connected { peer_id } => WsEvent::PeerConnected { peer_id },
        PeerEvent::Disconnected { peer_id, reason } => {
            WsEvent::PeerDisconnected { peer_id, reason }
        }
        PeerEvent::Message { peer_id, text } => WsEvent::Message { peer_id, text },
        PeerEvent::Binary {
            peer_id,
            mime,
            payload,
        } => WsEvent::Binary {
            peer_id,
            mime,
            payload_base64: base64::engine::general_purpose::STANDARD.encode(payload),
        },
        PeerEvent::StreamChunk {
            peer_id,
            stream_id,
            seq,
            payload,
        } => WsEvent::StreamChunk {
            peer_id,
            stream_id,
            seq,
            payload_base64: base64::engine::general_purpose::STANDARD.encode(payload),
        },
        PeerEvent::Error { message } => WsEvent::Error { message },
    }
}

/// Health endpoint.
async fn health() -> Response {
    axum::Json(serde_json::json!({ "status": "ok" })).into_response()
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
            state
                .hub
                .publish(&WsEvent::Error {
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
    }
}
