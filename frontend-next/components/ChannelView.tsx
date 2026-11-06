"use client";

/**
 * The channel view.
 *
 * Slack-shaped: a header (in the top bar), a list of messages grouped by
 * day, and a composer at the bottom. Opening a thread reveals a side panel
 * rather than replacing the conversation, so the main thread stays in view.
 */

import { useMemo, useState } from "react";

import { MessageList } from "./MessageList";
import { Composer } from "./Composer";
import { ThreadPanel } from "./ThreadPanel";
import { Badge, Button, EmptyState, IconButton } from "./ui";
import { HashIcon, PinIcon, PlusIcon, CloseIcon } from "./icons";
import type { RoomState } from "@/lib/protocol";
import { selectMessages, useAppState, type ChatMessage } from "@/lib/store";

/** The channel view. */
export function ChannelView({ room }: { room?: RoomState }) {
  const [thread, setThread] = useState<ChatMessage | null>(null);
  const roomId = room?.id ?? "general";

  const messages = useAppState(
    (state) => selectMessages(state, roomId),
    (a, b) => a.length === b.length && a.every((item, index) => item === b[index]),
  );
  const identity = useAppState((state) => state.identity);
  const peers = useAppState((state) => state.peers);

  const isMine = useMemo(
    () => (message: ChatMessage) => message.peerId === identity?.nodeId,
    [identity?.nodeId],
  );

  const pinned = useMemo(
    () => messages.filter((message) => message.text.startsWith("📌")).slice(-3),
    [messages],
  );

  if (!room) {
    return (
      <EmptyState
        icon={<HashIcon size={22} />}
        title="Pick a channel"
        description="Choose a room from the sidebar to start talking."
      />
    );
  }

  return (
    <div className="flex min-h-0 flex-1">
      <div className="flex min-w-0 flex-1 flex-col">
        {pinned.length > 0 && (
          <div className="flex items-center gap-2 border-b border-line bg-surface-2/60 px-4 py-2 text-xs">
            <PinIcon size={13} className="shrink-0 text-warn" />
            <span className="truncate text-fg-muted">
              {pinned[pinned.length - 1].text.replace("📌", "").trim()}
            </span>
            <Badge tone="warn" className="ml-auto shrink-0">
              pinned
            </Badge>
          </div>
        )}

        <MessageList
          messages={messages}
          isMine={isMine}
          onReply={setThread}
          emptyState={
            <EmptyState
              icon={<HashIcon size={22} />}
              title={`This is the start of #${room.title}`}
              description={
                peers.length > 0
                  ? "Messages in this channel travel directly between the nodes on this network."
                  : "No one else is on the network yet. Start another node and it will appear in the sidebar."
              }
              action={
                <Button variant="soft" size="sm">
                  <PlusIcon size={14} />
                  Invite a classmate
                </Button>
              }
            />
          }
        />

        <Composer
          room={roomId}
          placeholder={`Message #${room.title}`}
        />
      </div>

      {thread && (
        <ThreadPanel message={thread} onClose={() => setThread(null)} />
      )}
    </div>
  );
}
