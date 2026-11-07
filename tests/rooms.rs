//! Tests for rooms, private conversations and the chain.
//!
//! These are the tests that answer the question "does it actually work?".
//! They run three nodes on the loopback interface with the real encrypted
//! protocol between them and check the three promises the product makes:
//!
//! * a message in a room reaches the other node,
//! * a private conversation reaches *only* the person it was addressed to,
//! * and both nodes end up with the same chain.

use std::time::Duration;

use mezots::config::NodeConfig;
use mezots::peer::PeerEvent;
use mezots::rooms::conversation_room;
use mezots::PeerManager;
use tokio::sync::broadcast;

/// A node with discovery switched off, listening on a free port.
async fn start_node(node_id: &str) -> PeerManager {
    let mut config = NodeConfig::default().with_node_id(node_id);
    config.enable_mdns = false;
    config.enable_beacon = false;
    config.protocol_port = 0;
    config.heartbeat_interval = Duration::from_millis(200);
    config.heartbeat_timeout = Duration::from_millis(800);
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

/// Connect two nodes and wait until they see each other.
async fn connect(a: &PeerManager, b: &PeerManager, from: &str, to: &str) {
    b.connect_to(from, a.local_addr())
        .await
        .expect("connect failed");
    assert!(
        wait_for(Duration::from_secs(10), || async {
            a.peers()
                .await
                .iter()
                .any(|peer| peer.peer_id == to && peer.connected)
        })
        .await,
        "the nodes never connected"
    );
}

#[tokio::test]
async fn a_room_message_reaches_the_other_node() {
    let node_a = start_node("node-a").await;
    let node_b = start_node("node-b").await;
    let mut events_b = node_b.subscribe();

    connect(&node_a, &node_b, "node-a", "node-b").await;

    node_a
        .send_room_text("general", "hello everybody")
        .await
        .expect("the message could not be sent");

    let received = next_event(&mut events_b, Duration::from_secs(10), |event| {
        matches!(event, PeerEvent::Message { text, room, .. } if text == "hello everybody" && room == "general")
    })
    .await;
    assert!(received.is_some(), "the message never arrived in the room");
}

#[tokio::test]
async fn a_private_conversation_reaches_only_its_addressee() {
    let node_a = start_node("node-a").await;
    let node_b = start_node("node-b").await;
    let node_c = start_node("node-c").await;

    let mut events_b = node_b.subscribe();
    let mut events_c = node_c.subscribe();

    connect(&node_a, &node_b, "node-a", "node-b").await;
    connect(&node_a, &node_c, "node-a", "node-c").await;

    // A sends a private message to B.
    node_a
        .send_room_text("dm:node-b", "just between us")
        .await
        .expect("the message could not be sent");

    let received = next_event(&mut events_b, Duration::from_secs(10), |event| {
        matches!(event, PeerEvent::Message { text, .. } if text == "just between us")
    })
    .await;
    assert!(received.is_some(), "the addressee never got the message");

    // And C must not see it.
    let leaked = next_event(&mut events_c, Duration::from_secs(2), |event| {
        matches!(event, PeerEvent::Message { text, .. } if text == "just between us")
    })
    .await;
    assert!(
        leaked.is_none(),
        "a private conversation leaked to another node"
    );
}

#[tokio::test]
async fn both_sides_use_the_same_name_for_a_private_conversation() {
    let node_a = start_node("node-a").await;
    let node_b = start_node("node-b").await;
    let mut events_b = node_b.subscribe();

    connect(&node_a, &node_b, "node-a", "node-b").await;

    node_a
        .send_room_text("dm:node-b", "hello")
        .await
        .expect("the message could not be sent");

    let received = next_event(&mut events_b, Duration::from_secs(10), |event| {
        matches!(event, PeerEvent::Message { room, .. } if room.starts_with("dm:"))
    })
    .await;

    let Some(PeerEvent::Message { room, .. }) = received else {
        panic!("the private message never arrived");
    };

    // The room is named after both nodes, sorted, so both sides agree.
    assert_eq!(room, conversation_room("node-a", "node-b"));
    assert_eq!(room, "dm:node-a:node-b");
}

#[tokio::test]
async fn both_nodes_end_up_with_the_same_chain() {
    let node_a = start_node("node-a").await;
    let node_b = start_node("node-b").await;

    connect(&node_a, &node_b, "node-a", "node-b").await;

    node_a
        .send_room_text("general", "first")
        .await
        .expect("the message could not be sent");

    // The block travels with the message, so both chains grow.
    assert!(
        wait_for(Duration::from_secs(10), || async {
            node_b.ledger_snapshot(10).await.len() == 1
        })
        .await,
        "the peer never received the block"
    );

    node_b
        .send_room_text("general", "second")
        .await
        .expect("the message could not be sent");

    assert!(
        wait_for(Duration::from_secs(10), || async {
            node_a.ledger_snapshot(10).await.len() == 2
        })
        .await,
        "the first node never received the second block"
    );

    // And the two chains agree.
    assert!(
        wait_for(Duration::from_secs(10), || async {
            let head_a = node_a.ledger_status().await.head;
            let head_b = node_b.ledger_status().await.head;
            head_a == head_b && !head_a.is_empty()
        })
        .await,
        "the two nodes disagree about the head of the chain"
    );

    // Every chain verifies on its own.
    assert!(node_a.ledger_status().await.valid, "node-a's chain is broken");
    assert!(node_b.ledger_status().await.valid, "node-b's chain is broken");
}

#[tokio::test]
async fn a_block_commits_to_the_message_and_is_signed() {
    let node_a = start_node("node-a").await;
    let mut events_a = node_a.subscribe();

    node_a
        .send_room_text("general", "the homework is due on Friday")
        .await
        .expect("the message could not be sent");

    let block = next_event(&mut events_a, Duration::from_secs(5), |event| {
        matches!(event, PeerEvent::LedgerBlock { .. })
    })
    .await;

    let Some(PeerEvent::LedgerBlock { block }) = block else {
        panic!("no block was committed");
    };

    assert_eq!(block.index, 0, "the first block should be at height zero");
    assert_eq!(block.actor, "node-a");
    assert_eq!(block.room, "general");
    assert_eq!(block.kind.as_str(), "message");

    // The block commits to the hash of the message.
    let expected = mezots::ledger::hex(&mezots::ledger::sha256_bytes(
        b"the homework is due on Friday",
    ));
    assert_eq!(block.payload_hash, expected);

    // And the signature checks out.
    block.verify().expect("the block should verify");
    block
        .verify_signature()
        .expect("the signature should check out");
}

#[tokio::test]
async fn the_node_reports_what_it_did() {
    let node_a = start_node("node-a").await;
    let node_b = start_node("node-b").await;
    connect(&node_a, &node_b, "node-a", "node-b").await;

    node_a
        .send_room_text("general", "counted")
        .await
        .expect("the message could not be sent");

    let metrics = node_a.metrics().await;
    assert_eq!(metrics.peers_online, 1, "the node should see one peer");
    assert_eq!(metrics.ledger_height, 1, "the chain should hold one block");

    // The peer was found by an explicit dial, so the traffic counts as
    // leaving the local network - which is the honest answer for a peer
    // that was dialled by address.
    assert!(
        metrics.bytes_relayed + metrics.bytes_internet > 0,
        "the node did not account for the bytes it sent"
    );

    let identity = node_a.identity().await;
    assert_eq!(identity.node_id, "node-a");
    assert!(!identity.fingerprint.is_empty());
    assert!(!identity.public_key.is_empty());
}
