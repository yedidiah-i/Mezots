/**
 * The transport that carries the Mezots protocol.
 *
 * Everything the browser sends travels over one WebSocket to the local node.
 * There is no third-party SDK, no CDN and no cloud relay in the path: the
 * socket points at the node on the same machine (or the school's node), and
 * from there the node speaks the encrypted peer protocol over the LAN.
 *
 * The client is deliberately framework-agnostic. It owns a socket, a reconnect
 * schedule and an outbox, and hands events to whoever is listening. React
 * subscribes to it through the store.
 */

import type { WsCommand, WsEvent, WsEventOf, WsEventType } from "./protocol";

/** Where the client currently is in its connection lifecycle. */
export type ConnectionState =
  | "idle"
  | "connecting"
  | "online"
  | "reconnecting"
  | "offline";

/** A listener for a single event kind. */
type Listener = (event: WsEvent) => void;

/** A listener for connection state changes. */
type StateListener = (state: ConnectionState, detail?: string) => void;

/** Tunables for the reconnect schedule. */
interface ReconnectPolicy {
  /** Delay before the first retry, in milliseconds. */
  base: number;
  /** Upper bound on the delay, in milliseconds. */
  max: number;
  /** How many attempts before the client gives up and waits for a manual retry. */
  attemptsBeforeGivingUp: number;
}

const DEFAULT_POLICY: ReconnectPolicy = {
  base: 600,
  max: 20_000,
  attemptsBeforeGivingUp: 0, // never give up: the node may come back
};

/** A queued command that could not be sent yet. */
interface OutboxEntry {
  command: WsCommand;
  queuedAt: number;
}

/**
 * A resilient WebSocket client for the Mezots node.
 */
export class MezotsClient {
  private socket: WebSocket | null = null;
  private listeners = new Map<WsEventType, Set<Listener>>();
  private stateListeners = new Set<StateListener>();
  private outbox: OutboxEntry[] = [];
  private state: ConnectionState = "idle";
  private attempt = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  private manuallyClosed = false;
  private url: string;

  /**
   * @param url The WebSocket URL of the node, for example
   *   `ws://192.168.1.20:7701/ws`.
   */
  constructor(url: string, private policy: ReconnectPolicy = DEFAULT_POLICY) {
    this.url = url;
  }

  /** The URL this client points at. */
  get endpoint(): string {
    return this.url;
  }

  /** The current connection state. */
  get connectionState(): ConnectionState {
    return this.state;
  }

  /** Whether the socket is usable right now. */
  get isOnline(): boolean {
    return this.state === "online";
  }

  /** How many commands are waiting for the socket to come back. */
  get pendingCount(): number {
    return this.outbox.length;
  }

  /** Point the client at a different node and reconnect. */
  setEndpoint(url: string): void {
    if (url === this.url) return;
    this.url = url;
    this.attempt = 0;
    this.disconnect();
    this.connect();
  }

  /** Open the socket. Safe to call more than once. */
  connect(): void {
    if (
      this.socket &&
      (this.socket.readyState === WebSocket.OPEN ||
        this.socket.readyState === WebSocket.CONNECTING)
    ) {
      return;
    }
    this.manuallyClosed = false;
    this.setState(this.attempt === 0 ? "connecting" : "reconnecting");

    let socket: WebSocket;
    try {
      socket = new WebSocket(this.url);
    } catch (error) {
      this.setState("offline", describeError(error));
      this.scheduleReconnect();
      return;
    }
    this.socket = socket;

    socket.onopen = () => {
      this.attempt = 0;
      this.setState("online");
      this.flushOutbox();
      this.startHeartbeat();
    };

    socket.onmessage = (event) => {
      if (typeof event.data !== "string") return;
      let parsed: WsEvent;
      try {
        parsed = JSON.parse(event.data) as WsEvent;
      } catch {
        return;
      }
      this.dispatch(parsed);
    };

    socket.onerror = () => {
      // `onclose` always follows, so the state is settled there.
    };

    socket.onclose = (event) => {
      this.stopHeartbeat();
      this.socket = null;
      if (this.manuallyClosed) {
        this.setState("idle");
        return;
      }
      this.setState(
        "offline",
        event.reason || `connection closed (${event.code})`,
      );
      this.scheduleReconnect();
    };
  }

  /** Close the socket and stop reconnecting. */
  disconnect(): void {
    this.manuallyClosed = true;
    this.stopHeartbeat();
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    if (this.socket) {
      const socket = this.socket;
      this.socket = null;
      try {
        socket.close(1000, "client closed");
      } catch {
        /* the socket was already gone */
      }
    }
    this.setState("idle");
  }

  /**
   * Send a command.
   *
   * When the socket is down the command is parked in the outbox and sent as
   * soon as the node is reachable again. This is what keeps a student typing
   * on the school bus from losing a message.
   */
  send(command: WsCommand): boolean {
    if (this.socket && this.socket.readyState === WebSocket.OPEN) {
      this.socket.send(JSON.stringify(command));
      return true;
    }
    this.outbox.push({ command, queuedAt: Date.now() });
    this.connect();
    return false;
  }

  /** Listen for one kind of event. Returns an unsubscribe function. */
  on<T extends WsEventType>(
    type: T,
    listener: (event: WsEventOf<T>) => void,
  ): () => void {
    let bucket = this.listeners.get(type);
    if (!bucket) {
      bucket = new Set();
      this.listeners.set(type, bucket);
    }
    const wrapped = listener as Listener;
    bucket.add(wrapped);
    return () => {
      bucket?.delete(wrapped);
    };
  }

  /** Listen for every event. Returns an unsubscribe function. */
  onAny(listener: Listener): () => void {
    return this.on("peers" as WsEventType, listener as never);
  }

  /** Listen for connection state changes. */
  onState(listener: StateListener): () => void {
    this.stateListeners.add(listener);
    listener(this.state);
    return () => {
      this.stateListeners.delete(listener);
    };
  }

  /** Drop every queued command. */
  clearOutbox(): void {
    this.outbox = [];
  }

  /* ---------------------------------------------------------------------- */

  private dispatch(event: WsEvent): void {
    const bucket = this.listeners.get(event.type);
    if (!bucket) return;
    for (const listener of bucket) {
      try {
        listener(event);
      } catch (error) {
        console.error("[mezots] event listener failed", error);
      }
    }
  }

  private setState(state: ConnectionState, detail?: string): void {
    if (this.state === state) return;
    this.state = state;
    for (const listener of this.stateListeners) {
      try {
        listener(state, detail);
      } catch (error) {
        console.error("[mezots] state listener failed", error);
      }
    }
  }

  private flushOutbox(): void {
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN) return;
    const pending = this.outbox;
    this.outbox = [];
    for (const entry of pending) {
      this.socket.send(JSON.stringify(entry.command));
    }
  }

  private scheduleReconnect(): void {
    if (this.manuallyClosed || this.reconnectTimer) return;
    this.attempt += 1;
    // Exponential backoff with jitter, so a school full of nodes that all
    // reboot at 08:00 do not hammer the network in lockstep.
    const ceiling = Math.min(
      this.policy.max,
      this.policy.base * 2 ** Math.min(this.attempt - 1, 6),
    );
    const delay = Math.round(ceiling * (0.7 + Math.random() * 0.6));
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.connect();
    }, delay);
  }

  /**
   * Keep the socket warm.
   *
   * The node answers WebSocket pings on its own, so this is only a light
   * liveness probe that also detects half-open connections.
   */
  private startHeartbeat(): void {
    this.stopHeartbeat();
    this.heartbeatTimer = setInterval(() => {
      if (this.socket && this.socket.readyState === WebSocket.OPEN) {
        this.send({ type: "listPeers" });
      }
    }, 15_000);
  }

  private stopHeartbeat(): void {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
  }
}

/** Turn an unknown thrown value into a readable message. */
function describeError(error: unknown): string {
  if (error instanceof Error) return error.message;
  return String(error);
}

/**
 * Work out which node the browser should talk to.
 *
 * The precedence is deliberate:
 *
 * 1. `NEXT_PUBLIC_MEZOTS_NODE_URL` - set at build time for a fixed school node.
 * 2. A value the user typed in the app and we stored locally.
 * 3. The node next to the page we were served from.
 */
export function resolveNodeUrl(): string {
  const configured = process.env.NEXT_PUBLIC_MEZOTS_NODE_URL;
  if (configured) return normaliseNodeUrl(configured);

  try {
    const stored = window.localStorage.getItem("mezots.nodeUrl");
    if (stored) return normaliseNodeUrl(stored);
  } catch {
    /* storage is unavailable (private mode); fall through */
  }

  if (typeof window === "undefined") return "ws://127.0.0.1:7701/ws";

  const { protocol, hostname } = window.location;
  const scheme = protocol === "https:" ? "wss:" : "ws:";
  return `${scheme}//${hostname}:7701/ws`;
}

/** Accept either a bare host or a full WebSocket URL. */
export function normaliseNodeUrl(input: string): string {
  const trimmed = input.trim();
  if (!trimmed) return "ws://127.0.0.1:7701/ws";
  if (/^wss?:\/\//i.test(trimmed)) {
    return trimmed.endsWith("/ws") ? trimmed : `${trimmed.replace(/\/$/, "")}/ws`;
  }
  const host = trimmed.replace(/^https?:\/\//i, "").replace(/\/+$/, "");
  return `ws://${host}/ws`;
}
