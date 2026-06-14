import { useEffect, useState } from "react";
import { api } from "../api";
import { useStore } from "../store";
import type { AuditLogEntry } from "../types";
import { displayName, formatDayTime } from "../lib/format";
import { Modal } from "./ServerRail";

// Libellés FR de quelques types d'action courants (fallback : valeur brute).
const ACTIONS: Record<string, string> = {
  member_kick: "a expulsé un membre",
  member_ban_add: "a banni un membre",
  member_ban_remove: "a débanni un membre",
  member_update: "a modifié un membre",
  member_role_update: "a modifié les rôles d'un membre",
  role_create: "a créé un rôle",
  role_update: "a modifié un rôle",
  role_delete: "a supprimé un rôle",
  channel_create: "a créé un salon",
  channel_update: "a modifié un salon",
  channel_delete: "a supprimé un salon",
  guild_update: "a modifié le serveur",
  guild_owner_transfer: "a transféré la propriété",
  member_timeout: "a exclu temporairement un membre",
  invite_create: "a créé une invitation",
  invite_delete: "a révoqué une invitation",
  webhook_create: "a créé un webhook",
  webhook_delete: "a supprimé un webhook",
  message_delete: "a supprimé un message",
  message_pin: "a épinglé un message",
  message_unpin: "a désépinglé un message",
};

export function AuditLogModal({
  guildId,
  onClose,
  embedded,
}: {
  guildId: string;
  onClose?: () => void;
  embedded?: boolean;
}) {
  const [entries, setEntries] = useState<AuditLogEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const members = useStore((s) => s.membersByGuild[guildId]);

  useEffect(() => {
    let alive = true;
    (async () => {
      try {
        const e = await api.listAuditLogs(guildId);
        if (alive) setEntries(e);
      } catch (err) {
        if (alive) setError(err instanceof Error ? err.message : "Accès refusé.");
      }
    })();
    return () => {
      alive = false;
    };
  }, [guildId]);

  const nameOf = (id: string) =>
    members?.find((m) => m.user.id === id)?.user && displayName(members.find((m) => m.user.id === id)!.user);

  const content = (
      <div
        className={`flex flex-col overflow-hidden rounded-xl bg-modal ring-1 ring-cardline surface-card ${
          embedded ? "h-full w-full" : "h-[520px] w-[560px]"
        }`}
      >
        <div className="border-b border-line px-6 py-4">
          <h2 className="text-xl font-bold text-header">Journal d'audit</h2>
        </div>
        <div className="flex-1 overflow-y-auto p-3 scroll-thin">
          {error ? (
            <p className="p-3 text-sm text-dnd">{error}</p>
          ) : entries === null ? (
            <p className="p-3 text-sm text-muted">Chargement…</p>
          ) : entries.length === 0 ? (
            <p className="p-3 text-sm text-muted">Aucune entrée.</p>
          ) : (
            entries.map((e) => (
              <div key={e.id} className="rounded p-2 hover:bg-hover">
                <div className="text-sm text-normal">
                  <span className="font-medium text-header">{nameOf(e.user_id) || `#${e.user_id}`}</span>{" "}
                  {ACTIONS[e.action_type] ?? e.action_type}
                  {/* Nom de l'entité concernée (salon, rôle, webhook…) si fourni. */}
                  {e.changes?.name && (
                    <span className="text-muted"> : {e.changes.name}</span>
                  )}
                  {e.target_id && (
                    <span className="text-muted"> ({nameOf(e.target_id) || `#${e.target_id}`})</span>
                  )}
                </div>
                {e.reason && <div className="text-xs text-muted">Raison : {e.reason}</div>}
                <div className="text-[11px] text-muted">{formatDayTime(e.created_at)}</div>
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
