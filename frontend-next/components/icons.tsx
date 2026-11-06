/**
 * The icon set.
 *
 * Every glyph is an inline SVG so the app ships without an icon package. All
 * of them inherit `currentColor` and use a 24-unit grid.
 */

import type { SVGProps } from "react";

type IconProps = SVGProps<SVGSVGElement> & { size?: number };

/** The shared frame for every icon. */
function Icon({ size = 18, children, ...props }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...props}
    >
      {children}
    </svg>
  );
}

/** A `#` channel glyph. */
export function HashIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M9.5 4 8 20M16 4l-1.5 16M4 9h16M3.5 15h16" />
    </Icon>
  );
}

/** A padlock, for end-to-end encrypted conversations. */
export function LockIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <rect x="4" y="10" width="16" height="11" rx="2.5" />
      <path d="M8 10V7a4 4 0 0 1 8 0v3" />
    </Icon>
  );
}

/** An open padlock, for conversations without end-to-end encryption. */
export function UnlockIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <rect x="4" y="10" width="16" height="11" rx="2.5" />
      <path d="M8 10V7a4 4 0 0 1 7.5-2" />
    </Icon>
  );
}

/** A shield with a check, for the chain. */
export function ShieldIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M12 3 5 6v6c0 4.2 2.9 7.9 7 9 4.1-1.1 7-4.8 7-9V6l-7-3Z" />
      <path d="m9 12 2 2 4-4" />
    </Icon>
  );
}

/** A video camera, for the studio. */
export function VideoIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <rect x="3" y="6" width="12" height="12" rx="2.5" />
      <path d="m15 11 6-3.5v9L15 13" />
    </Icon>
  );
}

/** A play triangle. */
export function PlayIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M8 5.5v13l11-6.5-11-6.5Z" />
    </Icon>
  );
}

/** A broadcast dot, for the live indicator. */
export function BroadcastIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="12" cy="12" r="2.5" fill="currentColor" stroke="none" />
      <path d="M7.8 7.8a6 6 0 0 0 0 8.4M16.2 7.8a6 6 0 0 1 0 8.4" />
      <path d="M4.9 4.9a10 10 0 0 0 0 14.2M19.1 4.9a10 10 0 0 1 0 14.2" />
    </Icon>
  );
}

/** A paper plane, for the send button. */
export function SendIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M4 12 20 4l-6 16-2.5-6.5L4 12Z" />
    </Icon>
  );
}

/** A magnifying glass. */
export function SearchIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="11" cy="11" r="7" />
      <path d="m20 20-3.5-3.5" />
    </Icon>
  );
}

/** A cog. */
export function SettingsIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 14a1.6 1.6 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.6 1.6 0 0 0-2.7 1.1V20a2 2 0 1 1-4 0v-.1A1.6 1.6 0 0 0 7.5 18l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1A1.6 1.6 0 0 0 3 12.5H3a2 2 0 1 1 0-4h.1A1.6 1.6 0 0 0 4.6 6l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1A1.6 1.6 0 0 0 10 3.1V3a2 2 0 1 1 4 0v.1a1.6 1.6 0 0 0 2.7 1.1l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.6 1.6 0 0 0 1.1 2.7H21a2 2 0 1 1 0 4h-.1a1.6 1.6 0 0 0-1.5 1.3Z" />
    </Icon>
  );
}

/** A globe, for the network view. */
export function GlobeIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="12" cy="12" r="9" />
      <path d="M3 12h18M12 3a15 15 0 0 1 0 18 15 15 0 0 1 0-18Z" />
    </Icon>
  );
}

/** A group of people. */
export function UsersIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="9" cy="8" r="3.2" />
      <path d="M3.5 20a5.5 5.5 0 0 1 11 0" />
      <path d="M16 5.2a3.2 3.2 0 0 1 0 5.9M17 14.6a5.5 5.5 0 0 1 3.5 5.4" />
    </Icon>
  );
}

/** A microphone. */
export function MicIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <rect x="9" y="3" width="6" height="11" rx="3" />
      <path d="M5.5 11.5a6.5 6.5 0 0 0 13 0M12 18v3" />
    </Icon>
  );
}

/** A microphone with a slash. */
export function MicOffIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M15 5v-.5a3 3 0 0 0-5.8-1M9 9v3.5a3 3 0 0 0 4.6 2.5" />
      <path d="M5.5 11.5a6.5 6.5 0 0 0 9.9 5.6M18.5 11.5v.5M12 18v3" />
      <path d="m3 3 18 18" />
    </Icon>
  );
}

/** A camera. */
export function CameraIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M3 8.5A2.5 2.5 0 0 1 5.5 6h1.8l1.2-2h7l1.2 2h1.8A2.5 2.5 0 0 1 21 8.5v9A2.5 2.5 0 0 1 18.5 20h-13A2.5 2.5 0 0 1 3 17.5v-9Z" />
      <circle cx="12" cy="13" r="3.6" />
    </Icon>
  );
}

/** A screen with an arrow, for screen sharing. */
export function ScreenIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <rect x="2.5" y="4" width="19" height="13" rx="2" />
      <path d="M8 21h8M12 17v4" />
    </Icon>
  );
}

/** A heart, for reels. */
export function HeartIcon({ filled, ...props }: IconProps & { filled?: boolean }) {
  return (
    <Icon {...props} fill={filled ? "currentColor" : "none"}>
      <path d="M12 20s-7.5-4.6-7.5-9.6A4.4 4.4 0 0 1 12 7.4a4.4 4.4 0 0 1 7.5 3C19.5 15.4 12 20 12 20Z" />
    </Icon>
  );
}

/** A speech bubble, for reel comments. */
export function CommentIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M20 15a2.5 2.5 0 0 1-2.5 2.5H8l-4 3v-13A2.5 2.5 0 0 1 6.5 5h11A2.5 2.5 0 0 1 20 7.5V15Z" />
    </Icon>
  );
}

/** A share arrow. */
export function ShareIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M12 15V4m0 0L8.5 7.5M12 4l3.5 3.5" />
      <path d="M5 13v5.5A1.5 1.5 0 0 0 6.5 20h11a1.5 1.5 0 0 0 1.5-1.5V13" />
    </Icon>
  );
}

/** A chevron. */
export function ChevronIcon({
  direction = "down",
  ...props
}: IconProps & { direction?: "up" | "down" | "left" | "right" }) {
  const rotation = {
    up: "rotate(180 12 12)",
    down: "",
    left: "rotate(90 12 12)",
    right: "rotate(-90 12 12)",
  }[direction];
  return (
    <Icon {...props}>
      <path d="m6 9 6 6 6-6" transform={rotation} />
    </Icon>
  );
}

/** A plus sign. */
export function PlusIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M12 5v14M5 12h14" />
    </Icon>
  );
}

/** A cross. */
export function CloseIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="m6 6 12 12M18 6 6 18" />
    </Icon>
  );
}

/** A check mark. */
export function CheckIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="m5 12.5 4.5 4.5L19 7" />
    </Icon>
  );
}

/** A double check, for read receipts. */
export function DoubleCheckIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="m2.5 12.5 4 4L15 8" />
      <path d="m11 12.5 3.5 3.5L21.5 8" />
    </Icon>
  );
}

/** Three dots. */
export function DotsIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="5.5" cy="12" r="1.4" fill="currentColor" stroke="none" />
      <circle cx="12" cy="12" r="1.4" fill="currentColor" stroke="none" />
      <circle cx="18.5" cy="12" r="1.4" fill="currentColor" stroke="none" />
    </Icon>
  );
}

/** A pushpin. */
export function PinIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M9 4h6l-1 6 3.5 3H6.5L10 10 9 4Z" />
      <path d="M12 13v7" />
    </Icon>
  );
}

/** A reply arrow. */
export function ReplyIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M10 8V4L3.5 10.5 10 17v-4.5c5 0 8 1.5 9.5 5-.5-6-3.5-9.5-9.5-9.5Z" />
    </Icon>
  );
}

/** A smiling face, for reactions. */
export function SmileIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="12" cy="12" r="9" />
      <path d="M8.5 14.5a4.5 4.5 0 0 0 7 0" />
      <path d="M9 9.5h.01M15 9.5h.01" />
    </Icon>
  );
}

/** A paperclip. */
export function ClipIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M20 11.5 12.2 19.3a4.6 4.6 0 0 1-6.5-6.5l8-8a3.1 3.1 0 0 1 4.4 4.4l-7.9 8a1.5 1.5 0 0 1-2.2-2.2l7.3-7.3" />
    </Icon>
  );
}

/** A lightning bolt, for the fast path. */
export function BoltIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M13.5 2 4 13.5h6L10.5 22 20 10.5h-6L13.5 2Z" />
    </Icon>
  );
}

/** A book, for the library channel. */
export function BookIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M4 5.5A2.5 2.5 0 0 1 6.5 3H19v15H6.5A2.5 2.5 0 0 0 4 20.5v-15Z" />
      <path d="M4 20.5A2.5 2.5 0 0 1 6.5 18H19v3H6.5A2.5 2.5 0 0 1 4 20.5Z" />
    </Icon>
  );
}

/** A graduation cap. */
export function CapIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="m12 4 10 5-10 5L2 9l10-5Z" />
      <path d="M6 11.5V17c0 1.7 2.7 3 6 3s6-1.3 6-3v-5.5" />
    </Icon>
  );
}

/** A stack of blocks, for the chain. */
export function BlocksIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="m12 3 8 4.5-8 4.5-8-4.5L12 3Z" />
      <path d="m4 12 8 4.5 8-4.5M4 16.5 12 21l8-4.5" />
    </Icon>
  );
}

/** A wifi glyph, for the LAN indicator. */
export function WifiIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M2.5 9a15 15 0 0 1 19 0M5.5 12.5a10.5 10.5 0 0 1 13 0M8.5 16a6 6 0 0 1 7 0" />
      <path d="M12 19.5h.01" />
    </Icon>
  );
}

/** A key. */
export function KeyIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="8" cy="16" r="4" />
      <path d="m11 13 8-8 2 2-1.5 1.5L21 10l-2 2-1.5-1.5L16 12l-2-2" />
    </Icon>
  );
}

/** A link in a chain, for the chain view. */
export function LinkIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M10 13a5 5 0 0 0 7.5.5l2-2a5 5 0 0 0-7-7l-1 1" />
      <path d="M14 11a5 5 0 0 0-7.5-.5l-2 2a5 5 0 0 0 7 7l1-1" />
    </Icon>
  );
}

/** A clock, for delivery state. */
export function ClockIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="12" cy="12" r="8.5" />
      <path d="M12 7.5V12l3 2" />
    </Icon>
  );
}

/** A bell, for notifications. */
export function BellIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M6 9a6 6 0 0 1 12 0c0 4 1.5 5.5 1.5 5.5h-15S6 13 6 9Z" />
      <path d="M10 18a2.2 2.2 0 0 0 4 0" />
    </Icon>
  );
}

/** A sun, for the light theme. */
export function SunIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2.5M12 19.5V22M2 12h2.5M19.5 12H22M4.9 4.9l1.8 1.8M17.3 17.3l1.8 1.8M19.1 4.9l-1.8 1.8M6.7 17.3l-1.8 1.8" />
    </Icon>
  );
}

/** A moon, for the dark theme. */
export function MoonIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M20 14.5A8.5 8.5 0 0 1 9.5 4a8.5 8.5 0 1 0 10.5 10.5Z" />
    </Icon>
  );
}

/** A monitor, for the network mesh view. */
export function MonitorIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <rect x="2.5" y="4" width="19" height="13" rx="2" />
      <path d="M8.5 21h7M12 17v4" />
    </Icon>
  );
}

/** A phone, for the mobile layout hint. */
export function PhoneIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <rect x="6" y="2.5" width="12" height="19" rx="2.5" />
      <path d="M11 18.5h2" />
    </Icon>
  );
}
