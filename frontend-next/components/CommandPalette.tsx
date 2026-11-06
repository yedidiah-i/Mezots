"use client";

/**
 * The command palette.
 *
 * One search box for everything: rooms, people and actions. Opened with
 * `Ctrl+K` / `Cmd+K`, closed with `Escape`.
 */

import { useEffect, useMemo, useRef, useState } from "react";

import { Avatar, Kbd } from "./ui";
import {
  BlocksIcon,
  CloseIcon,
  HashIcon,
  MonitorIcon,
  SearchIcon,
  VideoIcon,
} from "./icons";
import type { NavSection } from "./AppRail";
import { selectRooms, useAppState } from "@/lib/store";

interface Item {
  id: string;
  title: string;
  subtitle?: string;
  kind: "room" | "person" | "action";
  icon: React.ReactNode;
  run: () => void;
}

/** The command palette. */
export function CommandPalette({
  open,
  onClose,
  onOpenRoom,
  onSection,
}: {
  open: boolean;
  onClose: () => void;
  onOpenRoom: (room: string) => void;
  onSection: (section: NavSection) => void;
}) {
  const [query, setQuery] = useState("");
  const [cursor, setCursor] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const rooms = useAppState(selectRooms);
  const peers = useAppState((state) => state.peers);
  const identity = useAppState((state) => state.identity);

  useEffect(() => {
    if (open) {
      setQuery("");
      setCursor(0);
      // Focus after the panel paints.
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [open]);

  const items = useMemo<Item[]>(() => {
    const actions: Item[] = [
      {
        id: "action-live",
        title: "Open the live studio",
        subtitle: "Broadcast your camera or screen",
        kind: "action",
        icon: <VideoIcon size={15} />,
        run: () => onSection("live"),
      },
      {
        id: "action-reels",
        title: "Open reels",
        subtitle: "Short clips from the network",
        kind: "action",
        icon: <VideoIcon size={15} />,
        run: () => onSection("reels"),
      },
      {
        id: "action-chain",
        title: "Open the private chain",
        subtitle: "Inspect the committed blocks",
        kind: "action",
        icon: <BlocksIcon size={15} />,
        run: () => onSection("chain"),
      },
      {
        id: "action-network",
        title: "Open the network view",
        subtitle: "Peers, routes and data use",
        kind: "action",
        icon: <MonitorIcon size={15} />,
        run: () => onSection("network"),
      },
    ];

    const roomItems: Item[] = rooms.map((room) => ({
      id: room.id,
      title: room.title,
      subtitle: room.kind === "direct" ? "Direct message" : "Channel",
      kind: "room",
      icon:
        room.kind === "direct" ? (
          <Avatar name={room.title} id={room.id.slice(3)} size="xs" />
        ) : (
          <HashIcon size={15} />
        ),
      run: () => onOpenRoom(room.id),
    }));

    const peopleItems: Item[] = peers.map((peer) => ({
      id: `peer-${peer.peerId}`,
      title: peer.displayName,
      subtitle: peer.connected ? "Online" : "Offline",
      kind: "person",
      icon: (
        <Avatar
          name={peer.displayName}
          id={peer.peerId}
          size="xs"
          online={peer.connected}
        />
      ),
      run: () => onOpenRoom(`dm:${peer.peerId}`),
    }));

    const all = [...roomItems, ...peopleItems, ...actions];
    const needle = query.trim().toLowerCase();
    if (!needle) return all.slice(0, 12);
    return all
      .filter(
        (item) =>
          item.title.toLowerCase().includes(needle) ||
          item.subtitle?.toLowerCase().includes(needle),
      )
      .slice(0, 12);
  }, [rooms, peers, query, onOpenRoom, onSection]);

  useEffect(() => {
    setCursor((value) => Math.min(value, Math.max(0, items.length - 1)));
  }, [items.length]);

  useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "ArrowDown") {
        event.preventDefault();
        setCursor((value) => Math.min(value + 1, items.length - 1));
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        setCursor((value) => Math.max(value - 1, 0));
      } else if (event.key === "Enter") {
        event.preventDefault();
        const item = items[cursor];
        if (item) {
          item.run();
          onClose();
        }
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, items, cursor, onClose]);

  // Keep the highlighted row in view.
  useEffect(() => {
    const list = listRef.current;
    const row = list?.children[cursor] as HTMLElement | undefined;
    row?.scrollIntoView({ block: "nearest" });
  }, [cursor]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/55 px-4 pt-[12vh] backdrop-blur-[2px]">
      <button
        type="button"
        aria-label="Close the palette"
        className="absolute inset-0 cursor-default"
        onClick={onClose}
      />
      <div className="relative w-full max-w-lg overflow-hidden rounded-xl border border-line bg-surface shadow-overlay animate-rise">
        <div className="flex items-center gap-2.5 border-b border-line px-3.5">
          <SearchIcon size={17} className="text-fg-subtle" />
          <input
            ref={inputRef}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search rooms, people and actions"
            className="h-12 min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-fg-subtle"
          />
          <Kbd>esc</Kbd>
          <button
            type="button"
            onClick={onClose}
            className="text-fg-subtle transition-colors hover:text-fg"
            aria-label="Close"
          >
            <CloseIcon size={15} />
          </button>
        </div>

        <div ref={listRef} className="max-h-[52vh] overflow-y-auto p-1.5">
          {items.length === 0 && (
            <p className="px-3 py-6 text-center text-sm text-fg-subtle">
              Nothing matches “{query}”.
            </p>
          )}
          {items.map((item, index) => (
            <button
              key={item.id}
              type="button"
              onMouseEnter={() => setCursor(index)}
              onClick={() => {
                item.run();
                onClose();
              }}
              className={`flex w-full items-center gap-3 rounded-lg px-2.5 py-2 text-left transition-colors ${
                index === cursor ? "bg-accent-soft" : "hover:bg-surface-3"
              }`}
            >
              <span className="grid size-7 shrink-0 place-items-center rounded-md bg-surface-3 text-fg-muted">
                {item.icon}
              </span>
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm font-medium">
                  {item.title}
                </span>
                {item.subtitle && (
                  <span className="block truncate text-[11px] text-fg-subtle">
                    {item.subtitle}
                  </span>
                )}
              </span>
              {index === cursor && (
                <Kbd>↵</Kbd>
              )}
            </button>
          ))}
        </div>

        <footer className="flex items-center gap-3 border-t border-line px-3.5 py-2 text-[10px] text-fg-subtle">
          <span className="flex items-center gap-1">
            <Kbd>↑</Kbd>
            <Kbd>↓</Kbd> navigate
          </span>
          <span className="flex items-center gap-1">
            <Kbd>↵</Kbd> open
          </span>
          <span className="ml-auto truncate">
            signed in as {identity?.displayName ?? "you"}
          </span>
        </footer>
      </div>
    </div>
  );
}
