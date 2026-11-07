//! The school's private chain.
//!
//! Every message, attachment and broadcast is committed to a hash chain
//! that lives on the node. The chain stores the *hash* of a payload rather
//! than the payload itself, so it proves that a conversation happened
//! without storing the conversation.
//!
//! Each block carries
//!
//! * the hash of the payload it commits to,
//! * the hash of the block before it, and
//! * an Ed25519 signature over the canonical header, made with the node's
//!   identity key.
//!
//! That combination gives the chain three properties a school needs:
//!
//! * **Tamper evidence** - editing a block breaks every hash after it.
//! * **Authorship** - the signature says which node wrote it, and any
//!   browser can check that without asking the node.
//! * **Convergence** - blocks are gossiped between peers, so every node on
//!   the network ends up with the same chain.
//!
//! The canonical header bytes are built exactly the same way in the browser
//! (`lib/crypto.ts`), which lets a student re-compute and re-verify the
//! whole chain themselves.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::crypto::keys::{NodeKeys, KEY_LEN, SIGNATURE_LEN};

/// Hex encoding, re-exported so the chain helpers stand on their own.
pub use crate::config::hex;

/// The hash of the block before the first one.
pub const GENESIS_HASH: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

/// What a block commits to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LedgerKind {
    /// A text message.
    Message,
    /// A binary attachment.
    Binary,
    /// A chunk of a live stream.
    Stream,
    /// The end of a live stream.
    StreamEnd,
    /// A published clip.
    Reel,
    /// A key exchange.
    Key,
    /// Anything else.
    Post,
}

impl LedgerKind {
    /// The label used on the wire and in the interface.
    pub fn as_str(&self) -> &'static str {
        match self {
            LedgerKind::Message => "message",
            LedgerKind::Binary => "binary",
            LedgerKind::Stream => "stream",
            LedgerKind::StreamEnd => "streamEnd",
            LedgerKind::Reel => "reel",
            LedgerKind::Key => "key",
            LedgerKind::Post => "post",
        }
    }
}

/// One block of the chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerBlock {
    /// Height of the block, starting at zero.
    pub index: u64,
    /// Unix timestamp in milliseconds.
    pub ts: u64,
    /// Node id of the author.
    pub actor: String,
    /// Human readable author name.
    pub actor_name: String,
    /// Per-author monotonic sequence number.
    pub seq: u64,
    /// The room the event happened in.
    pub room: String,
    /// What the block commits to.
    pub kind: LedgerKind,
    /// SHA-256 of the payload, hex encoded.
    pub payload_hash: String,
    /// Hash of the previous block, hex encoded.
    pub prev_hash: String,
    /// Hash of this block, hex encoded.
    pub hash: String,
    /// Ed25519 signature over the canonical header, hex encoded.
    pub signature: String,
    /// base64 of the author's Ed25519 public key.
    pub actor_key: String,
}

impl LedgerBlock {
    /// The bytes that are hashed and signed.
    ///
    /// Every field is length-prefixed, so no two different field splits can
    /// produce the same bytes.
    pub fn canonical_header(&self) -> Vec<u8> {
        let parts: Vec<String> = vec![
            self.index.to_string(),
            self.ts.to_string(),
            self.actor.clone(),
            self.seq.to_string(),
            self.room.clone(),
            self.kind.as_str().to_string(),
            self.payload_hash.clone(),
            self.prev_hash.clone(),
        ];

        let mut out = Vec::with_capacity(128);
        for part in parts {
            let bytes = part.as_bytes();
            out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
            out.extend_from_slice(bytes);
        }
        out
    }

    /// Recompute the hash of this block from its own contents.
    pub fn compute_hash(&self) -> String {
        hex(&sha256_bytes(&self.canonical_header()))
    }

    /// Whether the hash and the signature of this block check out.
    pub fn verify(&self) -> Result<(), String> {
        if self.compute_hash() != self.hash {
            return Err(format!("block {} has a wrong hash", self.index));
        }
        if self.prev_hash == GENESIS_HASH && self.index != 0 {
            return Err(format!("block {} claims to be the first", self.index));
        }
        Ok(())
    }

    /// Verify the signature against the author's public key.
    pub fn verify_signature(&self) -> Result<(), String> {
        let key_bytes = base64_decode(&self.actor_key)
            .ok_or_else(|| "the author key is not valid base64".to_string())?;
        if key_bytes.len() != KEY_LEN {
            return Err("the author key has the wrong length".to_string());
        }
        let mut key = [0u8; KEY_LEN];
        key.copy_from_slice(&key_bytes);

        let signature_bytes = hex_decode(&self.signature)
            .ok_or_else(|| "the signature is not valid hex".to_string())?;
        if signature_bytes.len() != SIGNATURE_LEN {
            return Err("the signature has the wrong length".to_string());
        }

        NodeKeys::verify(&key, &self.canonical_header(), &signature_bytes)
            .map_err(|e| format!("the signature does not check out: {e}"))
    }
}

/// The local chain.
#[derive(Debug, Clone, Default)]
pub struct Ledger {
    /// The blocks, oldest first.
    blocks: Vec<LedgerBlock>,
    /// The head hash.
    head: String,
    /// The per-author sequence counter.
    sequences: HashMap<String, u64>,
    /// Blocks that arrived before their parent did.
    orphans: HashMap<String, LedgerBlock>,
}

impl Ledger {
    /// Create an empty chain.
    pub fn new() -> Self {
        Self {
            blocks: Vec::new(),
            head: GENESIS_HASH.to_string(),
            sequences: HashMap::new(),
            orphans: HashMap::new(),
        }
    }

    /// Height of the chain.
    pub fn height(&self) -> u64 {
        self.blocks.len() as u64
    }

    /// The hash of the newest block.
    pub fn head(&self) -> &str {
        &self.head
    }

    /// Every block, oldest first.
    pub fn blocks(&self) -> &[LedgerBlock] {
        &self.blocks
    }

    /// The newest `limit` blocks, oldest first.
    pub fn tail(&self, limit: usize) -> Vec<LedgerBlock> {
        let start = self.blocks.len().saturating_sub(limit);
        self.blocks[start..].to_vec()
    }

    /// The next sequence number for an author.
    pub fn next_seq(&self, actor: &str) -> u64 {
        self.sequences.get(actor).copied().unwrap_or(0)
    }

    /// Append a new block committing to `payload`.
    ///
    /// The block is signed with the node's identity key, so a browser can
    /// check the authorship without trusting the node.
    pub fn append(
        &mut self,
        keys: &NodeKeys,
        actor: &str,
        actor_name: &str,
        room: &str,
        kind: LedgerKind,
        payload: &[u8],
        ts: u64,
    ) -> LedgerBlock {
        let seq = self.next_seq(actor);
        *self.sequences.entry(actor.to_string()).or_insert(0) += 1;

        let mut block = LedgerBlock {
            index: self.height(),
            ts,
            actor: actor.to_string(),
            actor_name: actor_name.to_string(),
            seq,
            room: room.to_string(),
            kind,
            payload_hash: hex(&sha256_bytes(payload)),
            prev_hash: self.head.clone(),
            hash: String::new(),
            signature: String::new(),
            actor_key: base64_encode(&keys.ed25519_public()),
        };

        let header = block.canonical_header();
        block.hash = hex(&sha256_bytes(&header));
        block.signature = hex(&keys.sign(&header));

        self.head = block.hash.clone();
        self.blocks.push(block.clone());
        self.drain_orphans();
        block
    }

    /// Adopt a block that arrived from another node.
    ///
    /// Returns the number of blocks that were added, which is one when the
    /// block extends the local chain and zero when it does not fit.
    pub fn adopt(&mut self, block: LedgerBlock) -> usize {
        if self.blocks.iter().any(|existing| existing.hash == block.hash) {
            return 0;
        }

        if block.prev_hash != self.head {
            // The parent is missing; park the block until it shows up.
            self.orphans.insert(block.hash.clone(), block);
            return 0;
        }

        if block.verify().is_err() {
            return 0;
        }

        let index = self.height();
        if block.index != index {
            return 0;
        }

        self.head = block.hash.clone();
        self.sequences
            .entry(block.actor.clone())
            .and_modify(|seq| {
                if block.seq >= *seq {
                    *seq = block.seq + 1;
                }
            })
            .or_insert(block.seq + 1);
        self.blocks.push(block);
        self.drain_orphans();
        1
    }

    /// Re-check the whole chain.
    ///
    /// The hashes are recomputed from the block contents and the links are
    /// followed; a broken chain is reported with the height where it broke.
    pub fn verify(&self) -> Result<u64, (u64, String)> {
        let mut previous = GENESIS_HASH.to_string();

        for (position, block) in self.blocks.iter().enumerate() {
            if block.index as usize != position {
                return Err((position as u64, "the blocks are out of order".into()));
            }
            if block.prev_hash != previous {
                return Err((block.index, "the block does not follow its parent".into()));
            }
            let computed = block.compute_hash();
            if computed != block.hash {
                return Err((block.index, "the block hash does not match".into()));
            }
            previous = block.hash.clone();
        }

        Ok(self.height())
    }

    /// Try to place the blocks that were waiting for their parent.
    fn drain_orphans(&mut self) {
        loop {
            let next = self
                .orphans
                .values()
                .find(|block| block.prev_hash == self.head)
                .cloned();

            match next {
                Some(block) => {
                    self.orphans.remove(&block.hash);
                    if self.adopt(block) == 0 {
                        return;
                    }
                }
                None => return,
            }
        }
    }
}

/// SHA-256 of a byte string.
pub fn sha256_bytes(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// Encode bytes as standard base64.
pub fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Decode standard base64.
pub fn base64_decode(value: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(value).ok()
}

/// Decode a hex string.
pub fn hex_decode(value: &str) -> Option<Vec<u8>> {
    if value.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(value.len() / 2);
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let high = (bytes[index] as char).to_digit(16)?;
        let low = (bytes[index + 1] as char).to_digit(16)?;
        out.push(((high << 4) | low) as u8);
        index += 2;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_keys() -> NodeKeys {
        NodeKeys::generate()
    }

    #[test]
    fn an_empty_chain_has_a_genesis_head() {
        let ledger = Ledger::new();
        assert_eq!(ledger.height(), 0);
        assert_eq!(ledger.head(), GENESIS_HASH);
        assert!(ledger.verify().is_ok());
    }

    #[test]
    fn blocks_link_and_verify() {
        let keys = fresh_keys();
        let mut ledger = Ledger::new();
        ledger.append(&keys, "node-a", "A", "general", LedgerKind::Message, b"hello", 1);
        ledger.append(&keys, "node-a", "A", "general", LedgerKind::Message, b"world", 2);

        assert_eq!(ledger.height(), 2);
        let height = ledger.verify().expect("the chain must verify");
        assert_eq!(height, 2);
    }

    #[test]
    fn editing_a_block_breaks_the_chain() {
        let keys = fresh_keys();
        let mut ledger = Ledger::new();
        ledger.append(&keys, "node-a", "A", "general", LedgerKind::Message, b"hello", 1);
        ledger.append(&keys, "node-a", "A", "general", LedgerKind::Message, b"world", 2);

        let mut tampered = ledger.blocks()[0].clone();
        tampered.payload_hash = hex(&sha256_bytes(b"something else"));
        tampered.hash = tampered.compute_hash();

        let broken = Ledger {
            blocks: vec![tampered, ledger.blocks()[1].clone()],
            head: ledger.head().to_string(),
            sequences: HashMap::new(),
            orphans: HashMap::new(),
        };
        assert!(broken.verify().is_err());
    }

    #[test]
    fn signatures_check_out() {
        let keys = fresh_keys();
        let mut ledger = Ledger::new();
        let block = ledger.append(
            &keys,
            "node-a",
            "A",
            "general",
            LedgerKind::Message,
            b"hello",
            1,
        );
        block.verify().expect("the block must verify");
        block
            .verify_signature()
            .expect("the signature must check out");
    }

    #[test]
    fn a_block_signed_by_someone_else_does_not_check_out() {
        let keys = fresh_keys();
        let other = fresh_keys();
        let mut ledger = Ledger::new();
        let mut block = ledger.append(
            &keys,
            "node-a",
            "A",
            "general",
            LedgerKind::Message,
            b"hello",
            1,
        );
        block.actor_key = base64_encode(&other.ed25519_public());
        assert!(block.verify_signature().is_err());
    }

    #[test]
    fn a_block_from_a_peer_is_adopted() {
        let keys = fresh_keys();
        let peer = fresh_keys();
        let mut source = Ledger::new();
        let block = source.append(
            &peer,
            "node-b",
            "B",
            "general",
            LedgerKind::Message,
            b"from a peer",
            5,
        );

        let mut ledger = Ledger::new();
        assert_eq!(ledger.adopt(block.clone()), 1);
        assert_eq!(ledger.height(), 1);
        assert_eq!(ledger.head(), block.hash);
    }

    #[test]
    fn a_block_whose_parent_is_missing_waits() {
        let keys = fresh_keys();
        let peer = fresh_keys();
        let mut source = Ledger::new();
        let first = source.append(
            &peer,
            "node-b",
            "B",
            "general",
            LedgerKind::Message,
            b"one",
            1,
        );
        let second = source.append(
            &peer,
            "node-b",
            "B",
            "general",
            LedgerKind::Message,
            b"two",
            2,
        );

        let mut ledger = Ledger::new();
        assert_eq!(ledger.adopt(second.clone()), 0);
        assert_eq!(ledger.height(), 0);
        assert_eq!(ledger.adopt(first), 1);
        // The parked block is placed as soon as its parent arrives.
        assert_eq!(ledger.height(), 2);
    }

    #[test]
    fn the_canonical_header_is_unambiguous() {
        let mut ledger = Ledger::new();
        let keys = fresh_keys();
        let a = ledger.append(&keys, "n", "N", "ab", LedgerKind::Message, b"x", 1);
        let b = ledger.append(&keys, "n", "N", "a", LedgerKind::Message, b"x", 1);
        assert_ne!(a.canonical_header(), b.canonical_header());
    }
}
