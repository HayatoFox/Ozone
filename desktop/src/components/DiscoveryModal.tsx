import { useEffect, useState } from "react";
import { api } from "../api";
import { useStore } from "../store";
import type { DiscoveryGuild } from "../types";
import { initials } from "../lib/format";
import { Modal } from "./ServerRail";
import { Spinner } from "./ui/Spinner";

// Annuaire des guildes publiques (découverte) + adhésion.
export function DiscoveryModal({ onClose }: { onClose: () => void }) {
  const [guilds, setGuilds] = useState<DiscoveryGuild[] | null>(null);
  const [joining, setJoining] = useState<string | null>(null);
  const refreshGuilds = useStore((s) => s.refreshGuilds);
  const selectGuild = useStore((s) => s.selectGuild);
  const myGuilds = useStore((s) => s.guilds);

  useEffect(() => {
    let alive = true;
    (async () => {
      try {
        const list = await api.listDiscovery();
        if (alive) setGuilds(list);
      } catch {
        if (alive) setGuilds([]);
      }
    })();
    return () => {
      alive = false;
    };
  }, []);

  async function join(g: DiscoveryGuild) {
    setJoining(g.id);
    try {
      await api.joinDiscovery(g.id);
      await refreshGuilds();
      onClose();
      await selectGuild(g.id);
    } catch {
      setJoining(null);
    }
  }

  return (
    <Modal onClose={onClose}>
      <div className="flex h-[560px] w-[680px] flex-col overflow-hidden rounded-xl bg-modal ring-1 ring-cardline surface-card">
        <div className="border-b border-line px-6 py-4">
          <h2 className="text-xl font-bold text-header">Découvrir des serveurs</h2>
          <p className="text-sm text-muted">Rejoins des communautés publiques de cette instance.</p>
        </div>
        <div className="flex-1 overflow-y-auto p-4 scroll-thin">
          {guilds === null ? (
            <p className="text-sm text-muted">Chargement…</p>
          ) : guilds.length === 0 ? (
            <p className="text-sm text-muted">Aucun serveur public pour le moment.</p>
          ) : (
            <div className="grid grid-cols-2 gap-3">
              {guilds.map((g) => {
                const joined = myGuilds.some((x) => x.id === g.id);
                return (
                  <div key={g.id} className="flex flex-col rounded-lg bg-sidebar p-4">
                    <div className="mb-2 flex items-center gap-3">
                      <div className="flex h-12 w-12 items-center justify-center rounded-2xl bg-deepest font-semibold text-white">
                        {initials(g.name)}
                      </div>
                      <div className="min-w-0">
                        <div className="truncate font-semibold text-header">{g.name}</div>
                        <div className="text-xs text-muted">{g.member_count} membres</div>
                      </div>
                    </div>
                    <p className="mb-3 line-clamp-2 flex-1 text-sm text-muted">
                      {g.description || "Pas de description."}
                    </p>
                    <button
                      disabled={joined || joining === g.id}
                      onClick={() => void join(g)}
                      className="pressable inline-flex items-center justify-center gap-2 rounded bg-online py-1.5 text-sm font-medium text-white hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
                    >
                      {joined ? "Déjà membre" : <>{joining === g.id && <Spinner size={14} />}Rejoindre</>}
                    </button>
                  </div>
                );
              })}
            </div>
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
