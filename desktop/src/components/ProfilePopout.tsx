import { useState, type ReactNode } from "react";
import * as Popover from "@radix-ui/react-popover";
import {
  ArrowRightLeft,
  Ban,
  HeadphoneOff,
  MessageCircle,
  MicOff,
  Pencil,
  PhoneOff,
  Plus,
  UserMinus,
  X,
} from "lucide-react";
import { api } from "../api";
import { canIn, roleColorHex, useStore } from "../store";
import { PERM } from "../lib/permissions";
import { OVERLAY_ANIM } from "../lib/anim";
import { CH_VOICE, type ModerateVoiceState, type UserProfile } from "../types";
import { Avatar } from "./Avatar";
import { colorFor, initials } from "../lib/format";
import { mediaUrl } from "../lib/instance";

// Carte de profil affichée au clic sur un avatar / pseudo.
// `open`/`onOpenChange` permettent un contrôle externe (ex. coexistence avec un menu contextuel
// clic-droit sans que l'un déclenche l'autre). Non fournis ⇒ le popover s'auto-gère (cas usuel).
export function UserPopover({
  userId,
  children,
  side = "right",
  guildId,
  open,
  onOpenChange: controlledOnOpenChange,
}: {
  userId: string;
  children: ReactNode;
  side?: "top" | "right" | "bottom" | "left";
  guildId?: string;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
}) {
  const [profile, setProfile] = useState<UserProfile | null>(null);
  const [loading, setLoading] = useState(false);
  const [showAdd, setShowAdd] = useState(false);
  const [editingNick, setEditingNick] = useState(false);
  const [nickDraft, setNickDraft] = useState("");
  // Note personnelle (privée, visible par moi seul).
  const [note, setNote] = useState("");
  const [noteLoaded, setNoteLoaded] = useState(false);
  const me = useStore((s) => s.me);
  const canModerate = useStore((s) => !!guildId && canIn(s, guildId, PERM.MODERATE_MEMBERS));
  // Surnom : pour soi avec CHANGE_NICKNAME, pour autrui avec MANAGE_NICKNAMES.
  const canChangeOwnNick = useStore((s) => !!guildId && canIn(s, guildId, PERM.CHANGE_NICKNAME));
  const canManageNicks = useStore((s) => !!guildId && canIn(s, guildId, PERM.MANAGE_NICKNAMES));
  const openDM = useStore((s) => s.openDM);
  const refreshDMs = useStore((s) => s.refreshDMs);
  const isOwner = useStore(
    (s) => !!guildId && s.guilds.find((g) => g.id === guildId)?.owner_id === s.me?.id,
  );
  // Modération (expulser/bannir) : propriétaire OU porteur des permissions correspondantes.
  const canKick = useStore((s) => !!guildId && canIn(s, guildId, PERM.KICK_MEMBERS));
  const canBan = useStore((s) => !!guildId && canIn(s, guildId, PERM.BAN_MEMBERS));
  // Gestion des rôles : porteur de MANAGE_ROLES (le propriétaire l'a via PERM_ALL).
  const canManageRoles = useStore((s) => !!guildId && canIn(s, guildId, PERM.MANAGE_ROLES));
  const guildRoles = useStore((s) => (guildId ? s.rolesByGuild[guildId] : undefined));
  const member = useStore((s) =>
    guildId ? s.membersByGuild[guildId]?.find((m) => m.user.id === userId) : undefined,
  );
  const myMember = useStore((s) =>
    guildId ? s.membersByGuild[guildId]?.find((m) => m.user.id === s.me?.id) : undefined,
  );
  const targetIsOwner = useStore(
    (s) => !!guildId && s.guilds.find((g) => g.id === guildId)?.owner_id === userId,
  );
  // Modération vocale : visible si la cible est connectée à un salon vocal de CETTE guilde.
  const targetVoice = useStore((s) =>
    guildId
      ? s.voiceStatesByGuild[guildId]?.find((v) => v.user_id === userId && v.channel_id)
      : undefined,
  );
  const canVoiceMute = useStore((s) => !!guildId && canIn(s, guildId, PERM.MUTE_MEMBERS));
  const canVoiceDeafen = useStore((s) => !!guildId && canIn(s, guildId, PERM.DEAFEN_MEMBERS));
  const canVoiceMove = useStore((s) => !!guildId && canIn(s, guildId, PERM.MOVE_MEMBERS));
  // RÈGLE : ne jamais retourner un tableau/objet NEUF depuis un sélecteur (boucle getSnapshot)
  // → on sélectionne la référence stable et on filtre en dehors.
  const guildChannels = useStore((s) => (guildId ? s.channelsByGuild[guildId] : undefined));
  const voiceChannels = (guildChannels ?? []).filter((c) => c.type === CH_VOICE);
  const [showMove, setShowMove] = useState(false);
  // Volume local du participant (réglage personnel, persisté).
  const userVolume = useStore((s) => s.userVolumes[userId]);
  const setUserVolume = useStore((s) => s.setUserVolume);

  // Rôles assignables (hors @everyone et rôles gérés). Position du rôle le plus élevé du viewer
  // → hiérarchie : on ne peut (dés)attribuer qu'un rôle STRICTEMENT sous le sien (∞ si propriétaire).
  const assignable = (guildRoles ?? []).filter((r) => !r.managed && r.id !== guildId);
  const myTopPos = isOwner
    ? Number.POSITIVE_INFINITY
    : (guildRoles ?? [])
        .filter((r) => r.id !== guildId && myMember?.roles.includes(r.id))
        .reduce((max, r) => Math.max(max, r.position), -1);
  const canAssign = (pos: number) => isOwner || pos < myTopPos;
  const heldRoles = assignable
    .filter((r) => member?.roles.includes(r.id))
    .sort((a, b) => b.position - a.position);
  const addableRoles = assignable
    .filter((r) => !member?.roles.includes(r.id) && canAssign(r.position))
    .sort((a, b) => b.position - a.position);
  const showRolesSection = heldRoles.length > 0 || (canManageRoles && addableRoles.length > 0);

  async function toggleRole(roleId: string, has: boolean) {
    if (!guildId) return;
    try {
      if (has) await api.removeMemberRole(guildId, userId, roleId);
      else await api.addMemberRole(guildId, userId, roleId);
      const members = await api.listMembers(guildId);
      useStore.setState((s) => ({ membersByGuild: { ...s.membersByGuild, [guildId]: members } }));
    } catch {
      /* ignore */
    }
  }

  async function refreshMembers() {
    if (!guildId) return;
    try {
      const members = await api.listMembers(guildId);
      useStore.setState((s) => ({ membersByGuild: { ...s.membersByGuild, [guildId]: members } }));
    } catch {
      /* ignore */
    }
  }

  const nickEditable = !!guildId && !!member && (me?.id === userId ? canChangeOwnNick : canManageNicks);

  async function saveNick() {
    if (!guildId) return;
    try {
      await api.updateMember(guildId, userId, { nick: nickDraft.trim() || null });
      await refreshMembers();
      setEditingNick(false);
    } catch {
      /* ignore */
    }
  }

  async function kick() {
    if (!guildId) return;
    try {
      await api.kickMember(guildId, userId);
      await refreshMembers();
    } catch {
      /* ignore */
    }
  }

  async function ban() {
    if (!guildId) return;
    try {
      await api.banMember(guildId, userId);
      await refreshMembers();
    } catch {
      /* ignore */
    }
  }

  async function onOpenChange(open: boolean) {
    controlledOnOpenChange?.(open); // propage au parent si le popover est piloté de l'extérieur
    if (open && !profile && !loading) {
      setLoading(true);
      try {
        setProfile(await api.userProfile(userId));
      } catch {
        /* ignore */
      } finally {
        setLoading(false);
      }
    }
    if (open && !noteLoaded && me?.id !== userId) {
      api
        .getNote(userId)
        .then((r) => {
          setNote(r.note ?? "");
          setNoteLoaded(true);
        })
        .catch(() => {});
    }
  }

  function saveNote() {
    if (!noteLoaded) return;
    void api.putNote(userId, note.trim()).catch(() => {});
  }

  // Modération vocale : mute/sourdine serveur, déplacement, déconnexion.
  async function moderateVoice(body: ModerateVoiceState) {
    if (!guildId) return;
    try {
      await api.moderateVoiceState(guildId, userId, body);
      // Reflet immédiat (la Gateway confirmera via VOICE_STATE_UPDATE).
      const states = await api.listVoiceStates(guildId);
      useStore.setState((s) => ({
        voiceStatesByGuild: { ...s.voiceStatesByGuild, [guildId]: states },
      }));
      setShowMove(false);
    } catch {
      /* ignore */
    }
  }

  // Exclusion temporaire (timeout) : fin = maintenant + durée ; 0 = lever immédiatement.
  async function timeout(seconds: number) {
    if (!guildId) return;
    try {
      await api.updateMember(guildId, userId, {
        communication_disabled_until: seconds > 0 ? Date.now() + seconds * 1000 : 1,
      });
      await refreshMembers();
    } catch {
      /* ignore */
    }
  }

  async function startDM() {
    try {
      const dm = await api.openDM({ recipients: [userId] });
      await refreshDMs();
      await openDM(dm.id);
    } catch {
      /* ignore */
    }
  }

  const name = profile?.display_name || profile?.username || "";
  const accent = profile?.accent_color ? roleColorHex(profile.accent_color) : colorFor(userId);

  return (
    <Popover.Root open={open} onOpenChange={(o) => void onOpenChange(o)}>
      <Popover.Trigger className="text-left outline-none">{children}</Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          side={side}
          align="start"
          sideOffset={8}
          className={`z-[60] w-[300px] overflow-hidden rounded-xl bg-floating shadow-pop ring-1 ring-cardline ${OVERLAY_ANIM}`}
        >
          {/* Bannière : image téléversée si présente, sinon dégradé doux (jamais un aplat brut). */}
          {profile?.banner_id ? (
            <img
              src={mediaUrl(`/api/users/${userId}/banner?v=${profile.banner_id}`)}
              alt=""
              className="h-[72px] w-full object-cover"
              draggable={false}
            />
          ) : (
            <div className="h-[72px]" style={{ background: `linear-gradient(150deg, ${accent}, #0c0c0e)` }} />
          )}
          <div className="px-4 pb-4">
            <div className="-mt-10 mb-2">
              <div
                className="flex h-20 w-20 items-center justify-center overflow-hidden rounded-full border-[6px] border-floating text-2xl font-semibold text-white"
                style={{ backgroundColor: colorFor(userId) }}
              >
                {profile?.avatar_id ? (
                  <img
                    src={mediaUrl(`/api/users/${userId}/avatar?v=${profile.avatar_id}`)}
                    alt=""
                    className="h-full w-full object-cover"
                    draggable={false}
                  />
                ) : (
                  initials(name || "?")
                )}
              </div>
            </div>

            {loading && !profile ? (
              <p className="text-sm text-muted">Chargement…</p>
            ) : profile ? (
              <div className="rounded-xl bg-deepest p-3 surface-card">
                <div className="flex items-center gap-1.5">
                  <span className="min-w-0 truncate text-lg font-bold text-header">
                    {member?.nick || name}
                  </span>
                  {nickEditable && !editingNick && (
                    <button
                      title="Changer le surnom de serveur"
                      onClick={() => {
                        setNickDraft(member?.nick ?? "");
                        setEditingNick(true);
                      }}
                      className="shrink-0 rounded p-1 text-muted opacity-70 hover:bg-hover hover:text-normal"
                    >
                      <Pencil size={13} />
                    </button>
                  )}
                </div>
                <div className="text-sm text-muted">@{profile.username}</div>
                {editingNick && (
                  <div className="animate-accordion mt-2 flex items-center gap-1.5">
                    <input
                      autoFocus
                      value={nickDraft}
                      onChange={(e) => setNickDraft(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") void saveNick();
                        if (e.key === "Escape") setEditingNick(false);
                      }}
                      maxLength={32}
                      placeholder="Surnom (vide = effacer)"
                      className="w-full rounded-lg bg-field px-2 py-1.5 text-sm text-normal outline-none ring-1 ring-transparent placeholder:text-muted focus:ring-accent"
                    />
                    <button
                      onClick={() => void saveNick()}
                      className="pressable shrink-0 rounded-lg btn-accent px-2.5 py-1.5 text-xs font-semibold text-white transition-colors"
                    >
                      OK
                    </button>
                  </div>
                )}

                {profile.pronouns && (
                  <div className="mt-1 text-xs text-muted">{profile.pronouns}</div>
                )}
                {profile.bio && (
                  <>
                    <Divider />
                    <div className="text-xs font-bold uppercase tracking-wide text-subtext">
                      À propos
                    </div>
                    <p className="mt-1 whitespace-pre-wrap break-words text-sm text-normal">
                      {profile.bio}
                    </p>
                  </>
                )}
                <Divider />
                <div className="text-xs font-bold uppercase tracking-wide text-subtext">
                  Membre depuis
                </div>
                <p className="mt-1 text-sm text-normal">
                  {new Date(profile.created_at).toLocaleDateString("fr-FR", {
                    day: "numeric",
                    month: "long",
                    year: "numeric",
                  })}
                </p>

                {me?.id !== userId && (
                  <Popover.Close asChild>
                    <button
                      onClick={() => void startDM()}
                      className="pressable mt-3 flex w-full items-center justify-center gap-2 rounded-lg btn-accent py-2 text-sm font-medium text-white transition-colors"
                    >
                      <MessageCircle size={16} />
                      Message
                    </button>
                  </Popover.Close>
                )}

                {showRolesSection && (
                  <div className="mt-3">
                    <div className="mb-1 text-xs font-bold uppercase tracking-wide text-subtext">
                      Rôles
                    </div>
                    <div className="flex flex-wrap items-center gap-1">
                      {heldRoles.map((r) => {
                        const c = r.color ? roleColorHex(r.color) : "#99aab5";
                        const removable = canManageRoles && canAssign(r.position);
                        return (
                          <span
                            key={r.id}
                            className="group/role flex items-center gap-1.5 rounded-full bg-deepest px-2 py-0.5 text-xs text-normal"
                          >
                            {removable ? (
                              <button
                                onClick={() => void toggleRole(r.id, true)}
                                title={`Retirer ${r.name}`}
                                className="pressable flex h-3 w-3 items-center justify-center rounded-full transition-colors group-hover/role:bg-dnd"
                                style={{ backgroundColor: c }}
                              >
                                <X size={9} className="text-white opacity-0 group-hover/role:opacity-100" strokeWidth={3} />
                              </button>
                            ) : (
                              <span className="h-2.5 w-2.5 rounded-full" style={{ backgroundColor: c }} />
                            )}
                            <span className="max-w-[140px] truncate">{r.name}</span>
                          </span>
                        );
                      })}
                      {heldRoles.length === 0 && !showAdd && (
                        <span className="text-xs text-muted">Aucun rôle.</span>
                      )}
                      {canManageRoles && addableRoles.length > 0 && (
                        <button
                          onClick={() => setShowAdd((v) => !v)}
                          title="Ajouter un rôle"
                          className="pressable flex h-[22px] w-[22px] items-center justify-center rounded-full border border-line text-muted transition-colors hover:bg-hover hover:text-normal"
                        >
                          <Plus size={13} />
                        </button>
                      )}
                    </div>
                    {showAdd && canManageRoles && (
                      <div className="animate-accordion mt-1.5 max-h-32 overflow-y-auto rounded-lg border border-line bg-deepest p-1 scroll-thin">
                        {addableRoles.length === 0 ? (
                          <p className="px-2 py-1 text-xs text-muted">Aucun rôle à ajouter.</p>
                        ) : (
                          addableRoles.map((r) => {
                            const c = r.color ? roleColorHex(r.color) : "#99aab5";
                            return (
                              <button
                                key={r.id}
                                onClick={() => void toggleRole(r.id, false)}
                                className="flex w-full items-center gap-2 rounded px-2 py-1 text-left text-xs text-normal hover:bg-hover"
                              >
                                <span className="h-2.5 w-2.5 shrink-0 rounded-full" style={{ backgroundColor: c }} />
                                <span className="truncate">{r.name}</span>
                              </button>
                            );
                          })
                        )}
                      </div>
                    )}
                  </div>
                )}

                {me?.id !== userId && (
                  <div className="mt-3">
                    <div className="mb-1 text-xs font-bold uppercase tracking-wide text-subtext">
                      Note
                    </div>
                    <textarea
                      value={note}
                      onChange={(e) => setNote(e.target.value.slice(0, 256))}
                      onBlur={saveNote}
                      rows={2}
                      placeholder="Ajouter une note (visible par toi seul)"
                      className="w-full resize-none rounded-lg bg-field px-2 py-1.5 text-xs text-normal outline-none ring-1 ring-transparent placeholder:text-muted focus:ring-accent"
                    />
                  </div>
                )}

                {me?.id !== userId && !targetIsOwner && canModerate && member && (
                  <div className="mt-2">
                    <div className="mb-1 text-xs font-bold uppercase tracking-wide text-subtext">
                      Exclure temporairement
                    </div>
                    <div className="flex flex-wrap gap-1">
                      {[
                        { s: 60, l: "60 s" },
                        { s: 300, l: "5 min" },
                        { s: 3600, l: "1 h" },
                        { s: 86400, l: "1 j" },
                      ].map((o) => (
                        <button
                          key={o.s}
                          onClick={() => void timeout(o.s)}
                          className="pressable rounded-full bg-active px-2.5 py-1 text-xs text-normal ring-1 ring-line transition-colors hover:bg-dnd/20 hover:text-dnd"
                        >
                          {o.l}
                        </button>
                      ))}
                      <button
                        onClick={() => void timeout(0)}
                        title="Lever l'exclusion"
                        className="pressable rounded-full bg-active px-2.5 py-1 text-xs text-online ring-1 ring-line transition-colors hover:bg-online hover:text-white"
                      >
                        Lever
                      </button>
                    </div>
                  </div>
                )}

                {me?.id !== userId && targetVoice && (
                  <div className="mt-2">
                    <div className="mb-1 flex items-center justify-between text-xs font-bold uppercase tracking-wide text-subtext">
                      <span>Volume utilisateur</span>
                      <span className="font-normal normal-case text-muted">
                        {Math.round((userVolume ?? 1) * 100)} %
                      </span>
                    </div>
                    <input
                      type="range"
                      min={0}
                      max={100}
                      value={Math.round((userVolume ?? 1) * 100)}
                      onChange={(e) => setUserVolume(userId, Number(e.target.value) / 100)}
                      className="w-full accent-[var(--accent)]"
                    />
                  </div>
                )}

                {me?.id !== userId &&
                  !targetIsOwner &&
                  targetVoice &&
                  (canVoiceMute || canVoiceDeafen || canVoiceMove) && (
                    <div className="mt-2">
                      <div className="mb-1 text-xs font-bold uppercase tracking-wide text-subtext">
                        Modération vocale
                      </div>
                      <div className="flex flex-wrap gap-1">
                        {canVoiceMute && (
                          <button
                            onClick={() => void moderateVoice({ mute: !targetVoice.mute })}
                            className={`pressable flex items-center gap-1 rounded-full px-2.5 py-1 text-xs ring-1 ring-line transition-colors ${
                              targetVoice.mute
                                ? "bg-dnd/20 text-dnd hover:bg-active hover:text-normal"
                                : "bg-active text-normal hover:bg-dnd/20 hover:text-dnd"
                            }`}
                          >
                            <MicOff size={12} />
                            {targetVoice.mute ? "Démuter" : "Muet serveur"}
                          </button>
                        )}
                        {canVoiceDeafen && (
                          <button
                            onClick={() => void moderateVoice({ deaf: !targetVoice.deaf })}
                            className={`pressable flex items-center gap-1 rounded-full px-2.5 py-1 text-xs ring-1 ring-line transition-colors ${
                              targetVoice.deaf
                                ? "bg-dnd/20 text-dnd hover:bg-active hover:text-normal"
                                : "bg-active text-normal hover:bg-dnd/20 hover:text-dnd"
                            }`}
                          >
                            <HeadphoneOff size={12} />
                            {targetVoice.deaf ? "Rendre l'audio" : "Sourdine serveur"}
                          </button>
                        )}
                        {canVoiceMove && voiceChannels.length > 1 && (
                          <button
                            onClick={() => setShowMove((v) => !v)}
                            className="pressable flex items-center gap-1 rounded-full bg-active px-2.5 py-1 text-xs text-normal ring-1 ring-line transition-colors hover:bg-hover"
                          >
                            <ArrowRightLeft size={12} />
                            Déplacer
                          </button>
                        )}
                        {canVoiceMove && (
                          <button
                            onClick={() => void moderateVoice({ disconnect: true })}
                            className="pressable flex items-center gap-1 rounded-full bg-active px-2.5 py-1 text-xs text-normal ring-1 ring-line transition-colors hover:bg-dnd/20 hover:text-dnd"
                          >
                            <PhoneOff size={12} />
                            Déconnecter
                          </button>
                        )}
                      </div>
                      {showMove && canVoiceMove && (
                        <div className="animate-accordion mt-1.5 max-h-32 overflow-y-auto rounded-lg border border-line bg-deepest p-1 scroll-thin">
                          {voiceChannels
                            .filter((c) => c.id !== targetVoice.channel_id)
                            .map((c) => (
                              <button
                                key={c.id}
                                onClick={() => void moderateVoice({ channel_id: c.id })}
                                className="flex w-full items-center gap-2 rounded px-2 py-1 text-left text-xs text-normal hover:bg-hover"
                              >
                                <ArrowRightLeft size={11} className="shrink-0 text-muted" />
                                <span className="truncate">{c.name}</span>
                              </button>
                            ))}
                        </div>
                      )}
                    </div>
                  )}

                {me?.id !== userId && !targetIsOwner && (canKick || canBan) && (
                  <div className="mt-2 flex gap-2">
                    {canKick && (
                      <Popover.Close asChild>
                        <button
                          onClick={() => void kick()}
                          className="pressable flex flex-1 items-center justify-center gap-1.5 rounded-lg border border-dnd py-1.5 text-xs font-medium text-dnd transition-colors hover:bg-dnd hover:text-white"
                        >
                          <UserMinus size={14} />
                          Expulser
                        </button>
                      </Popover.Close>
                    )}
                    {canBan && (
                      <Popover.Close asChild>
                        <button
                          onClick={() => void ban()}
                          className="pressable flex flex-1 items-center justify-center gap-1.5 rounded-lg bg-dnd py-1.5 text-xs font-medium text-white transition-colors hover:opacity-90"
                        >
                          <Ban size={14} />
                          Bannir
                        </button>
                      </Popover.Close>
                    )}
                  </div>
                )}
              </div>
            ) : (
              <p className="text-sm text-muted">Profil indisponible.</p>
            )}
          </div>
          <Popover.Arrow className="fill-floating" />
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

function Divider() {
  return <div className="my-2 h-px bg-white/10" />;
}

// Avatar cliquable ouvrant le profil (raccourci pratique).
export function ProfileAvatar({
  userId,
  name,
  size = 40,
  ring,
  status,
  avatarId,
}: {
  userId: string;
  name: string;
  size?: number;
  ring?: string;
  status?: string | null;
  avatarId?: string | null;
}) {
  return (
    <UserPopover userId={userId}>
      <Avatar name={name} id={userId} size={size} ring={ring} status={status} avatarId={avatarId} />
    </UserPopover>
  );
}
