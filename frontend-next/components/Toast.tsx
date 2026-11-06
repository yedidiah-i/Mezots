"use client";

/**
 * A transient message at the bottom of the window.
 *
 * Used for node errors that deserve attention but should not interrupt what
 * the student is doing.
 */

import { useEffect } from "react";

import { CloseIcon } from "./icons";

/** The toast. */
export function Toast({
  message,
  onDismiss,
}: {
  message: string | null;
  onDismiss: () => void;
}) {
  useEffect(() => {
    if (!message) return;
    const timer = setTimeout(onDismiss, 6000);
    return () => clearTimeout(timer);
  }, [message, onDismiss]);

  if (!message) return null;

  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-4 z-50 flex justify-center px-4">
      <div className="pointer-events-auto flex max-w-lg items-start gap-3 rounded-lg border border-danger/30 bg-surface px-3.5 py-2.5 shadow-overlay animate-rise">
        <span className="mt-0.5 size-2 shrink-0 rounded-full bg-danger" />
        <p className="text-sm text-fg-muted">{message}</p>
        <button
          type="button"
          onClick={onDismiss}
          className="text-fg-subtle transition-colors hover:text-fg"
          aria-label="Dismiss"
        >
          <CloseIcon size={15} />
        </button>
      </div>
    </div>
  );
}
