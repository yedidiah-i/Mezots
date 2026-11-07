"use client";

/**
 * The far-left rail.
 *
 * One icon per surface, plus the local identity at the bottom. This is the
 * only permanently visible navigation, so it stays narrow on purpose.
 */

import { Avatar, IconButton } from "./ui";
import {
  BlocksIcon,
  BookIcon,
  HashIcon,
  MonitorIcon,
  MoonIcon,
  SettingsIcon,
  SunIcon,
  VideoIcon,
} from "./icons";
import { useTheme } from "./ThemeProvider";
import { useAppState } from "@/lib/store";

/** The surfaces the rail can open. */
export type NavSection = "chat" | "live" | "reels" | "chain" | "network";

const NAV: Array<{
  id: NavSection;
  label: string;
  icon: typeof HashIcon;
}> = [
  { id: "chat", label: "Conversations", icon: HashIcon },
  { id: "live", label: "Live studio", icon: VideoIcon },
  { id: "reels", label: "Reels", icon: BookIcon },
  { id: "chain", label: "Chain", icon: BlocksIcon },
  { id: "network", label: "Network", icon: MonitorIcon },
];

/** The navigation rail. */
export function AppRail({
  section,
  onSection,
  onSettings,
}: {
  section: NavSection;
  onSection: (section: NavSection) => void;
  onSettings: () => void;
}) {
  const { resolved, toggle } = useTheme();
  const identity = useAppState((state) => state.identity);
  const peersOnline = useAppState(
    (state) => state.peers.filter((peer) => peer.connected).length,
  );
  const liveCount = useAppState(
    (state) => Object.keys(state.streams).length,
  );

  return (
    <nav className="z-40 flex w-[72px] shrink-0 flex-col items-center gap-1 border-r border-line bg-surface/90 py-4 backdrop-blur-xl">
      <div
        className="mb-2 grid size-10 place-items-center rounded-xl bg-gradient-to-br from-accent to-accent-2 font-bold text-accent-fg shadow-soft"
        title="Mezots"
      >
        M
      </div>

      {NAV.map((item) => {
        const Icon = item.icon;
        const active = section === item.id;
        const badge =
          item.id === "network" && peersOnline > 0
            ? peersOnline
            : item.id === "live" && liveCount > 0
              ? liveCount
              : null;
        return (
          <button
            key={item.id}
            type="button"
            onClick={() => onSection(item.id)}
            title={item.label}
            aria-label={item.label}
            aria-current={active ? "page" : undefined}
            className={[
              "group relative grid size-11 place-items-center rounded-xl transition-all duration-200",
              active
                ? "bg-accent-soft text-accent shadow-soft"
                : "text-fg-subtle hover:scale-[1.06] hover:bg-surface-3 hover:text-fg",
            ].join(" ")}
          >
            <Icon size={20} strokeWidth={active ? 2 : 1.7} />
            {badge !== null && (
              <span className="absolute -top-0.5 -right-0.5 grid min-w-4.5 place-items-center rounded-full bg-accent px-1 text-[10px] font-bold text-accent-fg shadow-soft">
                {badge}
              </span>
            )}
            {active && (
              <span className="absolute top-1/2 -left-[13px] h-6 w-[3px] -translate-y-1/2 rounded-r-full bg-accent shadow-[0_0_12px_var(--accent)]" />
            )}
          </button>
        );
      })}

      <div className="mt-auto flex flex-col items-center gap-1.5">
        <IconButton
          label={resolved === "dark" ? "Switch to the light theme" : "Switch to the dark theme"}
          onClick={toggle}
        >
          {resolved === "dark" ? <SunIcon size={17} /> : <MoonIcon size={17} />}
        </IconButton>
        <IconButton label="Settings" onClick={onSettings}>
          <SettingsIcon size={17} />
        </IconButton>
        <div title={identity?.displayName ?? "You"} className="mt-1">
          <Avatar
            name={identity?.displayName ?? "You"}
            id={identity?.nodeId}
            size="md"
            className="ring-2 ring-line"
          />
        </div>
      </div>
    </nav>
  );
}
