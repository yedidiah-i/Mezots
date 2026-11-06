"use client";

/**
 * Settings.
 *
 * Three things live here: which node this browser talks to, who the student
 * is, and how the theme behaves. The security section explains, in plain
 * words, what the padlock in the interface actually means.
 */

import { useEffect, useState } from "react";

import { Avatar, Badge, Button, Divider, Kbd, SectionLabel } from "./ui";
import { CheckIcon, CloseIcon, LockIcon, MonitorIcon, ShieldIcon } from "./icons";
import { useTheme } from "./ThemeProvider";
import { getClient, useAppState } from "@/lib/store";
import { writeDisplayName } from "@/lib/identity";
import { shortFingerprint } from "@/lib/identity";

/** The settings sheet. */
export function SettingsSheet({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const { mode, setMode } = useTheme();
  const identity = useAppState((state) => state.identity);
  const node = useAppState((state) => state.node);
  const connection = useAppState((state) => state.connection);
  const connectionDetail = useAppState((state) => state.connectionDetail);

  const [name, setName] = useState("");
  const [nodeUrl, setNodeUrl] = useState("");
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    if (!open) return;
    setName(identity?.displayName ?? "");
    setNodeUrl(getClient()?.endpoint ?? "");
    setSaved(false);
  }, [open, identity?.displayName]);

  if (!open) return null;

  const save = () => {
    if (name.trim() && name.trim() !== identity?.displayName) {
      writeDisplayName(name.trim());
      window.location.reload();
    }
    if (nodeUrl.trim() && nodeUrl.trim() !== getClient()?.endpoint) {
      import("@/lib/store").then(({ setNodeUrl: apply }) =>
        apply(nodeUrl.trim()),
      );
    }
    setSaved(true);
    setTimeout(() => setSaved(false), 1800);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/55 p-4 backdrop-blur-[2px]">
      <button
        type="button"
        aria-label="Close the settings"
        className="absolute inset-0 cursor-default"
        onClick={onClose}
      />
      <div className="relative max-h-[88vh] w-full max-w-lg overflow-y-auto rounded-xl border border-line bg-surface shadow-overlay">
        <header className="flex items-center justify-between border-b border-line px-5 py-4">
          <h2 className="text-fluid-lg font-semibold tracking-tight">Settings</h2>
          <button
            type="button"
            onClick={onClose}
            className="text-fg-subtle transition-colors hover:text-fg"
            aria-label="Close"
          >
            <CloseIcon size={17} />
          </button>
        </header>

        <div className="space-y-6 p-5">
          {/* Identity. */}
          <section>
            <SectionLabel>You</SectionLabel>
            <div className="flex items-center gap-3">
              <Avatar
                name={identity?.displayName ?? "You"}
                id={identity?.nodeId}
                size="lg"
              />
              <div className="min-w-0 flex-1">
                <input
                  value={name}
                  onChange={(event) => setName(event.target.value)}
                  placeholder="Your name"
                  className="h-9 w-full rounded-md border border-line bg-surface-3 px-2.5 text-sm outline-none transition-colors focus:border-accent/60"
                />
                <p className="mt-1 truncate font-mono text-[10px] text-fg-subtle">
                  {identity?.nodeId ?? "not loaded"}
                </p>
              </div>
            </div>
          </section>

          <Divider />

          {/* The node. */}
          <section>
            <SectionLabel>Node</SectionLabel>
            <label className="block text-xs text-fg-muted">
              <span className="mb-1.5 block">
                Which node this browser attaches to. On the school network this
                is the node running on the machine that served this page.
              </span>
              <input
                value={nodeUrl}
                onChange={(event) => setNodeUrl(event.target.value)}
                spellCheck={false}
                className="h-9 w-full rounded-md border border-line bg-surface-3 px-2.5 font-mono text-xs outline-none transition-colors focus:border-accent/60"
              />
            </label>
            <div className="mt-3 flex items-center gap-2">
              <Badge tone={connection === "online" ? "ok" : "warn"}>
                {connection}
              </Badge>
              {connectionDetail && (
                <span className="truncate text-[11px] text-fg-subtle">
                  {connectionDetail}
                </span>
              )}
            </div>
            {node && (
              <div className="mt-3 rounded-md bg-surface-3 p-2.5">
                <p className="text-[10px] text-fg-subtle">Node fingerprint</p>
                <p className="break-all font-mono text-[10px] text-fg-muted">
                  {node.fingerprint}
                </p>
              </div>
            )}
          </section>

          <Divider />

          {/* Appearance. */}
          <section>
            <SectionLabel>Appearance</SectionLabel>
            <div className="flex gap-2">
              {(["light", "dark", "system"] as const).map((option) => (
                <button
                  key={option}
                  type="button"
                  onClick={() => setMode(option)}
                  className={`flex-1 rounded-lg border px-3 py-2 text-sm capitalize transition-colors ${
                    mode === option
                      ? "border-accent bg-accent-soft text-accent"
                      : "border-line bg-surface-3 text-fg-muted hover:border-line-strong"
                  }`}
                >
                  {option}
                </button>
              ))}
            </div>
          </section>

          <Divider />

          {/* Security. */}
          <section>
            <SectionLabel>Security</SectionLabel>
            <div className="space-y-2.5">
              <div className="flex gap-3 rounded-lg border border-line bg-surface-3 p-3">
                <ShieldIcon size={17} className="mt-0.5 shrink-0 text-ok" />
                <p className="text-xs leading-relaxed text-fg-muted">
                  Every frame between two nodes is sealed with
                  ChaCha20-Poly1305 under a key agreed with X25519. Direct
                  messages are encrypted a second time, in this browser, so
                  even the node cannot read them.
                </p>
              </div>
              <div className="flex gap-3 rounded-lg border border-line bg-surface-3 p-3">
                <LockIcon size={17} className="mt-0.5 shrink-0 text-ok" />
                <p className="text-xs leading-relaxed text-fg-muted">
                  Your safety number is{" "}
                  <span className="font-mono text-fg">
                    {identity ? shortFingerprint(identity.fingerprint) : "········"}
                  </span>
                  . Compare it with the other person in a direct message to
                  confirm nobody is in the middle.
                </p>
              </div>
              <div className="flex gap-3 rounded-lg border border-line bg-surface-3 p-3">
                <MonitorIcon size={17} className="mt-0.5 shrink-0 text-accent" />
                <p className="text-xs leading-relaxed text-fg-muted">
                  Messages, attachments and streams are committed to a
                  tamper-evident chain. Every block carries the hash of the one
                  before it, so a message that was edited or removed after the
                  fact leaves a gap.
                </p>
              </div>
            </div>
          </section>

          <Divider />

          {/* Shortcuts. */}
          <section>
            <SectionLabel>Shortcuts</SectionLabel>
            <ul className="space-y-1.5 text-xs text-fg-muted">
              <li className="flex items-center justify-between">
                <span>Command palette</span>
                <span className="flex gap-1">
                  <Kbd>⌘</Kbd>
                  <Kbd>K</Kbd>
                </span>
              </li>
              <li className="flex items-center justify-between">
                <span>Close a dialog</span>
                <Kbd>esc</Kbd>
              </li>
              <li className="flex items-center justify-between">
                <span>Send a message</span>
                <Kbd>↵</Kbd>
              </li>
            </ul>
          </section>

          <div className="flex justify-end gap-2 pb-1">
            <Button variant="ghost" onClick={onClose}>
              Cancel
            </Button>
            <Button variant="primary" onClick={save}>
              {saved ? (
                <>
                  <CheckIcon size={14} />
                  Saved
                </>
              ) : (
                "Save"
              )}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
