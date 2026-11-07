//! The `mezots` node binary.
//!
//! Run a node, let it discover its neighbours on the LAN and send text to
//! every peer it finds:
//!
//! ```text
//! mezots --name "laptop" --port 7700
//! ```
//!
//! Type a line and press enter to broadcast it; `peers` lists the known
//! peers, `quit` stops the node.

use std::io::IsTerminal;
use std::sync::Arc;
use std::time::Duration;

use mezots::config::NodeConfig;
use mezots::peer::PeerEvent;
use mezots::transport::websocket::WebSocketServer;
use mezots::PeerManager;
use tokio::io::BufReader;
use tokio::sync::broadcast;
use tracing::{info, warn};

/// Parsed command line options.
#[derive(Debug, Default)]
struct Options {
    node_id: Option<String>,
    display_name: Option<String>,
    protocol_port: Option<u16>,
    websocket_port: Option<u16>,
    beacon_port: Option<u16>,
    beacon_group: Option<String>,
    mdns: bool,
    beacon: bool,
    websocket: bool,
    connect: Vec<String>,
    say: Option<String>,
    say_after: u64,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with_target(false)
        .init();

    let options = parse_args(std::env::args().skip(1).collect());

    let mut config = NodeConfig::default();
    if let Some(node_id) = options.node_id {
        config.node_id = node_id;
    }
    if let Some(name) = options.display_name {
        config.display_name = name;
    }
    if let Some(port) = options.protocol_port {
        config.protocol_port = port;
    }
    if let Some(port) = options.websocket_port {
        config.websocket_port = port;
    }
    if let Some(port) = options.beacon_port {
        config.beacon_port = port;
    }
    if let Some(group) = options.beacon_group {
        config.beacon_group = group;
    }
    config.enable_mdns = options.mdns;
    config.enable_beacon = options.beacon;
    config.enable_websocket = options.websocket;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(run(config, options.connect, options.say, options.say_after))
}

/// Parse the command line.
fn parse_args(args: Vec<String>) -> Options {
    let mut options = Options {
        mdns: true,
        beacon: true,
        websocket: true,
        ..Default::default()
    };

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--node-id" => options.node_id = iter.next(),
            "--name" => options.display_name = iter.next(),
            "--port" => options.protocol_port = iter.next().and_then(|p| p.parse().ok()),
            "--ws-port" => options.websocket_port = iter.next().and_then(|p| p.parse().ok()),
            "--beacon-port" => options.beacon_port = iter.next().and_then(|p| p.parse().ok()),
            "--beacon-group" => options.beacon_group = iter.next(),
            "--no-mdns" => options.mdns = false,
            "--no-beacon" => options.beacon = false,
            "--no-ws" => options.websocket = false,
            "--connect" => {
                if let Some(target) = iter.next() {
                    options.connect.push(target);
                }
            }
            "--say" => options.say = iter.next(),
            "--say-after" => options.say_after = iter.next().and_then(|s| s.parse().ok()).unwrap_or(6),
            "--help" | "-h" => {
                println!("{HELP}");
                std::process::exit(0);
            }
            other => {
                eprintln!("unknown argument: {other}");
            }
        }
    }
    options
}

const HELP: &str = "\
mezots - zero-trust LAN peer-to-peer node

USAGE:
    mezots [OPTIONS]

OPTIONS:
    --node-id <ID>          Identity of this node (default: random)
    --name <NAME>           Display name shown to peers
    --port <PORT>           TCP port for the peer protocol (default: 7700)
    --ws-port <PORT>        TCP port for the browser WebSocket API (default: 7701)
    --beacon-port <PORT>    UDP port of the multicast fallback beacon (default: 7702)
    --beacon-group <IP>     Multicast group for the beacon (default: 239.255.42.99)
    --no-mdns               Disable the mDNS discovery backend
    --no-beacon             Disable the multicast fallback beacon
    --no-ws                 Disable the browser WebSocket endpoint
    --connect <ID@ADDR>     Dial a peer explicitly, e.g. node-a@192.168.1.20:7700
    -h, --help              Print this help

While the node runs, type a message and press enter to broadcast it.
";

/// Run the node.
async fn run(
    config: NodeConfig,
    connect: Vec<String>,
    say: Option<String>,
    say_after: u64,
) -> anyhow::Result<()> {
    let node_id = config.node_id.clone();
    let display_name = config.display_name.clone();

    let manager = PeerManager::start(config.clone()).await?;
    let mut events = manager.subscribe();

    // The browser-facing bridge.
    let ws = if config.enable_websocket {
        let gateway: Arc<dyn mezots::transport::websocket::PeerGateway> =
            Arc::new(manager.clone());
        let reels = match mezots::reels::ReelStore::open(config.data_dir.join("reels")) {
            Ok(store) => store,
            Err(e) => {
                warn!(error = %e, "could not open the reels directory");
                mezots::reels::ReelStore::open(std::env::temp_dir().join("mezots-reels"))
                    .expect("a temporary reels directory should be usable")
            }
        };
        match WebSocketServer::start(
            config.websocket_port,
            gateway,
            manager.subscribe(),
            reels,
        )
        .await
        {
            Ok(server) => {
                info!(
                    url = %format!("ws://127.0.0.1:{}/ws", server.local_addr().port()),
                    "websocket endpoint ready"
                );
                Some(server)
            }
            Err(e) => {
                warn!(error = %e, "websocket endpoint unavailable");
                None
            }
        }
    } else {
        None
    };

    // Explicitly dialled peers.
    for target in &connect {
        let Some((peer_id, address)) = target.rsplit_once('@') else {
            warn!(target = %target, "expected <peer-id>@<host:port>");
            continue;
        };
        match address.parse() {
            Ok(address) => {
                if let Err(e) = manager.connect_to(peer_id, address).await {
                    warn!(peer_id = %peer_id, error = %e, "explicit connect failed");
                }
            }
            Err(e) => warn!(target = %target, error = %e, "invalid address"),
        }
    }

    println!();
    println!("  mezots node '{node_id}' ({display_name})");
    println!("  protocol port : {}", config.protocol_port);
    if config.enable_websocket {
        println!("  websocket      : ws://127.0.0.1:{}/ws", config.websocket_port);
    }
    println!(
        "  discovery      : mDNS={} beacon={}",
        if config.enable_mdns { "on" } else { "off" },
        if config.enable_beacon { "on" } else { "off" }
    );
    println!("  type a message and press enter to broadcast it, 'peers' to list peers");
    println!();

    // A scripted message, handy for demos and smoke tests.
    if let Some(say) = say.clone() {
        let manager = manager.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(say_after)).await;
            match manager.broadcast(&say).await {
                Ok(count) => println!("  [sent]      {count} peer(s): {say}"),
                Err(e) => println!("  [error]     {e}"),
            }
        });
    }

    // Event printer.
    let printer = tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(PeerEvent::Discovered { peer }) => {
                    println!(
                        "  [discovery] found peer '{}' at {} via {}",
                        peer.display_name,
                        peer.address.clone().unwrap_or_default(),
                        peer.source
                    );
                }
                Ok(PeerEvent::Connected { peer_id }) => {
                    println!("  [connect]   peer '{peer_id}' is online");
                }
                Ok(PeerEvent::Disconnected { peer_id, reason }) => {
                    println!("  [disconnect] peer '{peer_id}': {reason}");
                }
                Ok(PeerEvent::Message {
                    peer_id,
                    room,
                    text,
                    ..
                }) => {
                    println!("  [message]   {peer_id} in {room}: {text}");
                }
                Ok(PeerEvent::Binary {
                    peer_id,
                    room,
                    mime,
                    payload,
                    ..
                }) => {
                    println!(
                        "  [binary]    {peer_id} in {room}: {mime} ({} bytes)",
                        payload.len()
                    );
                }
                Ok(PeerEvent::StreamChunk {
                    peer_id,
                    room,
                    stream_id,
                    seq,
                    payload,
                }) => {
                    println!(
                        "  [stream]    {peer_id} in {room}: stream {stream_id} chunk {seq} ({} bytes)",
                        payload.len()
                    );
                }
                Ok(PeerEvent::StreamMeta {
                    peer_id,
                    room,
                    stream_id,
                    title,
                    ..
                }) => {
                    println!(
                        "  [live]      {peer_id} started '{title}' in {room} (stream {stream_id})"
                    );
                }
                Ok(PeerEvent::LedgerBlock { block }) => {
                    println!(
                        "  [chain]     block #{} by {} in {} ({})",
                        block.index,
                        block.actor_name,
                        block.room,
                        block.kind.as_str()
                    );
                }
                Ok(PeerEvent::StreamEnd {
                    peer_id,
                    room,
                    stream_id,
                }) => {
                    println!(
                        "  [live]      {peer_id} ended the stream {stream_id} in {room}"
                    );
                }
                Ok(PeerEvent::ChainState {
                    peer_id,
                    head,
                    height,
                }) => {
                    println!("  [chain]     {peer_id} is at height {height} ({head})");
                }
                Ok(PeerEvent::Error { message }) => {
                    println!("  [error]     {message}");
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    println!("  [warn]      {skipped} events were dropped");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Command loop.
    //
    // On a terminal the node is driven interactively and `quit` stops it.
    // Without a terminal (a service, a script, a container) the node keeps
    // running until it is signalled, because the end of a redirected stdin
    // must not take the daemon down.
    let interactive = std::io::stdin().is_terminal();
    if interactive {
        let stdin = BufReader::new(tokio::io::stdin());
        let mut lines = tokio::io::AsyncBufReadExt::lines(stdin);
        while let Ok(Some(line)) = lines.next_line().await {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match line {
                "quit" | "exit" => break,
                "peers" => {
                    for peer in manager.peers().await {
                        println!(
                            "  {}  {}  {}  {}",
                            peer.peer_id,
                            peer.state,
                            peer.address.unwrap_or_default(),
                            peer.source
                        );
                    }
                }
                text => match manager.broadcast(text).await {
                    Ok(count) => println!("  [sent]      {count} peer(s)"),
                    Err(e) => println!("  [error]     {e}"),
                },
            }
        }
    } else {
        println!("  running as a service; press Ctrl+C to stop");
        let _ = tokio::signal::ctrl_c().await;
    }

    println!("\n  shutting down...");
    printer.abort();
    manager.shutdown().await;
    if let Some(ws) = ws {
        ws.shutdown();
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    Ok(())
}
