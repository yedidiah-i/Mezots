//! Serialization of frames into compact byte arrays and back.
//!
//! `postcard` is used because it produces varint-sized integers with no
//! padding, which keeps control frames down to a handful of bytes while
//! remaining schema-free and forward compatible.

use crate::error::Result;

use super::frame::{Envelope, Frame};

/// Default upper bound for a single encoded frame.
pub const DEFAULT_MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Encode a frame into its wire representation.
pub fn encode_frame(frame: &Frame) -> Result<Vec<u8>> {
    Ok(postcard::to_allocvec(frame)?)
}

/// Decode a frame from its wire representation.
pub fn decode_frame(bytes: &[u8]) -> Result<Frame> {
    Ok(postcard::from_bytes(bytes)?)
}

/// Encode an envelope (frame plus acknowledgement id).
pub fn encode_envelope(envelope: &Envelope) -> Result<Vec<u8>> {
    Ok(postcard::to_allocvec(envelope)?)
}

/// Decode an envelope from its wire representation.
pub fn decode_envelope(bytes: &[u8]) -> Result<Envelope> {
    Ok(postcard::from_bytes(bytes)?)
}

/// A destination that accepts encoded envelopes.
///
/// Implemented by the encrypted transport so that the router can stay agnostic
/// of whether a frame is written in the clear or sealed.
pub trait FrameSink {
    /// Write one envelope to the destination.
    fn send_frame<'a>(
        &'a mut self,
        envelope: &'a Envelope,
    ) -> impl std::future::Future<Output = Result<()>> + Send + 'a;

    /// Release the destination.
    ///
    /// Called once the router stops writing, so the peer learns that the
    /// connection is gone instead of waiting for a timeout.
    fn shutdown(&mut self) -> impl std::future::Future<Output = ()> + Send + '_;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrip() {
        let frames = vec![
            Frame::Ping { nonce: 42, sent_ms: 1234 },
            Frame::ChatText {
                text: "hello µnïcødé".into(),
            },
            Frame::ChatBinary {
                mime: "image/png".into(),
                payload: vec![0, 1, 2, 255],
            },
            Frame::StreamChunk {
                stream_id: 7,
                seq: 99,
                payload: vec![9; 128],
            },
            Frame::NodeInfo {
                node_id: "node-a".into(),
                display_name: "A".into(),
                version: "0.1.0".into(),
            },
        ];
        for frame in frames {
            let bytes = encode_frame(&frame).unwrap();
            assert_eq!(decode_frame(&bytes).unwrap(), frame);
        }
    }

    #[test]
    fn envelope_roundtrip() {
        let envelope = Envelope::next(17, Frame::ChatText { text: "yo".into() });
        let bytes = encode_envelope(&envelope).unwrap();
        assert_eq!(decode_envelope(&bytes).unwrap(), envelope);
    }

    #[test]
    fn corrupt_bytes_are_rejected() {
        let mut bytes = encode_frame(&Frame::ChatText { text: "x".into() }).unwrap();
        // Layout: [variant tag][varint length][payload]. Claiming a payload
        // that is longer than the buffer must fail the decode.
        assert!(bytes.len() > 1, "unexpected encoding: {bytes:?}");
        bytes[1] = 0x7f;
        assert!(decode_frame(&bytes).is_err());
    }

    #[test]
    fn truncated_bytes_are_rejected() {
        let bytes = encode_frame(&Frame::ChatText { text: "hello".into() }).unwrap();
        assert!(decode_frame(&bytes[..bytes.len() - 2]).is_err());
    }

    #[test]
    fn encoding_is_compact() {
        // A ping carries two varints plus the variant tag; it must stay tiny.
        let bytes = encode_frame(&Frame::Ping { nonce: 1, sent_ms: 2 }).unwrap();
        assert!(bytes.len() < 16, "ping frame unexpectedly large: {bytes:?}");
    }
}
