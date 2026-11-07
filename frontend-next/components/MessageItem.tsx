"use client";

/**
 * One message in a channel.
 *
 * The Slack shape: an avatar, a name, a timestamp, the body, and a row of
 * actions that appear when the pointer is over the message. Everything that
 * makes the message trustworthy - who wrote it, when it was committed to the
 * chain, whether it left this browser encrypted - is visible without
 * opening anything.
 */

import { useState } from "react";

import { Avatar, Badge, IconButton } from "./ui";
import {
  BlocksIcon,
  ClockIcon,
  DotsIcon,
  LockIcon,
  ReplyIcon,
  SmileIcon,
} from "./icons";
import { RichText } from "./RichText";
import type { ChatMessage } from "@/lib/store";
import { formatTime, relativeTime, colourFor } from "@/lib/format";
import { toggleReaction } from "@/lib/store";

/** The reactions offered by the quick picker. */
const QUICK_REACTIONS = ["👍", "🎉", "❤️", "😂", "🙏", "👀"];

/** A single message. */
export function MessageItem({
  message,
  compact,
  isMine,
  onReply,
}: {
  message: ChatMessage;
  compact: boolean;
  isMine: boolean;
  onReply?: (message: ChatMessage) => void;
}) {
  const [pickerOpen, setPickerOpen] = useState(false);

  const reactionEntries = Object.entries(message.reactions).filter(
    ([, users]) => users.length > 0,
  );

  return (
    <article
      className={`group message-enter relative flex gap-3 rounded-lg px-3 transition-colors hover:bg-surface-2/60 sm:px-4 ${
        compact ? "py-0.5" : "pt-3 pb-1"
      }`}
    >
      <div className="w-9 shrink-0">
        {!compact && (
          <Avatar
            name={message.author}
            id={message.peerId}
            size="md"
          />
        )}
      </div>

      <div className="min-w-0 flex-1">
        {!compact && (
          <div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
            <span
              className="text-fluid-sm font-semibold text-fg"
              style={{ color: `hsl(${colourFor(message.peerId)} 62% 62%)` }}
            >
              {isMine ? "You" : message.author}
            </span>
            <time
              className="text-[11px] text-fg-subtle"
              title={new Date(message.ts).toLocaleString()}
            >
              {formatTime(message.ts)}
            </time>
            {message.encrypted && (
              <span
                className="inline-flex items-center gap-0.5 rounded-full bg-ok-soft px-1.5 py-px text-[10px] text-ok"
                title="Encrypted in the browser before it left this device"
              >
                <LockIcon size={9} />
                encrypted
              </span>
            )}
            {message.blockHash && (
              <span
                className="inline-flex items-center gap-0.5 rounded-full bg-surface-3 px-1.5 py-px font-mono text-[10px] text-fg-subtle"
                title={`Committed to the chain at block ${message.height ?? "?"}`}
              >
                <BlocksIcon size={9} />
                #{message.height ?? "?"}
              </span>
            )}
          </div>
        )}

        <div className="flex items-start gap-2">
          <p className="min-w-0 flex-1 text-fluid-base leading-relaxed break-words whitespace-pre-wrap text-fg-muted">
            <RichText text={message.text} />
          </p>

          {/* Hover actions. */}
          <div className="absolute -top-2.5 right-3 hidden items-center gap-0.5 rounded-lg border border-line bg-surface/90 px-0.5 py-0.5 shadow-float backdrop-blur-xl group-hover:flex animate-pop">
            <div className="relative">
              <IconButton
                label="Add a reaction"
                onClick={() => setPickerOpen((open) => !open)}
              >
                <SmileIcon size={15} />
              </IconButton>
              {pickerOpen && (
                <>
                  <button
                    type="button"
                    aria-label="Close the reaction picker"
                    className="fixed inset-0 z-10 cursor-default"
                    onClick={() => setPickerOpen(false)}
                  />
                  <div className="absolute right-0 bottom-9 z-20 flex gap-0.5 rounded-lg border border-line bg-surface p-1 shadow-overlay animate-rise">
                    {QUICK_REACTIONS.map((emoji) => (
                      <button
                        key={emoji}
                        type="button"
                        onClick={() => {
                          toggleReaction(message.room, message.id, emoji);
                          setPickerOpen(false);
                        }}
                        className="grid size-7 place-items-center rounded text-base transition-colors hover:bg-surface-3"
                      >
                        {emoji}
                      </button>
                    ))}
                  </div>
                </>
              )}
            </div>
            {onReply && (
              <IconButton label="Reply in a thread" onClick={() => onReply(message)}>
                <ReplyIcon size={15} />
              </IconButton>
            )}
            <IconButton label="More actions">
              <DotsIcon size={15} />
            </IconButton>
          </div>
        </div>

        {/* Reactions. */}
        {reactionEntries.length > 0 && (
          <div className="mt-1 flex flex-wrap gap-1">
            {reactionEntries.map(([emoji, users]) => (
              <button
                key={emoji}
                type="button"
                onClick={() => toggleReaction(message.room, message.id, emoji)}
                title={users.join(", ")}
                className={`inline-flex items-center gap-1 rounded-full border px-1.5 py-0.5 text-[11px] transition-colors ${
                  users.includes(message.peerId)
                    ? "border-accent/40 bg-accent-soft text-accent"
                    : "border-line bg-surface-3 text-fg-muted hover:border-line-strong"
                }`}
              >
                <span>{emoji}</span>
                <span className="font-mono text-[10px]">{users.length}</span>
              </button>
            ))}
          </div>
        )}

        {/* Delivery state. */}
        {message.pending && (
          <p className="mt-0.5 flex items-center gap-1 text-[10px] text-fg-subtle">
            <ClockIcon size={10} />
            sending…
          </p>
        )}
        {message.failed && (
          <p className="mt-0.5 text-[10px] text-danger">{message.failed}</p>
        )}
      </div>
    </article>
  );
}

/** A hint shown under a message that has replies. */
export function ThreadHint({
  count,
  onOpen,
}: {
  count: number;
  onOpen: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onOpen}
      className="mt-1 flex items-center gap-2 text-[11px] text-accent transition-opacity hover:opacity-80"
    >
      <span className="h-px w-8 bg-accent/40" />
      {count} {count === 1 ? "reply" : "replies"}
    </button>
  );
}
