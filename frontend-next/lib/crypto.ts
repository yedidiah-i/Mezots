/**
 * Cryptography helpers built on the browser's own WebCrypto API.
 *
 * The node already encrypts every peer-to-peer frame with ChaCha20-Poly1305.
 * This layer adds the part a browser *can* do that a relay cannot: pairwise
 * end-to-end encryption for direct messages. The shared secret is derived
 * from an ECDH exchange whose public halves travel through the node as an
 * ordinary attachment, so a node that is compromised still only ever sees
 * ciphertext for private conversations.
 *
 * Nothing here imports a library - `crypto.subtle` is part of every browser.
 */

/** Key agreement algorithms this build knows how to use. */
export type KeyAgreement = "X25519" | "P-256";

/** A key agreement key pair. */
export interface AgreementKeyPair {
  algorithm: KeyAgreement;
  publicKey: CryptoKey;
  privateKey: CryptoKey;
}

/** The result of an ECDH exchange. */
export interface SharedKey {
  algorithm: KeyAgreement;
  /** The raw ECDH secret, before derivation. */
  bits: ArrayBuffer;
}

/**
 * A byte buffer backed by a real `ArrayBuffer`.
 *
 * TypeScript 5.7 made `Uint8Array` generic over its buffer, and WebCrypto
 * still wants the non-shared variant. Naming it once keeps the casts in
 * one place.
 */
export type Bytes = Uint8Array<ArrayBuffer>;

/** Encode bytes as standard base64. */
export function toBase64(bytes: ArrayBuffer | Bytes): string {
  const view = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < view.length; i += chunk) {
    binary += String.fromCharCode(...view.subarray(i, i + chunk));
  }
  return btoa(binary);
}

/** Decode standard base64 into bytes. */
export function fromBase64(value: string): Bytes {
  const binary = atob(value);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

/** Hex-encode bytes. */
export function toHex(bytes: ArrayBuffer | Bytes): string {
  const view = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  let out = "";
  for (const byte of view) out += byte.toString(16).padStart(2, "0");
  return out;
}

/** Hex-decode a string. */
export function fromHex(value: string): Bytes {
  const clean = value.trim();
  const padded = clean.length % 2 === 0 ? clean : `0${clean}`;
  const bytes = new Uint8Array(padded.length / 2);
  for (let i = 0; i < bytes.length; i += 1) {
    bytes[i] = Number.parseInt(padded.slice(i * 2, i * 2 + 2), 16);
  }
  return bytes;
}

/** SHA-256 of a byte string, hex encoded. */
export async function sha256Hex(
  input: ArrayBuffer | Bytes | string,
): Promise<string> {
  const bytes =
    typeof input === "string"
      ? new TextEncoder().encode(input)
      : input instanceof Uint8Array
        ? input
        : new Uint8Array(input);
  const digest = await crypto.subtle.digest("SHA-256", bytes as BufferSource);
  return toHex(digest);
}

/** A short, URL-safe random identifier. */
export function randomId(prefix = ""): string {
  const bytes = crypto.getRandomValues(new Uint8Array(9));
  const body = toHex(bytes).slice(0, 12);
  return prefix ? `${prefix}-${body}` : body;
}

/**
 * Whether the browser supports a given key agreement algorithm.
 *
 * X25519 is preferred - it is what the Rust side of the stack uses - but
 * P-256 is supported everywhere, so it is the fallback.
 */
export async function detectKeyAgreement(): Promise<KeyAgreement> {
  if (typeof crypto === "undefined" || !crypto.subtle) return "P-256";
  try {
    await crypto.subtle.generateKey({ name: "X25519" }, true, ["deriveBits"]);
    return "X25519";
  } catch {
    return "P-256";
  }
}

/** Generate a key agreement key pair for the given algorithm. */
export async function generateAgreementKeyPair(
  algorithm: KeyAgreement,
): Promise<AgreementKeyPair> {
  if (algorithm === "X25519") {
    const pair = (await crypto.subtle.generateKey(
      { name: "X25519" },
      true,
      ["deriveBits"],
    )) as CryptoKeyPair;
    return { algorithm, publicKey: pair.publicKey, privateKey: pair.privateKey };
  }

  const pair = (await crypto.subtle.generateKey(
    { name: "ECDH", namedCurve: "P-256" },
    true,
    ["deriveBits"],
  )) as CryptoKeyPair;
  return { algorithm, publicKey: pair.publicKey, privateKey: pair.privateKey };
}

/** Import a peer's public key from the wire format. */
export async function importAgreementPublicKey(
  algorithm: KeyAgreement,
  raw: ArrayBuffer,
): Promise<CryptoKey> {
  if (algorithm === "X25519") {
    return crypto.subtle.importKey(
      "raw",
      raw,
      { name: "X25519" },
      true,
      [],
    );
  }
  return crypto.subtle.importKey(
    "raw",
    raw,
    { name: "ECDH", namedCurve: "P-256" },
    true,
    [],
  );
}

/** Export the raw bytes of a public key. */
export async function exportPublicKey(key: CryptoKey): Promise<Bytes> {
  return new Uint8Array(await crypto.subtle.exportKey("raw", key));
}

/** Run the ECDH exchange. */
export async function deriveSharedBits(
  pair: AgreementKeyPair,
  peerPublicKey: CryptoKey,
): Promise<SharedKey> {
  const bits = await crypto.subtle.deriveBits(
    { name: pair.algorithm, public: peerPublicKey },
    pair.privateKey,
    pair.algorithm === "X25519" ? 256 : 256,
  );
  return { algorithm: pair.algorithm, bits };
}

/**
 * Derive an AES-GCM key from a shared secret.
 *
 * The salt binds the key to *both* parties, so a key can only ever be used
 * with the exact peer it was derived with.
 */
export async function deriveMessageKey(
  shared: SharedKey,
  localPublicKey: CryptoKey,
  peerPublicKey: CryptoKey,
): Promise<CryptoKey> {
  const local = await exportPublicKey(localPublicKey);
  const peer = await exportPublicKey(peerPublicKey);

  // Order the halves by their bytes so both sides hash the same string.
  const [first, second] =
    toHex(local) <= toHex(peer) ? [local, peer] : [peer, local];
  const salt = new Uint8Array(await crypto.subtle.digest("SHA-256", first));

  const material = await crypto.subtle.importKey("raw", shared.bits, "HKDF", false, [
    "deriveKey",
  ]);

  return crypto.subtle.deriveKey(
    {
      name: "HKDF",
      hash: "SHA-256",
      salt: salt as BufferSource,
      info: new TextEncoder().encode(`mezots/dm/v1/${shared.algorithm}`),
    },
    material,
    { name: "AES-GCM", length: 256 },
    false,
    ["encrypt", "decrypt"],
  );
}

/** The envelope produced by {@link encryptMessage}. */
export interface EncryptedEnvelope {
  /** base64 of `iv || ciphertext`. */
  payload: string;
  /** The random nonce used, hex encoded. */
  iv: string;
}

/** Encrypt a string with AES-GCM. */
export async function encryptMessage(
  key: CryptoKey,
  plaintext: string,
): Promise<EncryptedEnvelope> {
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const cipher = await crypto.subtle.encrypt(
    { name: "AES-GCM", iv },
    key,
    new TextEncoder().encode(plaintext) as BufferSource,
  );
  const combined = new Uint8Array(iv.length + cipher.byteLength);
  combined.set(iv, 0);
  combined.set(new Uint8Array(cipher), iv.length);
  return { payload: toBase64(combined), iv: toHex(iv) };
}

/** Decrypt a payload produced by {@link encryptMessage}. */
export async function decryptMessage(
  key: CryptoKey,
  payload: string,
): Promise<string> {
  const combined = fromBase64(payload);
  const iv = combined.slice(0, 12);
  const body = combined.slice(12);
  const plain = await crypto.subtle.decrypt(
    { name: "AES-GCM", iv },
    key,
    body as BufferSource,
  );
  return new TextDecoder().decode(plain);
}

/**
 * Whether a browser can verify Ed25519 signatures.
 *
 * The chain explorer uses this to check the node's signature on a block. When
 * it is unavailable the chain is still verified by hash, and the UI says so.
 */
export async function supportsEd25519(): Promise<boolean> {
  if (typeof crypto === "undefined" || !crypto.subtle) return false;
  try {
    await crypto.subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"]);
    return true;
  } catch {
    return false;
  }
}

/** Sign bytes with an Ed25519 key pair. */
export async function signEd25519(
  privateKey: CryptoKey,
  data: Bytes,
): Promise<string> {
  const signature = await crypto.subtle.sign(
    { name: "Ed25519" },
    privateKey,
    data as BufferSource,
  );
  return toHex(signature);
}

/** Verify an Ed25519 signature. */
export async function verifyEd25519(
  publicKey: CryptoKey,
  signature: Bytes,
  data: Bytes,
): Promise<boolean> {
  try {
    return await crypto.subtle.verify(
      { name: "Ed25519" },
      publicKey,
      signature as BufferSource,
      data as BufferSource,
    );
  } catch {
    return false;
  }
}

/**
 * The canonical bytes of a ledger block header.
 *
 * The Rust node hashes and signs exactly these bytes, so both sides agree on
 * what a block commits to. Length prefixes stop two different field splits
 * from producing the same string.
 */
export function canonicalBlockHeader(block: {
  index: number;
  ts: number;
  actor: string;
  seq: number;
  room: string;
  kind: string;
  payloadHash: string;
  prevHash: string;
}): Bytes {
  const encoder = new TextEncoder();
  const parts = [
    String(block.index),
    String(block.ts),
    block.actor,
    String(block.seq),
    block.room,
    block.kind,
    block.payloadHash,
    block.prevHash,
  ];
  const bytes: number[] = [];
  for (const part of parts) {
    const encoded = encoder.encode(part);
    bytes.push((encoded.length >>> 24) & 0xff);
    bytes.push((encoded.length >>> 16) & 0xff);
    bytes.push((encoded.length >>> 8) & 0xff);
    bytes.push(encoded.length & 0xff);
    for (const byte of encoded) bytes.push(byte);
  }
  return new Uint8Array(bytes);
}
