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
 * Faut-il demander l'URL d'instance ? Vrai dans un build empaqueté (origine non-HTTP, p. ex.
 * `tauri:`) tant qu'aucune instance n'est configurée. En mode navigateur, on ne demande jamais
 * (l'origine sert l'API).
 */
export function needsInstanceUrl(): boolean {
  if (instanceBase) return false;
  if (typeof location === "undefined") return false;
  return location.protocol !== "http:" && location.protocol !== "https:";
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
