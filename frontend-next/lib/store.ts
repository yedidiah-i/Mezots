/**
 * The application store.
 *
 * One observable object holds everything the interface needs. React reads it
 * through {@link useAppState}, which subscribes with `useSyncExternalStore` so
 * that the socket can push updates without a render loop.
 *
 * The store is deliberately framework-free: it knows about the protocol and
 * about nothing else. If a message arrives while React is busy, the store
 * keeps it and React picks it up on the next frame.
 */

import { useRef, useSyncExternalStore } from "react";

import { MezotsClient, resolveNodeUrl, type ConnectionState } from "./client";
import type {
  LedgerBlock,
  LedgerStatus,
  NodeIdentity,
  NodeMetrics,
  PeerSummary,
  Reel,
  RoomBinary,
  RoomMessage,
  RoomState,
  StreamChunkEvent,
  StreamMeta,
  WsCommand,
} from "./protocol";
import { loadIdentity, type Identity } from "./identity";
import { closeStream, pushChunk } from "./stream";
import {
  decryptBody,
  encryptBody,
  ensureConversation,
  handleIncomingAttachment,
} from "./dm";

/** A message as the interface sees it, including local-only fields. */
export interface ChatMessage {
  /** Stable id inside the room. */
  id: string;
  room: string;
  /** Node id of the author. */
  peerId: string;
  /** Display name of the author. */
  author: string;
  text: string;
  ts: number;
  /** Correlation id used to reconcile the optimistic copy with the real one. */
  clientMsgId?: string;
  /** Hash of the ledger block that committed to this message. */
  blockHash?: string;
  /** Height of that block. */
  height?: number;
  /** True while the message is waiting for the node to acknowledge it. */
  pending?: boolean;
  /** Set when the message could not be delivered. */
  failed?: string;
  /** True when the body was encrypted in the browser. */
  encrypted?: boolean;
  /** True for the local copy the browser rendered optimistically. */
  optimistic?: boolean;
  /** Reactions, keyed by emoji. */
  reactions: Record<string, string[]>;
  /** Number of replies in the thread. */
  replyCount: number;
}

/** A live stream the browser knows about. */
export interface LiveStream {
  room: string;
  streamId: number;
  title: string;
  mime: string;
  publisher: string;
  publisherPeerId: string;
  viewers: number;
  /** True when this browser is publishing. */
  isLocal: boolean;
  startedAt: number;
}

/** The kind of room a given room id is. */
export type RoomKind = "channel" | "direct" | "stream" | "reels";

/** Everything the interface reads. */
export interface AppState {
  /** Connection state of the socket to the node. */
  connection: ConnectionState;
  /** Why the connection dropped, when it did. */
  connectionDetail: string | null;
  /** The node this browser is attached to. */
  node: NodeIdentity | null;
  /** The local browser's identity. */
  identity: Identity | null;
  /** Known peers. */
  peers: PeerSummary[];
  /** Rooms the node knows about. */
  rooms: RoomState[];
  /** The room the interface is showing. */
  activeRoom: string;
  /** Messages, keyed by room id. */
  messages: Record<string, ChatMessage[]>;
  /** Attachments, keyed by room id. */
  attachments: Record<string, RoomBinary[]>;
  /** The chain, newest first. */
  ledger: LedgerBlock[];
  /** Chain health. */
  ledgerStatus: LedgerStatus | null;
  /** Node counters. */
  metrics: NodeMetrics | null;
  /** Live streams, keyed by room id. */
  streams: Record<string, LiveStream>;
  /** Published reels. */
  reels: Reel[];
  /** The last error the node reported. */
  lastError: string | null;
  /** How many commands are waiting for the socket. */
  pendingCommands: number;
}

/** The rooms every workspace starts with. */
export const DEFAULT_CHANNELS: Array<{
  id: string;
  title: string;
  kind: RoomKind;
}> = [
  { id: "general", title: "general", kind: "channel" },
  { id: "announcements", title: "announcements", kind: "channel" },
  { id: "maths-101", title: "maths-101", kind: "channel" },
  { id: "science-201", title: "science-201", kind: "channel" },
  { id: "library", title: "library", kind: "channel" },
  { id: "reels", title: "reels", kind: "reels" },
];

const INITIAL_STATE: AppState = {
  connection: "idle",
  connectionDetail: null,
  node: null,
  identity: null,
  peers: [],
  rooms: [],
  activeRoom: "general",
  messages: {},
  attachments: {},
  ledger: [],
  ledgerStatus: null,
  metrics: null,
  streams: {},
  reels: [],
  lastError: null,
  pendingCommands: 0,
};

/** A minimal observable store. */
class Store {
  private state: AppState = INITIAL_STATE;
  private listeners = new Set<() => void>();

  getState = (): AppState => this.state;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  setState = (updater: (previous: AppState) => AppState): void => {
    const next = updater(this.state);
    if (next === this.state) return;
    this.state = next;
    for (const listener of this.listeners) listener();
  };
}

/** The single application store. */
export const store = new Store();

/**
 * Read a slice of the store.
 *
 * The value is cached between renders, so a component that selects a stable
 * reference (an array that did not change) does not churn.
 */
export function useAppState<T>(
  selector: (state: AppState) => T,
  isEqual: (a: T, b: T) => boolean = Object.is,
): T {
  const state = useSyncExternalStore(store.subscribe, store.getState, store.getState);
  const selectorRef = useRef(selector);
  selectorRef.current = selector;
  const cache = useRef<{ value: T; primed: boolean }>({
    value: undefined as never,
    primed: false,
  });
  const next = selectorRef.current(state);
  if (!cache.current.primed || !isEqual(cache.current.value, next)) {
    cache.current = { value: next, primed: true };
  }
  return cache.current.value;
}

/* -------------------------------------------------------------------------- */
/* Helpers                                                                     */
/* -------------------------------------------------------------------------- */

/** Compare two message arrays by identity of their members. */
function sameMessages(a: ChatMessage[], b: ChatMessage[]): boolean {
  if (a === b) return true;
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i += 1) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}

/** Push a message into a room, replacing an optimistic copy if there is one. */
function mergeMessage(
  list: ChatMessage[] | undefined,
  message: ChatMessage,
): ChatMessage[] {
  const current = list ?? [];
  const optimisticIndex = message.clientMsgId
    ? current.findIndex(
        (item) =>
          item.optimistic &&
          item.clientMsgId === message.clientMsgId &&
          item.peerId === message.peerId,
      )
    : -1;

  if (optimisticIndex >= 0) {
    const copy = [...current];
    copy[optimisticIndex] = { ...message, optimistic: false, pending: false };
    return copy;
  }

  // Never show the same message twice.
  const duplicate = current.findIndex(
    (item) =>
      message.clientMsgId !== undefined &&
      item.clientMsgId === message.clientMsgId &&
      item.peerId === message.peerId,
  );
  if (duplicate >= 0) {
    const copy = [...current];
    copy[duplicate] = { ...message, optimistic: false, pending: false };
    return copy;
  }

  return [...current, message];
}

/**
 * The room id used for a direct conversation with a peer.
 *
 * The node names a private conversation after both nodes, sorted, so that
 * the two sides and every browser attached to them agree on one name.
 */
export function directRoom(peerId: string, nodeId?: string | null): string {
  if (!nodeId) return `dm:${peerId}`;
  const pair = [nodeId, peerId].sort();
  return `dm:${pair[0]}:${pair[1]}`;
}

/**
 * The peer a direct room belongs to.
 *
 * Returns `null` when the room is not a private conversation or when the
 * room names two peers and we are neither of them.
 */
export function peerFromRoom(
  room: string,
  nodeId?: string | null,
): string | null {
  if (!room.startsWith("dm:")) return null;
  const parts = room.slice(3).split(":").filter(Boolean);
  if (parts.length === 1) return parts[0];
  if (parts.length === 2) {
    if (!nodeId) return parts[0];
    return parts.find((part) => part !== nodeId) ?? null;
  }
  return null;
}

/* -------------------------------------------------------------------------- */
/* Wiring                                                                     */
/* -------------------------------------------------------------------------- */

let client: MezotsClient | null = null;

/** The socket, once it exists. */
export function getClient(): MezotsClient | null {
  return client;
}

/** Start the app: load the identity, open the socket, wire the events. */
export async function bootstrap(): Promise<void> {
  const identity = await loadIdentity();
  store.setState((previous) => ({ ...previous, identity }));

  const socket = new MezotsClient(resolveNodeUrl());
  client = socket;

  socket.onState((state, detail) => {
    store.setState((previous) => ({
      ...previous,
      connection: state,
      connectionDetail: detail ?? null,
      pendingCommands: socket.pendingCount,
    }));
  });

  socket.on("identity", (event) => {
    store.setState((previous) => ({ ...previous, node: event.identity }));
  });

  socket.on("peers", (event) => {
    store.setState((previous) => ({ ...previous, peers: event.peers }));
  });

  socket.on("peerDiscovered", (event) => {
    store.setState((previous) => ({
      ...previous,
      peers: upsertPeer(previous.peers, event.peer),
    }));
  });

  socket.on("peerConnected", (event) => {
    store.setState((previous) => ({
      ...previous,
      peers: previous.peers.map((peer) =>
        peer.peerId === event.peerId
          ? { ...peer, connected: true, state: "connected" }
          : peer,
      ),
    }));
  });

  socket.on("peerDisconnected", (event) => {
    store.setState((previous) => ({
      ...previous,
      peers: previous.peers.map((peer) =>
        peer.peerId === event.peerId
          ? { ...peer, connected: false, state: "disconnected" }
          : peer,
      ),
    }));
  });

  socket.on("roomMessage", (event) => {
    const message = event.message;
    void (async () => {
      let text = message.text;
      let encrypted = message.encrypted ?? false;

      if (message.room.startsWith("dm:")) {
        const peerId = message.room.slice(3);
        const opened = await decryptBody(peerId, text);
        text = opened.body;
        encrypted = opened.encrypted;
      }

      store.setState((previous) => ({
        ...previous,
        messages: {
          ...previous.messages,
          [message.room]: mergeMessage(previous.messages[message.room], {
            id: `${message.peerId}:${message.clientMsgId ?? message.ts}:${message.blockHash ?? message.text.length}`,
            room: message.room,
            peerId: message.peerId,
            author: message.author || message.peerId,
            text,
            ts: message.ts,
            clientMsgId: message.clientMsgId,
            blockHash: message.blockHash,
            height: message.height,
            encrypted,
            reactions: {},
            replyCount: 0,
          }),
        },
      }));
    })();
  });

  socket.on("roomBinary", (event) => {
    const message = event.message;

    if (message.room.startsWith("dm:")) {
      const peerId = message.room.slice(3);
      const identity = store.getState().identity;
      if (identity) {
        void handleIncomingAttachment(
          peerId,
          message.room,
          message.mime,
          message.payloadBase64,
          identity,
          (command) => sendCommand(command),
        ).then((handled) => {
          if (handled) return;
          store.setState((previous) => ({
            ...previous,
            attachments: {
              ...previous.attachments,
              [message.room]: [
                ...(previous.attachments[message.room] ?? []),
                message,
              ],
            },
          }));
        });
        return;
      }
    }

    store.setState((previous) => ({
      ...previous,
      attachments: {
        ...previous.attachments,
        [message.room]: [
          ...(previous.attachments[message.room] ?? []),
          message,
        ],
      },
    }));
  });

  socket.on("roomState", (event) => {
    store.setState((previous) => ({ ...previous, rooms: event.rooms }));
  });

  socket.on("streamMeta", (event) => {
    const stream = event.stream;
    store.setState((previous) => ({
      ...previous,
      streams: {
        ...previous.streams,
        [stream.room]: {
          room: stream.room,
          streamId: stream.streamId,
          title: stream.title,
          mime: stream.mime,
          publisher: stream.publisher,
          publisherPeerId: stream.publisherPeerId,
          viewers: stream.viewers,
          isLocal: false,
          startedAt: Date.now(),
        },
      },
    }));
  });

  socket.on("roomStreamChunk", (event) => {
    const chunk = event.chunk;
    pushChunk(chunk.room, chunk.streamId, chunk.payloadBase64);
  });

  socket.on("roomStreamEnd", (event) => {
    closeStream(event.room);
  });

  socket.on("ledgerBlock", (event) => {
    store.setState((previous) => ({
      ...previous,
      ledger: [event.block, ...previous.ledger.filter((b) => b.hash !== event.block.hash)].slice(0, 500),
    }));
  });

  socket.on("ledgerSnapshot", (event) => {
    store.setState((previous) => ({
      ...previous,
      ledger: [...event.blocks].reverse().slice(0, 500),
    }));
  });

  socket.on("ledgerStatus", (event) => {
    store.setState((previous) => ({ ...previous, ledgerStatus: event.status }));
  });

  socket.on("metrics", (event) => {
    store.setState((previous) => ({ ...previous, metrics: event.metrics }));
  });

  socket.on("reels", (event) => {
    store.setState((previous) => ({ ...previous, reels: event.reels }));
  });

  socket.on("error", (event) => {
    store.setState((previous) => ({ ...previous, lastError: event.message }));
  });

  socket.connect();
}

/** Insert or update a peer in the table. */
function upsertPeer(peers: PeerSummary[], peer: PeerSummary): PeerSummary[] {
  const index = peers.findIndex((item) => item.peerId === peer.peerId);
  if (index === -1) return [...peers, peer];
  const copy = [...peers];
  copy[index] = { ...copy[index], ...peer };
  return copy;
}

/* -------------------------------------------------------------------------- */
/* Actions                                                                     */
/* -------------------------------------------------------------------------- */

/** Send a raw command to the node. */
export function sendCommand(command: WsCommand): boolean {
  if (!client) return false;
  const sent = client.send(command);
  store.setState((previous) => ({
    ...previous,
    pendingCommands: client?.pendingCount ?? previous.pendingCommands,
  }));
  return sent;
}

/** Switch to another room. */
export function setActiveRoom(room: string): void {
  store.setState((previous) => ({ ...previous, activeRoom: room }));
  sendCommand({ type: "join", room });

  // Opening a direct conversation starts the key exchange.
  const peerId = peerFromRoom(room, store.getState().node?.nodeId);
  if (peerId) {
    const identity = store.getState().identity;
    if (identity) {
      ensureConversation(peerId, identity, (command) => sendCommand(command));
    }
  }
}

/** Point the app at a different node. */
export function setNodeUrl(url: string): void {
  try {
    window.localStorage.setItem("mezots.nodeUrl", url);
  } catch {
    /* storage unavailable */
  }
  client?.setEndpoint(url);
}

/**
 * Send a text message into a room.
 *
 * The message is rendered immediately and reconciled when the node confirms
 * it, so typing never feels like it is waiting for the network. Direct
 * messages are encrypted in the browser first.
 */
export function sendText(room: string, text: string, encrypted = false): void {
  const trimmed = text.trim();
  if (!trimmed) return;

  const identity = store.getState().identity;
  const clientMsgId = `c-${Date.now().toString(36)}-${Math.random()
    .toString(36)
    .slice(2, 8)}`;
  const ts = Date.now();

  const isDirect = room.startsWith("dm:");
  const peerId = isDirect ? room.slice(3) : null;

  // The optimistic copy shows the plain text; the wire form is encrypted.
  const optimistic: ChatMessage = {
    id: clientMsgId,
    room,
    peerId: identity?.nodeId ?? "me",
    author: identity?.displayName ?? "You",
    text: trimmed,
    ts,
    clientMsgId,
    pending: true,
    encrypted,
    reactions: {},
    replyCount: 0,
  };

  store.setState((previous) => ({
    ...previous,
    messages: {
      ...previous.messages,
      [room]: mergeMessage(previous.messages[room], optimistic),
    },
  }));

  const deliver = async () => {
    let body = trimmed;
    let wasEncrypted = encrypted;
    if (peerId && identity) {
      const result = await encryptBody(peerId, trimmed);
      body = result.body;
      wasEncrypted = result.encrypted;
    }

    const sent = sendCommand({
      type: "sendRoomText",
      room,
      text: body,
      clientMsgId,
    });

    if (!sent) {
      store.setState((previous) => ({
        ...previous,
        messages: {
          ...previous.messages,
          [room]: (previous.messages[room] ?? []).map((message) =>
            message.clientMsgId === clientMsgId
              ? { ...message, pending: false, failed: "Waiting for the node" }
              : message,
          ),
        },
      }));
    } else if (wasEncrypted !== encrypted) {
      // Reflect the real protection level on the optimistic copy.
      store.setState((previous) => ({
        ...previous,
        messages: {
          ...previous.messages,
          [room]: (previous.messages[room] ?? []).map((message) =>
            message.clientMsgId === clientMsgId
              ? { ...message, encrypted: wasEncrypted }
              : message,
          ),
        },
      }));
    }
  };

  void deliver();
}

/** Send an attachment into a room. */
export function sendAttachment(
  room: string,
  mime: string,
  payloadBase64: string,
  name?: string,
): void {
  const identity = store.getState().identity;
  const clientMsgId = `b-${Date.now().toString(36)}-${Math.random()
    .toString(36)
    .slice(2, 8)}`;

  const binary: RoomBinary = {
    room,
    peerId: identity?.nodeId ?? "me",
    author: identity?.displayName ?? "You",
    mime,
    payloadBase64,
    ts: Date.now(),
    clientMsgId,
  };

  store.setState((previous) => ({
    ...previous,
    attachments: {
      ...previous.attachments,
      [room]: [...(previous.attachments[room] ?? []), binary],
    },
  }));

  sendCommand({
    type: "sendRoomBinary",
    room,
    mime: name ? `${mime};name=${encodeURIComponent(name)}` : mime,
    payloadBase64,
    clientMsgId,
  });
}

/** Mark a message as failed so the interface can offer a retry. */
export function markMessageFailed(room: string, clientMsgId: string, reason: string): void {
  store.setState((previous) => ({
    ...previous,
    messages: {
      ...previous.messages,
      [room]: (previous.messages[room] ?? []).map((message) =>
        message.clientMsgId === clientMsgId
          ? { ...message, pending: false, failed: reason }
          : message,
      ),
    },
  }));
}

/** Add a reaction to a message. */
export function toggleReaction(
  room: string,
  messageId: string,
  emoji: string,
): void {
  const identity = store.getState().identity;
  const me = identity?.nodeId ?? "me";
  store.setState((previous) => {
    const list = previous.messages[room] ?? [];
    let changed = false;
    const next = list.map((message) => {
      if (message.id !== messageId) return message;
      const reactions = { ...message.reactions };
      const users = reactions[emoji] ?? [];
      const updated = users.includes(me)
        ? users.filter((user) => user !== me)
        : [...users, me];
      if (updated.length === 0) delete reactions[emoji];
      else reactions[emoji] = updated;
      changed = true;
      return { ...message, reactions };
    });
    if (!changed) return previous;
    return { ...previous, messages: { ...previous.messages, [room]: next } };
  });
}

/** Ask the node to re-verify its chain. */
export function verifyChain(): void {
  sendCommand({ type: "ledgerVerify" });
}

/** Ask the node for the latest blocks. */
export function requestLedger(limit = 200): void {
  sendCommand({ type: "ledgerSnapshot", limit });
}

/** Ask the node for the reel list. */
export function requestReels(): void {
  sendCommand({ type: "listReels" });
}

/** Announce a live stream. */
export function announceStream(room: string, title: string, mime: string): void {
  sendCommand({ type: "streamStart", room, title, mime });
}

/** Push one chunk of a live stream. */
export function pushStreamChunk(
  room: string,
  streamId: number,
  payloadBase64: string,
): void {
  sendCommand({ type: "streamChunk", room, streamId, payloadBase64 });
}

/** Close a live stream. */
export function endStream(room: string, streamId: number): void {
  sendCommand({ type: "streamEnd", room, streamId });
}

/** Publish a clip to the reels feed. */
export function publishReel(title: string, mime: string, payloadBase64: string): void {
  sendCommand({ type: "storeReel", title, mime, payloadBase64 });
}

/* -------------------------------------------------------------------------- */
/* Selectors                                                                   */
/* -------------------------------------------------------------------------- */

/** All rooms, defaults first, then direct conversations, then live rooms. */
export function selectRooms(state: AppState): RoomState[] {
  const known = new Map(state.rooms.map((room) => [room.id, room]));
  const result: RoomState[] = [];

  for (const channel of DEFAULT_CHANNELS) {
    result.push(
      known.get(channel.id) ?? {
        id: channel.id,
        title: channel.title,
        kind: channel.kind,
        members: [],
        e2e: false,
      },
    );
  }

  for (const peer of state.peers) {
    const id = directRoom(peer.peerId, state.node?.nodeId);
    if (known.has(id)) {
      result.push(known.get(id)!);
    } else {
      result.push({
        id,
        title: peer.displayName || peer.peerId,
        kind: "direct",
        members: [peer.peerId],
        e2e: true,
      });
    }
  }

  for (const [roomId, stream] of Object.entries(state.streams)) {
    if (!result.some((room) => room.id === roomId)) {
      result.push({
        id: roomId,
        title: stream.title || roomId,
        kind: "stream",
        members: [stream.publisherPeerId],
        e2e: false,
      });
    }
  }

  return result;
}

/** The messages of a room. */
export function selectMessages(state: AppState, room: string): ChatMessage[] {
  return state.messages[room] ?? [];
}

/** The peers of a room. */
export function selectRoomPeers(state: AppState, room: string): PeerSummary[] {
  if (!room.startsWith("dm:")) return state.peers;
  const peerId = room.slice(3);
  return state.peers.filter((peer) => peer.peerId === peerId);
}

/** Re-export so components only import from one place. */
export { sameMessages };
