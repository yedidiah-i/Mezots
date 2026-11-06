"use client";

/**
 * A thread.
 *
 * Threads open beside the conversation instead of on top of it, so the main
 * channel keeps its place. On a phone the panel takes the whole view.
 */

import { useMemo } from "react";

import { Avatar, IconButton } from "./ui";
import { CloseIcon, LockIcon, ReplyIcon } from "./icons";
import { RichText } from "./RichText";
import { Composer } from "./Composer";
import { selectMessages, useAppState, type ChatMessage } from "@/lib/store";
import { formatTime } from "@/lib/format";

/** The thread panel. */
export function ThreadPanel({
  message,
  onClose,
}: {
  message: ChatMessage;
  onClose: () => void;
}) {
  const messages = useAppState(
    (state) => selectMessages(state, message.room),
    (a, b) => a.length === b.length && a.every((item, index) => item === b[index]),
  );
  const identity = useAppState((state) => state.identity);

  const replies = useMemo(
    () => messages.filter((item) => item.id !== message.id),
    [messages, message.id],
  );

  return (
    <aside className="absolute inset-y-0 right-0 z-20 flex w-full max-w-[420px] flex-col border-l border-line bg-surface shadow-overlay sm:relative sm:z-0 sm:shadow-none">
      <header className="flex h-12 shrink-0 items-center justify-between border-b border-line px-4">
        <div className="flex items-center gap-2">
          <ReplyIcon size={15} className="text-fg-subtle" />
          <h3 className="text-fluid-sm font-semibold tracking-tight">Thread</h3>
          {message.blockHash && (
            <span className="font-mono text-[10px] text-fg-subtle">
              #{message.height ?? "?"}
            </span>
          )}
        </div>
        <IconButton label="Close the thread" onClick={onClose}>
          <CloseIcon size={16} />
        </IconButton>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
        <article className="flex gap-3">
          <Avatar name={message.author} id={message.peerId} size="md" />
          <div className="min-w-0 flex-1">
            <div className="flex items-baseline gap-2">
              <span className="text-fluid-sm font-semibold">
                {message.peerId === identity?.nodeId ? "You" : message.author}
              </span>
              <time className="text-[11px] text-fg-subtle">
                {formatTime(message.ts)}
              </time>
              {message.encrypted && (
                <LockIcon size={11} className="text-ok" />
              )}
            </div>
            <p className="mt-0.5 text-fluid-base leading-relaxed whitespace-pre-wrap text-fg-muted">
              <RichText text={message.text} />
            </p>
          </div>
        </article>

        {replies.length > 0 && (
          <>
            <div className="my-4 flex items-center gap-3">
              <span className="h-px flex-1 bg-line" />
              <span className="text-[11px] text-fg-subtle">
                {replies.length} {replies.length === 1 ? "reply" : "replies"}
              </span>
              <span className="h-px flex-1 bg-line" />
            </div>
            <ul className="space-y-3">
              {replies.map((reply) => (
                <li key={reply.id} className="flex gap-3">
                  <Avatar name={reply.author} id={reply.peerId} size="sm" />
                  <div className="min-w-0 flex-1">
                    <div className="flex items-baseline gap-2">
                      <span className="text-sm font-semibold">
                        {reply.peerId === identity?.nodeId ? "You" : reply.author}
                      </span>
                      <time className="text-[11px] text-fg-subtle">
                        {formatTime(reply.ts)}
                      </time>
                    </div>
                    <p className="text-sm leading-relaxed whitespace-pre-wrap text-fg-muted">
                      <RichText text={reply.text} />
                    </p>
                  </div>
                </li>
              ))}
            </ul>
          </>
        )}
      </div>

      <Composer
        room={message.room}
        placeholder="Reply in this thread"
        autoFocus
      />
    </aside>
  );
}
