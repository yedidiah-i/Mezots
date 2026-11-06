"use client";

/**
 * The direct-message view.
 *
 * WhatsApp-shaped: bubbles that grow from the bottom, a delivery tick on
 * the messages you sent, and a single line at the top that says what the
 * padlock actually means. The safety number is one tap away so two people
 * can confirm they are talking to each other.
 */

import { useEffect, useMemo, useRef, useState } from "react";

import { Avatar, Badge, Button, IconButton, Kbd } from "./ui";
import {
  CheckIcon,
  CloseIcon,
  DoubleCheckIcon,
  LockIcon,
  ShieldIcon,
  WifiIcon,
} from "./icons";
import { Composer } from "./Composer";
import { RichText } from "./RichText";
import type { RoomState } from "@/lib/protocol";
import { selectMessages, useAppState } from "@/lib/store";
import { formatTime } from "@/lib/format";
import { safetyNumber } from "@/lib/identity";
import { hasConversationKey } from "@/lib/dmkeys";

/** The direct message view. */
export function DirectView({ room }: { room?: RoomState }) {
  const [safetyOpen, setSafetyOpen] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  const peerId = room?.id.slice(3) ?? "";
  const messages = useAppState(
    (state) => selectMessages(state, room?.id ?? ""),
    (a, b) => a.length === b.length && a.every((item, index) => item === b[index]),
  );
  const identity = useAppState((state) => state.identity);
  const peers = useAppState((state) => state.peers);
  const peer = peers.find((item) => item.peerId === peerId);

  const secured = useMemo(() => hasConversationKey(peerId), [peerId, messages.length]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ block: "end" });
  }, [messages.length]);

  if (!room) {
    return (
      <div className="grid flex-1 place-items-center text-sm text-fg-subtle">
        Pick a person to talk to.
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* The encryption banner. */}
      <button
        type="button"
        onClick={() => setSafetyOpen(true)}
        className="flex items-center gap-2 border-b border-line bg-surface-2/70 px-4 py-2 text-left transition-colors hover:bg-surface-3"
      >
        <LockIcon size={13} className={secured ? "text-ok" : "text-warn"} />
        <p className="min-w-0 flex-1 truncate text-[11px] text-fg-muted">
          {secured
            ? "Messages are encrypted in your browser. Only you and this person can read them."
            : "Establishing an encrypted session…"}
        </p>
        <span className="shrink-0 text-[11px] text-accent">View safety number</span>
      </button>

      {/* The conversation. */}
      <div
        ref={scrollRef}
        className="min-h-0 flex-1 overflow-y-auto px-3 py-4 sm:px-6"
        style={{
          backgroundImage:
            "radial-gradient(circle at 1px 1px, color-mix(in srgb, var(--border) 60%, transparent) 1px, transparent 0)",
          backgroundSize: "22px 22px",
        }}
      >
        <div className="mx-auto flex max-w-3xl flex-col gap-1.5">
          {messages.length === 0 && (
            <div className="py-10 text-center">
              <Avatar name={room.title} id={peerId} size="xl" className="mx-auto mb-3" />
              <p className="text-sm font-medium">{room.title}</p>
              <p className="mt-1 text-xs text-fg-subtle">
                This is the start of your private conversation.
              </p>
            </div>
          )}

          {messages.map((message, index) => {
            const mine = message.peerId === identity?.nodeId;
            const previous = messages[index - 1];
            const grouped =
              previous !== undefined &&
              previous.peerId === message.peerId &&
              message.ts - previous.ts < 3 * 60 * 1000;

            return (
              <div
                key={message.id}
                className={`flex ${mine ? "justify-end" : "justify-start"}`}
              >
                <div
                  className={[
                    "group relative max-w-[min(78%,520px)] rounded-2xl px-3.5 py-2 text-fluid-base leading-relaxed shadow-soft",
                    mine
                      ? "rounded-br-md bg-accent text-accent-fg"
                      : "rounded-bl-md border border-line bg-surface text-fg",
                    grouped ? "mt-0.5" : "mt-2",
                  ].join(" ")}
                >
                  <p className="break-words whitespace-pre-wrap">
                    <RichText text={message.text} />
                  </p>
                  <span
                    className={`mt-1 flex items-center justify-end gap-1 text-[10px] ${
                      mine ? "text-accent-fg/70" : "text-fg-subtle"
                    }`}
                  >
                    {message.encrypted && (
                      <LockIcon size={9} className="opacity-80" />
                    )}
                    {formatTime(message.ts)}
                    {mine &&
                      (message.pending ? (
                        <CheckIcon size={12} className="opacity-70" />
                      ) : (
                        <DoubleCheckIcon size={12} className="opacity-80" />
                      ))}
                  </span>
                </div>
              </div>
            );
          })}
          <div ref={bottomRef} className="h-2" />
        </div>
      </div>

      <Composer
        room={room.id}
        placeholder="Write a message"
        encrypted={secured}
      />

      {safetyOpen && (
        <SafetyNumberSheet
          room={room}
          peerId={peerId}
          peerName={room.title}
          peerFingerprint={peer?.peerId}
          onClose={() => setSafetyOpen(false)}
        />
      )}
    </div>
  );
}

/** The safety number dialog. */
function SafetyNumberSheet({
  room,
  peerId,
  peerName,
  peerFingerprint,
  onClose,
}: {
  room: RoomState;
  peerId: string;
  peerName: string;
  peerFingerprint?: string;
  onClose: () => void;
}) {
  const identity = useAppState((state) => state.identity);
  const number = identity
    ? safetyNumber(identity.fingerprint, peerFingerprint ?? peerId.padEnd(64, "0"))
    : "";

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/55 p-4 backdrop-blur-[2px]">
      <button
        type="button"
        aria-label="Close"
        className="absolute inset-0 cursor-default"
        onClick={onClose}
      />
      <div className="relative w-full max-w-md rounded-xl border border-line bg-surface p-5 shadow-overlay">
        <header className="mb-4 flex items-start justify-between">
          <div>
            <h2 className="text-fluid-lg font-semibold tracking-tight">
              Safety number
            </h2>
            <p className="mt-1 text-xs text-fg-muted">
              Compare this number with {peerName} on their own screen. If they
              match, nobody is listening in.
            </p>
          </div>
          <IconButton label="Close" onClick={onClose}>
            <CloseIcon size={16} />
          </IconButton>
        </header>

        <div className="rounded-lg border border-line bg-surface-3 p-4">
          <p className="font-mono text-xs leading-relaxed tracking-wider break-all text-fg-muted">
            {number || "unavailable"}
          </p>
        </div>

        <div className="mt-4 space-y-2.5">
          <div className="flex gap-2.5 text-xs text-fg-muted">
            <ShieldIcon size={15} className="mt-0.5 shrink-0 text-ok" />
            <p>
              The key is derived in this browser with X25519 and never leaves
              the device. The node relays ciphertext it cannot read.
            </p>
          </div>
          <div className="flex gap-2.5 text-xs text-fg-muted">
            <WifiIcon size={15} className="mt-0.5 shrink-0 text-accent" />
            <p>
              If {peerName} is on the same network, this conversation never
              touches the internet.
            </p>
          </div>
        </div>

        <div className="mt-5 flex justify-end">
          <Button variant="primary" onClick={onClose}>
            Done
          </Button>
        </div>

        <p className="mt-4 text-center text-[10px] text-fg-subtle">
          <Kbd>esc</Kbd> to close
        </p>
      </div>
    </div>
  );
}
