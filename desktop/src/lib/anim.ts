// Classes & helpers d'animation partagés.

// Surfaces flottantes Radix (tooltips, popovers, menus) : entrée ET sortie animées.
// S'appuie sur tailwindcss-animate + les états data-[state]/data-[side] exposés par Radix.
export const OVERLAY_ANIM =
  "origin-[var(--radix-popper-transform-origin)] " +
  "data-[state=open]:animate-in data-[state=closed]:animate-out " +
  "data-[state=open]:fade-in-0 data-[state=closed]:fade-out-0 " +
  "data-[state=open]:zoom-in-95 data-[state=closed]:zoom-out-95 " +
  "data-[state=open]:duration-150 data-[state=closed]:duration-100 " +
  "data-[side=top]:slide-in-from-bottom-1 data-[side=bottom]:slide-in-from-top-1 " +
  "data-[side=left]:slide-in-from-right-1 data-[side=right]:slide-in-from-left-1";

// Délai de stagger pour une entrée de liste : délai croissant par index, plafonné pour ne
// jamais faire attendre (au-delà de `max`, tout entre ensemble). À combiner avec `animate-row-in`.
export function staggerDelay(index: number, step = 28, max = 12): { animationDelay: string } {
  const i = Math.min(index, max);
  return { animationDelay: `${i * step}ms` };
}
