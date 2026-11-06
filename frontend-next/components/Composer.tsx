"use client";

/**
 * The message composer.
 *
 * A growing textarea with formatting helpers, an emoji picker, file
 * attachments and mention completion. It is used by both the channel view
 * and the direct-message view, because sending a message is the same action
 * in both - only the transport differs.
 */

import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ChangeEvent,
  type ClipboardEvent,
  type KeyboardEvent,
} from "react";

import { Avatar, IconButton } from "./ui";
import {
  ClipIcon,
  CloseIcon,
  LockIcon,
  SendIcon,
  SmileIcon,
} from "./icons";
import { sendAttachment, sendText, useAppState } from "@/lib/store";

/** The emoji offered by the picker. */
const EMOJI = [
  "😀", "😂", "🙂", "😉", "😍", "🤔", "😴", "😎",
  "👍", "👏", "🙏", "💪", "🎉", "🔥", "✨", "❤️",
  "✅", "❌", "⚠️", "📌", "📎", "🎓", "📚", "🧠",
];

/** The formatting marks the toolbar can wrap a selection with. */
const MARKS = [
  { label: "Bold", marker: "**", name: "B" },
  { label: "Italic", marker: "_", name: "I" },
  { label: "Code", marker: "`", name: "{ }" },
];

/** The composer. */
export function Composer({
  room,
  placeholder = "Write a message",
  encrypted = false,
  autoFocus = false,
  onSend,
}: {
  room: string;
  placeholder?: string;
  encrypted?: boolean;
  autoFocus?: boolean;
  onSend?: () => void;
}) {
  const [value, setValue] = useState("");
  const [emojiOpen, setEmojiOpen] = useState(false);
  const [attachments, setAttachments] = useState<
    Array<{ name: string; mime: string; size: number }>
  >([]);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const identity = useAppState((state) => state.identity);
  const peers = useAppState((state) => state.peers);

  // Grow the textarea with its content, up to a limit.
  const resize = useCallback(() => {
    const element = textareaRef.current;
    if (!element) return;
    element.style.height = "auto";
    element.style.height = `${Math.min(element.scrollHeight, 220)}px`;
  }, []);

  useEffect(() => {
    resize();
  }, [value, resize]);

  useEffect(() => {
    if (autoFocus) textareaRef.current?.focus();
  }, [autoFocus]);

  const submit = useCallback(() => {
    const text = value.trim();
    if (!text && attachments.length === 0) return;
    if (text) sendText(room, text, encrypted);
    setValue("");
    setAttachments([]);
    requestAnimationFrame(resize);
    onSend?.();
  }, [value, room, encrypted, onSend, resize]);

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      submit();
    }
  };

  const onPaste = (event: ClipboardEvent<HTMLTextAreaElement>) => {
    const files = Array.from(event.clipboardData.files);
    if (files.length === 0) return;
    event.preventDefault();
    void uploadFiles(files);
  };

  const uploadFiles = async (files: File[]) => {
    for (const file of files) {
      const buffer = await file.arrayBuffer();
      const base64 = toBase64(buffer);
      sendAttachment(room, file.type || "application/octet-stream", base64, file.name);
      setAttachments((current) => [
        ...current,
        { name: file.name, mime: file.type, size: file.size },
      ]);
    }
  };

  const onFileChange = (event: ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(event.target.files ?? []);
    if (files.length > 0) void uploadFiles(files);
    event.target.value = "";
  };

  /** Wrap the current selection with a markdown-ish marker. */
  const applyMark = (marker: string) => {
    const element = textareaRef.current;
    if (!element) return;
    const start = element.selectionStart;
    const end = element.selectionEnd;
    const selected = value.slice(start, end) || "text";
    const next = `${value.slice(0, start)}${marker}${selected}${marker}${value.slice(end)}`;
    setValue(next);
    requestAnimationFrame(() => {
      element.focus();
      element.setSelectionRange(
        start + marker.length,
        start + marker.length + selected.length,
      );
    });
  };

  const insertEmoji = (emoji: string) => {
    const element = textareaRef.current;
    const start = element?.selectionStart ?? value.length;
    setValue(`${value.slice(0, start)}${emoji}${value.slice(start)}`);
    setEmojiOpen(false);
    requestAnimationFrame(() => element?.focus());
  };

  /** Insert a mention when the user picks a person. */
  const insertMention = (name: string) => {
    const element = textareaRef.current;
    const start = element?.selectionStart ?? value.length;
    setValue(`${value.slice(0, start)}@${name} ${value.slice(start)}`);
    requestAnimationFrame(() => element?.focus());
  };

  return (
    <div className="px-3 pb-3 sm:px-4 sm:pb-4">
      <div className="rounded-xl border border-line bg-surface shadow-soft transition-colors focus-within:border-accent/50">
        {attachments.length > 0 && (
          <ul className="flex flex-wrap gap-2 border-b border-line px-3 py-2">
            {attachments.map((file, index) => (
              <li
                key={`${file.name}-${index}`}
                className="flex items-center gap-2 rounded-md bg-surface-3 px-2 py-1 text-[11px] text-fg-muted"
              >
                <ClipIcon size={12} />
                <span className="max-w-[160px] truncate">{file.name}</span>
                <span className="font-mono text-fg-subtle">
                  {(file.size / 1024).toFixed(0)} KB
                </span>
                <button
                  type="button"
                  onClick={() =>
                    setAttachments((current) =>
                      current.filter((_, position) => position !== index),
                    )
                  }
                  className="text-fg-subtle hover:text-danger"
                  aria-label={`Remove ${file.name}`}
                >
                  <CloseIcon size={11} />
                </button>
              </li>
            ))}
          </ul>
        )}

        <textarea
          ref={textareaRef}
          value={value}
          onChange={(event) => setValue(event.target.value)}
          onKeyDown={onKeyDown}
          onPaste={onPaste}
          placeholder={placeholder}
          rows={1}
          className="max-h-[220px] min-h-[44px] w-full resize-none bg-transparent px-3.5 pt-3 text-fluid-base leading-relaxed outline-none placeholder:text-fg-subtle"
        />

        <div className="flex items-center gap-1 px-2 pb-2">
          <div className="flex items-center gap-0.5">
            {MARKS.map((mark) => (
              <button
                key={mark.label}
                type="button"
                title={mark.label}
                onClick={() => applyMark(mark.marker)}
                className={`grid h-7 min-w-7 place-items-center rounded px-1.5 text-xs font-semibold text-fg-subtle transition-colors hover:bg-surface-3 hover:text-fg ${
                  mark.name === "I" ? "italic" : ""
                }`}
              >
                {mark.name}
              </button>
            ))}
          </div>

          <div className="mx-1 h-4 w-px bg-line" />

          <IconButton
            label="Attach a file"
            onClick={() => fileInputRef.current?.click()}
          >
            <ClipIcon size={16} />
          </IconButton>

          <div className="relative">
            <IconButton
              label="Insert an emoji"
              onClick={() => setEmojiOpen((open) => !open)}
            >
              <SmileIcon size={16} />
            </IconButton>
            {emojiOpen && (
              <>
                <button
                  type="button"
                  aria-label="Close the emoji picker"
                  className="fixed inset-0 z-10 cursor-default"
                  onClick={() => setEmojiOpen(false)}
                />
                <div className="absolute bottom-10 left-0 z-20 grid w-64 grid-cols-8 gap-0.5 rounded-lg border border-line bg-surface p-2 shadow-overlay animate-rise">
                  {EMOJI.map((emoji) => (
                    <button
                      key={emoji}
                      type="button"
                      onClick={() => insertEmoji(emoji)}
                      className="grid h-7 place-items-center rounded text-base transition-colors hover:bg-surface-3"
                    >
                      {emoji}
                    </button>
                  ))}
                </div>
              </>
            )}
          </div>

          {peers.length > 0 && (
            <div className="flex items-center gap-0.5">
              {peers.slice(0, 4).map((peer) => (
                <button
                  key={peer.peerId}
                  type="button"
                  title={`Mention ${peer.displayName}`}
                  onClick={() => insertMention(peer.displayName)}
                  className="rounded-full p-0.5 transition-transform hover:scale-110"
                >
                  <Avatar
                    name={peer.displayName}
                    id={peer.peerId}
                    size="xs"
                    online={peer.connected}
                  />
                </button>
              ))}
            </div>
          )}

          <div className="ml-auto flex items-center gap-2">
            {encrypted && (
              <span
                className="flex items-center gap-1 text-[11px] text-ok"
                title="This message is encrypted in your browser before it leaves"
              >
                <LockIcon size={12} />
                end-to-end
              </span>
            )}
            <span className="hidden text-[11px] text-fg-subtle sm:inline">
              <kbd className="font-sans">↵</kbd> to send
            </span>
            <button
              type="button"
              onClick={submit}
              disabled={!value.trim() && attachments.length === 0}
              className="grid size-8 place-items-center rounded-lg bg-accent text-accent-fg transition-[filter,transform] hover:brightness-110 active:scale-95 disabled:opacity-35"
              aria-label="Send"
            >
              <SendIcon size={15} />
            </button>
          </div>
        </div>
      </div>

      <input
        ref={fileInputRef}
        type="file"
        multiple
        className="hidden"
        onChange={onFileChange}
      />

      <p className="mt-1.5 hidden items-center gap-1.5 px-1 text-[10px] text-fg-subtle sm:flex">
        {identity?.displayName ? (
          <>
            Posting as <span className="text-fg-muted">{identity.displayName}</span>
            <span className="font-mono">· {identity.nodeId}</span>
          </>
        ) : null}
      </p>
    </div>
  );
}

/** base64 without pulling in a library. */
function toBase64(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}
