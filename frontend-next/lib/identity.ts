/**
 * Who this browser is, and the keys that prove it.
 *
 * The browser keeps two key pairs:
 *
 * - an **identity** pair (Ed25519) whose fingerprint is the safety number a
 *   student can read out loud to confirm they are talking to the right person;
 * - an **agreement** pair (X25519, or P-256 where X25519 is missing) used to
 *   derive the pairwise key that encrypts direct messages.
 *
 * The private keys never leave the device. They live in IndexedDB, so they
 * survive a reload but are not exposed to the node, to a server or to a
 * shared machine's profile synchronisation.
 */

import {
  detectKeyAgreement,
  exportPublicKey,
  generateAgreementKeyPair,
  sha256Hex,
  toBase64,
  type AgreementKeyPair,
  type KeyAgreement,
} from "./crypto";

const DB_NAME = "mezots-vault";
const DB_VERSION = 1;
const STORE = "keys";
const NAME_KEY = "identity";
const NAME_AGREEMENT = "agreement";
const PREF_NAME = "mezots.displayName";

/** The browser's identity. */
export interface Identity {
  /** Stable id derived from the fingerprint. */
  nodeId: string;
  /** Editable display name. */
  displayName: string;
  /** Hex SHA-256 of the public identity key. */
  fingerprint: string;
  /** base64 of the raw public identity key. */
  publicKey: string;
  /** The Ed25519 private key, for signing. */
  signingKey: CryptoKey;
  /** The public half of the signing key. */
  signingPublicKey: CryptoKey;
  /** The key agreement key pair used for direct messages. */
  agreement: AgreementKeyPair;
  /** base64 of the raw public agreement key. */
  agreementPublicKey: string;
  /** Which agreement algorithm is in use. */
  algorithm: KeyAgreement;
}

/* -------------------------------------------------------------------------- */
/* IndexedDB vault                                                             */
/* -------------------------------------------------------------------------- */

function openDatabase(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(STORE)) {
        db.createObjectStore(STORE);
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

async function readRecord<T>(name: string): Promise<T | undefined> {
  if (typeof indexedDB === "undefined") return undefined;
  try {
    const db = await openDatabase();
    return await new Promise<T | undefined>((resolve, reject) => {
      const tx = db.transaction(STORE, "readonly");
      const request = tx.objectStore(STORE).get(name);
      request.onsuccess = () => resolve(request.result as T | undefined);
      request.onerror = () => reject(request.error);
    });
  } catch {
    return undefined;
  }
}

async function writeRecord(name: string, value: unknown): Promise<void> {
  if (typeof indexedDB === "undefined") return;
  const db = await openDatabase();
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction(STORE, "readwrite");
    tx.objectStore(STORE).put(value, name);
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });
}

/* -------------------------------------------------------------------------- */
/* Identity                                                                    */
/* -------------------------------------------------------------------------- */

/** The stored shape of the signing key pair. */
interface StoredSigningKey {
  algorithm: "Ed25519";
  publicKey: string;
  privateKey: JsonWebKey;
}

/** The stored shape of the agreement key pair. */
interface StoredAgreementKey {
  algorithm: KeyAgreement;
  publicKey: string;
  privateKey: JsonWebKey;
}

/**
 * Load the identity, creating one the first time the app runs.
 *
 * The keys are generated locally and never transmitted.
 */
export async function loadIdentity(): Promise<Identity> {
  const algorithm = await detectKeyAgreement();

  let signing = await readRecord<StoredSigningKey>(NAME_KEY);
  let agreement = await readRecord<StoredAgreementKey>(NAME_AGREEMENT);

  // Ed25519 signing keys are optional: a browser without support still gets
  // working direct-message encryption, it just cannot verify chain
  // signatures locally.
  let signingKey: CryptoKey | null = null;
  let signingPublicKey: CryptoKey | null = null;
  let publicKeyB64 = "";

  if (signing) {
    try {
      signingKey = await crypto.subtle.importKey(
        "jwk",
        signing.privateKey,
        { name: "Ed25519" },
        true,
        ["sign"],
      );
      signingPublicKey = await crypto.subtle.importKey(
        "jwk",
        { ...signing.privateKey, key_ops: ["verify"], d: undefined },
        { name: "Ed25519" },
        true,
        ["verify"],
      );
      publicKeyB64 = signing.publicKey;
    } catch {
      signing = undefined;
    }
  }

  if (!signing) {
    const pair = (await crypto.subtle.generateKey(
      { name: "Ed25519" },
      true,
      ["sign", "verify"],
    )) as CryptoKeyPair;
    const raw = await exportPublicKey(pair.publicKey);
    const stored: StoredSigningKey = {
      algorithm: "Ed25519",
      publicKey: toBase64(raw),
      privateKey: await crypto.subtle.exportKey("jwk", pair.privateKey),
    };
    await writeRecord(NAME_KEY, stored);
    signingKey = pair.privateKey;
    signingPublicKey = pair.publicKey;
    publicKeyB64 = stored.publicKey;
  }

  let agreementPair: AgreementKeyPair | null = null;
  if (agreement) {
    try {
      const privateKey = await crypto.subtle.importKey(
        "jwk",
        agreement.privateKey,
        agreement.algorithm === "X25519"
          ? { name: "X25519" }
          : { name: "ECDH", namedCurve: "P-256" },
        true,
        ["deriveBits"],
      );
      const publicKey = await importPublic(agreement.algorithm, agreement.publicKey);
      agreementPair = { algorithm: agreement.algorithm, publicKey, privateKey };
    } catch {
      agreement = undefined;
    }
  }

  if (!agreementPair) {
    const pair = await generateAgreementKeyPair(algorithm);
    const raw = await exportPublicKey(pair.publicKey);
    const stored: StoredAgreementKey = {
      algorithm,
      publicKey: toBase64(raw),
      privateKey: await crypto.subtle.exportKey("jwk", pair.privateKey),
    };
    await writeRecord(NAME_AGREEMENT, stored);
    agreementPair = pair;
  }

  const fingerprint = await sha256Hex(fromBase64Compat(publicKeyB64));
  const displayName = readDisplayName() ?? defaultDisplayName();

  return {
    nodeId: `me-${fingerprint.slice(0, 12)}`,
    displayName,
    fingerprint,
    publicKey: publicKeyB64,
    signingKey: signingKey as CryptoKey,
    signingPublicKey: signingPublicKey as CryptoKey,
    agreement: agreementPair,
    agreementPublicKey: toBase64(
      await exportPublicKey(agreementPair.publicKey),
    ),
    algorithm,
  };
}

/** Import a public key that was exported as base64 raw bytes. */
async function importPublic(
  algorithm: KeyAgreement,
  base64: string,
): Promise<CryptoKey> {
  const raw = fromBase64Compat(base64);
  if (algorithm === "X25519") {
    return crypto.subtle.importKey("raw", raw, { name: "X25519" }, true, []);
  }
  return crypto.subtle.importKey(
    "raw",
    raw,
    { name: "ECDH", namedCurve: "P-256" },
    true,
    [],
  );
}

/** Small wrappers so the module does not depend on the crypto module's order. */
function fromBase64Compat(value: string): Uint8Array<ArrayBuffer> {
  const binary = atob(value);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

/** A friendly default name, derived from the fingerprint. */
function defaultDisplayName(): string {
  const adjectives = [
    "Quiet",
    "Bright",
    "Steady",
    "Clever",
    "Calm",
    "Sharp",
    "Kind",
    "Swift",
  ];
  const pick = adjectives[Math.floor(Math.random() * adjectives.length)];
  return `${pick} Student`;
}

/** The stored display name, if the student set one. */
export function readDisplayName(): string | null {
  try {
    return window.localStorage.getItem(PREF_NAME);
  } catch {
    return null;
  }
}

/** Store the display name. */
export function writeDisplayName(name: string): void {
  try {
    window.localStorage.setItem(PREF_NAME, name);
  } catch {
    /* storage unavailable */
  }
}

/**
 * The safety number shown in direct messages.
 *
 * It is the fingerprint of both identities, sorted, so both sides always
 * read the same digits.
 */
export function safetyNumber(
  localFingerprint: string,
  remoteFingerprint: string,
): string {
  const sorted = [localFingerprint, remoteFingerprint].sort();
  const joined = `${sorted[0]}${sorted[1]}`;
  const groups: string[] = [];
  for (let i = 0; i < joined.length; i += 5) {
    groups.push(joined.slice(i, i + 5).toUpperCase());
  }
  return groups.join(" ");
}

/** A short, readable form of a fingerprint. */
export function shortFingerprint(fingerprint: string): string {
  return fingerprint.slice(0, 8).toUpperCase();
}

/** The hex fingerprint of an arbitrary public key, for the node panel. */
export async function fingerprintOf(publicKeyBase64: string): Promise<string> {
  return sha256Hex(fromBase64Compat(publicKeyBase64));
}
