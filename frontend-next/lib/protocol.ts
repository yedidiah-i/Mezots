/**
 * The wire contract between a browser and a Mezots node.
 *
 * This file is the single source of truth for the browser side. The Rust
 * counterpart lives in `src/transport/websocket.rs` and must stay in sync
 * with the types below - both sides use the same camelCase JSON names
 * because the Rust enums are annotated with `#[serde(rename_all = "camelCase")]`.
 *
 * Nothing in here imports anything: the transport is a plain `WebSocket` and
 * the cryptography is the browser's own WebCrypto API. That is deliberate -
 * the stack a school deploys must not need a third-party runtime.
 */

/** How a peer was found on the network. */
export type DiscoverySource = "mdns" | "beacon" | "manual";

/** The lifecycle of a peer as tracked by the node. */
export type PeerState =
  | "discovered"
  | "connecting"
  | "handshaking"
  | "connected"
  | "unhealthy"
  | "disconnected";

/** A peer as seen by the browser. */
export interface PeerSummary {
  peerId: string;
  displayName: string;
  address: string | null;
  state: PeerState;
  source: DiscoverySource;
  connected: boolean;
}

/** What kind of event a ledger block commits to. */
export type LedgerKind =
  | "message"
  | "binary"
  | "stream"
  | "streamEnd"
  | "reel"
  | "key"
  | "post";

/**
 * One block of the school's private chain.
 *
 * A block commits to the *hash* of a payload, never to the payload itself.
 * That is what makes the log useful for a school: a member can prove that a
 * specific message existed at a specific height without the chain having to
 * store the conversation.
 */
export interface LedgerBlock {
  /** Height of the block, starting at zero. */
  index: number;
  /** Unix timestamp in milliseconds. */
  ts: number;
  /** Node id of the author. */
  actor: string;
  /** Human readable author name. */
  actorName: string;
  /** Per-author monotonic sequence number. */
  seq: number;
  /** Room the event happened in. */
  room: string;
  /** What the block commits to. */
  kind: LedgerKind;
  /** SHA-256 of the payload, hex encoded. */
  payloadHash: string;
  /** Hash of the previous block, hex encoded. */
  prevHash: string;
  /** Hash of this block, hex encoded. */
  hash: string;
  /** Ed25519 signature over the canonical header, hex encoded. */
  signature: string;
  /** base64 of the author's Ed25519 public key, so any browser can verify. */
  actorKey: string;
}

/** The state of the local view of the chain. */
export interface LedgerStatus {
  /** Height of the chain head. */
  height: number;
  /** Whether the local chain verifies end to end. */
  valid: boolean;
  /** Hash of the chain head. */
  head: string;
  /** How many peers agree on the head hash. */
  peersAgree: number;
  /** How many peers are connected. */
  peersTotal: number;
}

/** Live counters about what the node is doing. */
export interface NodeMetrics {
  /** Bytes relayed between peers on the local network. */
  bytesRelayed: number;
  /** Bytes that had to leave the local network (uplink). */
  bytesInternet: number;
  /** Number of connected peers. */
  peersOnline: number;
  /** How long the node has been running. */
  uptimeMs: number;
  /** Number of blocks in the local chain. */
  ledgerHeight: number;
}

/** A room as the node knows it. */
export interface RoomState {
  /** Stable room id, for example `general` or `dm:node-b`. */
  id: string;
  /** Display name. */
  title: string;
  /** What kind of room it is. */
  kind: "channel" | "direct" | "stream" | "reels";
  /** Members that are currently present. */
  members: string[];
  /** Whether the room is encrypted end to end for direct messages. */
  e2e: boolean;
}

/** A message delivered into a room. */
export interface RoomMessage {
  room: string;
  peerId: string;
  author: string;
  text: string;
  ts: number;
  /** Correlation id set by the sender so the UI can reconcile optimistically. */
  clientMsgId?: string;
  /** Hash of the ledger block that committed to this message. */
  blockHash?: string;
  /** Height of the committing block. */
  height?: number;
  /** True when the payload was encrypted in the browser before sending. */
  encrypted?: boolean;
}

/** A binary attachment delivered into a room. */
export interface RoomBinary {
  room: string;
  peerId: string;
  author: string;
  mime: string;
  payloadBase64: string;
  ts: number;
  clientMsgId?: string;
  blockHash?: string;
}

/** Metadata about a live stream. */
export interface StreamMeta {
  room: string;
  streamId: number;
  title: string;
  mime: string;
  publisher: string;
  publisherPeerId: string;
  viewers: number;
}

/** One chunk of a live stream. */
export interface StreamChunkEvent {
  room: string;
  streamId: number;
  seq: number;
  payloadBase64: string;
  publisherPeerId: string;
}

/** A published reel. */
export interface Reel {
  id: string;
  title: string;
  author: string;
  authorPeerId: string;
  mime: string;
  /** URL of the clip, served by the node itself. */
  url: string;
  ts: number;
  durationMs: number;
  /** How many students liked it. */
  likes: number;
  /** How many comments it has. */
  comments: number;
  /** Whether the reel was committed to the chain. */
  blockHash?: string;
}

/** Identity of the node this browser is attached to. */
export interface NodeIdentity {
  nodeId: string;
  displayName: string;
  /** SHA-256 fingerprint of the node's public identity key. */
  fingerprint: string;
  /** Base64 of the node's public identity key. */
  publicKey: string;
}

/* -------------------------------------------------------------------------- */
/* Browser -> node                                                            */
/* -------------------------------------------------------------------------- */

/** A command sent to the node. */
export type WsCommand =
  /** Ask for the current peer table. */
  | { type: "listPeers" }
  /** Send a text message to one peer (legacy, room-less). */
  | { type: "sendText"; peerId: string; text: string }
  /** Send a binary payload to one peer (legacy, room-less). */
  | {
      type: "sendBinary";
      peerId: string;
      mime: string;
      payloadBase64: string;
    }
  /** Subscribe the browser to a room. */
  | { type: "join"; room: string }
  /** Unsubscribe the browser from a room. */
  | { type: "leave"; room: string }
  /** Send a text message into a room. */
  | {
      type: "sendRoomText";
      room: string;
      text: string;
      clientMsgId?: string;
    }
  /** Send an attachment into a room. */
  | {
      type: "sendRoomBinary";
      room: string;
      mime: string;
      payloadBase64: string;
      clientMsgId?: string;
    }
  /** Announce a live stream. */
  | {
      type: "streamStart";
      room: string;
      title: string;
      mime: string;
    }
  /** Push one chunk of a live stream. */
  | {
      type: "streamChunk";
      room: string;
      streamId: number;
      payloadBase64: string;
    }
  /** Close a live stream. */
  | { type: "streamEnd"; room: string; streamId: number }
  /** Ask for the most recent blocks. */
  | { type: "ledgerSnapshot"; limit?: number }
  /** Ask the node to re-verify its chain. */
  | { type: "ledgerVerify" }
  /** Ask for the list of published reels. */
  | { type: "listReels" }
  /** Store a clip so it shows up in the reels feed. */
  | {
      type: "storeReel";
      title: string;
      mime: string;
      payloadBase64: string;
    };

/* -------------------------------------------------------------------------- */
/* Node -> browser                                                            */
/* -------------------------------------------------------------------------- */

/** An event pushed by the node. */
export type WsEvent =
  | { type: "peerDiscovered"; peer: PeerSummary }
  | { type: "peerConnected"; peerId: string }
  | { type: "peerDisconnected"; peerId: string; reason: string }
  | { type: "message"; peerId: string; text: string }
  | {
      type: "binary";
      peerId: string;
      mime: string;
      payloadBase64: string;
    }
  | {
      type: "streamChunk";
      peerId: string;
      streamId: number;
      seq: number;
      payloadBase64: string;
    }
  | { type: "peers"; peers: PeerSummary[] }
  | { type: "error"; message: string }
  | { type: "identity"; identity: NodeIdentity }
  | { type: "roomMessage"; message: RoomMessage }
  | { type: "roomBinary"; message: RoomBinary }
  | { type: "roomState"; rooms: RoomState[] }
  | { type: "streamMeta"; stream: StreamMeta }
  | { type: "roomStreamChunk"; chunk: StreamChunkEvent }
  | { type: "roomStreamEnd"; room: string; streamId: number }
  | { type: "ledgerBlock"; block: LedgerBlock }
  | { type: "ledgerStatus"; status: LedgerStatus }
  | { type: "ledgerSnapshot"; blocks: LedgerBlock[] }
  | { type: "metrics"; metrics: NodeMetrics }
  | { type: "reels"; reels: Reel[] };

/** Every event type the node can push. */
export type WsEventType = WsEvent["type"];

/** Narrow an event union to a single member. */
export type WsEventOf<T extends WsEventType> = Extract<WsEvent, { type: T }>;

/** A listener for one kind of event. */
export type EventListener<T extends WsEventType> = (
  event: WsEventOf<T>,
) => void;
