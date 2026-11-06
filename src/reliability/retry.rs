//! The session resumption and retry queue.
//!
//! Every outbound frame that expects an acknowledgement is remembered until
//! the peer confirms it. If the socket dies - a Wi-Fi drop, a sleeping
//! laptop - the unacknowledged frames stay in the queue. Once the peer is
//! rediscovered (possibly at a new IP address) and a fresh handshake
//! completes, the queued frames are replayed in their original order.
//!
//! The queue is bounded in both size and age so that a peer that is gone for
//! hours cannot grow the memory footprint without limit.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crate::protocol::frame::Frame;

/// A frame that was sent but not yet acknowledged.
#[derive(Debug, Clone)]
pub struct PendingFrame {
    /// The envelope id used on the wire.
    pub id: u64,
    /// The frame itself.
    pub frame: Frame,
    /// When the frame was first sent.
    pub queued_at: Instant,
    /// How often the frame has been replayed.
    pub attempts: u32,
}

/// A bounded queue of unacknowledged frames.
#[derive(Debug)]
pub struct RetryQueue {
    pending: BTreeMap<u64, PendingFrame>,
    max_frames: usize,
    max_age: Duration,
}

impl RetryQueue {
    /// Create a queue that holds at most `max_frames` frames for at most
    /// `max_age`.
    pub fn new(max_frames: usize, max_age: Duration) -> Self {
        Self {
            pending: BTreeMap::new(),
            max_frames: max_frames.max(1),
            max_age,
        }
    }

    /// Remember a frame until it is acknowledged.
    ///
    /// When the queue is full the oldest frame is dropped, because a stale
    /// frame is worth less than the newest one.
    pub fn track(&mut self, id: u64, frame: Frame) {
        if self.pending.len() >= self.max_frames {
            if let Some(oldest) = self.pending.keys().next().copied() {
                self.pending.remove(&oldest);
            }
        }
        self.pending.insert(
            id,
            PendingFrame {
                id,
                frame,
                queued_at: Instant::now(),
                attempts: 0,
            },
        );
    }

    /// Mark a frame as acknowledged.
    ///
    /// Returns `true` when the frame was still pending.
    pub fn acknowledge(&mut self, id: u64) -> bool {
        self.pending.remove(&id).is_some()
    }

    /// Whether the frame is still waiting for an acknowledgement.
    pub fn contains(&self, id: u64) -> bool {
        self.pending.contains_key(&id)
    }

    /// Number of pending frames.
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// Whether the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Take all pending frames, oldest first, and empty the queue.
    ///
    /// The frames are handed back to the caller, which replays them over the
    /// fresh session.
    pub fn take_all(&mut self) -> Vec<PendingFrame> {
        std::mem::take(&mut self.pending)
            .into_values()
            .collect()
    }

    /// Put frames back into the queue, preserving their order.
    pub fn restore(&mut self, frames: Vec<PendingFrame>) {
        for frame in frames {
            self.track(frame.id, frame.frame);
        }
    }

    /// Drop frames that have been waiting for too long.
    ///
    /// Returns the number of dropped frames.
    pub fn purge_expired(&mut self) -> usize {
        let now = Instant::now();
        let before = self.pending.len();
        self.pending
            .retain(|_, frame| now.duration_since(frame.queued_at) <= self.max_age);
        before - self.pending.len()
    }
}

impl Default for RetryQueue {
    fn default() -> Self {
        Self::new(1024, Duration::from_secs(300))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_acknowledged_once() {
        let mut queue = RetryQueue::default();
        queue.track(1, Frame::ChatText { text: "a".into() });
        assert!(queue.contains(1));
        assert!(queue.acknowledge(1));
        assert!(!queue.acknowledge(1));
        assert!(queue.is_empty());
    }

    #[test]
    fn unacknowledged_frames_survive_a_disconnect() {
        let mut queue = RetryQueue::default();
        queue.track(1, Frame::ChatText { text: "a".into() });
        queue.track(2, Frame::ChatText { text: "b".into() });

        // The socket dies: the queue is still holding the frames.
        let pending = queue.take_all();
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].id, 1);
        assert_eq!(pending[1].id, 2);
    }

    #[test]
    fn the_queue_is_bounded() {
        let mut queue = RetryQueue::new(2, Duration::from_secs(60));
        queue.track(1, Frame::ChatText { text: "a".into() });
        queue.track(2, Frame::ChatText { text: "b".into() });
        queue.track(3, Frame::ChatText { text: "c".into() });
        assert_eq!(queue.len(), 2);
        // The oldest frame was evicted.
        assert!(!queue.contains(1));
        assert!(queue.contains(3));
    }

    #[test]
    fn expired_frames_are_dropped() {
        let mut queue = RetryQueue::new(10, Duration::from_millis(1));
        queue.track(1, Frame::ChatText { text: "a".into() });
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(queue.purge_expired(), 1);
        assert!(queue.is_empty());
    }
}
