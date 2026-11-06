"use client";

/**
 * The reels feed.
 *
 * Instagram-shaped: a full-height column of short vertical clips that you
 * scroll through one at a time. The clips are published through the node,
 * so a class can share a clip with everyone on the network without a single
 * byte going out to the internet.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { Avatar, Badge, Button, EmptyState, IconButton } from "./ui";
import {
  BookIcon,
  CameraIcon,
  CloseIcon,
  CommentIcon,
  HeartIcon,
  PlayIcon,
  ShareIcon,
  VideoIcon,
} from "./icons";
import { publishReel, useAppState } from "@/lib/store";
import { blobToBase64, startCameraCapture } from "@/lib/stream";
import { formatDuration, relativeTime } from "@/lib/format";
import type { Reel } from "@/lib/protocol";

/** The reels feed. */
export function ReelsView() {
  const reels = useAppState((state) => state.reels);
  const identity = useAppState((state) => state.identity);
  const [recording, setRecording] = useState(false);
  const [title, setTitle] = useState("");
  const [error, setError] = useState<string | null>(null);
  const recorderRef = useRef<{ stop: () => void; chunks: Blob[] } | null>(null);

  useEffect(() => {
    return () => recorderRef.current?.stop();
  }, []);

  const startRecording = async () => {
    setError(null);
    const chunks: Blob[] = [];
    try {
      const capture = await startCameraCapture((base64) => {
        // The live broadcast path is separate; here we only keep the clip.
        void base64;
      });
      capture.recorder.ondataavailable = (event) => {
        if (event.data.size > 0) chunks.push(event.data);
      };
      recorderRef.current = {
        stop: capture.stop,
        chunks,
      };
      setRecording(true);
    } catch (cause) {
      setError(
        cause instanceof Error ? cause.message : "The camera is not available.",
      );
    }
  };

  const stopAndPublish = async () => {
    const recorder = recorderRef.current;
    if (!recorder) return;
    recorder.stop();
    recorderRef.current = null;
    setRecording(false);

    // Give the recorder a moment to flush its last chunk.
    await new Promise((resolve) => setTimeout(resolve, 400));
    if (recorder.chunks.length === 0) {
      setError("Nothing was recorded.");
      return;
    }
    const blob = new Blob(recorder.chunks, { type: recorder.chunks[0].type });
    const base64 = await blobToBase64(blob);
    publishReel(
      title.trim() || "Untitled clip",
      blob.type || "video/webm",
      base64,
    );
    setTitle("");
  };

  return (
    <div className="relative flex min-h-0 flex-1">
      <div className="mx-auto flex h-full w-full max-w-[440px] flex-col">
        <div className="flex items-center justify-between border-b border-line px-4 py-2.5">
          <h2 className="text-fluid-sm font-semibold tracking-tight">Reels</h2>
          <div className="flex items-center gap-2">
            {recording ? (
              <Button variant="danger" size="sm" onClick={() => void stopAndPublish()}>
                <CloseIcon size={14} />
                Stop and publish
              </Button>
            ) : (
              <Button
                variant="soft"
                size="sm"
                onClick={() => void startRecording()}
              >
                <CameraIcon size={14} />
                Record
              </Button>
            )}
          </div>
        </div>

        {recording && (
          <div className="flex items-center gap-2 border-b border-line bg-danger-soft px-4 py-2">
            <span className="size-2 animate-pulse-ring rounded-full bg-danger" />
            <input
              value={title}
              onChange={(event) => setTitle(event.target.value)}
              placeholder="Caption"
              className="min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-fg-subtle"
            />
          </div>
        )}

        {error && (
          <p className="border-b border-line bg-danger-soft px-4 py-2 text-xs text-danger">
            {error}
          </p>
        )}

        <div className="min-h-0 flex-1 overflow-y-auto snap-y snap-mandatory scroll-smooth">
          {reels.length === 0 ? (
            <EmptyState
              icon={<BookIcon size={22} />}
              title="No clips yet"
              description="Record a short clip and it appears here for everyone on the network."
            />
          ) : (
            reels.map((reel) => <ReelCard key={reel.id} reel={reel} />)
          )}
        </div>
      </div>
    </div>
  );
}

/** One reel. */
function ReelCard({ reel }: { reel: Reel }) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const [paused, setPaused] = useState(false);
  const [liked, setLiked] = useState(false);

  const toggle = useCallback(() => {
    const element = videoRef.current;
    if (!element) return;
    if (element.paused) {
      element.play().catch(() => undefined);
      setPaused(false);
    } else {
      element.pause();
      setPaused(true);
    }
  }, []);

  // Only one reel plays at a time.
  useEffect(() => {
    const element = videoRef.current;
    if (!element) return;
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            element.play().catch(() => undefined);
            setPaused(false);
          } else {
            element.pause();
          }
        }
      },
      { threshold: 0.6 },
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  return (
    <article className="relative flex h-full snap-start snap-always flex-col items-center justify-center bg-black">
      <video
        ref={videoRef}
        src={reel.url}
        playsInline
        loop
        muted
        onClick={toggle}
        className="h-full w-full object-contain"
      />

      {paused && (
        <button
          type="button"
          onClick={toggle}
          className="absolute inset-0 grid place-items-center bg-black/25"
          aria-label="Play"
        >
          <span className="grid size-16 place-items-center rounded-full bg-black/55 text-white backdrop-blur">
            <PlayIcon size={26} />
          </span>
        </button>
      )}

      <div className="absolute inset-x-0 bottom-0 flex items-end gap-3 bg-gradient-to-t from-black/85 via-black/40 to-transparent p-4 pt-16">
        <div className="min-w-0 flex-1">
          <div className="mb-1.5 flex items-center gap-2">
            <Avatar name={reel.author} id={reel.authorPeerId} size="xs" />
            <span className="truncate text-sm font-semibold text-white">
              {reel.author}
            </span>
            {reel.blockHash && (
              <Badge tone="ok" className="bg-black/40 text-white/90">
                on chain
              </Badge>
            )}
          </div>
          <p className="text-sm leading-snug text-white/85">{reel.title}</p>
          <p className="mt-1 text-[11px] text-white/55">
            {relativeTime(reel.ts)}
            {reel.durationMs > 0 && ` · ${formatDuration(reel.durationMs)}`}
          </p>
        </div>

        <div className="flex shrink-0 flex-col items-center gap-3">
          <button
            type="button"
            onClick={() => setLiked((value) => !value)}
            className="flex flex-col items-center gap-1 text-white/90 transition-transform active:scale-90"
            aria-label="Like"
          >
            <HeartIcon size={24} filled={liked} className={liked ? "text-danger" : ""} />
            <span className="text-[11px] font-medium">
              {reel.likes + (liked ? 1 : 0)}
            </span>
          </button>
          <button
            type="button"
            className="flex flex-col items-center gap-1 text-white/90"
            aria-label="Comment"
          >
            <CommentIcon size={23} />
            <span className="text-[11px] font-medium">{reel.comments}</span>
          </button>
          <button
            type="button"
            className="flex flex-col items-center gap-1 text-white/90"
            aria-label="Share"
          >
            <ShareIcon size={22} />
            <span className="text-[11px] font-medium">share</span>
          </button>
        </div>
      </div>

      {reel.mime.startsWith("video/") === false && (
        <span className="absolute top-3 left-3">
          <Badge tone="neutral">
            <VideoIcon size={11} />
            {reel.mime}
          </Badge>
        </span>
      )}
    </article>
  );
}
