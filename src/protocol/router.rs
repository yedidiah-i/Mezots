//! Stream multiplexer and backpressure router.
//!
//! Each peer gets three bounded queues:
//!
//! | Queue | Traffic | Behaviour when full |
//! |-------|---------|---------------------|
//! | control | ping / pong / ack / bye | rejected immediately, never blocks the socket |
//! | chat | text and attachments | waits for room, but keeps its place ahead of bulk data |
//! | stream | file and video chunks | waits for room, which slows the *sender* down |
//!
//! The writer drains the queues with a strict priority order, so a saturated
//! video stream can never push a chat message out of the socket buffer. When
//! the stream queue fills up the send call simply does not complete, and the
//! producer is expected to slow down - that is the backpressure signal.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, watch};

use crate::error::{Error, Result};

use super::codec::FrameSink;
use super::frame::{Envelope, Priority};

/// How many consecutive control frames may be written before the lower
/// priority queues get a turn.
pub const MAX_STARVATION_STREAK: usize = 8;

/// Queue capacities for one peer.
#[derive(Debug, Clone, Copy)]
pub struct RouterConfig {
    /// Capacity of the control queue.
    pub control_capacity: usize,
    /// Capacity of the chat queue.
    pub chat_capacity: usize,
    /// Capacity of the stream queue.
    pub stream_capacity: usize,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            control_capacity: 64,
            chat_capacity: 256,
            stream_capacity: 32,
        }
    }
}

/// A frame waiting to be written, tagged with the queue it came from.
#[derive(Debug)]
pub struct OutboundFrame {
    /// The payload.
    pub envelope: Envelope,
    /// The queue it was routed through.
    pub priority: Priority,
}

/// Shared counters describing the state of a router.
#[derive(Debug, Default)]
pub struct RouterStats {
    /// Frames currently waiting in the queues.
    queued: AtomicUsize,
    /// Frames that could not be queued because the target queue was full.
    rejected: AtomicU64,
    /// Frames successfully written to the transport.
    written: AtomicU64,
}

impl RouterStats {
    /// Number of frames waiting to be written.
    pub fn queued(&self) -> usize {
        self.queued.load(Ordering::Relaxed)
    }

    /// Number of frames rejected because a queue was full.
    pub fn rejected(&self) -> u64 {
        self.rejected.load(Ordering::Relaxed)
    }

    /// Number of frames written to the transport.
    pub fn written(&self) -> u64 {
        self.written.load(Ordering::Relaxed)
    }
}

/// The sending half of the multiplexer; cheap to clone.
#[derive(Debug, Clone)]
pub struct MuxSender {
    control: mpsc::Sender<OutboundFrame>,
    chat: mpsc::Sender<OutboundFrame>,
    stream: mpsc::Sender<OutboundFrame>,
    stats: Arc<RouterStats>,
    shutdown: watch::Sender<bool>,
}

impl MuxSender {
    /// Queue a frame without waiting.
    ///
    /// Returns [`Error::Backpressure`] when the matching queue is full. This
    /// is the right call for control frames, which must never block the
    /// caller.
    pub fn try_send(&self, envelope: Envelope) -> Result<()> {
        let priority = envelope.priority();
        let channel = match priority {
            Priority::Control => &self.control,
            Priority::Chat => &self.chat,
            Priority::Stream => &self.stream,
        };

        // `try_reserve` does not consume the frame, so it survives a failed
        // attempt and can be queued again.
        match channel.try_reserve() {
            Ok(permit) => {
                permit.send(OutboundFrame { envelope, priority });
                self.stats.queued.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(TrySendError::Full(_)) => {
                self.stats.rejected.fetch_add(1, Ordering::Relaxed);
                Err(Error::Backpressure {
                    queue: priority.as_str(),
                })
            }
            Err(TrySendError::Closed(_)) => Err(Error::TransportClosed),
        }
    }

    /// Queue a frame, waiting for room if the queue is full.
    ///
    /// This is the path used for chat: the message waits for capacity instead
    /// of being dropped, but it still overtakes queued bulk data.
    pub async fn send(&self, envelope: Envelope) -> Result<()> {
        let priority = envelope.priority();
        let item = OutboundFrame { envelope, priority };
        let result = match priority {
            Priority::Control => self.control.send(item).await,
            Priority::Chat => self.chat.send(item).await,
            Priority::Stream => self.stream.send(item).await,
        };
        result.map_err(|_| Error::TransportClosed)?;
        self.stats.queued.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Queue a bulk stream chunk, waiting for room.
    ///
    /// Because the stream queue is deliberately small, a slow reader makes
    /// this call block - that is exactly the backpressure the sender needs in
    /// order to slow down its producer.
    pub async fn send_stream(&self, envelope: Envelope) -> Result<()> {
        let item = OutboundFrame {
            priority: Priority::Stream,
            envelope,
        };
        self.stream
            .send(item)
            .await
            .map_err(|_| Error::TransportClosed)?;
        self.stats.queued.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Frames currently waiting to be written.
    pub fn queued(&self) -> usize {
        self.stats.queued()
    }

    /// Whether the stream queue is currently saturated.
    pub fn is_stream_saturated(&self) -> bool {
        self.stats.queued() > 0
            && self.stream.capacity() == self.stream.max_capacity()
            && self.stream.max_capacity() > 0
    }

    /// Counters for this router.
    pub fn stats(&self) -> &Arc<RouterStats> {
        &self.stats
    }

    /// Close the queues; the writer stops after draining them.
    pub fn close(&self) {
        let _ = self.shutdown.send(true);
    }
}

/// The receiving half of the multiplexer.
#[derive(Debug)]
pub struct MuxReceiver {
    control: mpsc::Receiver<OutboundFrame>,
    chat: mpsc::Receiver<OutboundFrame>,
    stream: mpsc::Receiver<OutboundFrame>,
    stats: Arc<RouterStats>,
    control_streak: usize,
    shutdown: watch::Receiver<bool>,
}

impl MuxReceiver {
    /// Take the next frame, honouring the priority order.
    ///
    /// Returns `None` once every queue is closed and drained, or as soon as
    /// the router is closed.
    pub async fn next(&mut self) -> Option<OutboundFrame> {
        // A separate handle keeps the shutdown signal independent of the
        // queue borrows below.
        let mut shutdown = self.shutdown.clone();

        let item = if self.control_streak >= MAX_STARVATION_STREAK {
            // Fairness guard: let the lower priority queues catch up.
            tokio::select! {
                biased;
                _ = wait_shutdown(&mut shutdown) => return None,
                Some(item) = self.chat.recv() => Some(item),
                Some(item) = self.control.recv() => Some(item),
                Some(item) = self.stream.recv() => Some(item),
                else => None,
            }
        } else {
            tokio::select! {
                biased;
                _ = wait_shutdown(&mut shutdown) => return None,
                Some(item) = self.control.recv() => Some(item),
                Some(item) = self.chat.recv() => Some(item),
                Some(item) = self.stream.recv() => Some(item),
                else => None,
            }
        }?;

        self.stats.queued.fetch_sub(1, Ordering::Relaxed);
        if item.priority == Priority::Control {
            self.control_streak += 1;
        } else {
            self.control_streak = 0;
        }
        Some(item)
    }
}

/// Wait until the router is closed.
async fn wait_shutdown(receiver: &mut watch::Receiver<bool>) {
    let already_closed = *receiver.borrow();
    if already_closed {
        return;
    }
    let _ = receiver.changed().await;
}

/// A per-peer multiplexer.
#[derive(Debug)]
pub struct MuxRouter {
    sender: MuxSender,
    receiver: MuxReceiver,
}

impl MuxRouter {
    /// Create a router with the given queue capacities.
    pub fn new(config: RouterConfig) -> Self {
        let stats = Arc::new(RouterStats::default());
        let (control_tx, control_rx) = mpsc::channel(config.control_capacity.max(1));
        let (chat_tx, chat_rx) = mpsc::channel(config.chat_capacity.max(1));
        let (stream_tx, stream_rx) = mpsc::channel(config.stream_capacity.max(1));
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let sender = MuxSender {
            control: control_tx,
            chat: chat_tx,
            stream: stream_tx,
            stats: Arc::clone(&stats),
            shutdown: shutdown_tx,
        };
        let receiver = MuxReceiver {
            control: control_rx,
            chat: chat_rx,
            stream: stream_rx,
            stats,
            control_streak: 0,
            shutdown: shutdown_rx,
        };
        Self { sender, receiver }
    }

    /// Cloneable handle used to queue frames.
    pub fn sender(&self) -> MuxSender {
        self.sender.clone()
    }

    /// Split into the sending and receiving halves.
    pub fn split(self) -> (MuxSender, MuxReceiver) {
        (self.sender, self.receiver)
    }

    /// Consume the router and return the sending half.
    pub fn into_sender(self) -> MuxSender {
        self.sender
    }
}

/// Spawn the writer task that drains a router into a sink.
///
/// The task runs until every queue is closed and drained, or until the sink
/// reports an error.
pub fn spawn_writer<S>(mut sink: S, mut receiver: MuxReceiver) -> tokio::task::JoinHandle<Result<()>>
where
    S: FrameSink + Send + 'static,
{
    tokio::spawn(async move {
        while let Some(item) = receiver.next().await {
            sink.send_frame(&item.envelope).await?;
            receiver
                .stats
                .written
                .fetch_add(1, Ordering::Relaxed);
        }
        // Let the peer know that the connection is over.
        sink.shutdown().await;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::frame::Frame;

    fn config() -> RouterConfig {
        RouterConfig {
            control_capacity: 4,
            chat_capacity: 4,
            stream_capacity: 2,
        }
    }

    #[tokio::test]
    async fn stream_queue_reports_backpressure() {
        let router = MuxRouter::new(config());
        let sender = router.sender();

        // Fill the stream queue to capacity.
        for seq in 0..2 {
            let env = Envelope::next(
                seq,
                Frame::StreamChunk {
                    stream_id: 1,
                    seq,
                    payload: vec![0; 8],
                },
            );
            sender.try_send(env).unwrap();
        }

        // The next one does not fit any more.
        let overflow = Envelope::next(
            99,
            Frame::StreamChunk {
                stream_id: 1,
                seq: 99,
                payload: vec![0; 8],
            },
        );
        match sender.try_send(overflow) {
            Err(Error::Backpressure { queue }) => assert_eq!(queue, "stream"),
            other => panic!("expected backpressure, got {other:?}"),
        }
        assert_eq!(sender.stats().rejected(), 1);
    }

    #[tokio::test]
    async fn control_frames_are_not_blocked_by_a_full_stream_queue() {
        let router = MuxRouter::new(config());
        let sender = router.sender();
        for seq in 0..2 {
            sender
                .try_send(Envelope::next(
                    seq,
                    Frame::StreamChunk {
                        stream_id: 1,
                        seq,
                        payload: vec![0; 8],
                    },
                ))
                .unwrap();
        }
        // The stream queue is full, but control traffic still goes through.
        sender
            .try_send(Envelope::next(1, Frame::Ping { nonce: 1, sent_ms: 0 }))
            .expect("control frames must not be blocked by the stream queue");
    }

    #[tokio::test]
    async fn control_frames_are_drained_before_chat_and_stream() {
        let router = MuxRouter::new(config());
        let sender = router.sender();
        let (_, mut receiver) = router.split();

        sender
            .try_send(Envelope::next(
                1,
                Frame::StreamChunk {
                    stream_id: 1,
                    seq: 0,
                    payload: vec![0],
                },
            ))
            .unwrap();
        sender
            .try_send(Envelope::next(2, Frame::ChatText { text: "hi".into() }))
            .unwrap();
        sender
            .try_send(Envelope::next(3, Frame::Ping { nonce: 1, sent_ms: 0 }))
            .unwrap();

        let first = receiver.next().await.unwrap();
        assert_eq!(first.priority, Priority::Control);
        let second = receiver.next().await.unwrap();
        assert_eq!(second.priority, Priority::Chat);
        let third = receiver.next().await.unwrap();
        assert_eq!(third.priority, Priority::Stream);
    }
}
