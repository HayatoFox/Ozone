import { Loader2 } from "lucide-react";

// Indicateur de chargement premium (icône Lucide en rotation) — remplace les « … » bruts.
export function Spinner({ size = 16, className = "" }: { size?: number; className?: string }) {
  return <Loader2 size={size} className={`animate-spin ${className}`} />;
}
