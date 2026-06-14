import { useEffect, useMemo, useRef, useState } from "react";
import * as Popover from "@radix-ui/react-popover";
import {
  ArrowLeft,
  Check,
  GripVertical,
  MoreVertical,
  Pencil,
  Plus,
  Search,
  Trash2,
  UserPlus,
  Users,
  X,
} from "lucide-react";
import { api } from "../api";
import { permsIn, roleColorHex, roleDotStyle, roleNameStyle, useStore } from "../store";
import {
  PERM,
  PERMISSIONS,
  PERM_CATEGORIES,
  hasPerm,
  togglePerm,
  type PermCat,
} from "../lib/permissions";
import { OVERLAY_ANIM } from "../lib/anim";
import { displayName } from "../lib/format";
import type { Member, Role, Snowflake } from "../types";
import { Avatar } from "./Avatar";
import { Spinner } from "./ui/Spinner";
import { ListSkeleton } from "./ui/Skeleton";

// Palette de couleurs de rôle (style Discord) ; 0 = couleur par défaut.
const ROLE_COLORS = [
  0x1abc9c, 0x2ecc71, 0x3498db, 0x9b59b6, 0xe91e63, 0xf1c40f, 0xe67e22, 0xe74c3c, 0x95a5a6,
  0x607d8b, 0x11806a, 0x1f8b4c, 0x206694, 0x71368a, 0xad1457, 0xc27c0e, 0xa84300, 0x992d22,
  0x979c9f, 0x546e7a,
];

// ───────────────────────────── Page Rôles (deux niveaux) ─────────────────────────────

export function RolesPage({ guildId }: { guildId: Snowflake }) {
  const guild = useStore((s) => s.guilds.find((g) => g.id === guildId));
  const me = useStore((s) => s.me);
  const roles = useStore((s) => s.rolesByGuild[guildId]);
  const myPerms = useStore((s) => permsIn(s, guildId));
  const refreshRoles = useStore((s) => s.refreshRoles);
  const refreshMembers = useStore((s) => s.refreshMembers);

  const [members, setMembers] = useState<Member[] | null>(null);
  const [editingId, setEditingId] = useState<Snowflake | null>(null);
  const [busy, setBusy] = useState(false);
  const [optimistic, setOptimistic] = useState<Role[] | null>(null);

  useEffect(() => {
    void refreshRoles(guildId);
    let alive = true;
    api
      .listMembers(guildId)
      .then((m) => alive && setMembers(m))
      .catch(() => alive && setMembers([]));
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [guildId]);

  const isOwner = !!guild && guild.owner_id === me?.id;
  const sorted = useMemo(
    () => [...(roles ?? [])].sort((a, b) => b.position - a.position),
    [roles],
  );
  // Ordre affiché : ordre optimiste pendant un glisser-déposer, sinon l'ordre du store.
  const display = optimistic ?? sorted;

  // Réordonne (ids = rôles hors @everyone, du plus haut au plus bas) : optimiste + persiste.
  async function reorder(orderedIds: Snowflake[]) {
    const byId = new Map(display.map((r) => [r.id, r] as const));
    const next: Role[] = orderedIds.map((id) => byId.get(id)).filter((r): r is Role => !!r);
    const everyone = display.find((r) => r.id === guildId);
    if (everyone) next.push(everyone);
    setOptimistic(next);
    try {
      await api.reorderRoles(guildId, orderedIds);
    } catch {
      /* le refresh ci-dessous restaure l'ordre réel */
    }
    await refreshRoles(guildId);
    setOptimistic(null);
  }
  // Position du rôle le plus élevé de l'utilisateur (hiérarchie). Propriétaire = ∞.
  const myTopPos = useMemo(() => {
    if (isOwner) return Number.POSITIVE_INFINITY;
    const mine = members?.find((m) => m.user.id === me?.id);
    if (!mine) return -1;
    return sorted
      .filter((r) => r.id !== guildId && mine.roles.includes(r.id))
      .reduce((max, r) => Math.max(max, r.position), -1);
  }, [isOwner, members, me?.id, sorted, guildId]);

  function canEditRole(r: Role): boolean {
    const isEveryone = r.id === guildId;
    // @everyone est marqué `managed` côté serveur mais reste éditable (permissions par défaut).
    // Les autres rôles `managed` (bots/intégrations) ne le sont pas.
    if (r.managed && !isEveryone) return false;
    if (isOwner) return true;
    if ((myPerms & PERM.MANAGE_ROLES) !== PERM.MANAGE_ROLES) return false;
    if (isEveryone) return true; // le backend valide les permissions réellement accordées
    return r.position < myTopPos;
  }

  function countForRole(roleId: Snowflake): number {
    const list = members ?? [];
    if (roleId === guildId) return list.length;
    return list.filter((m) => m.roles.includes(roleId)).length;
  }

  async function createRole() {
    setBusy(true);
    try {
      const r = await api.createRole(guildId, { name: "nouveau rôle" });
      await refreshRoles(guildId);
      setEditingId(r.id);
    } finally {
      setBusy(false);
    }
  }

  async function deleteRole(id: Snowflake) {
    setBusy(true);
    try {
      await api.deleteRole(guildId, id);
      if (editingId === id) setEditingId(null);
      await refreshRoles(guildId);
    } finally {
      setBusy(false);
    }
  }

  // Met à jour localement la liste des membres (rôle ajouté/retiré) pour un affichage optimiste.
  function patchMemberRole(userId: Snowflake, roleId: Snowflake, add: boolean) {
    setMembers((p) =>
      p?.map((m) =>
        m.user.id === userId
          ? {
              ...m,
              roles: add
                ? [...new Set([...m.roles, roleId])]
                : m.roles.filter((r) => r !== roleId),
            }
          : m,
      ) ?? p,
    );
  }

  const editing = editingId ? display.find((r) => r.id === editingId) ?? null : null;

  if (editing) {
    return (
      <RoleEditor
        guildId={guildId}
        role={editing}
        roles={display}
        members={members}
        myPerms={myPerms}
        canEdit={canEditRole(editing)}
        canEditRole={canEditRole}
        countForRole={countForRole}
        onSelect={(id) => setEditingId(id)}
        onBack={() => setEditingId(null)}
        onCreate={() => void createRole()}
        onDelete={(id) => void deleteRole(id)}
        onSaved={() => refreshRoles(guildId)}
        onReorder={(ids) => void reorder(ids)}
        onMemberRolePatched={patchMemberRole}
        onRefreshMembers={() => refreshMembers(guildId)}
      />
    );
  }

  return (
    <RolesList
      guildId={guildId}
      roles={display}
      membersLoaded={members !== null}
      busy={busy}
      countForRole={countForRole}
      canEditRole={canEditRole}
      onOpen={(id) => setEditingId(id)}
      onCreate={() => void createRole()}
      onDelete={(id) => void deleteRole(id)}
      onReorder={(ids) => void reorder(ids)}
    />
  );
}

// Renvoie `ids` avec `fromId` déplacé à la position de `toId`.
function reordered(ids: Snowflake[], fromId: Snowflake, toId: Snowflake): Snowflake[] {
  if (fromId === toId) return ids;
  const a = ids.indexOf(fromId);
  const b = ids.indexOf(toId);
  if (a < 0 || b < 0) return ids;
  const next = ids.slice();
  const [m] = next.splice(a, 1);
  next.splice(b, 0, m);
  return next;
}

// ───────────────────────────── Vue liste ─────────────────────────────

function RolesList({
  guildId,
  roles,
  membersLoaded,
  busy,
  countForRole,
  canEditRole,
  onOpen,
  onCreate,
  onDelete,
  onReorder,
}: {
  guildId: Snowflake;
  roles: Role[];
  membersLoaded: boolean;
  busy: boolean;
  countForRole: (id: Snowflake) => number;
  canEditRole: (r: Role) => boolean;
  onOpen: (id: Snowflake) => void;
  onCreate: () => void;
  onDelete: (id: Snowflake) => void;
  onReorder: (orderedIds: Snowflake[]) => void;
}) {
  const [q, setQ] = useState("");
  const dragId = useRef<Snowflake | null>(null);
  const [overId, setOverId] = useState<Snowflake | null>(null);
  const everyone = roles.find((r) => r.id === guildId);
  const searching = q.trim() !== "";
  const list = roles
    .filter((r) => r.id !== guildId)
    .filter((r) => r.name.toLowerCase().includes(q.toLowerCase().trim()));
  // Réordonnancement désactivé pendant une recherche (la liste affichée est partielle).
  const dndOn = !searching;

  function drop(targetId: Snowflake) {
    const from = dragId.current;
    dragId.current = null;
    setOverId(null);
    if (!from) return;
    const ids = roles.filter((r) => r.id !== guildId).map((r) => r.id);
    const next = reordered(ids, from, targetId);
    if (next.join() !== ids.join()) onReorder(next);
  }

  return (
    <div className="h-full overflow-y-auto px-10 py-12 scroll-thin">
      <div className="mx-auto max-w-[760px] pr-10">
        <h2 className="text-xl font-bold text-header">Rôles</h2>
        <p className="mt-1 text-sm text-muted">
          Utilise les rôles pour regrouper tes membres et leur attribuer des permissions.
        </p>

        {/* Permissions par défaut (@everyone) */}
        <button
          onClick={() => everyone && onOpen(everyone.id)}
          className="mt-6 flex w-full items-center gap-3 rounded-xl border border-line bg-deepest/40 px-4 py-3 text-left transition-colors hover:bg-hover"
        >
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-white/5 text-muted">
            <Users size={18} />
          </span>
          <span className="min-w-0 flex-1">
            <span className="block text-sm font-semibold text-header">Permissions par défaut</span>
            <span className="block truncate text-xs text-muted">
              @everyone · s'applique à tous les membres du serveur
            </span>
          </span>
          <Pencil size={16} className="shrink-0 text-muted" />
        </button>

        {/* Barre d'actions */}
        <div className="mt-6 flex items-center gap-3">
          <div className="flex flex-1 items-center gap-2 rounded-lg bg-deepest px-3 py-2">
            <Search size={15} className="text-muted" />
            <input
              value={q}
              onChange={(e) => setQ(e.target.value)}
              placeholder="Rechercher des rôles"
              className="w-full bg-transparent text-sm text-normal outline-none placeholder:text-muted"
            />
          </div>
          <button
            onClick={onCreate}
            disabled={busy}
            className="shrink-0 rounded-lg btn-accent px-4 py-2 text-sm font-semibold text-white transition disabled:opacity-50"
          >
            Création de rôle
          </button>
        </div>

        <p className="mt-4 text-xs text-muted">
          Les membres utilisent la couleur du rôle le plus élevé dans cette liste. Fais glisser les
          rôles pour les réorganiser.
        </p>

        {/* Tableau */}
        <div className="mt-3 overflow-hidden rounded-xl border border-line">
          <div className="grid grid-cols-[1fr_120px_72px] items-center gap-3 border-b border-line bg-deepest/40 px-4 py-2 text-[11px] font-bold uppercase tracking-wide text-muted">
            <span>Rôles — {list.length}</span>
            <span>Membres</span>
            <span />
          </div>
          {!membersLoaded ? (
            <ListSkeleton rows={4} />
          ) : list.length === 0 ? (
            <p className="px-4 py-8 text-center text-sm text-muted">
              {q ? "Aucun rôle ne correspond." : "Aucun rôle pour le moment."}
            </p>
          ) : (
            list.map((r) => {
              const editable = canEditRole(r);
              const draggable = dndOn && editable;
              return (
                <div
                  key={r.id}
                  draggable={draggable}
                  onDragStart={() => {
                    if (draggable) dragId.current = r.id;
                  }}
                  onDragOver={(e) => {
                    if (dragId.current) {
                      e.preventDefault();
                      if (overId !== r.id) setOverId(r.id);
                    }
                  }}
                  onDragEnd={() => {
                    dragId.current = null;
                    setOverId(null);
                  }}
                  onDrop={(e) => {
                    e.preventDefault();
                    drop(r.id);
                  }}
                  className={`group grid grid-cols-[1fr_120px_72px] items-center gap-3 border-t px-4 py-2.5 hover:bg-hover ${
                    overId === r.id && dragId.current ? "border-t-accent" : "border-line"
                  } ${dragId.current === r.id ? "opacity-40" : ""}`}
                >
                  <button
                    onClick={() => editable && onOpen(r.id)}
                    disabled={!editable}
                    className="flex min-w-0 items-center gap-2.5 text-left disabled:cursor-not-allowed"
                  >
                    <GripVertical
                      size={15}
                      className={`shrink-0 ${draggable ? "cursor-grab text-transparent group-hover:text-muted" : "text-transparent"}`}
                    />
                    <span
                      className="h-3.5 w-3.5 shrink-0 rounded-full"
                      style={roleDotStyle(r)}
                    />
                    <span className="truncate text-sm font-medium text-header">{r.name}</span>
                    {r.managed && (
                      <span className="shrink-0 rounded bg-white/5 px-1.5 py-0.5 text-[10px] text-muted">
                        géré
                      </span>
                    )}
                  </button>
                  <span className="flex items-center gap-1.5 text-sm text-muted">
                    {countForRole(r.id)}
                    <Users size={14} />
                  </span>
                  <div className="flex items-center justify-end gap-1">
                    {editable && (
                      <button
                        onClick={() => onOpen(r.id)}
                        title="Modifier"
                        className="flex h-7 w-7 items-center justify-center rounded text-muted hover:bg-white/10 hover:text-normal"
                      >
                        <Pencil size={15} />
                      </button>
                    )}
                    {editable && (
                      <RoleRowMenu onEdit={() => onOpen(r.id)} onDelete={() => onDelete(r.id)} />
                    )}
                  </div>
                </div>
              );
            })
          )}
        </div>
      </div>
    </div>
  );
}

function RoleRowMenu({ onEdit, onDelete }: { onEdit: () => void; onDelete: () => void }) {
  return (
    <Popover.Root>
      <Popover.Trigger className="flex h-7 w-7 items-center justify-center rounded text-muted outline-none hover:bg-white/10 hover:text-normal">
        <MoreVertical size={16} />
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          align="end"
          sideOffset={4}
          className={`z-[80] w-[200px] rounded-xl bg-floating p-1.5 shadow-pop ring-1 ring-line ${OVERLAY_ANIM}`}
        >
          <Popover.Close
            onClick={onEdit}
            className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm text-normal outline-none hover:bg-hover"
          >
            <Pencil size={14} /> Modifier le rôle
          </Popover.Close>
          <Popover.Close
            onClick={onDelete}
            className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm text-dnd outline-none hover:bg-dnd/15"
          >
            <Trash2 size={14} /> Supprimer le rôle
          </Popover.Close>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

// ───────────────────────────── Éditeur de rôle (onglets) ─────────────────────────────

type Tab = "display" | "permissions" | "members";

type ColorStyle = "solid" | "gradient" | "neon" | "wave";

interface RoleDraft {
  name: string;
  color: number;
  secondaryColor: number;
  colorStyle: ColorStyle;
  hoist: boolean;
  mentionable: boolean;
  permissions: string;
}

function RoleEditor({
  guildId,
  role,
  roles,
  members,
  myPerms,
  canEdit,
  canEditRole,
  countForRole,
  onSelect,
  onBack,
  onCreate,
  onDelete,
  onSaved,
  onReorder,
  onMemberRolePatched,
  onRefreshMembers,
}: {
  guildId: Snowflake;
  role: Role;
  roles: Role[];
  members: Member[] | null;
  myPerms: bigint;
  canEdit: boolean;
  canEditRole: (r: Role) => boolean;
  countForRole: (id: Snowflake) => number;
  onSelect: (id: Snowflake) => void;
  onBack: () => void;
  onCreate: () => void;
  onDelete: (id: Snowflake) => void;
  onSaved: () => Promise<void> | void;
  onReorder: (orderedIds: Snowflake[]) => void;
  onMemberRolePatched: (userId: Snowflake, roleId: Snowflake, add: boolean) => void;
  onRefreshMembers: () => Promise<void> | void;
}) {
  const isEveryone = role.id === guildId;
  const [tab, setTab] = useState<Tab>(isEveryone ? "permissions" : "display");
  const [draft, setDraft] = useState<RoleDraft>(() => toDraft(role));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const dragId = useRef<Snowflake | null>(null);
  const [overId, setOverId] = useState<Snowflake | null>(null);

  function dropRole(targetId: Snowflake) {
    const from = dragId.current;
    dragId.current = null;
    setOverId(null);
    if (!from) return;
    const ids = roles.filter((r) => r.id !== guildId).map((r) => r.id);
    const next = reordered(ids, from, targetId);
    if (next.join() !== ids.join()) onReorder(next);
  }

  // Réinitialise le brouillon quand on change de rôle.
  useEffect(() => {
    setDraft(toDraft(role));
    setError(null);
    setTab(role.id === guildId ? "permissions" : "display");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [role.id]);

  const dirty =
    draft.name !== role.name ||
    draft.color !== role.color ||
    draft.secondaryColor !== (role.secondary_color ?? 0) ||
    draft.colorStyle !== ((role.color_style as ColorStyle) || "solid") ||
    draft.hoist !== role.hoist ||
    draft.mentionable !== role.mentionable ||
    draft.permissions !== (role.permissions || "0");

  function set<K extends keyof RoleDraft>(key: K, value: RoleDraft[K]) {
    setDraft((d) => ({ ...d, [key]: value }));
  }

  async function save() {
    setBusy(true);
    setError(null);
    try {
      await api.updateRole(guildId, role.id, {
        name: draft.name.trim() || role.name,
        color: draft.color,
        secondary_color: draft.colorStyle === "gradient" || draft.colorStyle === "wave"
          ? draft.secondaryColor
          : null,
        color_style: draft.colorStyle,
        hoist: draft.hoist,
        mentionable: draft.mentionable,
        permissions: draft.permissions,
      });
      await onSaved();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Échec de l'enregistrement.");
    } finally {
      setBusy(false);
    }
  }

  const tabs: { id: Tab; label: string }[] = isEveryone
    ? [{ id: "permissions", label: "Permissions" }]
    : [
        { id: "display", label: "Affichage" },
        { id: "permissions", label: "Permissions" },
        { id: "members", label: `Gérer les membres — ${countForRole(role.id)}` },
      ];

  return (
    <div className="flex h-full">
      {/* Colonne liste des rôles */}
      <div className="flex w-[226px] shrink-0 flex-col border-r border-line bg-modal-nav">
        <button
          onClick={onBack}
          className="flex items-center gap-1.5 px-4 pb-2 pt-5 text-xs font-bold uppercase tracking-wide text-muted transition-colors hover:text-normal"
        >
          <ArrowLeft size={14} /> Retour
        </button>
        <div className="flex items-center justify-between px-4 pb-1">
          <span className="text-[11px] font-bold uppercase tracking-wide text-muted">Rôles</span>
          <button
            onClick={onCreate}
            title="Créer un rôle"
            className="text-muted transition-colors hover:text-normal"
          >
            <Plus size={15} />
          </button>
        </div>
        <div className="flex-1 overflow-y-auto px-2 pb-3 scroll-thin">
          {roles
            .filter((r) => r.id !== guildId)
            .map((r) => {
              const draggable = canEditRole(r);
              return (
                <button
                  key={r.id}
                  onClick={() => canEditRole(r) && onSelect(r.id)}
                  disabled={!canEditRole(r)}
                  draggable={draggable}
                  onDragStart={() => {
                    if (draggable) dragId.current = r.id;
                  }}
                  onDragOver={(e) => {
                    if (dragId.current) {
                      e.preventDefault();
                      if (overId !== r.id) setOverId(r.id);
                    }
                  }}
                  onDragEnd={() => {
                    dragId.current = null;
                    setOverId(null);
                  }}
                  onDrop={(e) => {
                    e.preventDefault();
                    dropRole(r.id);
                  }}
                  className={`mb-0.5 flex w-full items-center gap-2 rounded border-t-2 px-2 py-1.5 text-left text-sm transition-colors disabled:opacity-40 ${
                    overId === r.id && dragId.current ? "border-t-accent" : "border-t-transparent"
                  } ${
                    r.id === role.id ? "bg-selected text-header" : "text-channel hover:bg-hover"
                  } ${dragId.current === r.id ? "opacity-40" : ""}`}
                >
                  <span className="h-3 w-3 shrink-0 rounded-full" style={roleDotStyle(r)} />
                  <span className="truncate">{r.name}</span>
                </button>
              );
            })}
        </div>
        {/* @everyone épinglé en bas */}
        {(() => {
          const everyone = roles.find((r) => r.id === guildId);
          if (!everyone) return null;
          return (
            <button
              onClick={() => onSelect(everyone.id)}
              className={`m-2 flex items-center gap-2 rounded px-2 py-1.5 text-left text-sm transition-colors ${
                isEveryone ? "bg-selected text-header" : "text-channel hover:bg-hover"
              }`}
            >
              <Users size={14} className="shrink-0 text-muted" />
              <span className="truncate">@everyone</span>
            </button>
          );
        })()}
      </div>

      {/* Panneau d'édition */}
      <div className="relative flex min-w-0 flex-1 flex-col">
        {/* En-tête : nom + onglets */}
        <div className="shrink-0 border-b border-line px-8 pt-6 pr-16">
          <div className="mb-3 flex items-center gap-2">
            <span
              className="h-4 w-4 shrink-0 rounded-full"
              style={roleDotStyle(role)}
            />
            <h2
              className={`truncate text-lg font-bold text-header ${
                isEveryone ? "" : roleNameStyle(role)?.className ?? ""
              }`}
              style={isEveryone ? undefined : roleNameStyle(role)?.style}
            >
              {isEveryone ? "@everyone" : role.name}
            </h2>
          </div>
          <div className="flex gap-5">
            {tabs.map((t) => (
              <button
                key={t.id}
                onClick={() => setTab(t.id)}
                className={`-mb-px border-b-2 pb-2.5 text-sm font-medium transition-colors ${
                  tab === t.id
                    ? "border-accent text-header"
                    : "border-transparent text-muted hover:text-normal"
                }`}
              >
                {t.label}
              </button>
            ))}
          </div>
        </div>

        {/* Contenu de l'onglet */}
        <div className="min-h-0 flex-1 overflow-y-auto px-8 py-6 scroll-thin">
          {!canEdit && (
            <div className="mb-4 rounded-lg border border-line bg-deepest/40 px-4 py-3 text-sm text-muted">
              Tu n'as pas la permission de modifier ce rôle (hiérarchie ou rôle géré). Affichage en
              lecture seule.
            </div>
          )}
          {tab === "display" && !isEveryone && (
            <DisplayTab draft={draft} disabled={!canEdit} onChange={set} />
          )}
          {tab === "permissions" && (
            <PermissionsTab
              draft={draft}
              myPerms={myPerms}
              disabled={!canEdit}
              onChange={(perms) => set("permissions", perms)}
            />
          )}
          {tab === "members" && !isEveryone && (
            <MembersTab
              guildId={guildId}
              role={role}
              members={members}
              disabled={!canEdit}
              onPatch={onMemberRolePatched}
              onRefresh={onRefreshMembers}
            />
          )}
        </div>

        {/* Supprimer le rôle (pieds de page, sauf @everyone) */}
        {!isEveryone && canEdit && tab !== "members" && (
          <div className="shrink-0 border-t border-line px-8 py-3">
            <button
              onClick={() => onDelete(role.id)}
              className="flex items-center gap-2 text-sm font-medium text-dnd transition-colors hover:underline"
            >
              <Trash2 size={15} /> Supprimer le rôle
            </button>
          </div>
        )}

        {/* Barre d'enregistrement */}
        {dirty && canEdit && (
          <div className="absolute inset-x-4 bottom-4 z-10 flex animate-pop-in items-center justify-between gap-4 rounded-xl bg-floating px-4 py-2.5 shadow-pop ring-1 ring-cardline">
            <span className="text-sm text-normal">
              {error ? (
                <span className="text-dnd">{error}</span>
              ) : (
                "Attention — tu as des modifications non enregistrées !"
              )}
            </span>
            <div className="flex shrink-0 items-center gap-2">
              <button
                onClick={() => {
                  setDraft(toDraft(role));
                  setError(null);
                }}
                disabled={busy}
                className="rounded px-3 py-1.5 text-sm text-normal hover:underline disabled:opacity-50"
              >
                Réinitialiser
              </button>
              <button
                onClick={() => void save()}
                disabled={busy}
                className="pressable inline-flex items-center justify-center gap-2 rounded-md bg-online px-4 py-1.5 text-sm font-semibold text-white hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
              >
                {busy && <Spinner size={14} />}Enregistrer
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function toDraft(r: Role): RoleDraft {
  return {
    name: r.name,
    color: r.color,
    secondaryColor: r.secondary_color ?? 0,
    colorStyle: (r.color_style as ColorStyle) || "solid",
    hoist: r.hoist,
    mentionable: r.mentionable,
    permissions: r.permissions || "0",
  };
}

// ───────────────────────────── Onglet Affichage ─────────────────────────────

const COLOR_STYLES: { id: ColorStyle; label: string; hint: string }[] = [
  { id: "solid", label: "Solide", hint: "Une seule couleur." },
  { id: "gradient", label: "Dégradé", hint: "Fondu entre deux couleurs." },
  { id: "neon", label: "Néon", hint: "Couleur avec halo lumineux." },
  { id: "wave", label: "Vague", hint: "Dégradé animé qui ondule." },
];

function DisplayTab({
  draft,
  disabled,
  onChange,
}: {
  draft: RoleDraft;
  disabled: boolean;
  onChange: <K extends keyof RoleDraft>(key: K, value: RoleDraft[K]) => void;
}) {
  const usesSecondary = draft.colorStyle === "gradient" || draft.colorStyle === "wave";
  // Rôle fictif pour l'aperçu en direct du style choisi.
  const previewStyle = roleNameStyle({
    color: draft.color || 0x5865f2,
    secondary_color: usesSecondary ? draft.secondaryColor || draft.color || 0x5865f2 : null,
    color_style: draft.colorStyle,
  } as Role);

  return (
    <div className="max-w-[560px]">
      <label className="mb-1.5 block text-xs font-bold uppercase tracking-wide text-subtext">
        Nom du rôle
      </label>
      <input
        value={draft.name}
        onChange={(e) => onChange("name", e.target.value)}
        disabled={disabled}
        maxLength={100}
        className="mb-6 w-full rounded-lg bg-deepest px-3 py-2.5 text-normal outline-none ring-1 ring-transparent focus:ring-accent disabled:opacity-50"
      />

      {/* Style de couleur */}
      <label className="mb-2 block text-xs font-bold uppercase tracking-wide text-subtext">
        Style de couleur
      </label>
      <div className="mb-5 grid grid-cols-4 gap-2">
        {COLOR_STYLES.map((s) => (
          <button
            key={s.id}
            onClick={() => onChange("colorStyle", s.id)}
            disabled={disabled}
            title={s.hint}
            className={`rounded-lg border px-2 py-2 text-sm font-medium transition-colors disabled:opacity-50 ${
              draft.colorStyle === s.id
                ? "border-accent bg-accent/10 text-header"
                : "border-line text-muted hover:bg-hover hover:text-normal"
            }`}
          >
            {s.label}
          </button>
        ))}
      </div>

      {/* Aperçu en direct */}
      <div className="mb-5 flex items-center gap-3 rounded-lg bg-deepest/50 px-4 py-3">
        <span className="text-xs uppercase tracking-wide text-muted">Aperçu</span>
        <span
          className={`text-[15px] font-semibold ${previewStyle?.className ?? ""}`}
          style={previewStyle?.style ?? { color: "var(--text-header)" }}
        >
          {draft.name || "Nom du rôle"}
        </span>
      </div>

      {/* Couleur principale */}
      <ColorPalette
        label={usesSecondary ? "Couleur principale" : "Couleur du rôle"}
        value={draft.color}
        disabled={disabled}
        allowDefault={!usesSecondary}
        onPick={(c) => onChange("color", c)}
      />

      {/* Couleur secondaire (dégradé / vague) */}
      {usesSecondary && (
        <ColorPalette
          label="Couleur secondaire"
          value={draft.secondaryColor || draft.color}
          disabled={disabled}
          allowDefault={false}
          onPick={(c) => onChange("secondaryColor", c)}
        />
      )}

      <div className="my-5 h-px bg-white/5" />

      <ToggleRow
        label="Afficher les membres de ce rôle séparément"
        desc="Les membres avec ce rôle apparaissent dans une catégorie distincte dans la liste des membres."
        checked={draft.hoist}
        disabled={disabled}
        onChange={(v) => onChange("hoist", v)}
      />
      <ToggleRow
        label="Permettre à tout le monde de mentionner ce rôle"
        desc="Note : tout membre pouvant @everyone pourra aussi mentionner ce rôle."
        checked={draft.mentionable}
        disabled={disabled}
        onChange={(v) => onChange("mentionable", v)}
      />
    </div>
  );
}

// Palette de couleurs réutilisable (défaut + perso + swatches).
function ColorPalette({
  label,
  value,
  disabled,
  allowDefault,
  onPick,
}: {
  label: string;
  value: number;
  disabled: boolean;
  allowDefault: boolean;
  onPick: (c: number) => void;
}) {
  const customRef = useRef<HTMLInputElement>(null);
  return (
    <div className="mb-5">
      <label className="mb-2 block text-xs font-bold uppercase tracking-wide text-subtext">
        {label}
      </label>
      <div className="flex flex-wrap items-center gap-2">
        {allowDefault && (
          <button
            onClick={() => onPick(0)}
            disabled={disabled}
            title="Couleur par défaut"
            className={`flex h-9 w-9 items-center justify-center rounded-md bg-[#99aab5] disabled:opacity-50 ${
              value === 0 ? "ring-2 ring-white ring-offset-2 ring-offset-modal" : ""
            }`}
          >
            {value === 0 && <Check size={16} className="text-black/70" />}
          </button>
        )}
        <button
          onClick={() => customRef.current?.click()}
          disabled={disabled}
          title="Couleur personnalisée"
          className="relative flex h-9 w-9 items-center justify-center overflow-hidden rounded-md disabled:opacity-50"
          style={{ background: "conic-gradient(#f23f42,#f0b132,#23a559,#3aa0ff,#9b59b6,#eb459e,#f23f42)" }}
        >
          <Plus size={16} className="text-white drop-shadow" />
          <input
            ref={customRef}
            type="color"
            disabled={disabled}
            value={roleColorHex(value || 0x5865f2)}
            onChange={(e) => onPick(parseInt(e.target.value.slice(1), 16))}
            className="absolute inset-0 cursor-pointer opacity-0"
          />
        </button>
        <span className="mx-1 h-7 w-px bg-white/10" />
        {ROLE_COLORS.map((c) => (
          <button
            key={c}
            onClick={() => onPick(c)}
            disabled={disabled}
            className={`flex h-9 w-9 items-center justify-center rounded-md disabled:opacity-50 ${
              value === c ? "ring-2 ring-white ring-offset-2 ring-offset-modal" : ""
            }`}
            style={{ backgroundColor: roleColorHex(c) }}
          >
            {value === c && <Check size={16} className="text-white drop-shadow" />}
          </button>
        ))}
      </div>
    </div>
  );
}

// ───────────────────────────── Onglet Permissions ─────────────────────────────

function PermissionsTab({
  draft,
  myPerms,
  disabled,
  onChange,
}: {
  draft: RoleDraft;
  myPerms: bigint;
  disabled: boolean;
  onChange: (permissions: string) => void;
}) {
  const [q, setQ] = useState("");
  const isAdmin = (myPerms & PERM.ADMINISTRATOR) === PERM.ADMINISTRATOR;
  const ql = q.toLowerCase().trim();
  const visible = PERMISSIONS.filter(
    (p) => p.label.toLowerCase().includes(ql) || p.desc.toLowerCase().includes(ql),
  );
  const byCat = (cat: PermCat) => visible.filter((p) => p.cat === cat);

  return (
    <div className="max-w-[640px]">
      <div className="mb-4 flex items-center gap-3">
        <div className="flex flex-1 items-center gap-2 rounded-lg bg-deepest px-3 py-2">
          <Search size={15} className="text-muted" />
          <input
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="Rechercher des permissions"
            className="w-full bg-transparent text-sm text-normal outline-none placeholder:text-muted"
          />
        </div>
        {!disabled && draft.permissions !== "0" && (
          <button
            onClick={() => onChange("0")}
            className="shrink-0 rounded-lg border border-line px-3 py-2 text-sm font-medium text-normal transition-colors hover:bg-hover"
          >
            Effacer les permissions
          </button>
        )}
      </div>

      {PERM_CATEGORIES.map((cat) => {
        const perms = byCat(cat.id);
        if (perms.length === 0) return null;
        return (
          <div key={cat.id} className="mb-6">
            <div className="mb-2 text-[11px] font-bold uppercase tracking-wide text-muted">
              {cat.label}
            </div>
            <div className="overflow-hidden rounded-xl border border-line">
              {perms.map((p, i) => {
                const on = hasPerm(draft.permissions, p.bit);
                // On ne peut accorder que des permissions que l'on possède (sauf admin/propriétaire).
                const locked = disabled || (!isAdmin && (myPerms & p.bit) !== p.bit);
                return (
                  <div
                    key={p.key}
                    className={`flex items-start justify-between gap-4 px-4 py-3 ${
                      i > 0 ? "border-t border-line" : ""
                    }`}
                  >
                    <div className="min-w-0">
                      <div className="text-sm font-medium text-header">{p.label}</div>
                      <div className="mt-0.5 text-xs text-muted">{p.desc}</div>
                    </div>
                    <Toggle
                      checked={on}
                      disabled={locked}
                      onChange={(v) => onChange(togglePerm(draft.permissions, p.bit, v))}
                    />
                  </div>
                );
              })}
            </div>
          </div>
        );
      })}
      {visible.length === 0 && (
        <p className="py-8 text-center text-sm text-muted">Aucune permission ne correspond.</p>
      )}
    </div>
  );
}

// ───────────────────────────── Onglet Gérer les membres ─────────────────────────────

function MembersTab({
  guildId,
  role,
  members,
  disabled,
  onPatch,
  onRefresh,
}: {
  guildId: Snowflake;
  role: Role;
  members: Member[] | null;
  disabled: boolean;
  onPatch: (userId: Snowflake, roleId: Snowflake, add: boolean) => void;
  onRefresh: () => Promise<void> | void;
}) {
  const [q, setQ] = useState("");
  const [picker, setPicker] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);

  const inRole = (members ?? []).filter((m) => m.roles.includes(role.id));
  const ql = q.toLowerCase().trim();
  const shown = inRole.filter(
    (m) =>
      (m.nick || displayName(m.user)).toLowerCase().includes(ql) ||
      m.user.username.toLowerCase().includes(ql),
  );

  function toggleSel(id: string) {
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  }

  async function removeOne(userId: Snowflake) {
    onPatch(userId, role.id, false);
    setSelected((s) => {
      const n = new Set(s);
      n.delete(userId);
      return n;
    });
    try {
      await api.removeMemberRole(guildId, userId, role.id);
    } catch {
      onPatch(userId, role.id, true); // rollback
    }
    await onRefresh();
  }

  async function removeSelected() {
    const ids = [...selected];
    if (ids.length === 0) return;
    setBusy(true);
    for (const id of ids) {
      onPatch(id, role.id, false);
      try {
        await api.removeMemberRole(guildId, id, role.id);
      } catch {
        onPatch(id, role.id, true);
      }
    }
    setSelected(new Set());
    await onRefresh();
    setBusy(false);
  }

  async function addMany(ids: string[]) {
    setBusy(true);
    for (const id of ids) {
      onPatch(id, role.id, true);
      try {
        await api.addMemberRole(guildId, id, role.id);
      } catch {
        onPatch(id, role.id, false);
      }
    }
    await onRefresh();
    setBusy(false);
    setPicker(false);
  }

  return (
    <div className="max-w-[640px]">
      <div className="mb-4 flex items-center gap-3">
        <div className="flex flex-1 items-center gap-2 rounded-lg bg-deepest px-3 py-2">
          <Search size={15} className="text-muted" />
          <input
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="Rechercher des membres"
            className="w-full bg-transparent text-sm text-normal outline-none placeholder:text-muted"
          />
        </div>
        {!disabled && (
          <button
            onClick={() => setPicker(true)}
            disabled={busy}
            className="flex shrink-0 items-center gap-1.5 rounded-lg btn-accent px-4 py-2 text-sm font-semibold text-white transition disabled:opacity-50"
          >
            <UserPlus size={15} /> Ajouter des membres
          </button>
        )}
      </div>

      {/* Barre d'action de sélection (bulk remove) */}
      {selected.size > 0 && !disabled && (
        <div className="mb-3 flex items-center justify-between rounded-lg border border-line bg-deepest/40 px-3 py-2">
          <span className="text-sm text-normal">{selected.size} sélectionné(s)</span>
          <div className="flex items-center gap-2">
            <button
              onClick={() => setSelected(new Set())}
              className="rounded px-2 py-1 text-sm text-muted hover:text-normal"
            >
              Annuler
            </button>
            <button
              onClick={() => void removeSelected()}
              disabled={busy}
              className="rounded-md bg-dnd px-3 py-1 text-sm font-semibold text-white hover:opacity-90 disabled:opacity-50"
            >
              Retirer la sélection
            </button>
          </div>
        </div>
      )}

      {members === null ? (
        <ListSkeleton rows={4} />
      ) : shown.length === 0 ? (
        <div className="rounded-xl border border-dashed border-line py-12 text-center">
          <p className="text-sm text-muted">
            {q ? "Aucun membre ne correspond." : "Aucun membre n'a ce rôle pour l'instant."}
          </p>
        </div>
      ) : (
        <div className="overflow-hidden rounded-xl border border-line">
          {shown.map((m, i) => {
            const name = m.nick || displayName(m.user);
            const sel = selected.has(m.user.id);
            return (
              <div
                key={m.user.id}
                className={`flex items-center gap-3 px-3 py-2 hover:bg-hover ${
                  i > 0 ? "border-t border-line" : ""
                }`}
              >
                {!disabled && (
                  <input
                    type="checkbox"
                    checked={sel}
                    onChange={() => toggleSel(m.user.id)}
                    className="h-4 w-4 accent-[#5865f2]"
                  />
                )}
                <Avatar name={name} id={m.user.id} size={28} avatarId={m.user.avatar_id} />
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm font-medium text-header">{name}</div>
                  <div className="truncate text-xs text-muted">{m.user.username}</div>
                </div>
                {!disabled && (
                  <button
                    onClick={() => void removeOne(m.user.id)}
                    title="Retirer ce rôle"
                    className="flex h-7 w-7 items-center justify-center rounded text-muted hover:bg-dnd/15 hover:text-dnd"
                  >
                    <X size={16} />
                  </button>
                )}
              </div>
            );
          })}
        </div>
      )}

      {picker && (
        <AddMembersModal
          role={role}
          members={(members ?? []).filter((m) => !m.roles.includes(role.id))}
          busy={busy}
          onClose={() => setPicker(false)}
          onConfirm={(ids) => void addMany(ids)}
        />
      )}
    </div>
  );
}

function AddMembersModal({
  role,
  members,
  busy,
  onClose,
  onConfirm,
}: {
  role: Role;
  members: Member[];
  busy: boolean;
  onClose: () => void;
  onConfirm: (ids: string[]) => void;
}) {
  const [q, setQ] = useState("");
  const [sel, setSel] = useState<Set<string>>(new Set());

  // ESC ferme uniquement cette modale (capture, avant le gestionnaire des Paramètres).
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

  const ql = q.toLowerCase().trim();
  const shown = members.filter(
    (m) =>
      (m.nick || displayName(m.user)).toLowerCase().includes(ql) ||
      m.user.username.toLowerCase().includes(ql),
  );

  function toggle(id: string) {
    setSel((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  }

  return (
    <div
      className="fixed inset-0 z-[90] flex items-center justify-center bg-black/60 p-6 animate-overlay-in"
      onClick={onClose}
    >
      <div
        className="flex max-h-[70vh] w-[440px] animate-pop-in flex-col overflow-hidden rounded-xl border border-line bg-modal shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="border-b border-line px-5 py-4">
          <h3 className="text-base font-bold text-header">Ajouter des membres</h3>
          <p className="mt-0.5 flex items-center gap-1.5 text-sm text-muted">
            au rôle
            <span
              className="inline-flex items-center gap-1.5 rounded-full bg-deepest px-2 py-0.5 text-xs text-normal"
            >
              <span
                className="h-2 w-2 rounded-full"
                style={roleDotStyle(role)}
              />
              {role.name}
            </span>
          </p>
        </div>
        <div className="px-5 pt-4">
          <div className="flex items-center gap-2 rounded-lg bg-deepest px-3 py-2">
            <Search size={15} className="text-muted" />
            <input
              autoFocus
              value={q}
              onChange={(e) => setQ(e.target.value)}
              placeholder="Rechercher des membres"
              className="w-full bg-transparent text-sm text-normal outline-none placeholder:text-muted"
            />
          </div>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-3 py-3 scroll-thin">
          {shown.length === 0 ? (
            <p className="py-8 text-center text-sm text-muted">Aucun membre disponible.</p>
          ) : (
            shown.map((m) => {
              const name = m.nick || displayName(m.user);
              const checked = sel.has(m.user.id);
              return (
                <button
                  key={m.user.id}
                  onClick={() => toggle(m.user.id)}
                  className="flex w-full items-center gap-3 rounded-lg px-2 py-1.5 text-left hover:bg-hover"
                >
                  <span
                    className={`flex h-4 w-4 shrink-0 items-center justify-center rounded border ${
                      checked ? "border-accent bg-accent text-white" : "border-muted/50"
                    }`}
                  >
                    {checked && <Check size={12} />}
                  </span>
                  <Avatar name={name} id={m.user.id} size={28} avatarId={m.user.avatar_id} />
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium text-header">{name}</div>
                    <div className="truncate text-xs text-muted">{m.user.username}</div>
                  </div>
                </button>
              );
            })
          )}
        </div>
        <div className="flex items-center justify-between gap-3 border-t border-line px-5 py-3">
          <span className="text-sm text-muted">{sel.size} sélectionné(s)</span>
          <div className="flex items-center gap-2">
            <button
              onClick={onClose}
              className="rounded px-3 py-1.5 text-sm text-normal hover:underline"
            >
              Annuler
            </button>
            <button
              onClick={() => onConfirm([...sel])}
              disabled={busy || sel.size === 0}
              className="pressable inline-flex items-center justify-center gap-2 rounded-lg btn-accent px-4 py-1.5 text-sm font-semibold text-white disabled:cursor-not-allowed disabled:opacity-50"
            >
              {busy && <Spinner size={14} />}{`Ajouter${sel.size ? ` (${sel.size})` : ""}`}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

// ───────────────────────────── Petits composants ─────────────────────────────

function Toggle({
  checked,
  disabled,
  onChange,
}: {
  checked: boolean;
  disabled?: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <button
      onClick={() => !disabled && onChange(!checked)}
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      className={`pressable flex h-6 w-11 shrink-0 items-center rounded-full px-0.5 transition-colors duration-200 disabled:opacity-50 ${
        checked ? "bg-online" : "bg-white/15"
      }`}
    >
      <span
        className={`h-5 w-5 rounded-full bg-white shadow-sm transition-transform duration-200 ease-[cubic-bezier(0.34,1.56,0.64,1)] ${checked ? "translate-x-5" : "translate-x-0.5"}`}
      />
    </button>
  );
}

function ToggleRow({
  label,
  desc,
  checked,
  disabled,
  onChange,
}: {
  label: string;
  desc: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="mb-4 flex items-start justify-between gap-4">
      <div className="min-w-0">
        <div className="text-sm font-medium text-header">{label}</div>
        <div className="mt-0.5 text-xs text-muted">{desc}</div>
      </div>
      <Toggle checked={checked} disabled={disabled} onChange={onChange} />
    </div>
  );
}
