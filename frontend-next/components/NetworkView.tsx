"use client";

/**
 * The network view.
 *
 * A map of who is on the network and how the traffic reaches them. The
 * centre is this node; the ring around it is everyone else. A green link
 * means the peer is on the same physical network, so the bytes cost the
 * school nothing.
 */

import { useMemo, useState } from "react";

import { Avatar, Badge, Button, Meter, Panel, SectionLabel } from "./ui";
import {
  BlocksIcon,
  GlobeIcon,
  LockIcon,
  MonitorIcon,
  WifiIcon,
} from "./icons";
import { useAppState } from "@/lib/store";
import { formatBytes, formatUptime } from "@/lib/format";

/** The network view. */
export function NetworkView() {
  const peers = useAppState((state) => state.peers);
  const metrics = useAppState((state) => state.metrics);
  const node = useAppState((state) => state.node);
  const identity = useAppState((state) => state.identity);
  const ledgerStatus = useAppState((state) => state.ledgerStatus);
  const [copied, setCopied] = useState(false);

  const online = peers.filter((peer) => peer.connected);
  const lanPeers = online.filter(
    (peer) => peer.source === "mdns" || peer.source === "beacon",
  );
  const remotePeers = online.filter((peer) => peer.source === "manual");

  const bytesLan = metrics?.bytesRelayed ?? 0;
  const bytesInternet = metrics?.bytesInternet ?? 0;

  const inviteLink = useMemo(() => {
    const host =
      typeof window === "undefined" ? "" : window.location.hostname;
    return `mezots://${host}:7700`;
  }, []);

  const copyInvite = async () => {
    try {
      await navigator.clipboard.writeText(inviteLink);
      setCopied(true);
      setTimeout(() => setCopied(false), 1800);
    } catch {
      setCopied(false);
    }
  };

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="grid gap-4 p-4 lg:grid-cols-[minmax(0,1fr)_340px]">
        <div className="min-w-0 space-y-4">
          {/* The mesh. */}
          <Panel
            title={
              <span className="flex items-center gap-2">
                <MonitorIcon size={15} className="text-accent" />
                Peer mesh
              </span>
            }
            action={
              <Badge tone={remotePeers.length === 0 ? "ok" : "warn"}>
                {remotePeers.length === 0
                  ? "everything on the LAN"
                  : `${remotePeers.length} remote`}
              </Badge>
            }
          >
            <div className="chain-grid relative flex aspect-16/10 w-full items-center justify-center overflow-hidden p-6">
              <Mesh peers={online} />
            </div>
          </Panel>

          {/* The peers. */}
          <Panel title={`${peers.length} known peers`}>
            {peers.length === 0 ? (
              <p className="px-4 py-8 text-center text-sm text-fg-subtle">
                No peers yet. Start a node on the same network and it will
                appear here on its own - no address to type, no server to
                register with.
              </p>
            ) : (
              <ul className="divide-y divide-line">
                {peers.map((peer) => {
                  const isLan =
                    peer.source === "mdns" || peer.source === "beacon";
                  return (
                    <li
                      key={peer.peerId}
                      className="flex items-center gap-3 px-4 py-3"
                    >
                      <Avatar
                        name={peer.displayName}
                        id={peer.peerId}
                        size="md"
                        online={peer.connected}
                      />
                      <div className="min-w-0 flex-1">
                        <p className="truncate text-sm font-medium">
                          {peer.displayName}
                        </p>
                        <p className="truncate font-mono text-[11px] text-fg-subtle">
                          {peer.peerId} · {peer.address ?? "no address"}
                        </p>
                      </div>
                      <div className="flex shrink-0 items-center gap-2">
                        <Badge tone="neutral">{peer.source}</Badge>
                        <Badge tone={peer.connected ? "ok" : "warn"}>
                          {peer.state}
                        </Badge>
                      </div>
                    </li>
                  );
                })}
              </ul>
            )}
          </Panel>
        </div>

        {/* The accounting. */}
        <div className="space-y-4">
          <Panel
            title={
              <span className="flex items-center gap-2">
                <WifiIcon size={15} className="text-ok" />
                Data used
              </span>
            }
          >
            <div className="space-y-4 p-4">
              <div>
                <div className="mb-1.5 flex items-center justify-between">
                  <span className="text-xs text-fg-muted">
                    Inside the school network
                  </span>
                  <span className="font-mono text-sm">{formatBytes(bytesLan)}</span>
                </div>
                <Meter value={bytesLan} max={Math.max(bytesLan, 1)} tone="ok" />
                <p className="mt-1.5 text-[11px] text-fg-subtle">
                  {lanPeers.length} peer{lanPeers.length === 1 ? "" : "s"} ·
                  this traffic never leaves the building
                </p>
              </div>

              <div>
                <div className="mb-1.5 flex items-center justify-between">
                  <span className="text-xs text-fg-muted">
                    Over the internet
                  </span>
                  <span className="font-mono text-sm">
                    {formatBytes(bytesInternet)}
                  </span>
                </div>
                <Meter
                  value={bytesInternet}
                  max={Math.max(bytesInternet, 1)}
                  tone={bytesInternet > 0 ? "warn" : "ok"}
                />
                <p className="mt-1.5 text-[11px] text-fg-subtle">
                  {remotePeers.length === 0
                    ? "nothing left the network"
                    : `${remotePeers.length} peer${remotePeers.length === 1 ? "" : "s"} were reached over the uplink`}
                </p>
              </div>

              <div className="grid grid-cols-2 gap-2">
                <Figure label="Peers" value={online.length} />
                <Figure label="Uptime" value={formatUptime(metrics?.uptimeMs ?? 0)} />
                <Figure label="Blocks" value={metrics?.ledgerHeight ?? 0} />
                <Figure
                  label="Chain"
                  value={
                    ledgerStatus === null
                      ? "—"
                      : ledgerStatus.valid
                        ? "valid"
                        : "broken"
                  }
                />
              </div>
            </div>
          </Panel>

          <Panel
            title={
              <span className="flex items-center gap-2">
                <GlobeIcon size={15} className="text-accent" />
                Add a classmate
              </span>
            }
          >
            <div className="space-y-3 p-4">
              <p className="text-xs leading-relaxed text-fg-muted">
                Nodes announce themselves over mDNS and a multicast beacon, so
                most of the time there is nothing to configure. If the
                network blocks multicast, a classmate can dial this node
                directly.
              </p>
              <div className="flex items-center gap-2 rounded-md bg-surface-3 px-2.5 py-2">
                <code className="min-w-0 flex-1 truncate font-mono text-[11px]">
                  {inviteLink}
                </code>
                <Button size="sm" variant="secondary" onClick={copyInvite}>
                  {copied ? "Copied" : "Copy"}
                </Button>
              </div>
              <div className="flex items-start gap-2 text-[11px] text-fg-subtle">
                <LockIcon size={13} className="mt-0.5 shrink-0 text-ok" />
                The connection is authenticated with an X25519 exchange and
                sealed with ChaCha20-Poly1305. An outsider on the network
                cannot read a thing.
              </div>
            </div>
          </Panel>

          <Panel
            title={
              <span className="flex items-center gap-2">
                <BlocksIcon size={15} className="text-accent" />
                This node
              </span>
            }
          >
            <dl className="divide-y divide-line">
              <Row label="Name" value={node?.displayName ?? "—"} />
              <Row label="Node id" value={node?.nodeId ?? "—"} mono />
              <Row
                label="Fingerprint"
                value={node?.fingerprint ?? "—"}
                mono
              />
              <Row
                label="Your browser id"
                value={identity?.nodeId ?? "—"}
                mono
              />
            </dl>
          </Panel>
        </div>
      </div>
    </div>
  );
}

/** The mesh diagram. */
function Mesh({ peers }: { peers: Array<{ peerId: string; displayName: string; connected: boolean; source: string }> }) {
  const online = peers.filter((peer) => peer.connected);
  const radius = 34;

  return (
    <svg viewBox="0 0 100 100" className="h-full w-full max-h-[320px]">
      <defs>
        <radialGradient id="core">
          <stop offset="0%" stopColor="var(--accent)" stopOpacity="0.45" />
          <stop offset="100%" stopColor="var(--accent)" stopOpacity="0" />
        </radialGradient>
      </defs>

      <circle cx="50" cy="50" r="26" fill="url(#core)" />

      {online.map((peer, index) => {
        const angle = (index / Math.max(online.length, 1)) * Math.PI * 2 - Math.PI / 2;
        const x = 50 + Math.cos(angle) * radius;
        const y = 50 + Math.sin(angle) * radius;
        const isLan = peer.source !== "manual";
        const colour = isLan ? "var(--ok)" : "var(--warn)";
        return (
          <g key={peer.peerId}>
            <line
              x1="50"
              y1="50"
              x2={x}
              y2={y}
              stroke={colour}
              strokeWidth="0.5"
              strokeOpacity="0.55"
              strokeDasharray={isLan ? undefined : "2 1.5"}
            />
            <circle cx={x} cy={y} r="4.2" fill="var(--surface)" stroke={colour} strokeWidth="0.7" />
            <circle cx={x} cy={y} r="1.6" fill={colour} />
            <text
              x={x}
              y={y + 8.5}
              textAnchor="middle"
              className="fill-[var(--fg-subtle)] text-[3.4px]"
            >
              {peer.displayName.slice(0, 12)}
            </text>
          </g>
        );
      })}

      {/* This node in the middle. */}
      <circle cx="50" cy="50" r="7" fill="var(--surface)" stroke="var(--accent)" strokeWidth="0.9" />
      <circle cx="50" cy="50" r="3" fill="var(--accent)" />
      <text
        x="50"
        y="62"
        textAnchor="middle"
        className="fill-[var(--fg-muted)] text-[3.6px]"
      >
        this node
      </text>

      {online.length === 0 && (
        <text
          x="50"
          y="86"
          textAnchor="middle"
          className="fill-[var(--fg-subtle)] text-[3.4px]"
        >
          no peers on the network yet
        </text>
      )}
    </svg>
  );
}

/** A labelled figure. */
function Figure({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="rounded-md bg-surface-3 px-2.5 py-2">
      <p className="font-mono text-sm">{value}</p>
      <p className="text-[10px] text-fg-subtle">{label}</p>
    </div>
  );
}

/** A labelled row. */
function Row({
  label,
  value,
  mono,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div className="px-4 py-2.5">
      <dt className="text-[10px] tracking-wide text-fg-subtle uppercase">
        {label}
      </dt>
      <dd
        className={`mt-0.5 text-xs break-all text-fg-muted ${mono ? "font-mono text-[11px]" : ""}`}
      >
        {value}
      </dd>
    </div>
  );
}
