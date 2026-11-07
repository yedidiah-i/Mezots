//! Tests for the browser-facing WebSocket bridge.
//!
//! A browser cannot speak the raw peer protocol, so it talks JSON over this
//! endpoint. These tests drive it exactly like a web page would.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use mezots::config::NodeConfig;
use mezots::peer::PeerEvent;
use mezots::reels::ReelStore;
use mezots::transport::websocket::WebSocketServer;
use mezots::PeerManager;
use tokio_tungstenite::tungstenite::Message;

/// A temporary directory for the clips a test publishes.
fn temp_reels() -> ReelStore {
    let dir = std::env::temp_dir().join(format!(
        "mezots-test-reels-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    ReelStore::open(dir).expect("the reels store should open")
}

/// A node with discovery switched off, listening on a free port.
async fn start_node(node_id: &str) -> PeerManager {
    let mut config = NodeConfig::default().with_node_id(node_id);
    config.enable_mdns = false;
    config.enable_beacon = false;
    config.protocol_port = 0;
    PeerManager::start(config).await.expect("node failed to start")
}

/// Wait until an asynchronous predicate holds.
async fn wait_for<F, Fut>(timeout: Duration, mut predicate: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return false;
        }
        if predicate().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Read the next JSON event from the socket.
///
/// The bridge greets every browser with the identity of the node, so the
/// greeting is skipped when a test is looking for something else.
async fn next_json(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> String {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, socket.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                let payload = text.to_string();
                if payload.contains("\"type\":\"identity\"") {
                    continue;
                }
                return payload;
            }
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(e))) => panic!("websocket error: {e}"),
            Ok(None) => panic!("the websocket closed"),
            Err(_) => break,
        }
    }
    panic!("no websocket event arrived");
}

#[tokio::test]
async fn a_browser_can_list_the_peers() {
    let manager = start_node("node-a").await;
    let server = WebSocketServer::start(
        7801,
        Arc::new(manager.clone()),
        manager.subscribe(),
        temp_reels(),
    )
    .await
    .expect("websocket server failed to start");

    let (mut socket, _) = tokio_tungstenite::connect_async("ws://127.0.0.1:7801/ws")
        .await
        .expect("could not connect to the websocket endpoint");

    socket
        .send(Message::Text("{\"type\":\"listPeers\"}".into()))
        .await
        .expect("could not send the command");

    let response = next_json(&mut socket).await;
    assert!(
        response.contains("\"type\":\"peers\""),
        "unexpected response: {response}"
    );

    server.shutdown();
}

#[tokio::test]
async fn a_browser_message_reaches_the_peer() {
    let node_a = start_node("node-a").await;
    let node_b = start_node("node-b").await;
    let mut events_b = node_b.subscribe();

    // The two nodes find each other.
    node_b
        .connect_to("node-a", node_a.local_addr())
        .await
        .expect("connect failed");
    assert!(
        wait_for(Duration::from_secs(10), || async {
            node_a.peers().await.iter().any(|p| p.connected)
        })
        .await,
        "the nodes never connected"
    );

    // A browser attaches to node A.
    let server = WebSocketServer::start(
        7802,
        Arc::new(node_a.clone()),
        node_a.subscribe(),
        temp_reels(),
    )
    .await
    .expect("websocket server failed to start");
    let (mut socket, _) = tokio_tungstenite::connect_async("ws://127.0.0.1:7802/ws")
        .await
        .expect("could not connect to the websocket endpoint");

    // And sends a chat message to node B.
    socket
        .send(Message::Text(
            "{\"type\":\"sendText\",\"peerId\":\"node-b\",\"text\":\"from the browser\"}"
                .into(),
        ))
        .await
        .expect("could not send the command");

    // Node B must receive it.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut received = false;
    while tokio::time::Instant::now() < deadline && !received {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, events_b.recv()).await {
            Ok(Ok(PeerEvent::Message { peer_id, text, .. })) => {
                assert_eq!(peer_id, "node-a");
                received = text == "from the browser";
            }
            Ok(Ok(_)) => continue,
            Ok(Err(_)) | Err(_) => break,
        }
    }
    assert!(received, "the browser message never reached node-b");

    server.shutdown();
}

#[tokio::test]
async fn the_bridge_reports_an_unknown_peer() {
    let manager = start_node("node-a").await;
    let server = WebSocketServer::start(
        7803,
        Arc::new(manager.clone()),
        manager.subscribe(),
        temp_reels(),
    )
    .await
    .expect("websocket server failed to start");

    let (mut socket, _) = tokio_tungstenite::connect_async("ws://127.0.0.1:7803/ws")
        .await
        .expect("could not connect to the websocket endpoint");

    socket
        .send(Message::Text(
            "{\"type\":\"sendText\",\"peerId\":\"nobody\",\"text\":\"hello\"}".into(),
        ))
        .await
        .expect("could not send the command");

    let response = next_json(&mut socket).await;
    assert!(
        response.contains("\"type\":\"error\""),
        "an unknown peer should produce an error, got: {response}"
    );

    server.shutdown();
}

/// The browser contract is written down in `lib/protocol.ts`.
///
/// This test makes sure the names on the wire are the names the interface
/// expects, so the two halves cannot drift apart silently.
#[test]
fn the_wire_names_match_the_browser_contract() {
    use mezots::transport::websocket::{RoomMessage, WsEvent};

    let event = WsEvent::RoomMessage {
        message: RoomMessage {
            room: "general".into(),
            peer_id: "node-b".into(),
            author: "Bravo".into(),
            text: "hello".into(),
            ts: 42,
            client_msg_id: Some("c-1".into()),
            block_hash: Some("abc".into()),
            height: Some(7),
            encrypted: Some(true),
        },
    };

    let json = serde_json::to_string(&event).expect("the event should serialize");

    for key in [
        "\"type\":\"roomMessage\"",
        "\"room\":\"general\"",
        "\"peerId\":\"node-b\"",
        "\"author\":\"Bravo\"",
        "\"clientMsgId\":\"c-1\"",
        "\"blockHash\":\"abc\"",
        "\"height\":7",
        "\"encrypted\":true",
    ] {
        assert!(
            json.contains(key),
            "the event is missing {key}, got: {json}"
        );
    }
}

/// The commands the browser sends are read with the same names.
#[test]
fn the_commands_use_the_browser_names() {
    use mezots::transport::websocket::WsCommand;

    let command: WsCommand = serde_json::from_str(
        "{\"type\":\"sendRoomText\",\"room\":\"general\",\"text\":\"hi\",\"clientMsgId\":\"c-1\"}",
    )
    .expect("the command should parse");

    match command {
        WsCommand::SendRoomText {
            room,
            text,
            client_msg_id,
        } => {
            assert_eq!(room, "general");
            assert_eq!(text, "hi");
            assert_eq!(client_msg_id.as_deref(), Some("c-1"));
        }
        other => panic!("the command was read as the wrong variant: {other:?}"),
    }

    let command: WsCommand = serde_json::from_str(
        "{\"type\":\"streamChunk\",\"room\":\"stream:hall\",\"streamId\":3,\"payloadBase64\":\"AA==\"}",
    )
    .expect("the command should parse");

    match command {
        WsCommand::StreamChunk {
            room,
            stream_id,
            payload_base64,
        } => {
            assert_eq!(room, "stream:hall");
            assert_eq!(stream_id, 3);
            assert_eq!(payload_base64, "AA==");
        }
        other => panic!("the command was read as the wrong variant: {other:?}"),
    }
}
