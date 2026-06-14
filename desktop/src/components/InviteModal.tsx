import { useEffect, useState } from "react";
import { Check, Copy } from "lucide-react";
import { api } from "../api";
import type { Invite } from "../types";
import { Modal } from "./ServerRail";

// Crée (à l'ouverture) une invitation pour la guilde et propose de copier le code.
export function InviteModal({ guildId, onClose }: { guildId: string; onClose: () => void }) {
  const [invite, setInvite] = useState<Invite | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    let alive = true;
    (async () => {
      try {
        const inv = await api.createInvite(guildId, { max_uses: 0, max_age: 0 });
        if (alive) setInvite(inv);
      } catch (e) {
        if (alive) setError(e instanceof Error ? e.message : "Échec de la création.");
      }
    })();
    return () => {
      alive = false;
    };
  }, [guildId]);

  function copy() {
    if (!invite) return;
    void navigator.clipboard
      ?.writeText(invite.code)
      .then(() => {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1500);
      })
      .catch(() => {});
  }

  return (
    <Modal onClose={onClose}>
      <div className="w-[440px] rounded-xl bg-modal p-6 ring-1 ring-cardline surface-card">
        <h2 className="mb-1 text-lg font-bold text-header">Inviter des gens</h2>
        <p className="mb-4 text-sm text-muted">
          Partage ce code d'invitation pour que d'autres rejoignent ce serveur.
        </p>

        {error ? (
          <p className="text-sm text-dnd">{error}</p>
        ) : !invite ? (
          <p className="text-sm text-muted">Génération du code…</p>
        ) : (
          <>
            <label className="mb-1.5 block text-xs font-bold uppercase tracking-wide text-subtext">
              Code d'invitation
            </label>
            <div className="flex gap-2">
              <input
                readOnly
                value={invite.code}
                onFocus={(e) => e.currentTarget.select()}
                className="flex-1 rounded bg-deepest px-3 py-2 font-mono text-normal outline-none"
              />
              <button
                onClick={copy}
                className="flex items-center gap-1.5 rounded-lg btn-accent px-4 text-sm font-medium text-white"
              >
                {copied ? <Check size={16} /> : <Copy size={16} />}
                {copied ? "Copié" : "Copier"}
              </button>
            </div>
          </>
        )}

        <div className="mt-5 flex justify-end">
          <button onClick={onClose} className="px-4 py-2 text-sm text-normal hover:underline">
            Fermer
          </button>
        </div>
      </div>
    </Modal>
  );
}
