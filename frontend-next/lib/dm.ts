/**
 * The direct-message handshake.
 *
 * Opening a conversation publishes this browser's public key into it and
 * waits for the other side to publish theirs. Once both halves are known,
 * the shared key is derived and every message body is encrypted here, in the
 * browser, before it is handed to the node.
 *
 * The node only ever sees ciphertext for a direct message.
 *
 * The encrypted bodies are marked with a short prefix so the receiving side
 * knows to decrypt them. The prefix is not secret - it is a format marker.
 */

import {
  buildKeyPayload,
  decryptFrom,
  encryptFor,
  handleKeyPayload,
  hasConversationKey,
  isKeyPayload,
  KEY_MIME,
} from "./dmkeys";
import type { Identity } from "./identity";

/** The prefix that marks an encrypted body. */
export const ENCRYPTED_PREFIX = "e2e:";

/** How an attachment is delivered. */
type Sender = (command: {
  type: "sendRoomBinary";
  room: string;
  mime: string;
  payloadBase64: string;
  clientMsgId?: string;
}) => void;

/** Peers we have already published our key to. */
const published = new Set<string>();

/** The room id for a peer. */
export function roomFor(peerId: string): string {
  return `dm:${peerId}`;
}

/** Publish our public key into a conversation. */
export function publishKey(
  peerId: string,
  room: string,
  identity: Identity,
  send: Sender,
): void {
  if (published.has(peerId)) return;
  published.add(peerId);

  const payload = buildKeyPayload(
    identity.algorithm,
    identity.agreementPublicKey,
    identity.nodeId,
  );

  send({
    type: "sendRoomBinary",
    room,
    mime: KEY_MIME,
    payloadBase64: base64Encode(JSON.stringify(payload)),
    clientMsgId: `key-${identity.nodeId}-${peerId}`,
  });
}

/**
 * Make sure a conversation is ready.
 *
 * Called when a direct message room is opened.
 */
export function ensureConversation(
  peerId: string,
  identity: Identity,
  send: Sender,
): void {
  if (hasConversationKey(peerId)) return;
  publishKey(peerId, roomFor(peerId), identity, send);
}

/**
 * Handle an incoming attachment.
 *
 * Returns true when the attachment was a key exchange and should not be
 * shown in the conversation.
 */
export async function handleIncomingAttachment(
  peerId: string,
  room: string,
  mime: string,
  payloadBase64: string,
  identity: Identity,
  send: Sender,
): Promise<boolean> {
  if (!isKeyPayload(mime)) return false;

  try {
    const raw = base64Decode(payloadBase64);
    const payload = JSON.parse(raw) as {
      v: number;
      alg: "X25519" | "P-256";
      pub: string;
      from: string;
    };
    await handleKeyPayload(peerId, payload, identity.agreement);
    // Answer with our own key so the other side can derive too.
    publishKey(peerId, room, identity, send);
  } catch (error) {
    console.warn("[mezots] could not handle a key exchange", error);
  }
  return true;
}

/**
 * Encrypt a body for a peer.
 *
 * Returns the wire form and whether encryption actually happened. When the
 * key is not ready yet the body is sent as plain text and the interface
 * says so.
 */
export async function encryptBody(
  peerId: string,
  body: string,
): Promise<{ body: string; encrypted: boolean }> {
  const envelope = await encryptFor(peerId, body);
  if (!envelope) return { body, encrypted: false };
  return { body: `${ENCRYPTED_PREFIX}${envelope.text}`, encrypted: true };
}

/** Decrypt a body from a peer when it carries the encrypted marker. */
export async function decryptBody(
  peerId: string,
  body: string,
): Promise<{ body: string; encrypted: boolean }> {
  if (!body.startsWith(ENCRYPTED_PREFIX)) return { body, encrypted: false };
  const plaintext = await decryptFrom(peerId, body.slice(ENCRYPTED_PREFIX.length));
  if (plaintext === null) {
    // The key is not available yet: show the raw marker rather than pretend.
    return { body: "🔒 Encrypted message (the key has not arrived yet)", encrypted: true };
  }
  return { body: plaintext, encrypted: true };
}

/** Forget the published state, used when the identity changes. */
export function resetHandshake(): void {
  published.clear();
}

/** base64 without a library. */
export function base64Encode(value: string): string {
  return btoa(value);
}

/** base64 without a library. */
export function base64Decode(value: string): string {
  return atob(value);
}
