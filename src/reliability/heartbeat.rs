//! The application level heartbeat.
//!
//! Every few seconds each node sends a `Ping` frame to every connected peer.
//! The peer answers with a `Pong` that echoes the nonce. If a peer misses a
//! configurable number of consecutive pongs the connection is declared broken
//! and torn down, so a silently dead socket (a laptop that went to sleep, a
//! Wi-Fi link that dropped) does not look like a healthy one.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tokio::sync::{mpsc, watch};

use crate::protocol::frame::{Envelope, Frame};
use crate::protocol::router::MuxSender;

/// Configuration of the heartbeat loop.
#[derive(Debug, Clone, Copy)]
pub struct HeartbeatConfig {
    /// How often a ping is sent.
    pub interval: Duration,
    /// How long a peer may take to answer before the ping counts as missed.
    pub timeout: Duration,
    /// How many consecutive misses mark the connection as broken.
    pub max_missed: u32,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            timeout: Duration::from_secs(10),
            max_missed: 3,
        }
    }
}

#[derive(Debug)]
struct OutstandingPing {
    nonce: u64,
    sent_at: Instant,
}

/// Tracks the liveness of a single peer connection.
#[derive(Debug)]
pub struct HeartbeatMonitor {
    config: HeartbeatConfig,
    outstanding: Mutex<Option<OutstandingPing>>,
    missed: AtomicU64,
    rtt_micros: AtomicU64,
}

impl HeartbeatMonitor {
    /// Create a monitor with the given configuration.
    pub fn new(config: HeartbeatConfig) -> Self {
        Self {
            config,
            outstanding: Mutex::new(None),
            missed: AtomicU64::new(0),
            rtt_micros: AtomicU64::new(0),
        }
    }

    /// Record that a ping was sent.
    pub fn on_ping_sent(&self, nonce: u64, sent_at: Instant) {
        *self.outstanding.lock() = Some(OutstandingPing { nonce, sent_at });
    }

    /// Record a pong from the peer.
    ///
    /// Returns `true` when the pong matched an outstanding ping.
    pub fn on_pong(&self, nonce: u64) -> bool {
        let mut outstanding = self.outstanding.lock();
        let Some(ping) = outstanding.take() else {
            return false;
        };
        if ping.nonce != nonce {
            // Not the answer we are waiting for; keep waiting.
            *outstanding = Some(ping);
            return false;
        }
        let rtt = ping.sent_at.elapsed();
        self.rtt_micros
            .store(rtt.as_micros() as u64, Ordering::Relaxed);
        self.missed.store(0, Ordering::Relaxed);
        true
    }

    /// Check whether the outstanding ping has timed out.
    ///
    /// Returns `true` when the connection should be considered broken.
    pub fn check_expired(&self, now: Instant) -> bool {
        let expired = {
            let outstanding = self.outstanding.lock();
            match outstanding.as_ref() {
                Some(ping) => now.duration_since(ping.sent_at) > self.config.timeout,
                None => false,
            }
        };
        if expired {
            self.outstanding.lock().take();
            self.missed.fetch_add(1, Ordering::Relaxed);
        }
        self.missed.load(Ordering::Relaxed) >= self.config.max_missed as u64
    }

    /// Number of consecutive missed pongs.
    pub fn missed(&self) -> u32 {
        self.missed.load(Ordering::Relaxed) as u32
    }

    /// Last measured round trip time.
    pub fn rtt(&self) -> Option<Duration> {
        let micros = self.rtt_micros.load(Ordering::Relaxed);
        if micros == 0 {
            None
        } else {
            Some(Duration::from_micros(micros))
        }
    }

    /// The configuration in use.
    pub fn config(&self) -> HeartbeatConfig {
        self.config
    }
}

/// A background task that pings one peer and reports failures.
///
/// The task returns `true` while the connection is healthy and `false` once
/// the peer missed too many pongs.
pub async fn run_heartbeat(
    sender: MuxSender,
    monitor: Arc<HeartbeatMonitor>,
    ids: Arc<crate::reliability::manager::IdCounter>,
    peer_id: String,
    mut shutdown: watch::Receiver<bool>,
) -> bool {
    let mut ticker = tokio::time::interval(monitor.config().interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // The first tick completes immediately; consume it so the first real ping
    // is sent after one interval.
    ticker.tick().await;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if monitor.check_expired(Instant::now()) {
                    tracing::warn!(
                        peer_id = %peer_id,
                        missed = monitor.missed(),
                        "heartbeat failed, peer is unresponsive"
                    );
                    return false;
                }

                let nonce = ids.next_id();
                monitor.on_ping_sent(nonce, Instant::now());
                let envelope = Envelope::next(
                    ids.next_id(),
                    Frame::Ping { nonce, sent_ms: crate::discovery::beacon::now_ms() },
                );
                if sender.try_send(envelope).is_err() {
                    tracing::warn!(peer_id = %peer_id, "heartbeat ping could not be queued");
                }
            }
            _ = shutdown.changed() => {
                return true;
            }
        }
    }
}

/// A tiny helper that fans a shutdown signal out to several tasks.
#[derive(Debug, Clone)]
pub struct Shutdown {
    tx: watch::Sender<bool>,
}

impl Default for Shutdown {
    fn default() -> Self {
        Self::new()
    }
}

impl Shutdown {
    /// Create a fresh shutdown signal.
    pub fn new() -> Self {
        let (tx, _) = watch::channel(false);
        Self { tx }
    }

    /// Trigger the shutdown.
    pub fn trigger(&self) {
        let _ = self.tx.send(true);
    }

    /// Whether the shutdown was triggered.
    pub fn is_triggered(&self) -> bool {
        *self.tx.borrow()
    }

    /// Subscribe to the shutdown signal.
    pub fn subscribe(&self) -> watch::Receiver<bool> {
        self.tx.subscribe()
    }
}

/// A convenience alias for the channel used to talk to the manager.
pub type CommandSender<T> = mpsc::Sender<T>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_matching_pong_clears_the_outstanding_ping() {
        let monitor = HeartbeatMonitor::new(HeartbeatConfig {
            interval: Duration::from_secs(1),
            timeout: Duration::from_secs(1),
            max_missed: 2,
        });
        monitor.on_ping_sent(42, Instant::now());
        assert!(monitor.on_pong(42));
        assert_eq!(monitor.missed(), 0);
    }

    #[test]
    fn a_wrong_nonce_does_not_clear_the_outstanding_ping() {
        let monitor = HeartbeatMonitor::new(HeartbeatConfig::default());
        monitor.on_ping_sent(42, Instant::now());
        assert!(!monitor.on_pong(7));
        assert!(monitor.outstanding.lock().is_some());
    }

    #[test]
    fn missing_pongs_eventually_break_the_connection() {
        let config = HeartbeatConfig {
            interval: Duration::from_millis(10),
            timeout: Duration::from_millis(1),
            max_missed: 2,
        };
        let monitor = HeartbeatMonitor::new(config);
        let now = Instant::now();

        monitor.on_ping_sent(1, now);
        assert!(!monitor.check_expired(now));
        // The first missed pong is not enough: the limit is two.
        assert!(!monitor.check_expired(now + Duration::from_millis(10)));
        assert_eq!(monitor.missed(), 1);

        monitor.on_ping_sent(2, now);
        assert!(monitor.check_expired(now + Duration::from_millis(30)));
        assert_eq!(monitor.missed(), 2);
    }

    #[test]
    fn a_pong_resets_the_miss_counter() {
        let monitor = HeartbeatMonitor::new(HeartbeatConfig {
            interval: Duration::from_millis(10),
            timeout: Duration::from_millis(1),
            max_missed: 3,
        });
        let now = Instant::now();
        monitor.on_ping_sent(1, now);
        monitor.check_expired(now + Duration::from_millis(10));
        assert_eq!(monitor.missed(), 1);
        monitor.on_ping_sent(2, now);
        monitor.on_pong(2);
        assert_eq!(monitor.missed(), 0);
    }
}
