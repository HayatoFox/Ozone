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

import { api, setTokens } from "../api";
import type { RegisterRequest, Snowflake, TokenPair } from "../types";

const PRIV_KEY_STORAGE = "ozone.e2ee.priv";
const PUB_KEY_STORAGE = "ozone.e2ee.pub";
const SALT_STORAGE = "ozone.e2ee.salt"; // sel KDF du compte (pour re-dériver au changement de mdp)

let myPrivateKey: CryptoKey | null = null;
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
 * Oublie TOUT le matériel local (clé privée, sel, caches) — à appeler à la déconnexion. Évite qu'un
 * autre utilisateur sur le même appareil réutilise/charge par erreur la clé du précédent.
 */
export function e2eeForget(): void {
  myPrivateKey = null;
  keypairReady = null;
  sharedKeyCache.clear();
  pubKeyCache.clear();
  try {
    localStorage.removeItem(PRIV_KEY_STORAGE);
    localStorage.removeItem(PUB_KEY_STORAGE);
    localStorage.removeItem(SALT_STORAGE);
  } catch {
    /* localStorage indisponible */
  }
}

/** Installe la clé privée DM en mémoire (+ cache local) à partir de son JWK et de la clé publique SPKI. */
async function installPrivateKey(privJwk: JsonWebKey, pubSpki: string): Promise<void> {
  myPrivateKey = await crypto.subtle.importKey(
    "jwk",
    privJwk,
    { name: "ECDH", namedCurve: "P-256" },
    false,
    ["deriveKey"],
  );
  // Cache local (boot hors-ligne / réouverture sans ressaisir le mot de passe). La SOURCE DE VÉRITÉ
  // reste l'escrow serveur ; ce cache n'est qu'une commodité.
  localStorage.setItem(PRIV_KEY_STORAGE, JSON.stringify(privJwk));
  localStorage.setItem(PUB_KEY_STORAGE, pubSpki);
  // Les clés partagées dérivées avec l'ancienne clé locale (le cas échéant) sont caduques.
  sharedKeyCache.clear();
}

/**
 * Charge la clé privée DM depuis le cache local si présente (réouverture de session sans mot de passe).
 * NE génère NI ne publie plus de clé (modèle escrow : la clé est provisionnée au register / déverrouillée
 * au login). Sans cache, la clé reste indisponible jusqu'au prochain login (saisie du mot de passe).
 */
export function ensureKeypair(): Promise<void> {
  if (!e2eeAvailable()) return Promise.resolve();
  if (keypairReady) return keypairReady;
  keypairReady = (async () => {
    if (myPrivateKey) return;
    const storedPriv = localStorage.getItem(PRIV_KEY_STORAGE);
    const storedPub = localStorage.getItem(PUB_KEY_STORAGE);
    if (storedPriv && storedPub) {
      try {
        await installPrivateKey(JSON.parse(storedPriv), storedPub);
      } catch {
        myPrivateKey = null;
      }
    }
  })();
  return keypairReady;
}

/**
 * Provisionne une NOUVELLE paire de clés DM (inscription, ou migration v1→v2 sans clé existante) :
 * génère la paire, l'installe localement, et renvoie le matériel à déposer côté serveur (escrow).
 * `priv_wrapped` = clé privée chiffrée par la KEK dérivée du mot de passe (le serveur ne peut pas la lire).
 */
export async function provisionKeys(
  password: string,
  saltHex: string,
): Promise<{ public_key: string; priv_wrapped: string }> {
  // Réutilise une clé locale existante (ex. migration sur l'appareil d'origine) pour ne pas perdre
  // l'accès aux anciens MP ; sinon en génère une neuve.
  let privJwk: JsonWebKey;
  let pubSpki: string;
  const cachedPriv = localStorage.getItem(PRIV_KEY_STORAGE);
  const cachedPub = localStorage.getItem(PUB_KEY_STORAGE);
  if (cachedPriv && cachedPub) {
    privJwk = JSON.parse(cachedPriv);
    pubSpki = cachedPub;
  } else {
    const pair = await crypto.subtle.generateKey({ name: "ECDH", namedCurve: "P-256" }, true, [
      "deriveKey",
    ]);
    privJwk = await crypto.subtle.exportKey("jwk", pair.privateKey);
    pubSpki = b64encode(await crypto.subtle.exportKey("spki", pair.publicKey));
  }
  const { kek } = await deriveAuthKeys(password, saltHex);
  const priv_wrapped = await wrapPrivateKey(kek, privJwk);
  await installPrivateKey(privJwk, pubSpki);
  localStorage.setItem(SALT_STORAGE, saltHex);
  return { public_key: pubSpki, priv_wrapped };
}

/**
 * Déverrouille la clé privée DM depuis l'escrow serveur (login sur N'IMPORTE quel appareil) : récupère
 * `priv_wrapped`, le déballe avec la KEK dérivée du mot de passe, et installe la clé. Renvoie `true`
 * si la clé a été récupérée (l'utilisateur peut lire ses anciens MP).
 */
export async function unlockFromEscrow(password: string, saltHex: string): Promise<boolean> {
  if (!e2eeAvailable()) return false;
  let escrow: { priv_wrapped: string | null; public_key: string | null };
  try {
    escrow = await api.getEncryption();
  } catch {
    return false;
  }
  if (!escrow.priv_wrapped || !escrow.public_key) return false;
  const { kek } = await deriveAuthKeys(password, saltHex);
  try {
    const privJwk = await unwrapPrivateKey(kek, escrow.priv_wrapped);
    await installPrivateKey(privJwk, escrow.public_key);
    localStorage.setItem(SALT_STORAGE, saltHex);
    keypairReady = Promise.resolve();
    return true;
  } catch {
    // KEK incorrecte (ne devrait pas arriver après un login réussi) → on laisse la clé indisponible.
    return false;
  }
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

// ───────────────────── Dérivation de clés depuis le mot de passe (auth ZK + escrow) ─────────────────────
// Le serveur ne voit JAMAIS le mot de passe. Le client en dérive deux valeurs indépendantes :
//  • `authSecret` — envoyé au serveur en guise de « mot de passe » (le serveur n'en stocke qu'un hash Argon2) ;
//  • `KEK` (key-encryption key) — RESTE locale, (dé)chiffre la clé privée DM déposée chiffrée côté serveur.
// PBKDF2 (600k itérations) → clé maîtresse, puis HKDF pour séparer les deux. On ne peut pas remonter du
// `authSecret` (un hash) au mot de passe, donc pas à la KEK → l'admin d'instance ne peut pas déchiffrer.

const PBKDF2_ITERATIONS = 600_000;

/** Génère un sel KDF aléatoire (32 octets → hex 64 car.), choisi par le client à l'inscription. */
export function randomSaltHex(): string {
  const b = crypto.getRandomValues(new Uint8Array(32));
  return Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
}

/** PBKDF2(mot de passe, sel) → clé HKDF maîtresse. Étape coûteuse, une seule fois par login. */
async function deriveMasterKey(password: string, saltHex: string): Promise<CryptoKey> {
  const pwKey = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(password),
    "PBKDF2",
    false,
    ["deriveBits"],
  );
  const bits = await crypto.subtle.deriveBits(
    { name: "PBKDF2", salt: new TextEncoder().encode(saltHex), iterations: PBKDF2_ITERATIONS, hash: "SHA-256" },
    pwKey,
    256,
  );
  return crypto.subtle.importKey("raw", bits, "HKDF", false, ["deriveBits"]);
}

async function hkdf(master: CryptoKey, info: string, bits = 256): Promise<ArrayBuffer> {
  return crypto.subtle.deriveBits(
    {
      name: "HKDF",
      hash: "SHA-256",
      salt: new Uint8Array(0),
      info: new TextEncoder().encode(info),
    },
    master,
    bits,
  );
}

export interface AuthKeys {
  /** À envoyer au serveur en guise de « mot de passe » (base64 d'un hash de 256 bits). */
  authSecret: string;
  /** Clé de chiffrement de clé (KEK) — ne quitte JAMAIS le client. */
  kek: CryptoKey;
}

/** Dérive `authSecret` (envoyé) et `KEK` (locale) depuis le mot de passe et le sel KDF (du prelogin). */
export async function deriveAuthKeys(password: string, saltHex: string): Promise<AuthKeys> {
  const master = await deriveMasterKey(password, saltHex);
  const authSecret = b64encode(await hkdf(master, "ozone-auth-v2"));
  const kekBits = await hkdf(master, "ozone-kek-v2");
  const kek = await crypto.subtle.importKey("raw", kekBits, { name: "AES-GCM" }, false, [
    "encrypt",
    "decrypt",
  ]);
  return { authSecret, kek };
}

/** Emballe (chiffre) une clé privée DM (JWK) avec la KEK → blob « iv|ciphertext » base64. */
export async function wrapPrivateKey(kek: CryptoKey, privJwk: JsonWebKey): Promise<string> {
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ct = await crypto.subtle.encrypt(
    { name: "AES-GCM", iv },
    kek,
    new TextEncoder().encode(JSON.stringify(privJwk)),
  );
  return `${b64encode(iv.buffer)}|${b64encode(ct)}`;
}

/** Déballe la clé privée DM emballée par la KEK. Lève si la KEK (donc le mot de passe) est incorrecte. */
export async function unwrapPrivateKey(kek: CryptoKey, wrapped: string): Promise<JsonWebKey> {
  const sep = wrapped.indexOf("|");
  if (sep <= 0) throw new Error("format de clé emballée invalide");
  const pt = await crypto.subtle.decrypt(
    { name: "AES-GCM", iv: b64decode(wrapped.slice(0, sep)) },
    kek,
    b64decode(wrapped.slice(sep + 1)),
  );
  return JSON.parse(new TextDecoder().decode(pt)) as JsonWebKey;
}

// ───────────────────────── Orchestration auth (inscription / connexion) ─────────────────────────
// Le mot de passe BRUT ne quitte le poste qu'au prelogin (jamais) et à la migration v1→v2 (une fois).
// Sinon le serveur ne reçoit que l'`authSecret` dérivé. Ces fonctions posent les jetons puis (dé)bloquent
// la clé DM ; l'appelant n'a plus qu'à lancer `afterAuth()`.

/** Inscription (v2) : sel + paire de clés + escrow, puis on n'envoie que l'`authSecret`. */
export async function e2eeRegister(
  req: Omit<RegisterRequest, "password" | "public_key" | "priv_wrapped" | "kdf_salt"> & {
    password: string;
  },
): Promise<TokenPair> {
  if (!e2eeAvailable()) {
    // Pas de WebCrypto → inscription legacy (mot de passe brut, sans E2EE).
    const tokens = await api.register(req as RegisterRequest);
    setTokens(tokens);
    return tokens;
  }
  const kdf_salt = randomSaltHex();
  const { public_key, priv_wrapped } = await provisionKeys(req.password, kdf_salt);
  const { authSecret } = await deriveAuthKeys(req.password, kdf_salt);
  const tokens = await api.register({
    ...req,
    password: authSecret,
    public_key,
    priv_wrapped,
    kdf_salt,
  });
  setTokens(tokens);
  return tokens;
}

/** Connexion : prelogin → dérivation → login (v2) ou login legacy + migration v1→v2, puis déverrouillage. */
export async function e2eeLogin(
  login: string,
  password: string,
  gateToken?: string,
): Promise<TokenPair> {
  if (!e2eeAvailable()) {
    const tokens = await api.login({ login, password, gate_token: gateToken });
    setTokens(tokens);
    return tokens;
  }
  const { kdf_salt, pw_scheme } = await api.prelogin(login);
  if (pw_scheme === 2) {
    const { authSecret } = await deriveAuthKeys(password, kdf_salt);
    const tokens = await api.login({ login, password: authSecret, gate_token: gateToken });
    setTokens(tokens);
    await unlockFromEscrow(password, kdf_salt);
    return tokens;
  }
  // Compte legacy (v1) : on prouve le mot de passe brut une fois, puis bascule transparente en v2.
  const tokens = await api.login({ login, password, gate_token: gateToken });
  setTokens(tokens);
  try {
    const newSalt = randomSaltHex();
    const { public_key, priv_wrapped } = await provisionKeys(password, newSalt);
    const { authSecret } = await deriveAuthKeys(password, newSalt);
    await api.upgradeEncryption({
      current_password: password,
      auth_secret: authSecret,
      public_key,
      priv_wrapped,
      kdf_salt: newSalt,
    });
  } catch {
    // Échec de migration (réseau, etc.) : la session reste valide ; nouvel essai au prochain login.
  }
  return tokens;
}

/**
 * Changement de mot de passe (v2) : ré-emballe la clé privée DM avec la nouvelle KEK (le sel reste
 * inchangé), sinon l'escrow deviendrait indéchiffrable au prochain login. Repli legacy si pas de sel.
 */
export async function e2eeChangePassword(
  currentPassword: string,
  newPassword: string,
): Promise<void> {
  const saltHex = e2eeAvailable() ? localStorage.getItem(SALT_STORAGE) : null;
  if (!saltHex) {
    // Compte legacy/non chiffré : changement de mot de passe direct (mot de passe brut).
    await api.changePassword({ current_password: currentPassword, new_password: newPassword });
    return;
  }
  const cur = await deriveAuthKeys(currentPassword, saltHex);
  const next = await deriveAuthKeys(newPassword, saltHex);
  // Récupère la clé privée (cache local, sinon escrow déverrouillé avec l'ANCIENNE KEK) pour la ré-emballer.
  let privJwk: JsonWebKey | null = null;
  const cached = localStorage.getItem(PRIV_KEY_STORAGE);
  if (cached) {
    privJwk = JSON.parse(cached);
  } else {
    try {
      const escrow = await api.getEncryption();
      if (escrow.priv_wrapped) privJwk = await unwrapPrivateKey(cur.kek, escrow.priv_wrapped);
    } catch {
      /* pas d'escrow récupérable */
    }
  }
  const priv_wrapped = privJwk ? await wrapPrivateKey(next.kek, privJwk) : undefined;
  await api.changePassword({
    current_password: cur.authSecret,
    new_password: next.authSecret,
    priv_wrapped,
  });
}
