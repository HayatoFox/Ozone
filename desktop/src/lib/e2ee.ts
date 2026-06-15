// Chiffrement de bout en bout des messages privés 1:1.
//
// Modèle : chaque utilisateur possède une paire de clés ECDH P-256. La clé PRIVÉE ne quitte JAMAIS
// le poste (stockée en localStorage) ; seule la clé PUBLIQUE (SPKI base64) est publiée au serveur.
// Pour un MP entre A et B, les deux dérivent le MÊME secret partagé via ECDH(privA, pubB) ==
// ECDH(privB, pubA), d'où une clé AES-GCM 256 bits commune. Le serveur ne stocke qu'un blob opaque
// « iv|ciphertext » → l'administrateur de l'instance (accès BDD/SSH) ne peut pas lire les MP.
//
// Limite assumée : pas de sauvegarde de clé. Vider le stockage du navigateur ou se connecter depuis
// un autre poste génère une nouvelle paire → les anciens MP chiffrés deviennent illisibles. C'est le
// prix de l'inaccessibilité côté serveur (aucune clé privée n'y est jamais déposée).

import { api } from "../api";
import type { Snowflake } from "../types";

const PRIV_KEY_STORAGE = "ozone.e2ee.priv";
const PUB_KEY_STORAGE = "ozone.e2ee.pub";

let myPrivateKey: CryptoKey | null = null;
let myPublicSpki: string | null = null;
let keypairReady: Promise<void> | null = null;
const sharedKeyCache = new Map<Snowflake, Promise<CryptoKey>>();
const pubKeyCache = new Map<Snowflake, Promise<string | null>>();

function b64encode(buf: ArrayBuffer): string {
  const bytes = new Uint8Array(buf);
  let s = "";
  for (let i = 0; i < bytes.length; i += 1) s += String.fromCharCode(bytes[i]);
  return btoa(s);
}

function b64decode(s: string): ArrayBuffer {
  const bin = atob(s);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i += 1) out[i] = bin.charCodeAt(i);
  return out.buffer;
}

/** WebCrypto (SubtleCrypto) disponible ? Faux en contexte non sécurisé sans crypto.subtle. */
export function e2eeAvailable(): boolean {
  return typeof crypto !== "undefined" && !!crypto.subtle;
}

/**
 * Garantit l'existence d'une paire de clés locale et publie la clé publique (idempotent).
 * À appeler au démarrage de session. Best-effort : un échec réseau est réessayé au prochain boot.
 */
export function ensureKeypair(): Promise<void> {
  if (!e2eeAvailable()) return Promise.resolve();
  if (keypairReady) return keypairReady;
  keypairReady = (async () => {
    const storedPriv = localStorage.getItem(PRIV_KEY_STORAGE);
    const storedPub = localStorage.getItem(PUB_KEY_STORAGE);
    if (storedPriv && storedPub) {
      try {
        myPrivateKey = await crypto.subtle.importKey(
          "jwk",
          JSON.parse(storedPriv),
          { name: "ECDH", namedCurve: "P-256" },
          false,
          ["deriveKey"],
        );
        myPublicSpki = storedPub;
      } catch {
        myPrivateKey = null;
      }
    }
    if (!myPrivateKey) {
      const pair = await crypto.subtle.generateKey(
        { name: "ECDH", namedCurve: "P-256" },
        true,
        ["deriveKey"],
      );
      const privJwk = await crypto.subtle.exportKey("jwk", pair.privateKey);
      const pubSpki = b64encode(await crypto.subtle.exportKey("spki", pair.publicKey));
      localStorage.setItem(PRIV_KEY_STORAGE, JSON.stringify(privJwk));
      localStorage.setItem(PUB_KEY_STORAGE, pubSpki);
      // Réimport en clé non exportable pour l'usage courant (dérivation uniquement).
      myPrivateKey = await crypto.subtle.importKey(
        "jwk",
        privJwk,
        { name: "ECDH", namedCurve: "P-256" },
        false,
        ["deriveKey"],
      );
      myPublicSpki = pubSpki;
    }
    try {
      if (myPublicSpki) await api.putPublicKey(myPublicSpki);
    } catch {
      // Réessai au prochain démarrage — la clé locale est déjà persistée.
    }
  })();
  return keypairReady;
}

function fetchPublicKey(userId: Snowflake): Promise<string | null> {
  const cached = pubKeyCache.get(userId);
  if (cached) return cached;
  const p = (async () => {
    try {
      return (await api.getPublicKey(userId)).public_key ?? null;
    } catch {
      return null;
    }
  })();
  pubKeyCache.set(userId, p);
  // On ne mémorise QUE les clés présentes : un résultat `null` (pair sans clé encore publiée, ou
  // erreur réseau) est purgé pour autoriser un nouvel essai — sinon on resterait en clair toute
  // la session alors que le pair a peut-être publié sa clé entre-temps.
  void p.then((k) => {
    if (!k && pubKeyCache.get(userId) === p) pubKeyCache.delete(userId);
  });
  return p;
}

/** Dérive (et met en cache) la clé AES-GCM partagée avec un autre utilisateur. */
function sharedKey(otherUserId: Snowflake): Promise<CryptoKey> {
  let p = sharedKeyCache.get(otherUserId);
  if (!p) {
    p = (async () => {
      await ensureKeypair();
      if (!myPrivateKey) throw new Error("clé locale indisponible");
      const otherPub = await fetchPublicKey(otherUserId);
      if (!otherPub) throw new Error("clé publique du destinataire indisponible");
      const pubKey = await crypto.subtle.importKey(
        "spki",
        b64decode(otherPub),
        { name: "ECDH", namedCurve: "P-256" },
        false,
        [],
      );
      return crypto.subtle.deriveKey(
        { name: "ECDH", public: pubKey },
        myPrivateKey,
        { name: "AES-GCM", length: 256 },
        false,
        ["encrypt", "decrypt"],
      );
    })();
    sharedKeyCache.set(otherUserId, p);
  }
  return p;
}

/** Le destinataire a-t-il publié une clé publique ? (sinon on retombe en clair). */
export async function hasPublicKey(otherUserId: Snowflake): Promise<boolean> {
  if (!e2eeAvailable()) return false;
  return (await fetchPublicKey(otherUserId)) !== null;
}

/** Chiffre un texte pour un destinataire → blob « iv|ciphertext » base64. */
export async function encryptForUser(otherUserId: Snowflake, plaintext: string): Promise<string> {
  const key = await sharedKey(otherUserId);
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ct = await crypto.subtle.encrypt(
    { name: "AES-GCM", iv },
    key,
    new TextEncoder().encode(plaintext),
  );
  return `${b64encode(iv.buffer)}|${b64encode(ct)}`;
}

/** Déchiffre un blob « iv|ciphertext » reçu d'un MP avec un utilisateur donné. */
export async function decryptFromUser(otherUserId: Snowflake, cipher: string): Promise<string> {
  const sep = cipher.indexOf("|");
  if (sep <= 0) throw new Error("format chiffré invalide");
  const ivB64 = cipher.slice(0, sep);
  const ctB64 = cipher.slice(sep + 1);
  const attempt = async () => {
    const key = await sharedKey(otherUserId);
    const pt = await crypto.subtle.decrypt(
      { name: "AES-GCM", iv: b64decode(ivB64) },
      key,
      b64decode(ctB64),
    );
    return new TextDecoder().decode(pt);
  };
  try {
    return await attempt();
  } catch {
    // La clé du pair a peut-être tourné : on purge les caches et on retente une fois.
    pubKeyCache.delete(otherUserId);
    sharedKeyCache.delete(otherUserId);
    return await attempt();
  }
}
