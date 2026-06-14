// Placeholders animés (shimmer) pour les états de chargement — remplacent les « Chargement… ».

import type { CSSProperties } from "react";

export function Skeleton({ className, style }: { className?: string; style?: CSSProperties }) {
  return (
    <div
      className={`animate-shimmer rounded bg-white/5 ${className ?? ""}`}
      style={{
        backgroundImage:
          "linear-gradient(90deg, rgba(255,255,255,0.03) 25%, rgba(255,255,255,0.08) 37%, rgba(255,255,255,0.03) 63%)",
        backgroundSize: "400px 100%",
        ...style,
      }}
      aria-hidden
    />
  );
}

// Squelette d'une zone de chat (quelques lignes de messages).
export function ChatSkeleton() {
  const widths = ["60%", "42%", "75%", "35%", "55%", "48%", "68%"];
  return (
    <div className="flex-1 animate-overlay-in overflow-hidden px-4 py-4">
      {widths.map((w, i) => (
        <div key={i} className="mt-4 flex gap-4">
          <Skeleton className="h-10 w-10 shrink-0 rounded-full" />
          <div className="flex-1">
            <div className="mb-2 flex items-center gap-2">
              <Skeleton className="h-3.5 w-28" />
              <Skeleton className="h-2.5 w-16 opacity-60" />
            </div>
            <Skeleton className="h-3 rounded" style={undefined} />
            <div style={{ width: w }}>
              <Skeleton className="mt-1.5 h-3" />
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}

// Squelette d'une liste (membres, rôles, etc.).
export function ListSkeleton({ rows = 6 }: { rows?: number }) {
  return (
    <div className="space-y-2 p-2">
      {Array.from({ length: rows }).map((_, i) => (
        <div key={i} className="flex items-center gap-2 px-2">
          <Skeleton className="h-8 w-8 shrink-0 rounded-full" />
          <Skeleton className="h-3.5" style={{ width: `${40 + ((i * 13) % 45)}%` }} />
        </div>
      ))}
    </div>
  );
}
