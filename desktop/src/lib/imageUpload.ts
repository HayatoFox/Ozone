// Aides partagées pour les uploads d'images d'expressions (emojis, autocollants).
//
// Deux contraintes guident ce module :
//  1. Les GIF animés doivent rester animés. Le recadrage via <canvas> aplatit l'animation
//     (drawImage ne capture qu'une frame), donc on NE recadre PAS les GIF — on les envoie tels
//     quels après vérification de taille. Les images fixes (PNG/JPEG/WebP), elles, passent par
//     l'assistant de recadrage pour produire un asset carré bien cadré.
//  2. Limites de taille distinctes : emojis 512 Kio, autocollants 2 Mio (cf. backend
//     `routes_emojis.rs` MAX_EMOJI_SIZE / MAX_STICKER_SIZE).

export const EMOJI_MAX_BYTES = 512 * 1024;
export const STICKER_MAX_BYTES = 2 * 1024 * 1024;

// Taille de sortie du recadrage (carré, comme Discord). Les emojis sont affichés petits, les
// autocollants plus grands ; on garde une marge confortable sous les limites d'octets.
export const EMOJI_CROP_PX = 128;
export const STICKER_CROP_PX = 320;

/** Formate un nombre d'octets en Kio/Mio (français, 1 décimale en Mio). Pour afficher les limites. */
export function formatBytes(n: number): string {
  if (n < 1024) return `${n} o`;
  if (n < 1024 * 1024) return `${Math.round(n / 1024)} Kio`;
  return `${(n / (1024 * 1024)).toFixed(1)} Mio`;
}

/**
 * Vrai si le fichier est un GIF — détecté par ses OCTETS MAGIQUES (« GIF8 ») plutôt que par le
 * type MIME, qui peut être absent ou usurpé selon l'OS / le mode de sélection. Cela aligne la
 * décision recadrage/animation côté client sur la détection par octets magiques du backend, et
 * garantit qu'un vrai GIF reste animé (jamais aplati par le canvas). Repli sur le MIME si la
 * lecture échoue.
 */
export async function isGif(file: File): Promise<boolean> {
  try {
    const head = new Uint8Array(await file.slice(0, 4).arrayBuffer());
    // "GIF8" = 0x47 0x49 0x46 0x38 (GIF87a / GIF89a).
    return head[0] === 0x47 && head[1] === 0x49 && head[2] === 0x46 && head[3] === 0x38;
  } catch {
    return file.type === "image/gif";
  }
}

/**
 * Vérifie la taille d'un fichier contre une limite. Renvoie un message d'erreur localisé si le
 * fichier est trop lourd, sinon `null`. Permet un rejet immédiat côté client (avant l'aller-retour
 * réseau) avec un message clair.
 */
export function sizeError(file: File, maxBytes: number): string | null {
  if (file.size > maxBytes) {
    // Taille du fichier arrondie vers le HAUT en Kio : reste toujours strictement au-dessus de la
    // limite affichée (jamais « 2.0 Mio > max 2.0 Mio » qui semblerait contradictoire).
    const overKio = Math.ceil(file.size / 1024);
    return `Image trop volumineuse (${overKio} Kio). Maximum : ${formatBytes(maxBytes)}.`;
  }
  return null;
}
