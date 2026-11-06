/**
 * Interface primitives.
 *
 * Small, composable pieces that every view builds from. They read their
 * colours from the design tokens, so the light and dark palettes come for
 * free.
 */

import type { ButtonHTMLAttributes, ReactNode } from "react";

import { colourFor, initials } from "@/lib/format";

/* -------------------------------------------------------------------------- */
/* Avatar                                                                      */
/* -------------------------------------------------------------------------- */

const AVATAR_SIZES = {
  xs: "size-6 text-[10px]",
  sm: "size-8 text-[11px]",
  md: "size-9 text-xs",
  lg: "size-11 text-sm",
  xl: "size-14 text-base",
} as const;

/** Size of an avatar. */
export type AvatarSize = keyof typeof AVATAR_SIZES;

/** A coloured initial-letter avatar. */
export function Avatar({
  name,
  id,
  size = "md",
  className = "",
  online,
}: {
  name: string;
  id?: string;
  size?: AvatarSize;
  className?: string;
  online?: boolean;
}) {
  const seed = id ?? name;
  const hue = colourFor(seed);
  return (
    <span
      className={`relative inline-grid shrink-0 place-items-center rounded-full font-semibold tracking-tight text-white select-none ${AVATAR_SIZES[size]} ${className}`}
      style={{
        background: `linear-gradient(140deg, hsl(${hue} 62% 46%), hsl(${(hue + 38) % 360} 58% 34%))`,
      }}
      aria-hidden="true"
    >
      {initials(name)}
      {online !== undefined && (
        <PresenceDot online={online} className="absolute -right-0.5 -bottom-0.5" />
      )}
    </span>
  );
}

/** A small dot that shows whether someone is online. */
export function PresenceDot({
  online,
  className = "",
}: {
  online: boolean;
  className?: string;
}) {
  return (
    <span
      className={`block size-2.5 rounded-full border-2 border-surface ${className}`}
      style={{ background: online ? "var(--ok)" : "var(--fg-subtle)" }}
      title={online ? "Online" : "Offline"}
    />
  );
}

/* -------------------------------------------------------------------------- */
/* Buttons                                                                     */
/* -------------------------------------------------------------------------- */

type ButtonVariant = "primary" | "secondary" | "ghost" | "danger" | "soft";

const BUTTON_VARIANTS: Record<ButtonVariant, string> = {
  primary:
    "bg-accent text-accent-fg hover:brightness-110 active:brightness-95 shadow-soft",
  secondary:
    "bg-surface-3 text-fg border border-line hover:bg-surface-inset hover:border-line-strong",
  ghost: "text-fg-muted hover:text-fg hover:bg-surface-3",
  danger: "bg-danger text-white hover:brightness-110",
  soft: "bg-accent-soft text-accent hover:bg-accent/20",
};

/** A button. */
export function Button({
  variant = "secondary",
  size = "md",
  className = "",
  children,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ButtonVariant;
  size?: "sm" | "md" | "lg";
}) {
  const sizes = {
    sm: "h-7 px-2.5 text-xs gap-1.5",
    md: "h-9 px-3.5 text-sm gap-2",
    lg: "h-11 px-5 text-[15px] gap-2",
  }[size];
  return (
    <button
      className={`inline-flex items-center justify-center rounded-md font-medium transition-[background,color,filter,transform] duration-150 active:scale-[0.98] disabled:pointer-events-none disabled:opacity-45 ${sizes} ${BUTTON_VARIANTS[variant]} ${className}`}
      {...props}
    >
      {children}
    </button>
  );
}

/** A square button that holds an icon. */
export function IconButton({
  label,
  className = "",
  children,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  label: string;
  variant?: ButtonVariant;
}) {
  const variant = props.variant ?? "ghost";
  return (
    <button
      title={label}
      aria-label={label}
      className={`inline-grid size-8 place-items-center rounded-md transition-colors duration-150 ${BUTTON_VARIANTS[variant]} ${className}`}
      {...props}
    >
      {children}
    </button>
  );
}

/* -------------------------------------------------------------------------- */
/* Small display pieces                                                        */
/* -------------------------------------------------------------------------- */

/** A pill for a status or a count. */
export function Badge({
  children,
  tone = "neutral",
  className = "",
}: {
  children: ReactNode;
  tone?: "neutral" | "accent" | "ok" | "warn" | "danger";
  className?: string;
}) {
  const tones = {
    neutral: "bg-surface-3 text-fg-muted border-line",
    accent: "bg-accent-soft text-accent border-accent/25",
    ok: "bg-ok-soft text-ok border-ok/25",
    warn: "bg-warn-soft text-warn border-warn/25",
    danger: "bg-danger-soft text-danger border-danger/25",
  }[tone];
  return (
    <span
      className={`inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11px] leading-4 font-medium ${tones} ${className}`}
    >
      {children}
    </span>
  );
}

/** A keyboard key. */
export function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="inline-flex h-5 min-w-5 items-center justify-center rounded border border-line bg-surface-3 px-1.5 font-mono text-[10px] font-medium text-fg-muted">
      {children}
    </kbd>
  );
}

/** A small heading above a group of items. */
export function SectionLabel({
  children,
  action,
}: {
  children: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between px-2 pb-1">
      <span className="text-[11px] font-semibold tracking-[0.08em] text-fg-subtle uppercase">
        {children}
      </span>
      {action}
    </div>
  );
}

/** The pulsing dot used for live indicators. */
export function LiveDot({ className = "" }: { className?: string }) {
  return (
    <span className={`relative inline-grid size-2 place-items-center ${className}`}>
      <span
        className="absolute inline-block size-2 rounded-full bg-danger animate-pulse-ring"
        aria-hidden="true"
      />
      <span className="relative inline-block size-1.5 rounded-full bg-danger" />
    </span>
  );
}

/** A thin progress bar. */
export function Meter({
  value,
  max = 1,
  tone = "accent",
  className = "",
}: {
  value: number;
  max?: number;
  tone?: "accent" | "ok" | "warn" | "danger";
  className?: string;
}) {
  const pct = Math.max(0, Math.min(100, (value / (max || 1)) * 100));
  const colour = {
    accent: "var(--accent)",
    ok: "var(--ok)",
    warn: "var(--warn)",
    danger: "var(--danger)",
  }[tone];
  return (
    <div
      className={`h-1.5 w-full overflow-hidden rounded-full bg-surface-inset ${className}`}
      role="progressbar"
      aria-valuenow={Math.round(pct)}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <div
        className="h-full rounded-full transition-[width] duration-500"
        style={{ width: `${pct}%`, background: colour }}
      />
    </div>
  );
}

/** A placeholder shown when a list has nothing in it. */
export function EmptyState({
  icon,
  title,
  description,
  action,
}: {
  icon?: ReactNode;
  title: string;
  description?: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 px-6 py-14 text-center">
      {icon && (
        <div className="grid size-12 place-items-center rounded-full bg-surface-3 text-fg-subtle">
          {icon}
        </div>
      )}
      <div className="space-y-1">
        <p className="text-fluid-lg font-semibold">{title}</p>
        {description && (
          <p className="mx-auto max-w-sm text-sm text-fg-muted">{description}</p>
        )}
      </div>
      {action}
    </div>
  );
}

/** A thin separator. */
export function Divider({ className = "" }: { className?: string }) {
  return <div className={`h-px w-full bg-line ${className}`} />;
}

/** A card with a title row. */
export function Panel({
  title,
  action,
  children,
  className = "",
}: {
  title?: ReactNode;
  action?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={`card overflow-hidden ${className}`}>
      {title && (
        <header className="flex items-center justify-between gap-3 border-b border-line px-4 py-3">
          <h3 className="text-fluid-sm font-semibold tracking-tight">{title}</h3>
          {action}
        </header>
      )}
      {children}
    </section>
  );
}
