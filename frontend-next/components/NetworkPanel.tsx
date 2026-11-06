"use client";

/**
 * The network panel.
 *
 * This is the panel that answers the question a school administrator will
 * ask first: *is any of this leaving the school network?*
 *
 * Peers found over mDNS or the multicast beacon are physically on the same
 * network, so every byte that reaches them costs the school nothing. The
 * panel keeps the two counters apart - bytes relayed inside the LAN and
 * bytes that had to leave over the internet.
 */

import { Avatar, Badge, Button, Divider, LiveDot, Meter, SectionLabel } from "./ui";
import {
  BlocksIcon,
  CloseIcon,
  GlobeIcon,
  LockIcon,
  MonitorIcon,
  WifiIcon,
} from "./icons";
import { useAppState } from "@/lib/store";
import { formatBytes, formatUptime, relativeTime } from "@/lib/format";

/** The network status panel. */
export function NetworkPanel({ onClose }: { onClose?: () => void }) {
  const peers = useAppState((state) => state.peers);
  const metrics = useAppState((state) => state.metrics);
  const node = useAppState((state) => state.node);
  const identity = useAppState((state) => state.identity);
  const connection = useAppState((state) => state.connection);
  const ledgerStatus = useAppState((state) => state.ledgerStatus);
  const pending = useAppState((state) => state.pendingCommands);

  const online = peers.filter((peer) => peer.connected);
  const lanPeers = online.filter(
    (peer) => peer.source === "mdns" || peer.source === "beacon",
  );
  const remotePeers = online.filter((peer) => peer.source === "manual");
  const bytesLan = metrics?.bytesRelayed ?? 0;
  const bytesInternet = metrics?.bytesInternet ?? 0;
  const lanOnly = remotePeers.length === 0 && lanPeers.length > 0;

  return (
    <div className="flex min-h-full flex-col gap-5 p-4">
      <header className="flex items-center justify-between">
        <h2 className="text-fluid-sm font-semibold tracking-tight">Network</h2>
        {onClose && (
          <button
            type="button"
            onClick={onClose}
            className="text-fg-subtle transition-colors hover:text-fg"
            aria-label="Close"
          >
            <CloseIcon size={16} />
          </button>
        )}
      </header>

      {/* The headline: is anything leaving the network? */}
      <div
        className={`rounded-xl border p-4 ${
          lanOnly
            ? "border-ok/30 bg-ok-soft"
            : bytesInternet > 0
              ? "border-warn/30 bg-warn-soft"
              : "border-line bg-surface-3"
        }`}
      >
        <div className="mb-2 flex items-center gap-2">
          {lanOnly ? (
            <WifiIcon size={18} className="text-ok" />
          ) : (
            <GlobeIcon size={18} className="text-warn" />
          )}
          <p className="text-sm font-semibold">
            {lanOnly
              ? "Local network only"
              : bytesInternet > 0
                ? "Some traffic is using the internet"
                : "Waiting for peers"}
          </p>
        </div>
        <p className="text-xs leading-relaxed text-fg-muted">
          {lanOnly
            ? `Every message and stream is travelling directly between the ${lanPeers.length} node${lanPeers.length === 1 ? "" : "s"} on this network. No mobile data, no cloud, no per-seat billing.`
            : "Messages to peers outside this network leave over the internet uplink."}
        </p>

        <div className="mt-4 space-y-3">
          <div>
            <div className="mb-1.5 flex items-center justify-between text-[11px]">
              <span className="text-fg-muted">Relayed on the LAN</span>
              <span className="font-mono">{formatBytes(bytesLan)}</span>
            </div>
            <Meter value={bytesLan} max={Math.max(bytesLan, 1)} tone="ok" />
          </div>
          <div>
            <div className="mb-1.5 flex items-center justify-between text-[11px]">
              <span className="text-fg-muted">Left over the internet</span>
              <span className="font-mono">{formatBytes(bytesInternet)}</span>
            </div>
            <Meter
              value={bytesInternet}
              max={Math.max(bytesInternet, 1)}
              tone={bytesInternet > 0 ? "warn" : "ok"}
            />
          </div>
        </div>
      </div>

      {/* This node. */}
      <section>
        <SectionLabel>This node</SectionLabel>
        <div className="rounded-lg border border-line bg-surface-3 p-3">
          <div className="flex items-center gap-2.5">
            <Avatar name={node?.displayName ?? "Node"} id={node?.nodeId} size="sm" />
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm font-medium">
                {node?.displayName ?? identity?.displayName ?? "Local node"}
              </p>
              <p className="truncate font-mono text-[10px] text-fg-subtle">
                {node?.nodeId ?? "not connected"}
              </p>
            </div>
            <Badge tone={connection === "online" ? "ok" : "warn"}>
              {connection}
            </Badge>
          </div>

          {node?.fingerprint && (
            <div className="mt-3 flex items-start gap-2 rounded-md bg-surface-inset p-2">
              <LockIcon size={13} className="mt-0.5 shrink-0 text-ok" />
              <div className="min-w-0">
                <p className="text-[10px] font-medium text-fg-muted">
                  Identity fingerprint
                </p>
                <p className="break-all font-mono text-[10px] text-fg-subtle">
                  {node.fingerprint}
                </p>
              </div>
            </div>
          )}

          {metrics && (
            <div className="mt-3 grid grid-cols-2 gap-2 text-center">
              <div className="rounded-md bg-surface-inset px-2 py-1.5">
                <p className="font-mono text-sm">{metrics.ledgerHeight}</p>
                <p className="text-[10px] text-fg-subtle">blocks</p>
              </div>
              <div className="rounded-md bg-surface-inset px-2 py-1.5">
                <p className="font-mono text-sm">{formatUptime(metrics.uptimeMs)}</p>
                <p className="text-[10px] text-fg-subtle">uptime</p>
              </div>
            </div>
          )}
        </div>
      </section>

      {/* The peers. */}
      <section>
        <SectionLabel
          action={
            <span className="text-[11px] text-fg-subtle">
              {online.length}/{peers.length}
            </span>
          }
        >
          Peers
        </SectionLabel>
        {peers.length === 0 ? (
          <p className="rounded-lg border border-dashed border-line px-3 py-4 text-center text-xs text-fg-subtle">
            No peers yet. Start another node on this network and it will appear
            here on its own.
          </p>
        ) : (
          <ul className="space-y-1.5">
            {peers.map((peer) => {
              const isLan = peer.source === "mdns" || peer.source === "beacon";
              return (
                <li
                  key={peer.peerId}
                  className="flex items-center gap-2.5 rounded-lg border border-line bg-surface-3 px-2.5 py-2"
                >
                  <Avatar
                    name={peer.displayName}
                    id={peer.peerId}
                    size="sm"
                    online={peer.connected}
                  />
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-sm font-medium">
                      {peer.displayName}
                    </p>
                    <p className="truncate font-mono text-[10px] text-fg-subtle">
                      {peer.address ?? "no address"}
                    </p>
                  </div>
                  <Badge tone={isLan ? "ok" : "warn"}>
                    {isLan ? "LAN" : "remote"}
                  </Badge>
                </li>
              );
            })}
          </ul>
        )}
      </section>

      {/* The chain. */}
      <section>
        <SectionLabel>Chain</SectionLabel>
        <div className="rounded-lg border border-line bg-surface-3 p-3">
          <div className="flex items-center gap-2">
            <BlocksIcon size={16} className="text-accent" />
            <p className="text-sm font-medium">
              {ledgerStatus ? `Height ${ledgerStatus.height}` : "No blocks yet"}
            </p>
            <span className="ml-auto">
              {ledgerStatus?.valid === false ? (
                <Badge tone="danger">broken</Badge>
              ) : (
                <Badge tone="ok">verified</Badge>
              )}
            </span>
          </div>
          {ledgerStatus && (
            <>
              <Divider className="my-3" />
              <p className="truncate font-mono text-[10px] text-fg-subtle">
                head {ledgerStatus.head.slice(0, 32)}…
              </p>
              <p className="mt-1 text-[11px] text-fg-muted">
                {ledgerStatus.peersAgree} of {ledgerStatus.peersTotal} peers
                agree on this head.
              </p>
            </>
          )}
        </div>
      </section>

      {pending > 0 && (
        <div className="flex items-center gap-2 rounded-lg border border-warn/30 bg-warn-soft px-3 py-2 text-xs text-fg-muted">
          <LiveDot />
          {pending} message{pending === 1 ? "" : "s"} waiting for the node
        </div>
      )}

      <div className="mt-auto flex flex-col gap-2 pt-2">
        <Button variant="secondary" size="sm" className="w-full">
          <MonitorIcon size={14} />
          Open the mesh view
        </Button>
      </div>
    </div>
  );
}
