/**
 * Live stream assembly.
 *
 * The node relays a broadcast as a sequence of chunks. This module collects
 * them back into something a browser can play.
 *
 * When the browser supports Media Source Extensions the chunks are appended
 * to a `SourceBuffer` as they arrive, which gives a genuine live picture
 * with a second or two of delay. Otherwise the chunks are concatenated and
 * served as a blob, which is a little later but works everywhere.
 *
 * Both paths live here so the studio view only has to deal with a `src`.
 */

import { fromBase64 } from "./crypto";

/** The mime type used when the publisher does not say. */
export const DEFAULT_STREAM_MIME = "video/webm;codecs=vp8,opus";

/** One assembled stream. */
interface Assembly {
  room: string;
  streamId: number;
  mime: string;
  chunks: Uint8Array[];
  bytes: number;
  /** The element that plays the stream, when there is one. */
  media?: HTMLVideoElement;
  source?: MediaSource;
  buffer?: SourceBuffer;
  objectUrl?: string;
  updatedAt: number;
  /** When the fallback path last rebuilt its blob. */
  lastFlush: number;
  /** Set when the browser cannot append incrementally. */
  fallback: boolean;
}

/** Every assembly, keyed by room. */
const assemblies = new Map<string, Assembly>();

/** Listeners told when a stream has new data. */
const listeners = new Set<(room: string) => void>();

/** Subscribe to stream updates. */
export function onStreamUpdate(listener: (room: string) => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Notify listeners. */
function notify(room: string): void {
  for (const listener of listeners) listener(room);
}

/** How often the fallback path rebuilds its blob. */
const FALLBACK_INTERVAL = 1500;

/**
 * Attach a video element to a stream.
 *
 * The element is bound to the room so that it keeps receiving chunks.
 */
export function attachStream(
  room: string,
  streamId: number,
  mime: string,
  element: HTMLVideoElement | null,
): void {
  if (!element) return;

  let assembly = assemblies.get(room);
  if (!assembly || assembly.streamId !== streamId) {
    assembly = {
      room,
      streamId,
      mime: mime || DEFAULT_STREAM_MIME,
      chunks: [],
      bytes: 0,
      updatedAt: Date.now(),
      lastFlush: 0,
      fallback: false,
    };
    assemblies.set(room, assembly);
  }

  assembly.media = element;
  assembly.mime = mime || assembly.mime;

  // Try the live path first.
  if (typeof MediaSource !== "undefined" && MediaSource.isTypeSupported(assembly.mime)) {
    const source = new MediaSource();
    assembly.source = source;
    element.src = URL.createObjectURL(source);
    source.addEventListener("sourceopen", () => {
      try {
        const buffer = source.addSourceBuffer(assembly.mime);
        assembly.buffer = buffer;
        buffer.addEventListener("updateend", () => {
          flushBuffer(assembly as Assembly);
        });
        flushBuffer(assembly as Assembly);
      } catch {
        // Fall back to the blob path.
        (assembly as Assembly).fallback = true;
        applyFallback(assembly as Assembly);
      }
    });
  } else {
    assembly.fallback = true;
    applyFallback(assembly);
  }
}

/** Append the pending chunks to the source buffer. */
function flushBuffer(assembly: Assembly): void {
  const buffer = assembly.buffer;
  if (!buffer || buffer.updating || assembly.chunks.length === 0) return;
  const next = assembly.chunks.shift();
  if (!next) return;
  try {
    buffer.appendBuffer(next as BufferSource);
  } catch {
    assembly.fallback = true;
    applyFallback(assembly);
  }
}

/** Rebuild the blob URL from everything received so far. */
function applyFallback(assembly: Assembly): void {
  if (!assembly.media) return;
  if (assembly.objectUrl) URL.revokeObjectURL(assembly.objectUrl);
  assembly.objectUrl = URL.createObjectURL(
    new Blob(assembly.chunks as BlobPart[], { type: assembly.mime }),
  );
  assembly.media.src = assembly.objectUrl;
  assembly.lastFlush = Date.now();
  assembly.media.play().catch(() => {
    /* the browser may block autoplay; the student can press play */
  });
}

/** Add a chunk that arrived from the node. */
export function pushChunk(
  room: string,
  streamId: number,
  payloadBase64: string,
): void {
  let assembly = assemblies.get(room);
  if (!assembly) {
    assembly = {
      room,
      streamId,
      mime: DEFAULT_STREAM_MIME,
      chunks: [],
      bytes: 0,
      updatedAt: Date.now(),
      lastFlush: 0,
      fallback: false,
    };
    assemblies.set(room, assembly);
  }
  if (assembly.streamId !== streamId) {
    // A new broadcast replaced the old one.
    assembly.streamId = streamId;
    assembly.chunks = [];
    assembly.bytes = 0;
    assembly.source = undefined;
    assembly.buffer = undefined;
  }

  const bytes = fromBase64(payloadBase64);
  assembly.chunks.push(bytes);
  assembly.bytes += bytes.length;
  assembly.updatedAt = Date.now();

  if (assembly.fallback) {
    const now = Date.now();
    if (now - assembly.lastFlush > FALLBACK_INTERVAL) {
      applyFallback(assembly);
    }
  } else {
    flushBuffer(assembly);
  }

  notify(room);
}

/** Forget a stream. */
export function closeStream(room: string): void {
  const assembly = assemblies.get(room);
  if (!assembly) return;
  if (assembly.objectUrl) URL.revokeObjectURL(assembly.objectUrl);
  assemblies.delete(room);
  notify(room);
}

/** How many bytes have been received for a room. */
export function bytesFor(room: string): number {
  return assemblies.get(room)?.bytes ?? 0;
}

/** Whether a stream is being assembled for a room. */
export function hasStream(room: string): boolean {
  return assemblies.has(room);
}

/* -------------------------------------------------------------------------- */
/* Capture                                                                     */
/* -------------------------------------------------------------------------- */

/** How often the recorder emits a chunk, in milliseconds. */
export const CHUNK_INTERVAL = 1000;

/** A running capture. */
export interface Capture {
  /** The camera and microphone track. */
  stream: MediaStream;
  /** The recorder producing the chunks. */
  recorder: MediaRecorder;
  /** Stop everything and release the devices. */
  stop: () => void;
}

/** The mime types the recorder can produce, best first. */
function pickMime(): string {
  const candidates = [
    "video/webm;codecs=vp8,opus",
    "video/webm;codecs=vp9,opus",
    "video/webm",
    "video/mp4",
  ];
  for (const candidate of candidates) {
    if (typeof MediaRecorder !== "undefined" && MediaRecorder.isTypeSupported(candidate)) {
      return candidate;
    }
  }
  return "";
}

/**
 * Start capturing the camera and microphone.
 *
 * The recorder emits a chunk every second; each chunk is handed to `onChunk`
 * as base64 so it can be pushed to the node.
 */
export async function startCameraCapture(
  onChunk: (base64: string) => void,
): Promise<Capture> {
  const stream = await navigator.mediaDevices.getUserMedia({
    video: { width: { ideal: 1280 }, height: { ideal: 720 } },
    audio: true,
  });
  const recorder = new MediaRecorder(stream, pickMime() ? { mimeType: pickMime() } : undefined);
  recorder.ondataavailable = (event) => {
    if (event.data.size === 0) return;
    const buffer = new Uint8Array(event.data.size);
    event.data.arrayBuffer().then((raw) => {
      buffer.set(new Uint8Array(raw));
      let binary = "";
      const chunk = 0x8000;
      for (let i = 0; i < buffer.length; i += chunk) {
        binary += String.fromCharCode(...buffer.subarray(i, i + chunk));
      }
      onChunk(btoa(binary));
    });
  };
  recorder.start(CHUNK_INTERVAL);

  return {
    stream,
    recorder,
    stop: () => {
      try {
        if (recorder.state !== "inactive") recorder.stop();
      } catch {
        /* already stopped */
      }
      for (const track of stream.getTracks()) track.stop();
    },
  };
}

/**
 * Start capturing a screen.
 *
 * The microphone is optional here: a shared screen with the microphone
 * already in use by the camera would echo.
 */
export async function startScreenCapture(
  onChunk: (base64: string) => void,
): Promise<Capture> {
  const stream = await navigator.mediaDevices.getDisplayMedia({
    video: { frameRate: 15 },
    audio: false,
  });
  const recorder = new MediaRecorder(stream, pickMime() ? { mimeType: pickMime() } : undefined);
  recorder.ondataavailable = (event) => {
    if (event.data.size === 0) return;
    event.data.arrayBuffer().then((raw) => {
      const bytes = new Uint8Array(raw);
      let binary = "";
      const chunk = 0x8000;
      for (let i = 0; i < bytes.length; i += chunk) {
        binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
      }
      onChunk(btoa(binary));
    });
  };
  recorder.start(CHUNK_INTERVAL);

  return {
    stream,
    recorder,
    stop: () => {
      try {
        if (recorder.state !== "inactive") recorder.stop();
      } catch {
        /* already stopped */
      }
      for (const track of stream.getTracks()) track.stop();
    },
  };
}

/** Turn a blob into base64, for publishing a reel. */
export async function blobToBase64(blob: Blob): Promise<string> {
  const buffer = new Uint8Array(await blob.arrayBuffer());
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < buffer.length; i += chunk) {
    binary += String.fromCharCode(...buffer.subarray(i, i + chunk));
  }
  return btoa(binary);
}
