// Petits utilitaires d'affichage.

import type { User } from "../types";

export function displayName(u: User): string {
  return u.display_name || u.username;
}

// Message d'erreur lisible à partir d'une exception inconnue (repli générique).
export function errText(e: unknown): string {
  return e instanceof Error ? e.message : "Échec.";
}

export function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "?";
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}

// Couleur stable dérivée d'un identifiant (avatars par initiale).
const PALETTE = ["#5865f2", "#23a55a", "#e67e22", "#eb459e", "#3498db", "#9b59b6", "#f1c40f"];
export function colorFor(id: string): string {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) >>> 0;
  return PALETTE[h % PALETTE.length];
}

export function formatHM(ms: number): string {
  return new Date(ms).toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" });
}

// Epoch des snowflakes Ozone (cf. crates/ozone-proto/src/ids.rs) → date de création d'un compte.
const OZONE_EPOCH_MS = 1_735_689_600_000;
export function snowflakeMs(id: string): number {
  try {
    return Number(BigInt(id) >> 22n) + OZONE_EPOCH_MS;
  } catch {
    return 0;
  }
}

// « il y a X » (français, approximatif) — pour « membre depuis » et « a rejoint ».
export function timeAgo(ms: number): string {
  const s = Math.max(0, Math.floor((Date.now() - ms) / 1000));
  if (s < 60) return "à l'instant";
  const m = Math.floor(s / 60);
  if (m < 60) return `il y a ${m} min`;
  const h = Math.floor(m / 60);
  if (h < 24) return `il y a ${h} h`;
  const d = Math.floor(h / 24);
  if (d < 30) return `il y a ${d} j`;
  const mo = Math.floor(d / 30);
  if (mo < 12) return `il y a ${mo} mois`;
  const y = Math.floor(d / 365);
  return `il y a ${y} an${y > 1 ? "s" : ""}`;
}

export function formatDayTime(ms: number): string {
  const d = new Date(ms);
  const today = new Date();
  const sameDay = d.toDateString() === today.toDateString();
  const time = d.toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" });
  if (sameDay) return `Aujourd'hui à ${time}`;
  return d.toLocaleDateString("fr-FR", { day: "2-digit", month: "2-digit", year: "numeric" }) +
    ` ${time}`;
}
