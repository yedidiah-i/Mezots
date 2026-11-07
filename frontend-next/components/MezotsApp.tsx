"use client";

/**
 * The application shell.
 *
 * Three columns on a desktop, one on a phone:
 *
 * ```
 * ┌────┬──────────────┬───────────────────────────┬──────────────┐
 * │rail│ sidebar      │ main view                 │ network      │
 * │ 68 │ 268          │ flex                      │ 320          │
 * └────┴──────────────┴───────────────────────────┴──────────────┘
 * ```
 *
 * The rail switches between the five surfaces - conversations, live, reels,
 * the chain and the network - and the sidebar adapts to whichever is open.
 */

import { useCallback, useEffect, useMemo, useState } from "react";

import { AppRail, type NavSection } from "./AppRail";
import { Sidebar } from "./Sidebar";
import { TopBar } from "./TopBar";
import { ChannelView } from "./ChannelView";
import { DirectView } from "./DirectView";
import { StudioView } from "./StudioView";
import { ReelsView } from "./ReelsView";
import { LedgerView } from "./LedgerView";
import { NetworkView } from "./NetworkView";
import { CommandPalette } from "./CommandPalette";
import { SettingsSheet } from "./SettingsSheet";
import { NetworkPanel } from "./NetworkPanel";
import { Toast } from "./Toast";

import { bootstrap, selectRooms, useAppState } from "@/lib/store";
import { useIsCompact } from "@/lib/useMediaQuery";
import { ThemeProvider } from "./ThemeProvider";

/** The root of the interface. */
export function MezotsApp() {
  return (
    <ThemeProvider>
      <MezotsShell />
    </ThemeProvider>
  );
}

function MezotsShell() {
  const [section, setSection] = useState<NavSection>("chat");
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [panelOpen, setPanelOpen] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [ready, setReady] = useState(false);

  const compact = useIsCompact();
  const activeRoom = useAppState((state) => state.activeRoom);
  const rooms = useAppState(selectRooms, (a, b) => a.length === b.length && a.every((room, index) => room.id === b[index].id));
  const lastError = useAppState((state) => state.lastError);

  // Start the socket and load the identity.
  useEffect(() => {
    let cancelled = false;
    bootstrap()
      .catch((error) => {
        console.error("[mezots] bootstrap failed", error);
      })
      .finally(() => {
        if (!cancelled) setReady(true);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Command palette shortcut.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing =
        target?.tagName === "INPUT" ||
        target?.tagName === "TEXTAREA" ||
        target?.isContentEditable;
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setPaletteOpen((open) => !open);
      }
      if (event.key === "Escape") {
        setPaletteOpen(false);
        setSettingsOpen(false);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const openRoom = useCallback(
    (room: string) => {
      setSection("chat");
      setDrawerOpen(false);
      // The store performs the join.
      import("@/lib/store").then(({ setActiveRoom }) => setActiveRoom(room));
    },
    [],
  );

  const currentRoom = useMemo(
    () => rooms.find((room) => room.id === activeRoom),
    [rooms, activeRoom],
  );

  const main = useMemo(() => {
    switch (section) {
      case "chat":
        return currentRoom?.kind === "direct" ? (
          <DirectView room={currentRoom} />
        ) : (
          <ChannelView room={currentRoom} />
        );
      case "live":
        return <StudioView />;
      case "reels":
        return <ReelsView />;
      case "chain":
        return <LedgerView />;
      case "network":
        return <NetworkView />;
      default:
        return null;
    }
  }, [section, currentRoom]);

  return (
    <div className="relative flex h-dvh w-full overflow-hidden text-fg selection:bg-accent/20">
      <AppRail
        section={section}
        onSection={setSection}
        onSettings={() => setSettingsOpen(true)}
      />

      {/* The sidebar. On a phone it slides over the main view. */}
      <div
        className={[
          "z-30 flex shrink-0 flex-col border-r border-line bg-surface-2/70 backdrop-blur-2xl",
          compact ? "absolute inset-y-0 left-[72px]" : "relative w-[276px]",
          compact
            ? drawerOpen
              ? "translate-x-0 shadow-overlay"
              : "-translate-x-full"
            : "",
          "transition-transform duration-300 ease-[cubic-bezier(0.16,1,0.3,1)]",
        ].join(" ")}
      >
        <Sidebar
          section={section}
          onSection={setSection}
          onOpenRoom={openRoom}
          onCloseDrawer={() => setDrawerOpen(false)}
        />
      </div>

      {compact && drawerOpen && (
        <button
          type="button"
          aria-label="Close the sidebar"
          className="absolute inset-0 z-20 cursor-default bg-black/50 backdrop-blur-[2px]"
          onClick={() => setDrawerOpen(false)}
        />
      )}

      <main className="relative z-10 flex min-w-0 flex-1 flex-col bg-bg/55 backdrop-blur-xl">
        <TopBar
          section={section}
          room={currentRoom}
          onOpenDrawer={() => setDrawerOpen(true)}
          onOpenPanel={() => setPanelOpen(true)}
          onOpenPalette={() => setPaletteOpen(true)}
          compact={compact}
        />
        <div className="min-h-0 flex-1">{main}</div>
      </main>

      {/* The network panel, docked on wide screens and a sheet otherwise. */}
      {!compact && (
        <aside className="hidden w-[320px] shrink-0 overflow-y-auto border-l border-line bg-surface-2/70 backdrop-blur-2xl xl:block">
          <NetworkPanel />
        </aside>
      )}

      {compact && panelOpen && (
        <div className="absolute inset-0 z-40 flex justify-end bg-black/50">
          <div className="w-[min(360px,90vw)] overflow-y-auto border-l border-line bg-surface-2">
            <NetworkPanel onClose={() => setPanelOpen(false)} />
          </div>
        </div>
      )}

      <CommandPalette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        onOpenRoom={openRoom}
        onSection={(next) => {
          setSection(next);
          setPaletteOpen(false);
        }}
      />

      <SettingsSheet open={settingsOpen} onClose={() => setSettingsOpen(false)} />

      <Toast message={lastError} onDismiss={() => {
        import("@/lib/store").then(({ store }) =>
          store.setState((previous) => ({ ...previous, lastError: null })),
        );
      }} />

      {!ready && (
        <div className="absolute inset-0 z-50 grid place-items-center bg-bg">
          <div className="flex flex-col items-center gap-3">
            <div className="size-8 animate-spin rounded-full border-2 border-line border-t-accent" />
            <p className="text-sm text-fg-muted">Starting Mezots…</p>
          </div>
        </div>
      )}
    </div>
  );
}
