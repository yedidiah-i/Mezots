/**
 * Pairwise end-to-end encryption for direct messages.
 *
 * The node relays direct messages and can see their metadata, but not their
 * bodies: the body is encrypted in the browser with a key that only the two
 * participants can derive.
 *
 * The exchange works like this:
 *
 * 1. Every browser has an X25519 (or P-256) key pair.
 * 2. When a student opens a conversation, their public key is published into
 *    the conversation as a reserved attachment (`application/vnd.mezots.key`).
 * 3. Both sides run ECDH over the two public halves and derive an AES-GCM key
 *    with HKDF. The salt is the sorted concatenation of both public keys, so
 *    the key is bound to that exact pair of people.
 * 4. From then on the body is ciphertext. The node stores and forwards
 *    ciphertext, and the ledger commits to the *hash* of the ciphertext.
 *
 * A room message therefore has two layers of protection: the node's
 * ChaCha20-Poly1305 channel between nodes, and this browser-level envelope
 * for the private conversations.
 */

import {
  decryptMessage,
  deriveMessageKey,
  deriveSharedBits,
  encryptMessage,
  exportPublicKey,
  fromBase64,
  importAgreementPublicKey,
  toBase64,
  type KeyAgreement,
} from "./crypto";

/** The MIME type that marks a key exchange attachment. */
export const KEY_MIME = "application/vnd.mezots.key";

/** The version tag carried by a key exchange attachment. */
export const KEY_VERSION = 1;

/** A key exchange payload as it travels through the node. */
export interface KeyPayload {
  v: number;
  alg: KeyAgreement;
  /** base64 of the raw public key. */
  pub: string;
  /** The node id of the publisher. */
  from: string;
}

/** A conversation key plus the peer it belongs to. */
interface ConversationKey {
  peerId: string;
  key: CryptoKey;
  algorithm: KeyAgreement;
  establishedAt: number;
}

/** Keys, kept outside React because a `CryptoKey` is not serialisable. */
const keys = new Map<string, ConversationKey>();

/** Public keys we have seen, keyed by peer id. */
const publicKeys = new Map<string, KeyPayload>();

/** Callbacks fired when a conversation key becomes usable. */
const listeners = new Set<(peerId: string) => void>();

/** Listen for a key being established. */
export function onKeyEstablished(
  listener: (peerId: string) => void,
): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Whether a conversation has a usable key. */
export function hasConversationKey(peerId: string): boolean {
  return keys.has(peerId);
}

/** The key for a conversation, if it exists. */
export function getConversationKey(peerId: string): CryptoKey | null {
  return keys.get(peerId)?.key ?? null;
}

/**
 * Build the key exchange payload for this browser.
 *
 * The caller is responsible for sending it into the conversation.
 */
export function buildKeyPayload(
  algorithm: KeyAgreement,
  publicKeyB64: string,
  nodeId: string,
): KeyPayload {
  return { v: KEY_VERSION, alg: algorithm, pub: publicKeyB64, from: nodeId };
}

/** Whether a MIME type marks a key exchange attachment. */
export function isKeyPayload(mime: string): boolean {
  return mime.split(";")[0].trim() === KEY_MIME;
}

/**
 * Handle an incoming key exchange payload.
 *
 * Returns the payload when it was a key, so the caller can ignore it in the
 * message list.
 */
export async function handleKeyPayload(
  peerId: string,
  payload: KeyPayload,
  localAgreement: { algorithm: KeyAgreement; publicKey: CryptoKey; privateKey: CryptoKey },
): Promise<boolean> {
  if (!payload || payload.v !== KEY_VERSION || !payload.pub) return false;

  publicKeys.set(peerId, payload);

  const peerPublicKey = await importAgreementPublicKey(
    payload.alg,
    fromBase64(payload.pub).buffer as ArrayBuffer,
  );
  const shared = await deriveSharedBits(localAgreement, peerPublicKey);
  const key = await deriveMessageKey(
    shared,
    localAgreement.publicKey,
    peerPublicKey,
  );

  const existing = keys.get(peerId);
  if (!existing || existing.algorithm !== payload.alg) {
    keys.set(peerId, {
      peerId,
      key,
      algorithm: payload.alg,
      establishedAt: Date.now(),
    });
    for (const listener of listeners) listener(peerId);
  }
  return true;
}

/** The public key we last saw for a peer. */
export function getPeerPublicKey(peerId: string): KeyPayload | null {
  return publicKeys.get(peerId) ?? null;
}

/** Encrypt a message body for a peer. */
export async function encryptFor(
  peerId: string,
  plaintext: string,
): Promise<{ text: string; encrypted: true } | null> {
  const key = keys.get(peerId);
  if (!key) return null;
  const envelope = await encryptMessage(key.key, plaintext);
  return { text: envelope.payload, encrypted: true };
}

/** Decrypt a message body from a peer. */
export async function decryptFrom(
  peerId: string,
  ciphertext: string,
): Promise<string | null> {
  const key = keys.get(peerId);
  if (!key) return null;
  try {
    return await decryptMessage(key.key, ciphertext);
  } catch {
    return null;
  }
}

/** Forget every key (used when the identity changes). */
export function resetKeys(): void {
  keys.clear();
  publicKeys.clear();
}

/** Export a raw public key as base64. */
export async function exportPublicKeyBase64(key: CryptoKey): Promise<string> {
  return toBase64(await exportPublicKey(key));
}
