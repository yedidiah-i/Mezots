//! The application-layer peer-to-peer handshake.
//!
//! The handshake runs in the clear (there is nothing secret to protect yet)
//! but it is *authenticated*: both peers sign a transcript of the exchange,
//! so a third party on the LAN cannot impersonate either side.
//!
//! ```text
//!   Initiator                                    Responder
//!   ---------                                    ---------
//!   Hello  (node id, X25519 pub, Ed25519 pub, nonce)
//!   ---------------------------------------------->
//!
//!                     HelloAck (X25519 pub, Ed25519 pub,
//!                     nonce, signature over transcript0)
//!   <----------------------------------------------
//!
//!   Confirm (signature over transcript1)
//!   ---------------------------------------------->
//!
//!   both sides derive:  key = HKDF(X25519_shared, salt = transcript1)
//! ```

use hkdf::Hkdf;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::aead::{Direction, FrameCipher};
use super::keys::{NodeKeys, KEY_LEN, SIGNATURE_LEN};
use super::CryptoError;

/// Domain separation tag mixed into every transcript.
pub const HANDSHAKE_MAGIC: &[u8] = b"mezots-hs-v1";

/// Current handshake protocol version.
pub const HANDSHAKE_VERSION: u8 = 1;

/// Largest handshake frame accepted from the wire.
pub const MAX_HANDSHAKE_FRAME: usize = 8 * 1024;

/// Frames exchanged during the handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HandshakeFrame {
    /// First message, sent by the initiator.
    Hello {
        /// Protocol version.
        version: u8,
        /// Initiating node id.
        node_id: String,
        /// Initiator's ephemeral X25519 public key.
        x25519: [u8; KEY_LEN],
        /// Initiator's Ed25519 identity public key.
        ed25519: [u8; KEY_LEN],
        /// Random 32 bytes; the first 4 become the initiator's AEAD nonce prefix.
        nonce: [u8; 32],
    },
    /// Second message, sent by the responder.
    HelloAck {
        /// Responding node id.
        node_id: String,
        /// Responder's ephemeral X25519 public key.
        x25519: [u8; KEY_LEN],
        /// Responder's Ed25519 identity public key.
        ed25519: [u8; KEY_LEN],
        /// Random 32 bytes; the first 4 become the responder's AEAD nonce prefix.
        nonce: [u8; 32],
        /// Ed25519 signature over `transcript0`.
        signature: Vec<u8>,
    },
    /// Third message: the initiator proves it holds the private identity key.
    Confirm {
        /// Ed25519 signature over `transcript1`.
        signature: Vec<u8>,
    },
}

/// Result of a completed handshake.
#[derive(Debug)]
pub struct HandshakeOutcome {
    /// Node id reported by the remote peer.
    pub peer_node_id: String,
    /// Remote peer's Ed25519 identity key (to be pinned by the trust store).
    pub peer_ed25519: [u8; KEY_LEN],
    /// Remote peer's X25519 public key.
    pub peer_x25519: [u8; KEY_LEN],
    /// Cipher used to seal outbound frames.
    pub tx: FrameCipher,
    /// Cipher used to open inbound frames.
    pub rx: FrameCipher,
    /// Identifier of this session, derived from the transcript.
    pub session_id: [u8; 8],
}

/// Run the handshake as the initiator (the side that opened the socket).
pub async fn initiator_handshake<S>(stream: &mut S, keys: &NodeKeys, node_id: &str) -> Result<HandshakeOutcome, CryptoError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut nonce = [0u8; 32];
    rand::rng().fill_bytes(&mut nonce);

    let hello = HandshakeFrame::Hello {
        version: HANDSHAKE_VERSION,
        node_id: node_id.to_string(),
        x25519: keys.x25519_public(),
        ed25519: keys.ed25519_public(),
        nonce,
    };
    let hello_bytes = postcard::to_allocvec(&hello)?;
    write_payload(stream, &hello_bytes).await?;

    // --- step 2: receive the acknowledgement -------------------------------
    let (ack, ack_bytes) = read_payload(stream).await?;
    let (peer_node_id, peer_x25519, peer_ed25519, peer_nonce, signature) = match ack {
        HandshakeFrame::HelloAck {
            node_id,
            x25519,
            ed25519,
            nonce,
            signature,
        } => (node_id, x25519, ed25519, nonce, signature),
        other => {
            return Err(CryptoError::Unexpected(format!(
                "expected HelloAck, got {other:?}"
            )));
        }
    };

    if signature.len() != SIGNATURE_LEN {
        return Err(CryptoError::Unexpected("malformed signature length".into()));
    }

    // --- step 3: verify the responder's signature --------------------------
    let transcript0 = transcript0(&hello_bytes, &peer_x25519, &peer_ed25519, &peer_nonce);
    NodeKeys::verify(&peer_ed25519, &transcript0, &signature).map_err(CryptoError::Signature)?;

    // --- step 4: sign the full transcript ----------------------------------
    let transcript1 = transcript1(&hello_bytes, &ack_bytes);
    let confirm_signature = keys.sign(&transcript1);
    write_payload(
        stream,
        &postcard::to_allocvec(&HandshakeFrame::Confirm {
            signature: confirm_signature.to_vec(),
        })?,
    )
    .await?;

    // --- step 5: derive the session keys -----------------------------------
    let shared = keys
        .diffie_hellman(&peer_x25519)
        .ok_or_else(|| CryptoError::KeyExchange("degenerate shared secret".into()))?;

    let session_id = session_id(&transcript1);
    let (tx, rx) = derive_ciphers(
        &shared,
        &transcript1,
        &nonce,
        &peer_nonce,
        Direction::InitiatorToResponder,
        Direction::ResponderToInitiator,
    )?;

    Ok(HandshakeOutcome {
        peer_node_id,
        peer_ed25519,
        peer_x25519,
        tx,
        rx,
        session_id,
    })
}

/// Run the handshake as the responder (the side that accepted the socket).
pub async fn responder_handshake<S>(
    stream: &mut S,
    keys: &NodeKeys,
    node_id: &str,
) -> Result<HandshakeOutcome, CryptoError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    // --- step 1: receive the hello -----------------------------------------
    let (hello, hello_bytes) = read_payload(stream).await?;
    let (peer_node_id, peer_x25519, peer_ed25519, peer_nonce) = match hello {
        HandshakeFrame::Hello {
            version,
            node_id,
            x25519,
            ed25519,
            nonce,
        } => {
            if version != HANDSHAKE_VERSION {
                return Err(CryptoError::Unexpected(format!(
                    "unsupported handshake version {version}"
                )));
            }
            (node_id, x25519, ed25519, nonce)
        }
        other => {
            return Err(CryptoError::Unexpected(format!(
                "expected Hello, got {other:?}"
            )));
        }
    };

    // --- step 2: answer with our own public material ----------------------
    let mut nonce = [0u8; 32];
    rand::rng().fill_bytes(&mut nonce);

    let ack = HandshakeFrame::HelloAck {
        node_id: node_id.to_string(),
        x25519: keys.x25519_public(),
        ed25519: keys.ed25519_public(),
        nonce,
        signature: keys
            .sign(&transcript0(&hello_bytes, &keys.x25519_public(), &keys.ed25519_public(), &nonce))
            .to_vec(),
    };
    let ack_bytes = postcard::to_allocvec(&ack)?;
    write_payload(stream, &ack_bytes).await?;

    // --- step 3: verify the initiator's confirmation ------------------------
    let (confirm, _) = read_payload(stream).await?;
    let signature = match confirm {
        HandshakeFrame::Confirm { signature } => signature,
        other => {
            return Err(CryptoError::Unexpected(format!(
                "expected Confirm, got {other:?}"
            )));
        }
    };
    if signature.len() != SIGNATURE_LEN {
        return Err(CryptoError::Unexpected("malformed signature length".into()));
    }

    let transcript1 = transcript1(&hello_bytes, &ack_bytes);
    NodeKeys::verify(&peer_ed25519, &transcript1, &signature).map_err(CryptoError::Signature)?;

    // --- step 4: derive the session keys -----------------------------------
    let shared = keys
        .diffie_hellman(&peer_x25519)
        .ok_or_else(|| CryptoError::KeyExchange("degenerate shared secret".into()))?;

    let session_id = session_id(&transcript1);
    let (tx, rx) = derive_ciphers(
        &shared,
        &transcript1,
        &nonce,
        &peer_nonce,
        Direction::ResponderToInitiator,
        Direction::InitiatorToResponder,
    )?;

    Ok(HandshakeOutcome {
        peer_node_id,
        peer_ed25519,
        peer_x25519,
        tx,
        rx,
        session_id,
    })
}

/// Transcript covered by the responder's signature.
fn transcript0(
    hello_bytes: &[u8],
    peer_x25519: &[u8; KEY_LEN],
    peer_ed25519: &[u8; KEY_LEN],
    peer_nonce: &[u8; 32],
) -> [u8; 32] {
    let mut buf = Vec::with_capacity(HANDSHAKE_MAGIC.len() + hello_bytes.len() + 96);
    buf.extend_from_slice(HANDSHAKE_MAGIC);
    buf.extend_from_slice(hello_bytes);
    buf.extend_from_slice(peer_x25519);
    buf.extend_from_slice(peer_ed25519);
    buf.extend_from_slice(peer_nonce);
    super::keys::sha256(&buf)
}

/// Transcript covered by the initiator's confirmation.
fn transcript1(hello_bytes: &[u8], ack_bytes: &[u8]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(HANDSHAKE_MAGIC.len() + hello_bytes.len() + ack_bytes.len());
    buf.extend_from_slice(HANDSHAKE_MAGIC);
    buf.extend_from_slice(hello_bytes);
    buf.extend_from_slice(ack_bytes);
    super::keys::sha256(&buf)
}

/// Derive the two directional session identifiers.
fn session_id(transcript: &[u8; 32]) -> [u8; 8] {
    let mut id = [0u8; 8];
    id.copy_from_slice(&transcript[..8]);
    id
}

/// Derive the send and receive ciphers for one side of the session.
fn derive_ciphers(
    shared: &[u8; 32],
    transcript: &[u8; 32],
    local_nonce: &[u8; 32],
    peer_nonce: &[u8; 32],
    local_direction: Direction,
    peer_direction: Direction,
) -> Result<(FrameCipher, FrameCipher), CryptoError> {
    let local_info: &[u8] = match local_direction {
        Direction::InitiatorToResponder => b"mezots/v1/initiator-tx",
        Direction::ResponderToInitiator => b"mezots/v1/responder-tx",
    };
    let peer_info: &[u8] = match peer_direction {
        Direction::InitiatorToResponder => b"mezots/v1/initiator-tx",
        Direction::ResponderToInitiator => b"mezots/v1/responder-tx",
    };

    let tx_key = derive_key(shared, transcript, local_info)?;
    let rx_key = derive_key(shared, transcript, peer_info)?;

    let session = session_id(transcript);

    let tx = FrameCipher::new(
        tx_key,
        [local_nonce[0], local_nonce[1], local_nonce[2], local_nonce[3]],
        local_direction,
        session,
    );
    let rx = FrameCipher::new(
        rx_key,
        [peer_nonce[0], peer_nonce[1], peer_nonce[2], peer_nonce[3]],
        peer_direction,
        session,
    );
    Ok((tx, rx))
}

/// HKDF-SHA256 key derivation.
fn derive_key(ikm: &[u8; 32], salt: &[u8; 32], info: &[u8]) -> Result<[u8; 32], CryptoError> {
    let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
    let mut okm = [0u8; 32];
    hk.expand(info, &mut okm)
        .map_err(|e| CryptoError::KeyExchange(format!("hkdf failed: {e}")))?;
    Ok(okm)
}

/// Write a length-prefixed handshake payload.
async fn write_payload<S>(stream: &mut S, payload: &[u8]) -> Result<(), CryptoError>
where
    S: AsyncWrite + Unpin,
{
    if payload.len() > MAX_HANDSHAKE_FRAME {
        return Err(CryptoError::Unexpected("handshake frame too large".into()));
    }
    stream.write_all(&(payload.len() as u32).to_be_bytes()).await?;
    stream.write_all(payload).await?;
    stream.flush().await?;
    Ok(())
}

/// Read a length-prefixed handshake payload.
async fn read_payload<S>(stream: &mut S) -> Result<(HandshakeFrame, Vec<u8>), CryptoError>
where
    S: AsyncRead + Unpin,
{
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_HANDSHAKE_FRAME {
        return Err(CryptoError::Unexpected("handshake frame too large".into()));
    }
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).await?;
    let frame: HandshakeFrame = postcard::from_bytes(&payload)?;
    Ok((frame, payload))
}
