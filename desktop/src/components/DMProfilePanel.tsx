import { useEffect, useState } from "react";
import { ChevronRight, LogOut, Pencil, X } from "lucide-react";
import { api } from "../api";
import { roleColorHex, useStore } from "../store";
import { colorFor, displayName, initials } from "../lib/format";
import type { DMChannel, User, UserProfile } from "../types";
import { CH_GROUP } from "../types";
import { Avatar } from "./Avatar";
import { UserPopover } from "./ProfilePopout";

interface Mutual {
  guilds: { id: string; name: string; icon_id: string | null }[];
  friends: User[];
}

// Panneau latéral droit des MP : profil du correspondant (1:1) ou membres (groupe).
export function DMProfilePanel({ dm }: { dm: DMChannel }) {
  const me = useStore((s) => s.me);
  const isGroup = dm.type === CH_GROUP || dm.recipients.filter((u) => u.id !== me?.id).length > 1;

  if (isGroup) return <GroupPanel dm={dm} />;
  const partner = dm.recipients.find((u) => u.id !== me?.id);
  if (!partner) return <aside className="w-[340px] shrink-0 border-l border-line bg-sidebar" />;
  return <UserPanel partner={partner} />;
}

function UserPanel({ partner }: { partner: User }) {
  const relationship = useStore((s) => s.relationships.find((r) => r.user.id === partner.id));
  const [profile, setProfile] = useState<UserProfile | null>(null);
  const [mutual, setMutual] = useState<Mutual | null>(null);

  useEffect(() => {
    let alive = true;
    setProfile(null);
    setMutual(null);
    api.userProfile(partner.id).then((p) => alive && setProfile(p)).catch(() => {});
    api.userMutual(partner.id).then((m) => alive && setMutual(m)).catch(() => {});
    return () => {
      alive = false;
    };
  }, [partner.id]);

  const name = profile?.display_name || profile?.username || displayName(partner);
  const accent = profile?.accent_color ? roleColorHex(profile.accent_color) : colorFor(partner.id);

  return (
    <aside className="flex w-[340px] shrink-0 flex-col overflow-y-auto border-l border-line bg-sidebar scroll-thin">
      {/* Bannière : image téléversée si présente, sinon dégradé doux depuis l'accent. */}
      {profile?.banner_id ? (
        <img
          src={`/api/users/${partner.id}/banner?v=${profile.banner_id}`}
          alt=""
          className="h-[110px] w-full shrink-0 object-cover"
          draggable={false}
        />
      ) : (
        <div
          className="h-[110px] shrink-0"
          style={{ background: `linear-gradient(150deg, ${accent}, #0c0c0e)` }}
        />
      )}
      <div className="px-4">
        <div className="-mt-10 mb-2">
          <div className="inline-block rounded-full border-[6px] border-sidebar">
            <Avatar name={name} id={partner.id} size={76} avatarId={profile?.avatar_id ?? partner.avatar_id} />
          </div>
        </div>

        <div className="rounded-2xl bg-deepest p-4 shadow-sm">
          <div className="text-xl font-bold text-header">{name}</div>
          <div className="text-sm text-muted">@{profile?.username ?? partner.username}</div>
          {profile?.pronouns && <div className="mt-1 text-xs text-muted">{profile.pronouns}</div>}

          {profile?.bio && (
            <Block label="Bio">
              <p className="whitespace-pre-wrap break-words text-sm text-normal">{profile.bio}</p>
            </Block>
          )}

          {profile && (
            <Block label="Membre depuis">
              <p className="text-sm text-normal">{fmtDate(profile.created_at)}</p>
            </Block>
          )}
          {relationship?.type === "friend" && (
            <Block label="Amis depuis">
              <p className="text-sm text-normal">{fmtDate(relationship.since)}</p>
            </Block>
          )}

          <div className="mt-4">
            <UserPopover userId={partner.id} side="left">
              <span className="block w-full cursor-pointer rounded-lg bg-white/5 py-2 text-center text-sm font-medium text-normal transition-colors hover:bg-white/10">
                Voir le profil complet
              </span>
            </UserPopover>
          </div>
        </div>

        <MutualSection
          title="Serveurs en commun"
          count={mutual?.guilds.length}
          items={(mutual?.guilds ?? []).map((g) => (
            <div key={g.id} className="flex items-center gap-2.5 rounded-lg px-2 py-1.5 hover:bg-hover">
              {g.icon_id ? (
                <img
                  src={`/api/guilds/${g.id}/icon?v=${g.icon_id}`}
                  alt=""
                  className="h-8 w-8 rounded-full object-cover"
                />
              ) : (
                <span
                  className="flex h-8 w-8 items-center justify-center rounded-full text-xs font-semibold text-white"
                  style={{ backgroundColor: colorFor(g.id) }}
                >
                  {initials(g.name)}
                </span>
              )}
              <span className="truncate text-sm text-normal">{g.name}</span>
            </div>
          ))}
        />
        <MutualSection
          title="Amis en commun"
          count={mutual?.friends.length}
          items={(mutual?.friends ?? []).map((f) => (
            <div key={f.id} className="flex items-center gap-2.5 rounded-lg px-2 py-1.5 hover:bg-hover">
              <Avatar name={displayName(f)} id={f.id} size={32} avatarId={f.avatar_id} />
              <span className="truncate text-sm text-normal">{displayName(f)}</span>
            </div>
          ))}
        />
        <div className="h-4" />
      </div>
    </aside>
  );
}

function GroupPanel({ dm }: { dm: DMChannel }) {
  const me = useStore((s) => s.me);
  const refreshDMs = useStore((s) => s.refreshDMs);
  const others = dm.recipients.filter((u) => u.id !== me?.id);
  const isOwner = !!me && dm.owner_id === me.id;
  const [renaming, setRenaming] = useState(false);
  const [nameDraft, setNameDraft] = useState("");

  async function rename() {
    try {
      await api.updateChannel(dm.id, { name: nameDraft.trim() || null });
      await refreshDMs();
      setRenaming(false);
    } catch {
      /* ignore */
    }
  }

  async function removeMember(userId: string) {
    try {
      await api.removeRecipient(dm.id, userId);
      await refreshDMs();
    } catch {
      /* ignore */
    }
  }

  async function leaveGroup() {
    if (!me) return;
    try {
      await api.removeRecipient(dm.id, me.id);
      useStore.setState((s) => ({
        dms: s.dms.filter((x) => x.id !== dm.id),
        activeDM: s.activeDM === dm.id ? null : s.activeDM,
      }));
      await refreshDMs();
    } catch {
      /* ignore */
    }
  }

  return (
    <aside className="flex w-[340px] shrink-0 flex-col overflow-y-auto border-l border-line bg-sidebar px-4 py-4 scroll-thin">
      <div className="flex flex-col items-center gap-2 pb-3">
        <span
          className="flex h-16 w-16 items-center justify-center rounded-2xl text-xl font-bold text-white"
          style={{ backgroundColor: colorFor(dm.id) }}
        >
          {initials(dm.name || others.map(displayName).join(", ") || "Groupe")}
        </span>
        {renaming ? (
          <div className="flex w-full items-center gap-1.5 px-2">
            <input
              autoFocus
              value={nameDraft}
              onChange={(e) => setNameDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void rename();
                if (e.key === "Escape") setRenaming(false);
              }}
              maxLength={100}
              placeholder="Nom du groupe"
              className="w-full rounded-lg bg-deepest px-2 py-1.5 text-sm text-normal outline-none ring-1 ring-transparent placeholder:text-muted focus:ring-accent"
            />
            <button
              onClick={() => void rename()}
              className="shrink-0 rounded-lg btn-accent px-2.5 py-1.5 text-xs font-semibold text-white"
            >
              OK
            </button>
          </div>
        ) : (
          <div className="flex items-center gap-1.5">
            <div className="text-center text-base font-bold text-header">
              {dm.name || others.map(displayName).join(", ")}
            </div>
            {isOwner && (
              <button
                title="Renommer le groupe"
                onClick={() => {
                  setNameDraft(dm.name ?? "");
                  setRenaming(true);
                }}
                className="rounded p-1 text-muted opacity-70 hover:bg-hover hover:text-normal"
              >
                <Pencil size={13} />
              </button>
            )}
          </div>
        )}
        <div className="text-xs text-muted">{dm.recipients.length} membres</div>
      </div>
      <div className="mb-1 mt-2 px-2 text-xs font-semibold uppercase tracking-wide text-channel">
        Membres — {dm.recipients.length}
      </div>
      {dm.recipients.map((u) => (
        <div key={u.id} className="group flex w-full items-center gap-2.5 rounded-lg px-2 py-1.5 hover:bg-hover">
          <UserPopover userId={u.id} side="left">
            <div className="flex min-w-0 flex-1 items-center gap-2.5">
              <Avatar name={displayName(u)} id={u.id} size={32} avatarId={u.avatar_id} />
              <span className="truncate text-sm text-normal">
                {displayName(u)}
                {u.id === me?.id ? " (vous)" : ""}
                {u.id === dm.owner_id ? " · propriétaire" : ""}
              </span>
            </div>
          </UserPopover>
          {isOwner && u.id !== me?.id && (
            <button
              title="Retirer du groupe"
              onClick={() => void removeMember(u.id)}
              className="ml-auto shrink-0 rounded p-1 text-muted opacity-0 transition-opacity hover:bg-dnd/15 hover:text-dnd group-hover:opacity-100"
            >
              <X size={14} />
            </button>
          )}
        </div>
      ))}

      <div className="mt-auto pt-4">
        <button
          onClick={() => void leaveGroup()}
          className="flex w-full items-center justify-center gap-2 rounded-lg border border-dnd/60 py-2 text-sm font-medium text-dnd transition-colors hover:bg-dnd hover:text-white"
        >
          <LogOut size={15} /> Quitter le groupe
        </button>
      </div>
    </aside>
  );
}

function Block({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <>
      <div className="my-2.5 h-px bg-white/5" />
      <div className="mb-1 text-xs font-bold uppercase tracking-wide text-subtext">{label}</div>
      {children}
    </>
  );
}

function MutualSection({
  title,
  count,
  items,
}: {
  title: string;
  count: number | undefined;
  items: React.ReactNode[];
}) {
  const [open, setOpen] = useState(false);
  if (!count) return null;
  return (
    <div className="mt-2">
      <button
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center justify-between rounded-lg px-2 py-2 text-sm font-medium text-channel transition-colors hover:bg-hover hover:text-normal"
      >
        <span>
          {title} — {count}
        </span>
        <ChevronRight size={16} className={`transition-transform ${open ? "rotate-90" : ""}`} />
      </button>
      {open && <div className="mt-0.5">{items}</div>}
    </div>
  );
}

function fmtDate(ms: number): string {
  return new Date(ms).toLocaleDateString("fr-FR", { day: "numeric", month: "long", year: "numeric" });
}
