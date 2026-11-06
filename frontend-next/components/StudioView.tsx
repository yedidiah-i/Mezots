"use client";

/**
 * The live studio.
 *
 * Twitch-shaped: a big picture, a live chat beside it, and a control bar
 * for whoever is on camera.
 *
 * The broadcast is captured in the browser with `MediaRecorder`, chopped into
 * one-second chunks and pushed to the node, which relays them to the other
 * nodes on the network. A viewer on the same school network receives them
 * without a single byte crossing the internet.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { Avatar, Badge, Button, EmptyState, IconButton, LiveDot } from "./ui";
import {
  CameraIcon,
  CloseIcon,
  MicIcon,
  MicOffIcon,
  MonitorIcon,
  SendIcon,
  VideoIcon,
  WifiIcon,
} from "./icons";
import {
  announceStream,
  endStream,
  pushStreamChunk,
  sendText,
  useAppState,
} from "@/lib/store";
import {
  attachStream,
  closeStream,
  onStreamUpdate,
  startCameraCapture,
  startScreenCapture,
  type Capture,
} from "@/lib/stream";
import { formatBytes } from "@/lib/format";

/** The room that carries the broadcast. */
const BROADCAST_ROOM = "stream:hall";

/** The studio. */
export function StudioView() {
  const [publishing, setPublishing] = useState(false);
  const [title, setTitle] = useState("");
  const [streamId, setStreamId] = useState(0);
  const [muted, setMuted] = useState(false);
  const [cameraOff, setCameraOff] = useState(false);
  const [sharing, setSharing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [chatDraft, setChatDraft] = useState("");

  const localVideoRef = useRef<HTMLVideoElement>(null);
  const remoteVideoRef = useRef<HTMLVideoElement>(null);
  const captureRef = useRef<Capture | null>(null);

  const streams = useAppState((state) => state.streams);
  const messages = useAppState(
    (state) => state.messages[BROADCAST_ROOM] ?? [],
    (a, b) => a.length === b.length && a.every((item, index) => item === b[index]),
  );
  const peers = useAppState((state) => state.peers);
  const identity = useAppState((state) => state.identity);

  const remote = streams[BROADCAST_ROOM];
  const watching = remote && !publishing;

  // Bind the remote video element to the incoming stream.
  useEffect(() => {
    if (!remote) return;
    attachStream(BROADCAST_ROOM, remote.streamId, remote.mime, remoteVideoRef.current);
  }, [remote?.streamId, remote?.mime]);

  useEffect(() => {
    return onStreamUpdate((room) => {
      if (room !== BROADCAST_ROOM) return;
      // The element may have mounted after the first chunk.
      const element = remoteVideoRef.current;
      if (element && element.src === "") {
        const current = streams[BROADCAST_ROOM];
        if (current) {
          attachStream(BROADCAST_ROOM, current.streamId, current.mime, element);
        }
      }
    });
  }, [streams]);

  const stop = useCallback(() => {
    captureRef.current?.stop();
    captureRef.current = null;
    if (streamId) endStream(BROADCAST_ROOM, streamId);
    setPublishing(false);
    setSharing(false);
    setStreamId(0);
    closeStream(BROADCAST_ROOM);
  }, [streamId]);

  useEffect(() => () => captureRef.current?.stop(), []);

  const goLive = async (mode: "camera" | "screen") => {
    setError(null);
    try {
      const capture =
        mode === "camera"
          ? await startCameraCapture((base64) => {
              if (streamId) pushStreamChunk(BROADCAST_ROOM, streamId, base64);
            })
          : await startScreenCapture((base64) => {
              if (streamId) pushStreamChunk(BROADCAST_ROOM, streamId, base64);
            });

      captureRef.current = capture;

      // The stream id is chosen by the node; until it answers we use a local
      // one so the first chunks are not lost.
      const localId = Math.floor(Date.now() % 100000) + 1;
      setStreamId(localId);
      setPublishing(true);
      setSharing(mode === "screen");

      announceStream(
        BROADCAST_ROOM,
        title.trim() || "Untitled broadcast",
        capture.recorder.mimeType || "video/webm",
      );
    } catch (cause) {
      setError(
        cause instanceof Error
          ? cause.message
          : "The browser refused to give access to the camera.",
      );
    }
  };

  // The publisher sees their own preview.
  useEffect(() => {
    if (publishing && localVideoRef.current && captureRef.current) {
      localVideoRef.current.srcObject = captureRef.current.stream;
      localVideoRef.current.play().catch(() => undefined);
    }
  }, [publishing]);

  const toggleMute = () => {
    const capture = captureRef.current;
    if (!capture) return;
    const next = !muted;
    for (const track of capture.stream.getAudioTracks()) {
      track.enabled = !next;
    }
    setMuted(next);
  };

  const toggleCamera = () => {
    const capture = captureRef.current;
    if (!capture) return;
    const next = !cameraOff;
    for (const track of capture.stream.getVideoTracks()) {
      track.enabled = !next;
    }
    setCameraOff(next);
  };

  const sendChat = () => {
    const text = chatDraft.trim();
    if (!text) return;
    sendText(BROADCAST_ROOM, text);
    setChatDraft("");
  };

  const viewerCount = remote?.viewers ?? peers.filter((p) => p.connected).length;

  return (
    <div className="flex min-h-0 flex-1 flex-col lg:flex-row">
      {/* The picture. */}
      <div className="flex min-h-0 flex-1 flex-col bg-black/25">
        <div className="relative flex min-h-[280px] flex-1 items-center justify-center overflow-hidden bg-black">
          {watching && remote ? (
            <video
              ref={remoteVideoRef}
              playsInline
              autoPlay
              muted
              className="h-full w-full object-contain"
            />
          ) : publishing ? (
            <video
              ref={localVideoRef}
              playsInline
              muted
              className="h-full w-full object-contain"
            />
          ) : (
            <EmptyState
              icon={<VideoIcon size={24} />}
              title="Nobody is live right now"
              description="Start a broadcast and it will reach every node on this network directly."
            />
          )}

          {publishing && (
            <span className="absolute top-3 left-3 flex items-center gap-2 rounded-md bg-black/65 px-2.5 py-1 text-xs font-medium text-white backdrop-blur">
              <LiveDot />
              LIVE
            </span>
          )}

          {remote && !publishing && (
            <span className="absolute top-3 left-3 flex items-center gap-2 rounded-md bg-black/65 px-2.5 py-1 text-xs font-medium text-white backdrop-blur">
              <LiveDot />
              {remote.title} · {viewerCount} watching
            </span>
          )}

          <span className="absolute right-3 bottom-3 flex items-center gap-1.5 rounded-md bg-black/65 px-2 py-1 text-[10px] text-white/80 backdrop-blur">
            <WifiIcon size={11} className="text-ok" />
            relayed over the local network
          </span>
        </div>

        {/* The control bar. */}
        <div className="flex flex-wrap items-center gap-2 border-t border-line bg-surface px-4 py-3">
          {publishing ? (
            <>
              <input
                value={title}
                onChange={(event) => setTitle(event.target.value)}
                placeholder="Title of this broadcast"
                className="h-9 min-w-0 flex-1 rounded-md border border-line bg-surface-3 px-3 text-sm outline-none focus:border-accent/60"
              />
              <IconButton
                label={muted ? "Unmute" : "Mute"}
                onClick={toggleMute}
                className={muted ? "text-danger" : ""}
              >
                {muted ? <MicOffIcon size={17} /> : <MicIcon size={17} />}
              </IconButton>
              <IconButton
                label={cameraOff ? "Show the camera" : "Hide the camera"}
                onClick={toggleCamera}
                className={cameraOff ? "text-danger" : ""}
              >
                <CameraIcon size={17} />
              </IconButton>
              <Button variant="danger" onClick={stop}>
                <CloseIcon size={15} />
                End the broadcast
              </Button>
            </>
          ) : (
            <>
              <input
                value={title}
                onChange={(event) => setTitle(event.target.value)}
                placeholder="What are you broadcasting?"
                className="h-9 min-w-0 flex-1 rounded-md border border-line bg-surface-3 px-3 text-sm outline-none focus:border-accent/60"
              />
              <Button variant="primary" onClick={() => void goLive("camera")}>
                <CameraIcon size={15} />
                Go live
              </Button>
              <Button variant="secondary" onClick={() => void goLive("screen")}>
                <MonitorIcon size={15} />
                Share a screen
              </Button>
            </>
          )}
        </div>

        {error && (
          <p className="border-t border-line bg-danger-soft px-4 py-2 text-xs text-danger">
            {error}
          </p>
        )}
      </div>

      {/* The chat. */}
      <aside className="flex w-full shrink-0 flex-col border-t border-line bg-surface-2 lg:w-[340px] lg:border-t-0 lg:border-l">
        <header className="flex h-11 shrink-0 items-center justify-between border-b border-line px-4">
          <h3 className="text-fluid-sm font-semibold tracking-tight">Live chat</h3>
          <Badge tone={remote ? "danger" : "neutral"}>
            {viewerCount} watching
          </Badge>
        </header>

        <div className="min-h-0 flex-1 space-y-2 overflow-y-auto px-3 py-3">
          {messages.length === 0 && (
            <p className="px-2 py-6 text-center text-xs text-fg-subtle">
              No messages yet. Anything written here goes to everyone watching
              on this network.
            </p>
          )}
          {messages.map((message) => {
            const mine = message.peerId === identity?.nodeId;
            return (
              <div key={message.id} className="flex gap-2">
                <Avatar
                  name={message.author}
                  id={message.peerId}
                  size="xs"
                />
                <div className="min-w-0 flex-1">
                  <p className="text-[11px] leading-tight">
                    <span className="font-semibold text-fg">
                      {mine ? "You" : message.author}
                    </span>{" "}
                    <span className="text-fg-muted">{message.text}</span>
                  </p>
                </div>
              </div>
            );
          })}
        </div>

        <div className="flex shrink-0 items-center gap-2 border-t border-line px-3 py-2.5">
          <input
            value={chatDraft}
            onChange={(event) => setChatDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") sendChat();
            }}
            placeholder="Say something"
            className="h-9 min-w-0 flex-1 rounded-md border border-line bg-surface-3 px-3 text-sm outline-none focus:border-accent/60"
          />
          <button
            type="button"
            onClick={sendChat}
            disabled={!chatDraft.trim()}
            className="grid size-9 place-items-center rounded-md bg-accent text-accent-fg transition-[filter,transform] hover:brightness-110 active:scale-95 disabled:opacity-40"
            aria-label="Send"
          >
            <SendIcon size={15} />
          </button>
        </div>

        {remote && (
          <div className="border-t border-line px-4 py-2.5 text-[11px] text-fg-subtle">
            Broadcasting from{" "}
            <span className="text-fg-muted">{remote.publisher}</span> · the
            picture travels peer to peer
          </div>
        )}
      </aside>
    </div>
  );
}
