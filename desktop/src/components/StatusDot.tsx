import { useId } from "react";

// Point de présence aux formes exactes de Discord (via masques SVG) :
//   online = disque plein · idle = croissant · dnd = disque barré · offline = anneau.
// La découpe autour du point (anneau de fond) est gérée par le conteneur dans Avatar.

type Status = "online" | "idle" | "dnd" | "offline";

const COLOR: Record<Status, string> = {
  online: "var(--status-online)",
  idle: "var(--status-idle)",
  dnd: "var(--status-dnd)",
  offline: "var(--status-offline)",
};

export function StatusDot({
  status,
  size = 12,
}: {
  status: string;
  size?: number;
}) {
  const s = (["online", "idle", "dnd", "offline"].includes(status) ? status : "offline") as Status;
  const id = useId().replace(/:/g, "");
  const maskId = `st-${id}`;

  return (
    <svg width={size} height={size} viewBox="0 0 1 1" aria-hidden>
      <defs>
        <mask id={maskId}>
          <rect width="1" height="1" fill="white" />
          {s === "idle" && <circle cx="0.27" cy="0.27" r="0.40" fill="black" />}
          {s === "dnd" && (
            <rect x="0.18" y="0.40" width="0.64" height="0.20" rx="0.10" fill="black" />
          )}
          {s === "offline" && <circle cx="0.5" cy="0.5" r="0.22" fill="black" />}
        </mask>
      </defs>
      {/* La couleur transitionne en douceur d'un statut à l'autre (online↔idle↔dnd). */}
      <circle
        cx="0.5"
        cy="0.5"
        r="0.5"
        fill={COLOR[s]}
        mask={`url(#${maskId})`}
        style={{ transition: "fill 200ms var(--ease-out)" }}
      />
    </svg>
  );
}
