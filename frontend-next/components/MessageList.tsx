"use client";

/**
 * The scrollable message list.
 *
 * Messages are grouped by day and by author, the way a chat should read.
 * The list sticks to the bottom while new messages arrive, but gets out of
 * the way when the student scrolls up to read something.
 */

import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { MessageItem } from "./MessageItem";
import { ChevronIcon } from "./icons";
import type { ChatMessage } from "@/lib/store";
import { dayKey, formatDay } from "@/lib/format";

/** The message list. */
export function MessageList({
  messages,
  isMine,
  onReply,
  emptyState,
}: {
  messages: ChatMessage[];
  isMine: (message: ChatMessage) => boolean;
  onReply?: (message: ChatMessage) => void;
  emptyState?: React.ReactNode;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const [pinned, setPinned] = useState(true);
  const [unread, setUnread] = useState(0);

  // Decide whether we are following the tail.
  const onScroll = () => {
    const element = scrollRef.current;
    if (!element) return;
    const distance = element.scrollHeight - element.scrollTop - element.clientHeight;
    const atBottom = distance < 120;
    setPinned(atBottom);
    if (atBottom) setUnread(0);
  };

  // Scroll to the bottom when the list changes and we are following it.
  useLayoutEffect(() => {
    if (!pinned) {
      setUnread((count) => count + 1);
      return;
    }
    bottomRef.current?.scrollIntoView({ block: "end" });
  }, [messages, pinned]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ block: "end" });
  }, []);

  const jumpToLatest = () => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
    setPinned(true);
    setUnread(0);
  };

  if (messages.length === 0 && emptyState) {
    return (
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="flex h-full items-center justify-center px-6">
          {emptyState}
        </div>
      </div>
    );
  }

  let previous: ChatMessage | null = null;
  let previousDay = "";

  return (
    <div className="relative min-h-0 flex-1">
      <div
        ref={scrollRef}
        onScroll={onScroll}
        className="h-full overflow-y-auto overscroll-contain pb-2"
      >
        {messages.map((message) => {
          const day = dayKey(message.ts);
          const newDay = day !== previousDay;
          previousDay = day;

          const sameAuthor =
            !newDay &&
            previous !== null &&
            previous.peerId === message.peerId &&
            message.ts - previous.ts < 5 * 60 * 1000;
          const compact = sameAuthor;

          previous = message;

          return (
            <div key={message.id}>
              {newDay && (
                <div className="sticky top-0 z-10 flex items-center gap-3 px-4 py-2">
                  <span className="h-px flex-1 bg-line" />
                  <span className="rounded-full border border-line bg-surface px-2.5 py-0.5 text-[11px] font-medium text-fg-subtle">
                    {formatDay(message.ts)}
                  </span>
                  <span className="h-px flex-1 bg-line" />
                </div>
              )}
              <MessageItem
                message={message}
                compact={compact}
                isMine={isMine(message)}
                onReply={onReply}
              />
            </div>
          );
        })}
        <div ref={bottomRef} className="h-2" />
      </div>

      {!pinned && (
        <button
          type="button"
          onClick={jumpToLatest}
          className="absolute bottom-4 left-1/2 flex -translate-x-1/2 items-center gap-1.5 rounded-full border border-line bg-surface px-3 py-1.5 text-xs font-medium shadow-overlay transition-transform hover:scale-105"
        >
          <ChevronIcon direction="down" size={13} />
          {unread > 1 ? `${unread} new messages` : "Jump to the latest"}
        </button>
      )}
    </div>
  );
}
