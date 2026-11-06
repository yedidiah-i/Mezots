"use client";

/**
 * The bar across the top of the main view.
 *
 * It carries the room title, the live status of the room, the search
 * shortcut, and the network badge - the little indicator that tells a
 * student whether the conversation is travelling over the local network or
 * over the internet.
 */

import { Avatar, Badge, IconButton, Kbd } from "./ui";
import {
  BlocksIcon,
  HashIcon,
  LockIcon,
  MonitorIcon,
  SearchIcon,
  VideoIcon,
  WifiIcon,
} from "./icons";
import type { NavSection } from "./AppRail";
import type { RoomState } from "@/lib/protocol";
import { useAppState } from "@/lib/store";
import { formatUptime } from "@/lib/format";

const SECTION_TITLES: Record<NavSection, string> = {
  chat: "Conversations",
  live: "Live studio",
  reels: "Reels",
  chain: "Private chain",
  network: "Network",
};

/** The top bar. */
export function TopBar({
  section,
  room,
  onOpenDrawer,
  onOpenPanel,
  onOpenPalette,
  compact,
}: {
  section: NavSection;
  room?: RoomState;
  onOpenDrawer: () => void;
  onOpenPanel: () => void;
  onOpenPalette: () => void;
  compact: boolean;
}) {
  const connection = useAppState((state) => state.connection);
  const metrics = useAppState((state) => state.metrics);
  const peers = useAppState((state) => state.peers);
  const streams = useAppState((state) => state.streams);

  const online = peers.filter((peer) => peer.connected).length;
  const localOnly = peers
    .filter((peer) => peer.connected)
    .every((peer) => peer.source === "mdns" || peer.source === "beacon");
  const internetBytes = metrics?.bytesInternet ?? 0;
  const liveStream = room ? streams[room.id] : undefined;

  return (
    <header className="flex h-16 shrink-0 items-center gap-3 border-b border-line bg-surface/65 px-4 backdrop-blur-xl sm:px-6">
      {compact && (
        <IconButton label="Open the sidebar" onClick={onOpenDrawer}>
          <span className="flex flex-col gap-[3px]">
            <span className="block h-[1.5px] w-4 bg-current" />
            <span className="block h-[1.5px] w-4 bg-current" />
            <span className="block h-[1.5px] w-4 bg-current" />
          </span>
        </IconButton>
      )}

      <div className="flex min-w-0 flex-1 items-center gap-2">
        {section === "chat" && room && (
          <>
            {room.kind === "direct" ? (
              <Avatar name={room.title} id={room.id.slice(3)} size="sm" />
            ) : (
              <HashIcon size={17} className="shrink-0 text-fg-subtle" />
            )}
            <h2 className="truncate text-fluid-base font-semibold tracking-tight">
              {room.title}
            </h2>
            {room.kind === "direct" && room.e2e && (
              <Badge tone="ok" className="hidden sm:inline-flex">
                <LockIcon size={11} />
                End-to-end
              </Badge>
            )}
            {liveStream && (
              <Badge tone="danger">
                <span className="size-1.5 rounded-full bg-danger" />
                Live · {liveStream.viewers} watching
              </Badge>
            )}
          </>
        )}
        {section !== "chat" && (
          <>
            <h2 className="truncate text-fluid-base font-semibold tracking-tight">
              {SECTION_TITLES[section]}
            </h2>
            {section === "chain" && (
              <Badge tone="accent">
                <BlocksIcon size={11} />
                Tamper evident
              </Badge>
            )}
          </>
        )}
      </div>

      <div className="flex shrink-0 items-center gap-1.5">
        <button
          type="button"
          onClick={onOpenPalette}
          className="hidden h-8 items-center gap-2 rounded-md border border-line bg-surface-3 px-2.5 text-xs text-fg-muted transition-colors hover:border-line-strong hover:text-fg sm:flex"
        >
          <SearchIcon size={14} />
          <span>Search</span>
          <Kbd>⌘K</Kbd>
        </button>

        {section !== "network" && (
          <IconButton
            label="Network status"
            onClick={onOpenPanel}
            className="relative"
          >
            <MonitorIcon size={17} />
            <span
              className={`absolute -top-0.5 -right-0.5 size-2 rounded-full border border-surface ${
                connection === "online" ? "bg-ok" : "bg-warn"
              }`}
            />
          </IconButton>
        )}

        <div
          className="hidden items-center gap-2 rounded-md border border-line bg-surface-3 px-2.5 py-1.5 md:flex"
          title={
            localOnly
              ? "Every peer is on the local network - no internet data is used"
              : "Some peers are reached over the internet"
          }
        >
          <WifiIcon
            size={14}
            className={localOnly ? "text-ok" : "text-warn"}
          />
          <div className="leading-tight">
            <p className="text-[10px] font-medium text-fg-muted">
              {online} peer{online === 1 ? "" : "s"}
            </p>
            <p className="font-mono text-[10px] text-fg-subtle">
              {internetBytes === 0
                ? "0 B internet"
                : `${(internetBytes / 1024).toFixed(0)} KB up`}
            </p>
          </div>
        </div>

        {metrics && !compact && (
          <span className="hidden font-mono text-[10px] text-fg-subtle lg:inline">
            up {formatUptime(metrics.uptimeMs)}
          </span>
        )}

        {section === "live" && (
          <Badge tone="danger" className="sm:hidden">
            <VideoIcon size={11} />
          </Badge>
        )}
      </div>
    </header>
  );
}
