//! End-to-end tests over real TCP sockets.
//!
//! These tests exercise the complete stack: discovery is replaced by an
//! explicit dial, but everything after that - the handshake, the AEAD
//! framing, the router, the acknowledgement and the event plumbing - runs
//! exactly as it does in production.

use std::time::Duration;

use mezots::config::NodeConfig;
use mezots::crypto::handshake::initiator_handshake;
use mezots::crypto::NodeKeys;
use mezots::peer::PeerEvent;
use mezots::protocol::codec::{decode_envelope, encode_envelope};
use mezots::protocol::frame::{Envelope, Frame};
use mezots::PeerManager;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::broadcast;

/// A node with discovery switched off, listening on a free port.
async fn start_node(node_id: &str) -> PeerManager {
    let mut config = NodeConfig::default().with_node_id(node_id);
    config.enable_mdns = false;
    config.enable_beacon = false;
    config.protocol_port = 0;
    config.heartbeat_interval = Duration::from_millis(200);
    config.heartbeat_timeout = Duration::from_millis(800);
    config.max_missed_pongs = 3;
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

/// Wait for the next event that matches the predicate.
async fn next_event<F>(
    events: &mut broadcast::Receiver<PeerEvent>,
    timeout: Duration,
    mut predicate: F,
) -> Option<PeerEvent>
where
    F: FnMut(&PeerEvent) -> bool,
{
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return None;
        }
        match tokio::time::timeout(remaining, events.recv()).await {
            Ok(Ok(event)) => {
                if predicate(&event) {
                    return Some(event);
                }
            }
            Ok(Err(_)) | Err(_) => return None,
        }
    }
}

/// Wait until a peer shows up in the table and is connected.
async fn wait_for_connected(manager: &PeerManager, peer_id: &str) -> bool {
    wait_for(Duration::from_secs(10), || async {
        manager
            .peers()
            .await
            .iter()
            .any(|peer| peer.peer_id == peer_id && peer.connected)
    })
    .await
}

#[tokio::test]
async fn two_nodes_connect_and_exchange_text() {
    let node_a = start_node("node-a").await;
    let node_b = start_node("node-b").await;

    let mut events_a = node_a.subscribe();
    let mut events_b = node_b.subscribe();

    // Node B dials node A.
    node_b
        .connect_to("node-a", node_a.local_addr())
        .await
        .expect("connect failed");

    // Both sides must report the peer as connected.
    let connected = next_event(&mut events_a, Duration::from_secs(10), |event| {
        matches!(event, PeerEvent::Connected { peer_id } if peer_id == "node-b")
    })
    .await;
    assert!(connected.is_some(), "node-a never saw node-b connect");

    let connected = next_event(&mut events_b, Duration::from_secs(10), |event| {
        matches!(event, PeerEvent::Connected { peer_id } if peer_id == "node-a")
    })
    .await;
    assert!(connected.is_some(), "node-b never saw node-a connect");

    // A sends a message, B must receive it.
    node_a
        .send_message("node-b", "hello from A")
        .await
        .expect("send failed");

    let received = next_event(&mut events_b, Duration::from_secs(10), |event| {
        matches!(event, PeerEvent::Message { peer_id, text }
            if peer_id == "node-a" && text == "hello from A")
    })
    .await;
    assert!(received.is_some(), "node-b never received the message");

    // And the other direction.
    node_b
        .send_message("node-a", "hello from B")
        .await
        .expect("send failed");
    let received = next_event(&mut events_a, Duration::from_secs(10), |event| {
        matches!(event, PeerEvent::Message { peer_id, text }
            if peer_id == "node-b" && text == "hello from B")
    })
    .await;
    assert!(received.is_some(), "node-a never received the reply");
}

#[tokio::test]
async fn broadcast_reaches_every_connected_peer() {
    let node_a = start_node("node-a").await;
    let node_b = start_node("node-b").await;
    let mut events_b = node_b.subscribe();

    node_b
        .connect_to("node-a", node_a.local_addr())
        .await
        .expect("connect failed");
    assert!(wait_for_connected(&node_a, "node-b").await);

    let sent = node_a.broadcast("broadcasting").await.expect("broadcast failed");
    assert_eq!(sent, 1, "the broadcast should reach exactly one peer");

    let received = next_event(&mut events_b, Duration::from_secs(10), |event| {
        matches!(event, PeerEvent::Message { text, .. } if text == "broadcasting")
    })
    .await;
    assert!(received.is_some(), "the broadcast never arrived");
}

#[tokio::test]
async fn stream_chunks_arrive_in_order() {
    let node_a = start_node("node-a").await;
    let node_b = start_node("node-b").await;
    let mut events_b = node_b.subscribe();

    node_b
        .connect_to("node-a", node_a.local_addr())
        .await
        .expect("connect failed");
    assert!(wait_for_connected(&node_a, "node-b").await);

    // Send a handful of chunks of one stream.
    for seq in 0..5u8 {
        node_a
            .send_stream_chunk("node-b", 7, vec![seq; 32], false)
            .await
            .expect("stream chunk failed");
    }

    let mut seen: Vec<u64> = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while seen.len() < 5 && tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, events_b.recv()).await {
            Ok(Ok(PeerEvent::StreamChunk {
                stream_id,
                seq,
                payload,
                ..
            })) => {
                assert_eq!(stream_id, 7, "the stream id was not preserved");
                assert_eq!(payload.len(), 32, "the payload was truncated");
                seen.push(seq);
            }
            Ok(Ok(_)) => continue,
            Ok(Err(_)) | Err(_) => break,
        }
    }

    assert_eq!(seen, vec![0, 1, 2, 3, 4], "chunks arrived out of order");
}

#[tokio::test]
async fn binary_payloads_survive_the_round_trip() {
    let node_a = start_node("node-a").await;
    let node_b = start_node("node-b").await;
    let mut events_b = node_b.subscribe();

    node_b
        .connect_to("node-a", node_a.local_addr())
        .await
        .expect("connect failed");
    assert!(wait_for_connected(&node_a, "node-b").await);

    let payload: Vec<u8> = (0..=255u8).collect();
    node_a
        .send_binary("node-b", "application/octet-stream", payload.clone())
        .await
        .expect("binary send failed");

    let received = next_event(&mut events_b, Duration::from_secs(10), |event| {
        matches!(event, PeerEvent::Binary { payload: p, .. } if p == &payload)
    })
    .await;
    assert!(received.is_some(), "the binary payload never arrived intact");
}

#[tokio::test]
async fn the_wire_carries_ciphertext_not_plaintext() {
    let node_a = start_node("node-a").await;
    let marker = "PLAIN-TEXT-MARKER-9f2a";

    // A raw socket that speaks the protocol by hand.
    let mut stream = TcpStream::connect(node_a.local_addr())
        .await
        .expect("could not connect");

    let keys = NodeKeys::generate();
    let outcome = initiator_handshake(&mut stream, &keys, "eavesdropper")
        .await
        .expect("handshake failed");

    // The node registers the raw client as a peer.
    assert!(
        wait_for(Duration::from_secs(10), || async {
            node_a.peers().await.iter().any(|p| p.peer_id == "eavesdropper")
        })
        .await,
        "the node never registered the raw client"
    );

    // The node sends us a chat message...
    node_a
        .send_message("eavesdropper", marker)
        .await
        .expect("send failed");

    // ...and we read the raw bytes off the socket. Heartbeats may arrive
    // first, so we read until the chat frame shows up.
    let mut found = false;
    for _ in 0..16 {
        let mut length = [0u8; 4];
        stream.read_exact(&mut length).await.expect("no frame");
        let length = u32::from_be_bytes(length) as usize;
        let mut raw = vec![0u8; length];
        stream.read_exact(&mut raw).await.expect("short frame");

        // The marker must not appear in the clear.
        assert!(
            !raw
                .windows(marker.len())
                .any(|window| window == marker.as_bytes()),
            "the plaintext marker leaked onto the wire"
        );

        // But the receiver can still open it.
        let plaintext = outcome.rx.decrypt(&raw).expect("decryption failed");
        let envelope = decode_envelope(&plaintext).expect("decoding failed");
        if let Frame::ChatText { text } = &envelope.frame {
            if text == marker {
                found = true;
                break;
            }
        }
    }
    assert!(found, "the chat frame never arrived");
}

#[tokio::test]
async fn garbage_on_the_socket_is_rejected() {
    let node_a = start_node("node-a").await;

    // A socket that does not speak the protocol.
    let mut stream = TcpStream::connect(node_a.local_addr())
        .await
        .expect("could not connect");
    stream
        .write_all(&[0xff; 64])
        .await
        .expect("write failed");
    let _ = stream.shutdown().await;

    // The node must stay healthy and must not register a peer.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        node_a.peers().await.is_empty(),
        "a garbage connection must not create a peer"
    );

    // And the node still works afterwards.
    let node_b = start_node("node-b").await;
    let mut events_a = node_a.subscribe();
    node_b
        .connect_to("node-a", node_a.local_addr())
        .await
        .expect("connect failed");
    let connected = next_event(&mut events_a, Duration::from_secs(10), |event| {
        matches!(event, PeerEvent::Connected { peer_id } if peer_id == "node-b")
    })
    .await;
    assert!(
        connected.is_some(),
        "the node did not recover after a bogus connection"
    );
}

#[tokio::test]
async fn a_tampered_frame_is_rejected() {
    let node_a = start_node("node-a").await;
    let mut stream = TcpStream::connect(node_a.local_addr())
        .await
        .expect("could not connect");

    let keys = NodeKeys::generate();
    let outcome = initiator_handshake(&mut stream, &keys, "attacker")
        .await
        .expect("handshake failed");

    // Build a valid frame, then flip a bit in the ciphertext.
    let envelope = Envelope::next(1, Frame::ChatText {
        text: "injected".into(),
    });
    let plaintext = encode_envelope(&envelope).expect("encoding failed");
    let mut ciphertext = outcome.tx.encrypt(&plaintext).expect("encryption failed");
    ciphertext[0] ^= 0x01;

    stream
        .write_all(&(ciphertext.len() as u32).to_be_bytes())
        .await
        .expect("write failed");
    stream.write_all(&ciphertext).await.expect("write failed");

    // The node must drop the connection instead of accepting the frame.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let peers = node_a.peers().await;
    assert!(
        !peers.iter().any(|p| p.peer_id == "attacker" && p.connected),
        "a tampered frame was accepted"
    );
}

#[tokio::test]
async fn the_heartbeat_keeps_the_connection_alive() {
    let node_a = start_node("node-a").await;
    let node_b = start_node("node-b").await;

    node_b
        .connect_to("node-a", node_a.local_addr())
        .await
        .expect("connect failed");
    assert!(wait_for_connected(&node_a, "node-b").await);

    // The heartbeat runs every 200 ms; after a second both peers must still
    // consider each other connected.
    tokio::time::sleep(Duration::from_secs(1)).await;

    assert!(
        node_a.peers().await.iter().any(|p| p.connected),
        "the connection did not survive the heartbeat"
    );
    assert!(
        node_b.peers().await.iter().any(|p| p.connected),
        "the connection did not survive the heartbeat"
    );
}

#[tokio::test]
async fn a_dropped_connection_is_re_established() {
    let node_a = start_node("node-a").await;
    let node_b = start_node("node-b").await;
    let mut events_a = node_a.subscribe();

    node_b
        .connect_to("node-a", node_a.local_addr())
        .await
        .expect("connect failed");
    assert!(wait_for_connected(&node_a, "node-b").await);

    // Force the connection down.
    node_b.disconnect("node-a").await;

    // The surviving side must notice.
    let reported = next_event(&mut events_a, Duration::from_secs(10), |event| {
        matches!(event, PeerEvent::Disconnected { peer_id, .. } if peer_id == "node-b")
    })
    .await;
    assert!(reported.is_some(), "the disconnect was never reported");

    // And the link must come back on its own: the reconnect task rebuilds
    // the handshake and replays whatever was not acknowledged.
    let reconnected = next_event(&mut events_a, Duration::from_secs(30), |event| {
        matches!(event, PeerEvent::Connected { peer_id } if peer_id == "node-b")
    })
    .await;
    assert!(
        reconnected.is_some(),
        "the connection was not re-established automatically"
    );

    // The rebuilt session carries traffic again.
    node_a
        .send_message("node-b", "back online")
        .await
        .expect("send failed");
    let mut events_b = node_b.subscribe();
    let received = next_event(&mut events_b, Duration::from_secs(10), |event| {
        matches!(event, PeerEvent::Message { text, .. } if text == "back online")
    })
    .await;
    assert!(
        received.is_some(),
        "the rebuilt session did not carry the message"
    );
}

#[tokio::test]
async fn a_disconnected_peer_is_reported() {
    let node_a = start_node("node-a").await;
    let node_b = start_node("node-b").await;
    let mut events_a = node_a.subscribe();

    node_b
        .connect_to("node-a", node_a.local_addr())
        .await
        .expect("connect failed");
    assert!(wait_for_connected(&node_a, "node-b").await);

    node_b.disconnect("node-a").await;

    let reported = next_event(&mut events_a, Duration::from_secs(10), |event| {
        matches!(event, PeerEvent::Disconnected { peer_id, .. } if peer_id == "node-b")
    })
    .await;
    assert!(reported.is_some(), "the disconnect was never reported");
}
