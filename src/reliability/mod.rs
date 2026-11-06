//! Reliability layer: heartbeat, retry queue and the peer manager actor.

pub mod heartbeat;
pub mod manager;
pub mod retry;

pub use crate::peer::PeerEvent;
pub use heartbeat::HeartbeatMonitor;
pub use manager::PeerManager;
pub use retry::{PendingFrame, RetryQueue};
