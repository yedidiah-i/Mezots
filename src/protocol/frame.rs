//! The type-safe data layout exchanged between nodes.

use serde::{Deserialize, Serialize};

use crate::ledger::LedgerBlock;

/// Every message that can travel between two nodes.
///
/// The split between variants matters: it lets the router route control
/// traffic, chat traffic and bulk stream traffic on separate queues, and it
/// lets the reliability layer decide which frames need an acknowledgement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Frame {
    /// Liveness probe. Carries the nonce that the peer must echo back.
    Ping {
        /// Random value that must be echoed in the matching [`Frame::Pong`].
        nonce: u64,
        /// Sender's monotonic clock in milliseconds.
        sent_ms: u64,
    },
    /// Liveness reply.
    Pong {
        /// Echoed value from the [`Frame::Ping`].
        nonce: u64,
        /// The `sent_ms` of the ping being answered.
        sent_ms: u64,
    },
    /// Acknowledgement for a previously received envelope.
    Ack {
        /// The `id` of the envelope being acknowledged.
        id: u64,
    },
    /// Plaintext chat message.
    ChatText {
        /// The room the message belongs to, for example `general`.
        room: String,
        /// Message body.
        text: String,
    },
    /// Binary chat attachment (image, file, ...).
    ChatBinary {
        /// The room the attachment belongs to.
        room: String,
        /// MIME type of the payload.
        mime: String,
        /// Raw bytes.
        payload: Vec<u8>,
    },
    /// One chunk of a bulk stream such as a file or video transfer.
    StreamChunk {
        /// The room the stream belongs to.
        room: String,
        /// Identifier of the stream this chunk belongs to.
        stream_id: u32,
        /// Monotonic sequence number within the stream.
        seq: u64,
        /// Raw bytes.
        payload: Vec<u8>,
    },
    /// Marks the end of a bulk stream.
    StreamEnd {
        /// The room the stream belongs to.
        room: String,
        /// Identifier of the finished stream.
        stream_id: u32,
    },
    /// Announces a live stream before its first chunk.
    StreamMeta {
        /// The room the stream belongs to.
        room: String,
        /// Identifier of the stream.
        stream_id: u32,
        /// Title of the broadcast.
        title: String,
        /// MIME type of the chunks that follow.
        mime: String,
    },
    /// Peer metadata, exchanged right after the handshake.
    NodeInfo {
        /// Node id.
        node_id: String,
        /// Human readable name.
        display_name: String,
        /// Protocol version.
        version: String,
    },
    /// A block of the school's private chain, gossiped between nodes.
    LedgerBlock {
        /// The block itself.
        block: LedgerBlock,
    },
    /// The head of a peer's chain, used to measure agreement.
    ChainState {
        /// Hash of the peer's newest block.
        head: String,
        /// Height of the peer's chain.
        height: u64,
    },
    /// Graceful shutdown notice.
    Bye {
        /// Why the sender is going away.
        reason: String,
    },
}

impl Frame {
    /// Which queue this frame should be routed through.
    pub fn priority(&self) -> Priority {
        match self {
            Frame::Ping { .. } | Frame::Pong { .. } | Frame::Ack { .. } => Priority::Control,
            Frame::ChatText { .. } | Frame::ChatBinary { .. } | Frame::NodeInfo { .. } => {
                Priority::Chat
            }
            Frame::StreamChunk { .. } | Frame::StreamEnd { .. } | Frame::StreamMeta { .. } => {
                Priority::Stream
            }
            Frame::LedgerBlock { .. } | Frame::ChainState { .. } => Priority::Control,
            Frame::Bye { .. } => Priority::Control,
        }
    }

    /// Whether the sender expects an [`Frame::Ack`] for this frame.
    ///
    /// Bulk stream chunks and chat payloads are acknowledged; liveness and
    /// control frames are not, which keeps the acknowledgement traffic down.
    pub fn expects_ack(&self) -> bool {
        matches!(
            self,
            Frame::ChatText { .. }
                | Frame::ChatBinary { .. }
                | Frame::StreamChunk { .. }
                | Frame::StreamEnd { .. }
                | Frame::StreamMeta { .. }
                | Frame::NodeInfo { .. }
        )
    }

    /// Short label used in logs.
    pub fn kind(&self) -> &'static str {
        match self {
            Frame::Ping { .. } => "ping",
            Frame::Pong { .. } => "pong",
            Frame::Ack { .. } => "ack",
            Frame::ChatText { .. } => "chat_text",
            Frame::ChatBinary { .. } => "chat_binary",
            Frame::StreamChunk { .. } => "stream_chunk",
            Frame::StreamEnd { .. } => "stream_end",
            Frame::StreamMeta { .. } => "stream_meta",
            Frame::NodeInfo { .. } => "node_info",
            Frame::LedgerBlock { .. } => "ledger_block",
            Frame::ChainState { .. } => "chain_state",
            Frame::Bye { .. } => "bye",
        }
    }
}

/// Delivery class of a frame, used to pick the queue it is routed through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// Control traffic (ping/pong/ack/bye): must never be delayed.
    Control = 0,
    /// Interactive traffic (chat, node info).
    Chat = 1,
    /// Bulk traffic (file and video chunks): may be throttled.
    Stream = 2,
}

impl Priority {
    /// Label used in backpressure error messages.
    pub fn as_str(&self) -> &'static str {
        match self {
            Priority::Control => "control",
            Priority::Chat => "chat",
            Priority::Stream => "stream",
        }
    }
}

/// A frame plus the sequential id used for acknowledgements and retries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    /// Monotonically increasing id, unique per sender.
    pub id: u64,
    /// The payload.
    pub frame: Frame,
}

impl Envelope {
    /// Wrap a frame with the next id of a counter.
    pub fn next(id: u64, frame: Frame) -> Self {
        Self { id, frame }
    }

    /// Delivery class of the wrapped frame.
    pub fn priority(&self) -> Priority {
        self.frame.priority()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priorities_are_assigned_by_frame_kind() {
        assert_eq!(Frame::Ping { nonce: 1, sent_ms: 0 }.priority(), Priority::Control);
        assert_eq!(
            Frame::ChatText {
                room: "general".into(),
                text: "hi".into()
            }
            .priority(),
            Priority::Chat
        );
        assert_eq!(
            Frame::StreamChunk {
                room: "stream:hall".into(),
                stream_id: 1,
                seq: 0,
                payload: vec![]
            }
            .priority(),
            Priority::Stream
        );
    }

    #[test]
    fn only_data_frames_expect_acknowledgements() {
        assert!(Frame::ChatText {
            room: "general".into(),
            text: "x".into()
        }
        .expects_ack());
        assert!(!Frame::Ping { nonce: 0, sent_ms: 0 }.expects_ack());
    }
}
