//! The TCP listener that accepts inbound peer connections.

use std::net::SocketAddr;

use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tracing::{debug, error, info};

/// A bound TCP listener for the peer protocol.
#[derive(Debug)]
pub struct PeerListener {
    listener: TcpListener,
    local_addr: SocketAddr,
}

/// Handle that keeps the accept loop alive.
#[derive(Debug)]
pub struct TcpAcceptor {
    local_addr: SocketAddr,
    task: JoinHandle<()>,
}

impl PeerListener {
    /// Bind to the given port on all interfaces.
    ///
    /// Passing `0` asks the operating system for a free port, which is handy
    /// in tests.
    pub async fn bind(port: u16) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("0.0.0.0", port)).await?;
        let local_addr = listener.local_addr()?;
        Ok(Self { listener, local_addr })
    }

    /// The address the listener is bound to.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Spawn the accept loop.
    ///
    /// Every accepted socket is handed to `handler`, which is expected to
    /// perform the cryptographic handshake and register the peer.
    pub fn spawn<F, Fut>(self, handler: F) -> TcpAcceptor
    where
        F: Fn(TcpStream, SocketAddr) -> Fut + Send + Clone + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let local_addr = self.local_addr;
        let task = tokio::spawn(async move {
            info!(%local_addr, "peer listener accepting connections");
            loop {
                let (stream, peer_addr) = match self.listener.accept().await {
                    Ok(pair) => pair,
                    Err(e) => {
                        error!(error = %e, "accept failed");
                        // A transient error (for example too many open files)
                        // should not kill the listener.
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        continue;
                    }
                };
                debug!(%peer_addr, "inbound connection accepted");
                let handler = handler.clone();
                tokio::spawn(async move {
                    handler(stream, peer_addr).await;
                });
            }
        });
        TcpAcceptor { local_addr, task }
    }
}

impl TcpAcceptor {
    /// The address the listener is bound to.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Stop accepting new connections.
    pub fn abort(&self) {
        self.task.abort();
    }
}
