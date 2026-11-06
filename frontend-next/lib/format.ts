/**
 * Small formatting helpers used across the interface.
 */

/** Format a timestamp as `HH:MM` in the viewer's locale. */
export function formatTime(ts: number): string {
  return new Date(ts).toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** Format a timestamp as a full date and time. */
export function formatDateTime(ts: number): string {
  return new Date(ts).toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  });
}

/** The calendar day a timestamp falls on, as `YYYY-MM-DD`. */
export function dayKey(ts: number): string {
  const date = new Date(ts);
  const month = `${date.getMonth() + 1}`.padStart(2, "0");
  const day = `${date.getDate()}`.padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}

/** A friendly label for a day: `Today`, `Yesterday` or a date. */
export function formatDay(ts: number): string {
  const today = dayKey(Date.now());
  const yesterday = dayKey(Date.now() - 86_400_000);
  const key = dayKey(ts);
  if (key === today) return "Today";
  if (key === yesterday) return "Yesterday";
  return new Date(ts).toLocaleDateString(undefined, {
    weekday: "long",
    day: "numeric",
    month: "long",
    year:
      new Date(ts).getFullYear() === new Date().getFullYear() ? undefined : "numeric",
  });
}

/** A short relative time such as `3m ago`. */
export function relativeTime(ts: number): string {
  const seconds = Math.max(0, Math.floor((Date.now() - ts) / 1000));
  if (seconds < 45) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days}d ago`;
  return new Date(ts).toLocaleDateString(undefined, {
    day: "numeric",
    month: "short",
  });
}

/** A duration such as `1:05` or `1:02:03`. */
export function formatDuration(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = total % 60;
  const pad = (value: number) => `${value}`.padStart(2, "0");
  if (hours > 0) return `${hours}:${pad(minutes)}:${pad(seconds)}`;
  return `${minutes}:${pad(seconds)}`;
}

/** A byte count such as `4.2 MB`. */
export function formatBytes(bytes: number): string {
  if (bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const exponent = Math.min(
    units.length - 1,
    Math.floor(Math.log(bytes) / Math.log(1024)),
  );
  const value = bytes / 1024 ** exponent;
  const decimals = value >= 100 || exponent === 0 ? 0 : 1;
  return `${value.toFixed(decimals)} ${units[exponent]}`;
}

/** How long ago something happened, in words. */
export function formatUptime(ms: number): string {
  const seconds = Math.floor(ms / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ${minutes % 60}m`;
  return `${Math.floor(hours / 24)}d ${hours % 24}h`;
}

/** The initials shown in an avatar. */
export function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "?";
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return `${parts[0][0]}${parts[parts.length - 1][0]}`.toUpperCase();
}

/**
 * A stable colour for an identity.
 *
 * The same node always gets the same hue, so people are recognisable at a
 * glance without the node having to send an avatar.
 */
export function colourFor(seed: string): number {
  let hash = 0;
  for (let i = 0; i < seed.length; i += 1) {
    hash = (hash * 31 + seed.charCodeAt(i)) | 0;
  }
  return Math.abs(hash) % 360;
}

/** Turn a plain URL into a safe `href`. */
export function safeHref(url: string): string {
  if (/^(https?:|blob:|data:image\/)/i.test(url)) return url;
  return "#";
}

/** Split text into words for the mention parser. */
export function extractMentions(text: string): string[] {
  const matches = text.match(/@([a-z0-9._-]{2,32})/gi);
  return matches ? Array.from(new Set(matches.map((m) => m.slice(1)))) : [];
}
