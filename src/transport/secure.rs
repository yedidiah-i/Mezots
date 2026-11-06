//! The encrypted frame channel.
//!
//! This is the seam between the protocol layer and the wire. A frame is
//! encoded with `postcard`, sealed with ChaCha20-Poly1305 and finally written
//! as a length-delimited message. On the other side the order is reversed:
//! the message is read, opened and only then decoded.
//!
//! The TCP stream is split into an owned read half and an owned write half
//! *before* the codec is applied. That way the write half can be shut down
//! on its own, which is what tells the peer that the connection is over -
//! dropping a split `Framed` would leave the socket open.

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use crate::crypto::aead::FrameCipher;
use crate::error::{Error, Result};
use crate::protocol::codec::{decode_envelope, encode_envelope, FrameSink};
use crate::protocol::frame::Envelope;

/// Extra room for the AEAD tag on top of the plaintext size.
const TAG_OVERHEAD: usize = 16;

/// The sending half of an encrypted channel.
#[derive(Debug)]
pub struct SecureSender {
    sink: Framed<OwnedWriteHalf, LengthDelimitedCodec>,
    cipher: FrameCipher,
    max_frame_bytes: usize,
}

impl SecureSender {
    /// Number of frames sealed by this sender.
    pub fn frames_sent(&self) -> u64 {
        self.cipher.frames_processed()
    }
}

impl FrameSink for SecureSender {
    async fn send_frame(&mut self, envelope: &Envelope) -> Result<()> {
        let plaintext = encode_envelope(envelope)?;
        if plaintext.len() > self.max_frame_bytes {
            return Err(Error::Protocol(format!(
                "frame of {} bytes exceeds the {} byte limit",
                plaintext.len(),
                self.max_frame_bytes
            )));
        }

        // Step 11: seal the frame *after* serialization, *before* the wire.
        let ciphertext = self.cipher.encrypt(&plaintext)?;
        self.sink.send(Bytes::from(ciphertext)).await?;
        Ok(())
    }

    async fn shutdown(&mut self) {
        // Send a FIN so the peer sees the disconnect right away.
        let _ = self.sink.get_mut().shutdown().await;
    }
}

/// The receiving half of an encrypted channel.
#[derive(Debug)]
pub struct SecureReceiver {
    stream: Framed<OwnedReadHalf, LengthDelimitedCodec>,
    cipher: FrameCipher,
}

impl SecureReceiver {
    /// Read the next authenticated envelope.
    ///
    /// Returns `Ok(None)` when the peer closed the socket. A frame that fails
    /// authentication is reported as an error and the channel must be torn
    /// down - the plaintext is never handed on.
    pub async fn recv(&mut self) -> Result<Option<Envelope>> {
        match self.stream.next().await {
            Some(Ok(message)) => {
                let plaintext = self.cipher.decrypt(&message)?;
                Ok(Some(decode_envelope(&plaintext)?))
            }
            Some(Err(e)) => Err(Error::Io(e)),
            None => Ok(None),
        }
    }

    /// Number of frames opened by this receiver.
    pub fn frames_received(&self) -> u64 {
        self.cipher.frames_processed()
    }
}

/// A connected, encrypted peer channel.
#[derive(Debug)]
pub struct SecureChannel {
    /// Sending half.
    pub sender: SecureSender,
    /// Receiving half.
    pub receiver: SecureReceiver,
}

impl SecureChannel {
    /// Build a channel from an already handshaken TCP stream.
    pub fn establish(
        stream: TcpStream,
        tx_cipher: FrameCipher,
        rx_cipher: FrameCipher,
        max_frame_bytes: usize,
    ) -> Self {
        let (read_half, write_half) = stream.into_split();
        let codec = LengthDelimitedCodec::builder()
            .max_frame_length(max_frame_bytes + TAG_OVERHEAD)
            .new_codec();

        Self {
            sender: SecureSender {
                sink: Framed::new(write_half, codec.clone()),
                cipher: tx_cipher,
                max_frame_bytes,
            },
            receiver: SecureReceiver {
                stream: Framed::new(read_half, codec),
                cipher: rx_cipher,
            },
        }
    }
}
