"use client";

/**
 * The sidebar.
 *
 * In the conversations surface it lists channels, direct messages and the
 * streams that are live right now. In the other surfaces it lists whatever
 * belongs to that surface, so the second column is never empty.
 */

import { useMemo, useState } from "react";

import { Avatar, Badge, PresenceDot, SectionLabel } from "./ui";
import {
  BlocksIcon,
  BookIcon,
  ChevronIcon,
  HashIcon,
  LockIcon,
  MonitorIcon,
  PlusIcon,
  SearchIcon,
  VideoIcon,
} from "./icons";
import type { NavSection } from "./AppRail";
import { selectRooms, setActiveRoom, useAppState } from "@/lib/store";

/** The sidebar. */
export function Sidebar({
  section,
  onSection,
  onOpenRoom,
  onCloseDrawer,
}: {
  section: NavSection;
  onSection: (section: NavSection) => void;
  onOpenRoom: (room: string) => void;
  onCloseDrawer?: () => void;
}) {
  const [query, setQuery] = useState("");

  const rooms = useAppState(
    selectRooms,
    (a, b) => a.length === b.length && a.every((room, index) => room.id === b[index].id),
  );
  const peers = useAppState((state) => state.peers);
  const streams = useAppState((state) => state.streams);
  const activeRoom = useAppState((state) => state.activeRoom);
  const identity = useAppState((state) => state.identity);
  const ledgerHeight = useAppState((state) => state.ledger.length);
  const ledgerStatus = useAppState((state) => state.ledgerStatus);

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return rooms;
    return rooms.filter(
      (room) =>
        room.title.toLowerCase().includes(needle) ||
        room.id.toLowerCase().includes(needle),
    );
  }, [rooms, query]);

  const channels = filtered.filter((room) => room.kind === "channel");
  const directs = filtered.filter((room) => room.kind === "direct");
  const liveRooms = Object.values(streams);

  const open = (room: string) => {
    setActiveRoom(room);
    onOpenRoom(room);
    onCloseDrawer?.();
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <header className="flex items-center gap-2 px-4 pt-4 pb-3">
        <div className="min-w-0 flex-1">
          <h1 className="truncate text-fluid-lg font-semibold tracking-[-0.02em]">
            {section === "chat" && "School network"}
            {section === "live" && "Live studio"}
            {section === "reels" && "Reels"}
            {section === "chain" && "Private chain"}
            {section === "network" && "Network"}
          </h1>
          <p className="truncate text-[11px] text-fg-subtle">
            {section === "chat" && `${peers.filter((p) => p.connected).length} peers online`}
            {section === "live" && `${liveRooms.length} broadcast${liveRooms.length === 1 ? "" : "s"}`}
            {section === "reels" && "short clips from the network"}
            {section === "chain" && `${ledgerHeight} block${ledgerHeight === 1 ? "" : "s"} committed`}
            {section === "network" && "who is on this network"}
          </p>
        </div>
        <button
          type="button"
          className="grid size-8 place-items-center rounded-md text-fg-subtle transition-colors hover:bg-surface-3 hover:text-fg"
          title="Add"
        >
          <PlusIcon size={17} />
        </button>
      </header>

      <div className="px-3 pb-3">
        <label className="flex h-9 items-center gap-2 rounded-lg border border-line bg-surface-3 px-2.5 transition-colors focus-within:border-accent/60">
          <SearchIcon size={15} className="text-fg-subtle" />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search rooms and people"
            className="min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-fg-subtle"
          />
        </label>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-6">
        {section === "chat" && (
          <>
            <SectionLabel>Channels</SectionLabel>
            <ul className="mb-4 space-y-0.5">
              {channels.map((room) => (
                <li key={room.id}>
                  <button
                    type="button"
                    onClick={() => open(room.id)}
                    className={[
                      "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm transition-colors",
                      activeRoom === room.id
                        ? "bg-accent-soft font-medium text-accent"
                        : "text-fg-muted hover:bg-surface-3 hover:text-fg",
                    ].join(" ")}
                  >
                    <HashIcon size={15} className="shrink-0 opacity-70" />
                    <span className="truncate">{room.title}</span>
                  </button>
                </li>
              ))}
            </ul>

            <SectionLabel>Direct messages</SectionLabel>
            <ul className="mb-4 space-y-0.5">
              {directs.length === 0 && (
                <li className="px-2 py-1.5 text-xs text-fg-subtle">
                  Direct messages appear when a peer joins the network.
                </li>
              )}
              {directs.map((room) => {
                const peerId = room.id.slice(3);
                const peer = peers.find((item) => item.peerId === peerId);
                return (
                  <li key={room.id}>
                    <button
                      type="button"
                      onClick={() => open(room.id)}
                      className={[
                        "flex w-full items-center gap-2.5 rounded-md px-2 py-1.5 text-left transition-colors",
                        activeRoom === room.id
                          ? "bg-accent-soft"
                          : "hover:bg-surface-3",
                      ].join(" ")}
                    >
                      <Avatar
                        name={room.title}
                        id={peerId}
                        size="sm"
                        online={peer?.connected}
                      />
                      <span
                        className={[
                          "min-w-0 flex-1 truncate text-sm",
                          activeRoom === room.id
                            ? "font-medium text-accent"
                            : "text-fg-muted",
                        ].join(" ")}
                      >
                        {room.title}
                      </span>
                      {room.e2e && (
                        <LockIcon size={13} className="shrink-0 text-ok/70" />
                      )}
                    </button>
                  </li>
                );
              })}
            </ul>

            {liveRooms.length > 0 && (
              <>
                <SectionLabel>Live now</SectionLabel>
                <ul className="mb-4 space-y-0.5">
                  {liveRooms.map((stream) => (
                    <li key={stream.room}>
                      <button
                        type="button"
                        onClick={() => {
                          onSection("live");
                          onCloseDrawer?.();
                        }}
                        className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm text-fg-muted transition-colors hover:bg-surface-3 hover:text-fg"
                      >
                        <span className="relative grid size-4 place-items-center">
                          <span className="absolute size-2 rounded-full bg-danger animate-pulse-ring" />
                          <span className="size-1.5 rounded-full bg-danger" />
                        </span>
                        <span className="min-w-0 flex-1 truncate">
                          {stream.title || stream.room}
                        </span>
                        <span className="text-[11px] text-fg-subtle">
                          {stream.viewers}
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              </>
            )}
          </>
        )}

        {section === "live" && (
          <>
            <SectionLabel>Broadcast rooms</SectionLabel>
            <ul className="mb-4 space-y-0.5">
              {liveRooms.length === 0 && (
                <li className="px-2 py-1.5 text-xs text-fg-subtle">
                  Nobody is broadcasting. Start one from the studio.
                </li>
              )}
              {liveRooms.map((stream) => (
                <li key={stream.room}>
                  <button
                    type="button"
                    className="flex w-full items-center gap-2 rounded-md bg-surface-3 px-2 py-1.5 text-left text-sm text-fg-muted transition-colors hover:bg-surface-inset"
                  >
                    <VideoIcon size={15} className="text-danger" />
                    <span className="min-w-0 flex-1 truncate">
                      {stream.title || stream.room}
                    </span>
                    <Badge tone="danger">{stream.viewers}</Badge>
                  </button>
                </li>
              ))}
            </ul>
            <SectionLabel>Shortcuts</SectionLabel>
            <ul className="space-y-0.5">
              <li>
                <button
                  type="button"
                  onClick={() => onSection("chat")}
                  className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm text-fg-muted transition-colors hover:bg-surface-3 hover:text-fg"
                >
                  <HashIcon size={15} />
                  Back to conversations
                </button>
              </li>
            </ul>
          </>
        )}

        {section === "reels" && (
          <>
            <SectionLabel>Feeds</SectionLabel>
            <ul className="mb-4 space-y-0.5">
              <li>
                <button
                  type="button"
                  className="flex w-full items-center gap-2 rounded-md bg-accent-soft px-2 py-1.5 text-left text-sm font-medium text-accent"
                >
                  <BookIcon size={15} />
                  For you
                </button>
              </li>
              <li>
                <button
                  type="button"
                  className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm text-fg-muted transition-colors hover:bg-surface-3 hover:text-fg"
                >
                  <HashIcon size={15} />
                  From your class
                </button>
              </li>
            </ul>
            <SectionLabel>Shortcuts</SectionLabel>
            <ul className="space-y-0.5">
              <li>
                <button
                  type="button"
                  onClick={() => onSection("live")}
                  className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm text-fg-muted transition-colors hover:bg-surface-3 hover:text-fg"
                >
                  <VideoIcon size={15} />
                  Record a reel in the studio
                </button>
              </li>
            </ul>
          </>
        )}

        {section === "chain" && (
          <>
            <SectionLabel>Chain health</SectionLabel>
            <div className="mb-4 rounded-lg border border-line bg-surface-3 p-3">
              <div className="mb-2 flex items-center justify-between">
                <span className="text-xs text-fg-muted">Height</span>
                <span className="font-mono text-sm">{ledgerHeight}</span>
              </div>
              <div className="mb-2 flex items-center justify-between">
                <span className="text-xs text-fg-muted">State</span>
                <Badge tone={ledgerStatus?.valid === false ? "danger" : "ok"}>
                  {ledgerStatus?.valid === false ? "broken" : "verified"}
                </Badge>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-xs text-fg-muted">Agreement</span>
                <span className="text-sm">
                  {ledgerStatus
                    ? `${ledgerStatus.peersAgree}/${ledgerStatus.peersTotal}`
                    : "—"}
                </span>
              </div>
            </div>
            <SectionLabel>Shortcuts</SectionLabel>
            <ul className="space-y-0.5">
              <li>
                <button
                  type="button"
                  onClick={() => onSection("network")}
                  className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm text-fg-muted transition-colors hover:bg-surface-3 hover:text-fg"
                >
                  <MonitorIcon size={15} />
                  Inspect the peer mesh
                </button>
              </li>
            </ul>
          </>
        )}

        {section === "network" && (
          <>
            <SectionLabel>Peers</SectionLabel>
            <ul className="mb-4 space-y-0.5">
              {peers.length === 0 && (
                <li className="px-2 py-1.5 text-xs text-fg-subtle">
                  No peers found yet. Nodes announce themselves over mDNS.
                </li>
              )}
              {peers.map((peer) => (
                <li
                  key={peer.peerId}
                  className="flex items-center gap-2.5 rounded-md px-2 py-1.5"
                >
                  <Avatar
                    name={peer.displayName}
                    id={peer.peerId}
                    size="sm"
                    online={peer.connected}
                  />
                  <span className="min-w-0 flex-1 truncate text-sm text-fg-muted">
                    {peer.displayName}
                  </span>
                  <PresenceDot online={peer.connected} />
                </li>
              ))}
            </ul>
            <SectionLabel>Shortcuts</SectionLabel>
            <ul className="space-y-0.5">
              <li>
                <button
                  type="button"
                  onClick={() => onSection("chat")}
                  className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm text-fg-muted transition-colors hover:bg-surface-3 hover:text-fg"
                >
                  <HashIcon size={15} />
                  Back to conversations
                </button>
              </li>
            </ul>
          </>
        )}
      </div>

      <footer className="border-t border-line px-4 py-3">
        <div className="flex items-center gap-2.5">
          <Avatar
            name={identity?.displayName ?? "You"}
            id={identity?.nodeId}
            size="sm"
          />
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-medium">
              {identity?.displayName ?? "You"}
            </p>
            <p className="truncate font-mono text-[10px] text-fg-subtle">
              {identity?.fingerprint.slice(0, 16) ?? "········"}
            </p>
          </div>
          <ChevronIcon direction="up" size={14} className="text-fg-subtle" />
        </div>
      </footer>
    </div>
  );
}
