// Panneau d'administration d'instance (self-hoster) : config, invitations d'instance,
// comptes (suspension, rôle d'instance). Visible uniquement si GET /instance/admin/config
// répond (feature-détection côté Settings).

import { useEffect, useState } from "react";
import { Copy, Plus, ShieldCheck, Trash2 } from "lucide-react";
import { api } from "../api";
import { useStore } from "../store";
import { displayName } from "../lib/format";
import { Avatar } from "./Avatar";
import type { InstanceAdminConfig, InstanceInvite, InstanceUserView } from "../types";

function SectionTitle({ children }: { children: React.ReactNode }) {
  return <h2 className="mb-5 text-xl font-bold text-header">{children}</h2>;
}
function SubTitle({ children }: { children: React.ReactNode }) {
  return (
    <h3 className="mb-2 mt-6 text-xs font-bold uppercase tracking-wide text-subtext first:mt-0">
      {children}
    </h3>
  );
}

const POLICY_LABEL: Record<string, string> = {
  open: "Ouverte (tout le monde peut s'inscrire)",
  invite: "Sur invitation",
  closed: "Fermée",
};

export function InstanceAdminSection() {
  const me = useStore((s) => s.me);
  const [config, setConfig] = useState<InstanceAdminConfig | null>(null);
  const [invites, setInvites] = useState<InstanceInvite[]>([]);
  const [users, setUsers] = useState<InstanceUserView[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [maxUses, setMaxUses] = useState(0);
  const [maxAgeH, setMaxAgeH] = useState(0);
  const [busy, setBusy] = useState(false);

  async function reload() {
    try {
      const [cfg, inv, usr] = await Promise.all([
        api.adminConfig(),
        api.adminListInvites().catch(() => [] as InstanceInvite[]),
        api.adminListUsers().catch(() => [] as InstanceUserView[]),
      ]);
      setConfig(cfg);
      setInvites(inv);
      setUsers(usr);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Accès refusé.");
    }
  }

  useEffect(() => {
    void reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function createInvite() {
    setBusy(true);
    try {
      await api.adminCreateInvite({ max_uses: maxUses, max_age: maxAgeH * 3600 });
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Échec.");
    } finally {
      setBusy(false);
    }
  }

  async function setSuspended(u: InstanceUserView, suspended: boolean) {
    try {
      await api.adminSetSuspended(u.user.id, suspended);
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Échec.");
    }
  }

  async function setRole(u: InstanceUserView, role: string) {
    try {
      await api.adminSetRole(u.user.id, role);
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Échec (réservé au propriétaire de l'instance).");
    }
  }

  const copy = (text: string) => void navigator.clipboard?.writeText(text).catch(() => {});

  if (!config) {
    return (
      <>
        <SectionTitle>Administration de l'instance</SectionTitle>
        <p className="text-sm text-muted">{error ?? "Chargement…"}</p>
      </>
    );
  }

  return (
    <>
      <SectionTitle>Administration de l'instance</SectionTitle>
      {error && <p className="mb-3 text-sm text-dnd">{error}</p>}

      <SubTitle>Configuration</SubTitle>
      <div className="rounded-lg bg-sidebar p-4 text-sm">
        <div className="flex items-center gap-2 font-medium text-header">
          <ShieldCheck size={16} className="text-online" />
          {config.name}
          <span className="ml-auto text-xs font-normal text-muted">v{config.version}</span>
        </div>
        {config.description && <p className="mt-1 text-muted">{config.description}</p>}
        <div className="mt-2 grid grid-cols-2 gap-x-4 gap-y-1 text-muted">
          <span>
            Inscription :{" "}
            <span className="text-normal">
              {POLICY_LABEL[config.registration_policy] ?? config.registration_policy}
            </span>
          </span>
          <span>
            Mot de passe d'accès :{" "}
            <span className="text-normal">{config.gate_enabled ? "activé" : "désactivé"}</span>
          </span>
        </div>
      </div>

      <SubTitle>Invitations d'instance</SubTitle>
      <p className="mb-2 text-sm text-muted">
        Codes nécessaires à l'inscription quand la politique est « sur invitation ».
      </p>
      <div className="mb-2 flex items-end gap-2 rounded-lg bg-sidebar p-3">
        <div>
          <label className="mb-1 block text-xs text-muted">Utilisations max (0 = ∞)</label>
          <input
            type="number"
            min={0}
            value={maxUses}
            onChange={(e) => setMaxUses(Math.max(0, Number(e.target.value) || 0))}
            className="w-32 rounded-lg bg-deepest px-3 py-1.5 text-sm text-normal outline-none ring-1 ring-transparent focus:ring-accent"
          />
        </div>
        <div>
          <label className="mb-1 block text-xs text-muted">Validité en heures (0 = ∞)</label>
          <input
            type="number"
            min={0}
            value={maxAgeH}
            onChange={(e) => setMaxAgeH(Math.max(0, Number(e.target.value) || 0))}
            className="w-32 rounded-lg bg-deepest px-3 py-1.5 text-sm text-normal outline-none ring-1 ring-transparent focus:ring-accent"
          />
        </div>
        <button
          onClick={() => void createInvite()}
          disabled={busy}
          className="flex items-center gap-1.5 rounded-lg btn-accent px-3 py-1.5 text-sm font-medium text-white disabled:opacity-50"
        >
          <Plus size={15} />
          Générer
        </button>
      </div>
      {invites.length === 0 ? (
        <p className="text-sm text-muted">Aucune invitation d'instance.</p>
      ) : (
        <div className="flex flex-col gap-1.5">
          {invites.map((inv) => (
            <div key={inv.code} className="flex items-center gap-3 rounded-lg bg-sidebar px-3 py-2">
              <code className="font-mono text-sm text-header">{inv.code}</code>
              <span className="text-xs text-muted">
                {inv.uses}/{inv.max_uses === 0 ? "∞" : inv.max_uses} utilisations
                {inv.expires_at
                  ? ` · expire le ${new Date(inv.expires_at).toLocaleDateString("fr-FR")}`
                  : " · sans expiration"}
              </span>
              <div className="ml-auto flex items-center gap-2">
                <button onClick={() => copy(inv.code)} title="Copier le code" className="text-muted hover:text-normal">
                  <Copy size={15} />
                </button>
                <button
                  onClick={() => void api.adminRevokeInvite(inv.code).then(reload).catch(() => {})}
                  title="Révoquer"
                  className="text-muted hover:text-dnd"
                >
                  <Trash2 size={15} />
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      <SubTitle>Comptes ({users.length})</SubTitle>
      <div className="flex flex-col gap-1.5">
        {users.map((u) => {
          const isMe = u.user.id === me?.id;
          const isOwner = u.role === "owner";
          return (
            <div key={u.user.id} className="flex items-center gap-3 rounded-lg bg-sidebar px-3 py-2">
              <Avatar name={displayName(u.user)} id={u.user.id} size={28} avatarId={u.user.avatar_id} />
              <div className="min-w-0">
                <div className={`truncate text-sm ${u.suspended ? "text-muted line-through" : "text-header"}`}>
                  {displayName(u.user)}
                </div>
                <div className="text-xs text-muted">@{u.user.username}</div>
              </div>
              <div className="ml-auto flex items-center gap-2">
                {isOwner ? (
                  <span className="rounded-full bg-active px-2 py-0.5 text-xs text-online ring-1 ring-line">
                    Propriétaire
                  </span>
                ) : (
                  <select
                    value={u.role}
                    onChange={(e) => void setRole(u, e.target.value)}
                    className="rounded bg-deepest px-2 py-1 text-xs text-normal outline-none"
                  >
                    <option value="user">Utilisateur</option>
                    <option value="moderator">Modérateur</option>
                    <option value="admin">Admin</option>
                  </select>
                )}
                {!isOwner && !isMe && (
                  <button
                    onClick={() => void setSuspended(u, !u.suspended)}
                    className={`rounded-full px-2.5 py-1 text-xs ring-1 ring-line transition-colors ${
                      u.suspended
                        ? "bg-online/20 text-online hover:bg-online hover:text-white"
                        : "bg-active text-normal hover:bg-dnd/20 hover:text-dnd"
                    }`}
                  >
                    {u.suspended ? "Réactiver" : "Suspendre"}
                  </button>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </>
  );
}
