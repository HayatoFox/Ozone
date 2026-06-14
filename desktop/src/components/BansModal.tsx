import { useEffect, useState } from "react";
import { api } from "../api";
import type { Ban } from "../types";
import { displayName } from "../lib/format";
import { Avatar } from "./Avatar";
import { Modal } from "./ServerRail";

// Liste des bannissements + révocation. `embedded` : rendu en page (Paramètres du serveur).
export function BansModal({
  guildId,
  onClose,
  embedded,
}: {
  guildId: string;
  onClose?: () => void;
  embedded?: boolean;
}) {
  const [bans, setBans] = useState<Ban[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [working, setWorking] = useState<string | null>(null);

  async function reload() {
    try {
      setBans(await api.listBans(guildId));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Accès refusé.");
    }
  }

  useEffect(() => {
    void reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [guildId]);

  async function unban(userId: string) {
    setWorking(userId);
    try {
      await api.unbanMember(guildId, userId);
      await reload();
    } finally {
      setWorking(null);
    }
  }

  const content = (
      <div
        className={`flex flex-col overflow-hidden rounded-xl bg-modal ring-1 ring-cardline surface-card ${
          embedded ? "h-full w-full" : "h-[520px] w-[560px]"
        }`}
      >
        <div className="border-b border-line px-6 py-4">
          <h2 className="text-xl font-bold text-header">Bannissements</h2>
        </div>
        <div className="flex-1 overflow-y-auto p-3 scroll-thin">
          {error ? (
            <p className="p-3 text-sm text-dnd">{error}</p>
          ) : bans === null ? (
            <p className="p-3 text-sm text-muted">Chargement…</p>
          ) : bans.length === 0 ? (
            <p className="p-3 text-sm text-muted">Aucun membre banni.</p>
          ) : (
            bans.map((b) => (
              <div key={b.user.id} className="flex items-center gap-3 rounded p-2 hover:bg-hover">
                <Avatar name={displayName(b.user)} id={b.user.id} size={36} avatarId={b.user.avatar_id} />
                <div className="min-w-0 flex-1">
                  <div className="truncate font-medium text-header">{displayName(b.user)}</div>
                  <div className="truncate text-xs text-muted">
                    {b.reason ? `Raison : ${b.reason}` : "Aucune raison"}
                  </div>
                </div>
                <button
                  onClick={() => void unban(b.user.id)}
                  disabled={working === b.user.id}
                  className="rounded bg-sidebar px-3 py-1.5 text-sm font-medium text-normal hover:bg-deepest disabled:opacity-50"
                >
                  Révoquer
                </button>
              </div>
            ))
          )}
        </div>
        {!embedded && (
          <div className="flex justify-end border-t border-line px-6 py-3">
            <button onClick={onClose} className="px-4 py-2 text-sm text-normal hover:underline">
              Fermer
            </button>
          </div>
        )}
      </div>
  );
  return embedded ? content : <Modal onClose={onClose ?? (() => {})}>{content}</Modal>;
}
