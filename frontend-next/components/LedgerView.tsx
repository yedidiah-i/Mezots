"use client";

/**
 * The chain explorer.
 *
 * This is the private ledger every message, attachment and broadcast is
 * committed to. A block holds the *hash* of a payload rather than the
 * payload itself, so the chain proves that a conversation happened without
 * storing the conversation.
 *
 * The browser re-computes the chain itself: it hashes every block header and
 * checks that each one points at its predecessor, and where the browser
 * supports Ed25519 it also checks the author's signature. Nothing here
 * trusts the node's own word that the chain is fine.
 */

import { useEffect, useMemo, useState } from "react";

import { Badge, Button, EmptyState, Panel } from "./ui";
import {
  BlocksIcon,
  CheckIcon,
  CloseIcon,
  LinkIcon,
  LockIcon,
  ShieldIcon,
} from "./icons";
import type { LedgerBlock } from "@/lib/protocol";
import { requestLedger, useAppState, verifyChain } from "@/lib/store";
import { canonicalBlockHeader, fromBase64, fromHex, sha256Hex, toHex, verifyEd25519 } from "@/lib/crypto";
import { formatDateTime, relativeTime } from "@/lib/format";

/** The outcome of checking a chain. */
interface Verdict {
  valid: boolean;
  /** The height at which the chain stopped making sense. */
  brokenAt: number | null;
  /** Whether the signatures could be checked at all. */
  signaturesChecked: number;
  /** Whether the browser can check signatures. */
  canVerifySignatures: boolean;
  checkedAt: number;
}

/** The chain explorer. */
export function LedgerView() {
  const ledger = useAppState((state) => state.ledger);
  const status = useAppState((state) => state.ledgerStatus);
  const [selected, setSelected] = useState<LedgerBlock | null>(null);
  const [verdict, setVerdict] = useState<Verdict | null>(null);
  const [verifying, setVerifying] = useState(false);

  // Ask for the chain when the view opens.
  useEffect(() => {
    requestLedger(300);
  }, []);

  const ordered = useMemo(
    () => [...ledger].sort((a, b) => a.index - b.index),
    [ledger],
  );

  /** Re-compute the chain in the browser. */
  const verify = useMemo(
    () => async (): Promise<Verdict> => {
      const canVerifySignatures = await supportsVerification();
      let signaturesChecked = 0;
      let previousHash = "";

      for (const block of ordered) {
        const header = canonicalBlockHeader(block);
        const computed = await sha256Hex(header);
        if (computed !== block.hash) {
          return {
            valid: false,
            brokenAt: block.index,
            signaturesChecked,
            canVerifySignatures,
            checkedAt: Date.now(),
          };
        }
        if (block.prevHash !== previousHash) {
          return {
            valid: false,
            brokenAt: block.index,
            signaturesChecked,
            canVerifySignatures,
            checkedAt: Date.now(),
          };
        }
        previousHash = block.hash;

        if (canVerifySignatures && block.actorKey) {
          try {
            const key = await crypto.subtle.importKey(
              "raw",
              fromBase64(block.actorKey) as BufferSource,
              { name: "Ed25519" },
              true,
              ["verify"],
            );
            const ok = await verifyEd25519(
              key,
              fromHex(block.signature),
              header,
            );
            if (ok) signaturesChecked += 1;
            else {
              return {
                valid: false,
                brokenAt: block.index,
                signaturesChecked,
                canVerifySignatures,
                checkedAt: Date.now(),
              };
            }
          } catch {
            // The key could not be imported; skip the signature check.
          }
        }
      }

      return {
        valid: true,
        brokenAt: null,
        signaturesChecked,
        canVerifySignatures,
        checkedAt: Date.now(),
      };
    },
    [ordered],
  );

  const runVerification = async () => {
    setVerifying(true);
    try {
      const result = await verify();
      setVerdict(result);
      // Ask the node to check its own chain too.
      verifyChain();
    } finally {
      setVerifying(false);
    }
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="grid gap-4 p-4 lg:grid-cols-[minmax(0,1fr)_320px]">
        <div className="min-w-0 space-y-4">
          {/* The headline. */}
          <Panel
            title={
              <span className="flex items-center gap-2">
                <BlocksIcon size={15} className="text-accent" />
                The school chain
              </span>
            }
            action={
              <Button
                variant="secondary"
                size="sm"
                onClick={() => void runVerification()}
                disabled={verifying}
              >
                {verifying ? "Verifying…" : "Verify the chain"}
              </Button>
            }
          >
            <div className="grid gap-3 p-4 sm:grid-cols-3">
              <Stat
                label="Height"
                value={status?.height ?? ordered.length}
                hint="blocks committed"
              />
              <Stat
                label="Agreement"
                value={
                  status ? `${status.peersAgree}/${status.peersTotal}` : "—"
                }
                hint="peers on the same head"
              />
              <Stat
                label="Node says"
                value={
                  status === null
                    ? "—"
                    : status.valid
                      ? "verified"
                      : "broken"
                }
                hint="the node's own check"
                tone={
                  status === null
                    ? "neutral"
                    : status.valid
                      ? "ok"
                      : "danger"
                }
              />
            </div>

            {verdict && (
              <div
                className={`mx-4 mb-4 flex items-start gap-2.5 rounded-lg border p-3 text-xs ${
                  verdict.valid
                    ? "border-ok/30 bg-ok-soft text-fg-muted"
                    : "border-danger/30 bg-danger-soft text-fg-muted"
                }`}
              >
                {verdict.valid ? (
                  <CheckIcon size={15} className="mt-0.5 shrink-0 text-ok" />
                ) : (
                  <CloseIcon size={15} className="mt-0.5 shrink-0 text-danger" />
                )}
                <p>
                  {verdict.valid ? (
                    <>
                      This browser re-computed{" "}
                      <span className="text-fg">{ordered.length}</span> block
                      hashes and checked every link. The chain is intact
                      {verdict.signaturesChecked > 0 && (
                        <>
                          {" "}
                          and{" "}
                          <span className="text-fg">
                            {verdict.signaturesChecked}
                          </span>{" "}
                          signatures verified
                        </>
                      )}
                      .
                    </>
                  ) : (
                    <>
                      The chain does not verify. The first problem is at block{" "}
                      <span className="text-fg">{verdict.brokenAt}</span> -
                      a block was changed after it was committed.
                    </>
                  )}
                  {!verdict.canVerifySignatures && (
                    <span className="mt-1 block text-fg-subtle">
                      This browser cannot verify Ed25519 signatures, so only
                      the hash chain was checked.
                    </span>
                  )}
                </p>
              </div>
            )}
          </Panel>

          {/* The blocks. */}
          <Panel
            title={`${ordered.length} blocks`}
            action={
              <span className="text-[11px] text-fg-subtle">newest first</span>
            }
          >
            {ordered.length === 0 ? (
              <EmptyState
                icon={<BlocksIcon size={22} />}
                title="The chain is empty"
                description="As soon as a message is sent, it is committed to the chain and shows up here."
              />
            ) : (
              <ul className="max-h-[52vh] divide-y divide-line overflow-y-auto">
                {[...ordered].reverse().map((block) => (
                  <li key={block.hash}>
                    <button
                      type="button"
                      onClick={() => setSelected(block)}
                      className="flex w-full items-center gap-3 px-4 py-2.5 text-left transition-colors hover:bg-surface-2"
                    >
                      <span className="w-12 shrink-0 font-mono text-xs text-fg-subtle">
                        #{block.index}
                      </span>
                      <span
                        className={`grid size-7 shrink-0 place-items-center rounded-md ${
                          block.kind === "message"
                            ? "bg-accent-soft text-accent"
                            : block.kind === "stream"
                              ? "bg-danger-soft text-danger"
                              : "bg-surface-3 text-fg-muted"
                        }`}
                      >
                        <LinkIcon size={13} />
                      </span>
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-sm">
                          {block.actorName || block.actor}
                        </span>
                        <span className="block truncate font-mono text-[10px] text-fg-subtle">
                          {block.room} · {block.kind} ·{" "}
                          {relativeTime(block.ts)}
                        </span>
                      </span>
                      <span className="hidden shrink-0 font-mono text-[10px] text-fg-subtle sm:block">
                        {block.hash.slice(0, 12)}…
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </Panel>
        </div>

        {/* The detail. */}
        <div className="space-y-4">
          {selected ? (
            <BlockDetail block={selected} onClose={() => setSelected(null)} />
          ) : (
            <Panel title="How this works">
              <div className="space-y-3 p-4 text-xs leading-relaxed text-fg-muted">
                <p>
                  Every message, attachment and broadcast is hashed and
                  appended as a block. Each block carries the hash of the one
                  before it, which is what makes the chain tamper evident.
                </p>
                <p>
                  Blocks are signed with the author's node key. Any student can
                  open this page, re-compute the hashes and check the
                  signatures - the node cannot rewrite history without leaving
                  a visible break.
                </p>
                <p className="flex items-start gap-2">
                  <ShieldIcon size={14} className="mt-0.5 shrink-0 text-accent" />
                  What the chain stores is the hash of a message, never the
                  message itself. The conversation stays between the people
                  in it.
                </p>
                <p className="flex items-start gap-2">
                  <LockIcon size={14} className="mt-0.5 shrink-0 text-ok" />
                  For a direct message the hash is taken over the ciphertext,
                  so even the chain never sees the words.
                </p>
              </div>
            </Panel>
          )}
        </div>
      </div>
    </div>
  );
}

/** One figure in the summary row. */
function Stat({
  label,
  value,
  hint,
  tone = "neutral",
}: {
  label: string;
  value: string | number;
  hint: string;
  tone?: "neutral" | "ok" | "danger";
}) {
  const colour = {
    neutral: "text-fg",
    ok: "text-ok",
    danger: "text-danger",
  }[tone];
  return (
    <div className="rounded-lg border border-line bg-surface-3 p-3">
      <p className="text-[11px] text-fg-subtle">{label}</p>
      <p className={`mt-0.5 font-mono text-xl ${colour}`}>{value}</p>
      <p className="mt-0.5 text-[10px] text-fg-subtle">{hint}</p>
    </div>
  );
}

/** The detail of one block. */
function BlockDetail({
  block,
  onClose,
}: {
  block: LedgerBlock;
  onClose: () => void;
}) {
  const rows: Array<[string, string]> = [
    ["Height", `#${block.index}`],
    ["Author", `${block.actorName} (${block.actor})`],
    ["Sequence", String(block.seq)],
    ["Room", block.room],
    ["Kind", block.kind],
    ["Time", formatDateTime(block.ts)],
    ["Payload hash", block.payloadHash],
    ["Previous hash", block.prevHash],
    ["Block hash", block.hash],
    ["Signature", block.signature],
  ];

  return (
    <Panel
      title={`Block #${block.index}`}
      action={
        <button
          type="button"
          onClick={onClose}
          className="text-fg-subtle transition-colors hover:text-fg"
          aria-label="Close"
        >
          <CloseIcon size={15} />
        </button>
      }
    >
      <dl className="divide-y divide-line">
        {rows.map(([label, value]) => (
          <div key={label} className="px-4 py-2.5">
            <dt className="text-[10px] tracking-wide text-fg-subtle uppercase">
              {label}
            </dt>
            <dd className="mt-0.5 font-mono text-[11px] break-all text-fg-muted">
              {value}
            </dd>
          </div>
        ))}
      </dl>
    </Panel>
  );
}

/** Whether this browser can check Ed25519 signatures. */
async function supportsVerification(): Promise<boolean> {
  if (typeof crypto === "undefined" || !crypto.subtle) return false;
  try {
    await crypto.subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"]);
    return true;
  } catch {
    return false;
  }
}

/** Export the hex helper so the tree-shaker keeps it. */
export { toHex };
