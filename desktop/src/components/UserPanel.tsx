import { useState } from "react";
import * as Popover from "@radix-ui/react-popover";
import { Settings as SettingsIcon, X } from "lucide-react";
import { useStore } from "../store";
import { Avatar } from "./Avatar";
import { StatusDot } from "./StatusDot";
import { displayName } from "../lib/format";
import { OVERLAY_ANIM } from "../lib/anim";

const STATUS_OPTIONS: { id: string; label: string; dot: string }[] = [
  { id: "online", label: "En ligne", dot: "online" },
  { id: "idle", label: "Absent", dot: "idle" },
  { id: "dnd", label: "Ne pas déranger", dot: "dnd" },
  { id: "invisible", label: "Invisible", dot: "offline" },
];

const STATUS_LABEL: Record<string, string> = {
  online: "En ligne",
  idle: "Absent",
  dnd: "Ne pas déranger",
  offline: "Hors ligne",
};

export function UserPanel() {
  const me = useStore((s) => s.me);
  const openSettings = useStore((s) => s.setSettingsOpen);
  const status = useStore((s) => (me ? s.presences[me.id] : undefined));
  const setPresenceStatus = useStore((s) => s.setPresenceStatus);
  const myCustom = useStore((s) => (me ? s.customStatus[me.id] : null)) ?? null;
  const setCustomStatus = useStore((s) => s.setCustomStatus);
  const [draft, setDraft] = useState("");
  if (!me) return null;

  return (
    // Puce flottante (façon « widget ») plutôt qu'une bande pleine largeur.
    <div className="m-2 flex items-center gap-2 rounded-xl bg-hover px-2 py-1.5 ring-1 ring-line surface-card">
      <Popover.Root>
        <Popover.Trigger className="flex min-w-0 flex-1 items-center gap-2 rounded px-1 py-1 text-left outline-none hover:bg-white/5">
          <Avatar name={displayName(me)} id={me.id} size={32} status={status ?? "online"} ring="var(--bg-sidebar)" avatarId={me.avatar_id} />
          <div className="min-w-0 leading-tight">
            <div className="truncate text-sm font-medium text-header">{displayName(me)}</div>
            <div className="truncate text-xs text-muted">
              {myCustom || (STATUS_LABEL[status ?? "online"] ?? "En ligne")}
            </div>
          </div>
        </Popover.Trigger>
        <Popover.Portal>
          <Popover.Content
            side="top"
            align="start"
            sideOffset={8}
            className={`z-[60] w-[200px] rounded-xl bg-floating p-1.5 shadow-pop ring-1 ring-line ${OVERLAY_ANIM}`}
          >
            <div className="px-2 pb-1 text-xs font-semibold uppercase text-muted">Statut</div>
            {STATUS_OPTIONS.map((o) => (
              <Popover.Close
                key={o.id}
                onClick={() => void setPresenceStatus(o.id)}
                className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm text-normal outline-none hover:bg-hover"
              >
                <StatusDot status={o.dot} size={12} />
                {o.label}
              </Popover.Close>
            ))}

            <div className="my-1.5 h-px bg-white/10" />
            <div className="px-2 pb-1 text-xs font-semibold uppercase text-muted">
              Statut personnalisé
            </div>
            <div className="flex items-center gap-1 px-1 pb-1">
              <input
                value={draft || myCustom || ""}
                onChange={(e) => setDraft(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    void setCustomStatus(draft.trim() || null);
                    setDraft("");
                  }
                }}
                maxLength={128}
                placeholder="Quoi de neuf ?"
                className="w-full rounded-lg bg-deepest px-2 py-1.5 text-sm text-normal outline-none ring-1 ring-transparent placeholder:text-muted focus:ring-accent"
              />
              {(myCustom || draft) && (
                <button
                  title="Effacer le statut"
                  onClick={() => {
                    setDraft("");
                    void setCustomStatus(null);
                  }}
                  className="shrink-0 rounded p-1 text-muted hover:bg-hover hover:text-normal"
                >
                  <X size={14} />
                </button>
              )}
            </div>
            <p className="px-2 pb-1 text-[11px] text-muted">Entrée pour enregistrer.</p>
          </Popover.Content>
        </Popover.Portal>
      </Popover.Root>

      <button
        title="Paramètres utilisateur"
        onClick={() => openSettings(true)}
        className="rounded p-2 text-interactive-normal hover:bg-white/5 hover:text-interactive-hover"
      >
        <SettingsIcon size={18} />
      </button>
    </div>
  );
}
