import { useState } from "react";
import { colorFor, initials } from "../lib/format";
import { StatusDot } from "./StatusDot";

interface Props {
  name: string;
  id: string;
  size?: number;
  status?: string | null;
  /** `avatar_id` de l'utilisateur : si présent, on affiche son image (repli : initiales). */
  avatarId?: string | null;
  /** Couleur du fond derrière l'avatar (pour la découpe autour du point de statut). */
  ring?: string;
}

// Avatar utilisateur : image téléversée si disponible, sinon initiale sur fond coloré stable.
export function Avatar({ name, id, size = 40, status, avatarId, ring = "var(--bg-sidebar)" }: Props) {
  const dot = Math.max(10, Math.round(size * 0.32));
  const pad = Math.max(2, Math.round(size * 0.05));
  // Image en échec (fichier disparu) → on retombe sur les initiales sans casser la grappe.
  const [broken, setBroken] = useState(false);
  const showImg = !!avatarId && !broken;
  return (
    <div className="relative shrink-0" style={{ width: size, height: size }}>
      <div
        className="flex items-center justify-center overflow-hidden rounded-full font-medium text-white select-none"
        style={{
          width: size,
          height: size,
          backgroundColor: colorFor(id),
          fontSize: size * 0.4,
        }}
      >
        {showImg ? (
          // `?v=` : cache-buster lié à l'avatar_id → se rafraîchit en direct au changement.
          // Fondu doux à l'arrivée de l'image (évite le « pop » brut au chargement réseau).
          <img
            src={`/api/users/${id}/avatar?v=${avatarId}`}
            alt=""
            className="h-full w-full object-cover opacity-0 transition-opacity duration-300 ease-out"
            draggable={false}
            onLoad={(e) => e.currentTarget.classList.remove("opacity-0")}
            onError={() => setBroken(true)}
          />
        ) : (
          initials(name)
        )}
      </div>
      {status && (
        <span
          className="absolute -bottom-0.5 -right-0.5 flex items-center justify-center rounded-full"
          style={{ backgroundColor: ring, padding: pad }}
        >
          <StatusDot status={status} size={dot} />
        </span>
      )}
    </div>
  );
}
