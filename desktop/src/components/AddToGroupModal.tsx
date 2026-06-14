import { useEffect, useState } from "react";
import { Check, Search, X } from "lucide-react";
import { api } from "../api";
import { useStore } from "../store";
import { displayName } from "../lib/format";
import { CH_GROUP, type DMChannel } from "../types";
import { Avatar } from "./Avatar";
import { Spinner } from "./ui/Spinner";

const MAX_GROUP = 10;

// Modal « Ajouter au MP » : sélection d'amis → crée un groupe privé (ou y ajoute des membres).
export function AddToGroupModal({ dm, onClose }: { dm: DMChannel; onClose: () => void }) {
  const me = useStore((s) => s.me);
  const relationships = useStore((s) => s.relationships);
  const refreshDMs = useStore((s) => s.refreshDMs);
  const openDM = useStore((s) => s.openDM);
  const [q, setQ] = useState("");
  const [sel, setSel] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopImmediatePropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  // Destinataires actuels (hors moi) : déjà dans la conversation.
  const currentOthers = dm.recipients.filter((u) => u.id !== me?.id).map((u) => u.id);
  const inDm = new Set([...(me ? [me.id] : []), ...currentOthers]);
  const remaining = Math.max(0, MAX_GROUP - inDm.size);

  const ql = q.toLowerCase().trim();
  const friends = relationships
    .filter((r) => r.type === "friend" && !inDm.has(r.user.id))
    .filter(
      (r) =>
        displayName(r.user).toLowerCase().includes(ql) ||
        r.user.username.toLowerCase().includes(ql),
    );

  function toggle(id: string) {
    setSel((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else if (n.size < remaining) n.add(id);
      return n;
    });
  }

  const isGroup = dm.type === CH_GROUP;

  async function create() {
    if (sel.size === 0) return;
    setBusy(true);
    try {
      if (isGroup) {
        // Groupe existant : on AJOUTE les membres (pas de nouveau canal).
        for (const id of sel) await api.addRecipient(dm.id, id);
        await refreshDMs();
      } else {
        // Depuis un MP 1:1 : crée un groupe avec les destinataires actuels + la sélection.
        const recipients = [...currentOthers, ...sel];
        const created = await api.openDM({ recipients });
        await refreshDMs();
        await openDM(created.id);
      }
      onClose();
    } catch {
      setBusy(false);
    }
  }

  return (
    <div
      className="fixed inset-0 z-[80] flex items-center justify-center bg-black/60 p-6 animate-overlay-in"
      onClick={onClose}
    >
      <div
        className="flex max-h-[70vh] w-[460px] animate-pop-in flex-col overflow-hidden rounded-2xl border border-line bg-modal shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start justify-between px-5 pt-5">
          <div>
            <h3 className="text-lg font-bold text-header">Ajouter au MP</h3>
            <p className="mt-0.5 text-sm text-muted">
              {remaining > 0
                ? `Tu peux encore ajouter ${remaining} ami${remaining > 1 ? "s" : ""}.`
                : "Groupe complet."}
            </p>
          </div>
          <button onClick={onClose} className="text-muted hover:text-header" title="Fermer">
            <X size={20} />
          </button>
        </div>

        <div className="px-5 pt-3">
          <div className="flex items-center gap-2 rounded-lg bg-deepest px-3 py-2 ring-1 ring-white/[0.04] focus-within:ring-2 focus-within:ring-accent/50">
            <Search size={15} className="text-muted" />
            <input
              autoFocus
              value={q}
              onChange={(e) => setQ(e.target.value)}
              placeholder="Rechercher des amis"
              className="w-full bg-transparent text-sm text-normal outline-none placeholder:text-muted"
            />
          </div>
        </div>

        <div className="mt-3 min-h-0 flex-1 overflow-y-auto px-3 scroll-thin">
          {friends.length === 0 ? (
            <p className="px-2 py-6 text-center text-sm text-muted">
              {relationships.some((r) => r.type === "friend")
                ? "Aucun ami à ajouter."
                : "Ajoute des amis pour créer un groupe."}
            </p>
          ) : (
            friends.map((r) => {
              const checked = sel.has(r.user.id);
              const disabled = !checked && sel.size >= remaining;
              return (
                <button
                  key={r.user.id}
                  onClick={() => toggle(r.user.id)}
                  disabled={disabled}
                  className="flex w-full items-center gap-3 rounded-lg px-2 py-2 text-left hover:bg-hover disabled:opacity-40"
                >
                  <Avatar name={displayName(r.user)} id={r.user.id} size={32} avatarId={r.user.avatar_id} />
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium text-header">
                      {displayName(r.user)}
                    </div>
                    <div className="truncate text-xs text-muted">{r.user.username}</div>
                  </div>
                  <span
                    className={`flex h-5 w-5 shrink-0 items-center justify-center rounded-md border ${
                      checked ? "border-accent bg-accent text-white" : "border-muted/50"
                    }`}
                  >
                    {checked && <Check size={13} strokeWidth={3} />}
                  </span>
                </button>
              );
            })
          )}
        </div>

        <div className="border-t border-line p-4">
          <button
            onClick={() => void create()}
            disabled={busy || sel.size === 0}
            className="pressable inline-flex w-full items-center justify-center gap-2 rounded-lg btn-accent py-2.5 text-sm font-semibold text-white disabled:cursor-not-allowed disabled:opacity-50"
          >
            {busy && <Spinner size={14} />}{isGroup ? "Ajouter au groupe" : "Créer un groupe privé"}
          </button>
        </div>
      </div>
    </div>
  );
}
