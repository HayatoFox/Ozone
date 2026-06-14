// Ciblage de l'instance Ozone (serveur API + Gateway + SFU).
//
// Deux modes :
//  - WEB (navigateur, dev via proxy Vite ou prod derrière un reverse-proxy) : aucune URL n'est
//    configurée → on cible l'ORIGINE COURANTE par des chemins relatifs (`/api`, `/gateway`,
//    `/sfu`). C'est le comportement historique, inchangé.
//  - EMPAQUETÉ (.exe Tauri) : le front est chargé depuis `tauri://localhost`, qui n'expose aucune
//    API. L'utilisateur saisit l'URL de son instance (ex. https://chat.exemple.fr) ; elle est
//    persistée et sert de base ABSOLUE à toutes les requêtes REST et WebSocket.

const INSTANCE_KEY = "ozone.instanceUrl";
// Préfixe des routes REST sur l'instance. En déploiement standard (front + reverse-proxy), l'API
// est servie sous `/api`. Si le .exe vise l'API NUE (ex. http://ip:8080 sans proxy), le préfixe
// est vide. Déterminé au moment de la connexion (cf. InstanceGate) et persisté.
const PREFIX_KEY = "ozone.instancePrefix";

let instanceBase: string | null = loadInstanceUrl();
let instancePrefix: string =
  typeof localStorage !== "undefined" ? localStorage.getItem(PREFIX_KEY) ?? "/api" : "/api";

/** URL d'instance persistée (sans slash final), ou null si non configurée (mode web/origine). */
export function loadInstanceUrl(): string | null {
  if (typeof localStorage === "undefined") return null;
  const v = localStorage.getItem(INSTANCE_KEY);
  return v ? v.replace(/\/+$/, "") : null;
}

/** Définit (et persiste) l'URL d'instance + le préfixe REST. `null` efface → mode origine. */
export function setInstanceUrl(url: string | null, prefix = "/api"): void {
  const clean = url ? url.trim().replace(/\/+$/, "") : null;
  instanceBase = clean;
  instancePrefix = prefix;
  if (typeof localStorage === "undefined") return;
  if (clean) {
    localStorage.setItem(INSTANCE_KEY, clean);
    localStorage.setItem(PREFIX_KEY, prefix);
  } else {
    localStorage.removeItem(INSTANCE_KEY);
    localStorage.removeItem(PREFIX_KEY);
  }
}

/**
 * Sommes-nous dans le client empaqueté (Tauri) plutôt que dans un navigateur web ?
 *
 * On NE peut PAS se fier au protocole : Tauri v2 sert le front via `http(s)://tauri.localhost`
 * (Windows) ou `tauri://localhost` (macOS/Linux) — donc `location.protocol` vaut souvent `http:`.
 * On détecte la présence des objets injectés par le runtime Tauri dans `window`, et en repli
 * l'hôte `tauri.localhost`.
 */
export function isPackaged(): boolean {
  if (typeof window === "undefined") return false;
  const w = window as unknown as Record<string, unknown>;
  if ("__TAURI_INTERNALS__" in w || "__TAURI__" in w || "__TAURI_METADATA__" in w) return true;
  if (typeof location !== "undefined") {
    const h = location.hostname;
    if (h === "tauri.localhost" || location.protocol === "tauri:") return true;
  }
  return false;
}

/**
 * Faut-il demander l'URL d'instance ? Vrai dans le client empaqueté tant qu'aucune instance n'est
 * configurée. En mode navigateur web, jamais (l'origine sert l'API).
 */
export function needsInstanceUrl(): boolean {
  if (instanceBase) return false;
  return isPackaged();
}

/** Base HTTP des routes REST. Mode origine : `/api`. Mode instance : `<url><prefix>`. */
export function apiBase(): string {
  return instanceBase ? `${instanceBase}${instancePrefix}` : "/api";
}

/** Base HTTP du SFU (sans suffixe ; l'appelant ajoute `/sfu/...`). */
export function httpBase(): string {
  return instanceBase ?? "";
}

/**
 * URL absolue d'un média servi par l'API (avatars, emojis, stickers, bannières, sons…).
 * `path` commence par `/api/...`. En mode navigateur, renvoie le chemin tel quel (origine) ;
 * dans le .exe, le préfixe par l'instance configurée. Indispensable pour les `<img src>` /
 * `new Audio()` codés en chemin relatif, qui sinon pointeraient vers `tauri://localhost`.
 */
export function mediaUrl(path: string): string {
  if (!instanceBase) return path; // mode origine : inchangé (le proxy retire /api)
  // `path` commence par `/api/...` ; on remplace ce préfixe par celui réellement utilisé par
  // l'instance (vide si l'API est servie nue, `/api` derrière un reverse-proxy).
  const rest = path.startsWith("/api") ? path.slice(4) : path;
  return `${instanceBase}${instancePrefix}${rest}`;
}

/** URL WebSocket de la Gateway temps réel. */
export function gatewayWsUrl(): string {
  if (instanceBase) {
    return `${instanceBase.replace(/^http/, "ws")}/gateway`;
  }
  const proto = location.protocol === "https:" ? "wss:" : "ws:";
  return `${proto}//${location.host}/gateway`;
}

/** Base WebSocket du SFU (l'appelant ajoute `/sfu/rooms/...`). */
export function sfuWsBase(): string {
  if (instanceBase) {
    return instanceBase.replace(/^http/, "ws");
  }
  const proto = location.protocol === "https:" ? "wss:" : "ws:";
  // Dev (port Vite 1420) : le proxy ws de Vite n'aboutit pas vers le SFU → connexion directe :8081.
  if (import.meta.env.DEV && location.port === "1420") {
    return `${proto}//${location.hostname}:8081`;
  }
  return `${proto}//${location.host}`;
}

// ───────────────────────────── fetch ─────────────────────────────
// En mode EMPAQUETÉ, le front (origine `tauri.localhost`) ne peut pas `fetch` une instance distante :
// le WebView applique la politique CORS et l'API ne renvoie pas d'`Access-Control-Allow-Origin`.
// On route donc les requêtes via le plugin HTTP NATIF de Tauri (requête Rust, hors navigateur, donc
// non soumise à CORS). En mode navigateur web, on garde le `fetch` standard (même origine, pas de CORS).

let tauriFetch: typeof fetch | null = null;
let tauriFetchLoading: Promise<typeof fetch> | null = null;

async function getTauriFetch(): Promise<typeof fetch> {
  if (tauriFetch) return tauriFetch;
  if (!tauriFetchLoading) {
    tauriFetchLoading = import("@tauri-apps/plugin-http").then((m) => {
      tauriFetch = m.fetch as unknown as typeof fetch;
      return tauriFetch;
    });
  }
  return tauriFetchLoading;
}

/** `fetch` adapté au contexte : natif Tauri (sans CORS) en .exe, `window.fetch` en navigateur. */
export async function appFetch(input: string, init?: RequestInit): Promise<Response> {
  if (isPackaged()) {
    const f = await getTauriFetch();
    return f(input, init);
  }
  return fetch(input, init);
}
