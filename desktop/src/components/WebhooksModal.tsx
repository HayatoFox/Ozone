import { useEffect, useState } from "react";
import { Copy, Plus, RefreshCw, Trash2 } from "lucide-react";
import { api } from "../api";
import type { Webhook } from "../types";
import { Modal } from "./ServerRail";
import { InlineName } from "./ExpressionPages";

export function WebhooksModal({
  channelId,
  channelName,
  onClose,
}: {
  channelId: string;
  channelName: string;
  onClose: () => void;
}) {
  const [hooks, setHooks] = useState<Webhook[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  // Jetons révélés (présents uniquement après création/régénération) : id -> token.
  const [tokens, setTokens] = useState<Record<string, string>>({});

  async function reload() {
    try {
      setHooks(await api.listChannelWebhooks(channelId));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Accès refusé.");
    }
  }

  useEffect(() => {
    void reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [channelId]);

  async function create() {
    if (!name.trim()) return;
    setBusy(true);
    try {
      const wh = await api.createWebhook(channelId, { name: name.trim() });
      if (wh.token) setTokens((t) => ({ ...t, [wh.id]: wh.token! }));
      setName("");
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Échec.");
    } finally {
      setBusy(false);
    }
  }

  async function regen(id: string) {
    try {
      const wh = await api.regenerateWebhook(id);
      if (wh.token) setTokens((t) => ({ ...t, [id]: wh.token! }));
    } catch {
      /* ignore */
    }
  }

  const urlFor = (id: string, token: string) => `${location.origin}/api/webhooks/${id}/${token}`;
  const copy = (text: string) => void navigator.clipboard?.writeText(text).catch(() => {});

  return (
    <Modal onClose={onClose}>
      <div className="flex h-[520px] w-[600px] flex-col overflow-hidden rounded-xl bg-modal ring-1 ring-cardline surface-card">
        <div className="border-b border-line px-6 py-4">
          <h2 className="text-xl font-bold text-header">Webhooks · #{channelName}</h2>
        </div>

        <div className="flex gap-2 border-b border-line bg-deepest/40 px-6 py-4">
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Nom du webhook"
            className="flex-1 rounded-lg bg-deepest px-3 py-2 text-normal outline-none ring-1 ring-transparent focus:ring-accent"
          />
          <button
            onClick={() => void create()}
            disabled={busy || !name.trim()}
            className="flex items-center gap-1.5 rounded-lg btn-accent px-4 py-2 text-sm font-medium text-white disabled:opacity-50"
          >
            <Plus size={16} />
            Créer
          </button>
        </div>

        <div className="flex-1 overflow-y-auto p-4 scroll-thin">
          {error ? (
            <p className="text-sm text-dnd">{error}</p>
          ) : hooks === null ? (
            <p className="text-sm text-muted">Chargement…</p>
          ) : hooks.length === 0 ? (
            <p className="text-sm text-muted">Aucun webhook.</p>
          ) : (
            hooks.map((w) => {
              const token = tokens[w.id];
              return (
                <div key={w.id} className="mb-2 rounded-lg bg-sidebar p-3">
                  <div className="flex items-center justify-between gap-3">
                    <div className="min-w-0 flex-1">
                      <InlineName
                        value={w.name}
                        onRename={(n) => api.updateWebhook(w.id, { name: n }).then(reload)}
                      />
                    </div>
                    <div className="flex gap-2">
                      <button
                        onClick={() => void regen(w.id)}
                        title="Régénérer le jeton"
                        className="text-muted hover:text-normal"
                      >
                        <RefreshCw size={16} />
                      </button>
                      <button
                        onClick={() => void api.deleteWebhook(w.id).then(reload).catch(() => {})}
                        title="Supprimer"
                        className="text-muted hover:text-dnd"
                      >
                        <Trash2 size={16} />
                      </button>
                    </div>
                  </div>
                  {token ? (
                    <div className="mt-2">
                      <div className="mb-1 text-xs text-online">
                        Jeton affiché une seule fois — copie l'URL maintenant.
                      </div>
                      <div className="flex gap-2">
                        <input
                          readOnly
                          value={urlFor(w.id, token)}
                          onFocus={(e) => e.currentTarget.select()}
                          className="flex-1 rounded bg-deepest px-2 py-1.5 font-mono text-xs text-normal outline-none"
                        />
                        <button
                          onClick={() => copy(urlFor(w.id, token))}
                          className="flex items-center gap-1 rounded-lg btn-accent px-3 text-xs font-medium text-white"
                        >
                          <Copy size={14} />
                          Copier
                        </button>
                      </div>
                    </div>
                  ) : (
                    <div className="mt-1 text-xs text-muted">
                      Jeton masqué (régénère pour obtenir une nouvelle URL).
                    </div>
                  )}
                </div>
              );
            })
          )}
        </div>

        <div className="flex justify-end border-t border-line px-6 py-3">
          <button onClick={onClose} className="px-4 py-2 text-sm text-normal hover:underline">
            Fermer
          </button>
        </div>
      </div>
    </Modal>
  );
}
