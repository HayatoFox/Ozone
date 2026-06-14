import { useEffect, useRef, useState } from "react";
import * as Popover from "@radix-ui/react-popover";
import * as CM from "@radix-ui/react-context-menu";
import {
  Bell,
  BellOff,
  CalendarDays,
  ChevronDown,
  Folder,
  FolderPlus,
  Hash,
  HeadphoneOff,
  Headphones,
  Lock,
  LogOut,
  MessagesSquare,
  Mic,
  MicOff,
  Pencil,
  PhoneOff,
  Plus,
  Settings as SettingsIcon,
  Trash2,
  UserPlus,
  Users,
  Video,
  VideoOff,
  Volume2,
  VolumeX,
} from "lucide-react";
import { api } from "../api";
import {
  canIn,
  channelTree,
  isChannelUnread,
  isMuted,
  permsIn,
  reorderChannelPlan,
  roleColorHex,
  sortChannels,
  useStore,
} from "../store";
import { PERM } from "../lib/permissions";
import { CH_CATEGORY, CH_VOICE, type Channel, type DMChannel, type Member, type VoiceState } from "../types";
import { displayName } from "../lib/format";
import { mediaUrl } from "../lib/instance";
import { OVERLAY_ANIM } from "../lib/anim";
import { Avatar } from "./Avatar";
import { UserPanel } from "./UserPanel";
import { Modal } from "./ServerRail";
import { InviteModal } from "./InviteModal";
import { EventsModal } from "./EventsModal";
import { WebhooksModal } from "./WebhooksModal";
import { ServerSettings } from "./ServerSettings";
import { ChannelSettings } from "./ChannelSettings";
import { ChannelContextMenu } from "./ui/ChannelContextMenu";
import { VoiceMemberContextMenu } from "./ui/VoiceMemberContextMenu";
import { UserPopover } from "./ProfilePopout";
import { Spinner } from "./ui/Spinner";

export function ChannelSidebar() {
  const view = useStore((s) => s.view);
  return (
    <div className="flex w-60 shrink-0 flex-col border-r border-line bg-sidebar">
      {/* `flex flex-col` : permet au remplisseur de fond (clic droit → créer un salon) de
          s'étendre jusqu'en bas de la colonne. */}
      <div className="flex flex-1 flex-col overflow-y-auto scroll-thin">
        {view.kind === "guild" ? (
          <GuildChannels key={view.guildId} guildId={view.guildId} />
        ) : (
          <HomeSidebar />
        )}
      </div>
      <VoiceBar />
      <UserPanel />
    </div>
  );
}

function VoiceBar() {
  const myVoice = useStore((s) => s.myVoice);
  const connecting = useStore((s) => s.voiceConnecting);
  const channelName = useStore((s) =>
    myVoice ? s.channelsByGuild[myVoice.guildId]?.find((c) => c.id === myVoice.channelId)?.name : undefined,
  );
  const leave = useStore((s) => s.leaveVoiceChannel);
  const toggleMute = useStore((s) => s.toggleSelfMute);
  const toggleDeaf = useStore((s) => s.toggleSelfDeaf);
  const toggleVideo = useStore((s) => s.toggleSelfVideo);
  if (!myVoice) return null;
  const muted = myVoice.selfMute || myVoice.serverMute;
  const deaf = myVoice.selfDeaf || myVoice.serverDeaf;
  return (
    // Barre vocale compacte au-dessus du panneau utilisateur (façon Discord) : statut sur une
    // ligne dense, puis une rangée d'actions fine. On évite la hauteur excessive.
    <div className="mx-2 mt-2 animate-accordion rounded-lg bg-deepest/60 px-2.5 py-1.5 ring-1 ring-line">
      {/* Statut : icône + « Vocal connecté » + salon, le tout sur UNE ligne. */}
      <div className="flex items-center gap-1.5">
        {connecting ? (
          <Spinner size={13} />
        ) : (
          <Volume2 size={15} className="shrink-0 text-online" />
        )}
        <span className="shrink-0 text-[13px] font-semibold text-online">
          {connecting ? "Connexion…" : "Vocal connecté"}
        </span>
        <span className="ml-1 min-w-0 flex-1 truncate text-[11px] text-muted">
          {channelName ?? "Salon vocal"}
        </span>
      </div>
      {/* Actions : rangée fine, boutons serrés. Raccrocher légèrement détaché à droite. */}
      <div className="mt-1 flex items-center gap-0.5">
        <VoiceBtn
          onClick={() => void toggleMute()}
          active={muted}
          title={
            myVoice.serverMute
              ? "Rendu muet par le serveur"
              : myVoice.selfMute
                ? "Réactiver le micro"
                : "Couper le micro"
          }
        >
          {muted ? <MicOff size={16} /> : <Mic size={16} />}
        </VoiceBtn>
        <VoiceBtn
          onClick={() => void toggleDeaf()}
          active={deaf}
          title={
            myVoice.serverDeaf
              ? "Mis en sourdine par le serveur"
              : myVoice.selfDeaf
                ? "Réactiver le son"
                : "Se rendre sourd"
          }
        >
          {deaf ? <HeadphoneOff size={16} /> : <Headphones size={16} />}
        </VoiceBtn>
        <VoiceBtn
          onClick={() => void toggleVideo()}
          disabled={connecting}
          on={myVoice.selfVideo}
          title={myVoice.selfVideo ? "Couper la caméra" : "Activer la caméra"}
        >
          {myVoice.selfVideo ? <Video size={16} /> : <VideoOff size={16} />}
        </VoiceBtn>
        <button
          onClick={() => void leave()}
          title="Se déconnecter"
          className="pressable ml-auto rounded-md p-1.5 text-interactive-normal transition-colors hover:bg-dnd/20 hover:text-dnd"
        >
          <PhoneOff size={16} />
        </button>
      </div>
    </div>
  );
}

// Bouton d'action compact de la barre vocale. `active` (rouge) = mute/sourdine ; `on` (vert) = cam.
function VoiceBtn({
  children,
  onClick,
  title,
  active,
  on,
  disabled,
}: {
  children: React.ReactNode;
  onClick: () => void;
  title: string;
  active?: boolean;
  on?: boolean;
  disabled?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      title={title}
      className={`pressable flex flex-1 items-center justify-center rounded-md py-1.5 transition-colors hover:bg-white/5 disabled:opacity-40 ${
        active ? "text-dnd" : on ? "text-online" : "text-interactive-normal"
      }`}
    >
      {children}
    </button>
  );
}

// ───────────────────────────── Guilde ─────────────────────────────

function GuildChannels({ guildId }: { guildId: string }) {
  const guild = useStore((s) => s.guilds.find((g) => g.id === guildId));
  const channels = useStore((s) => s.channelsByGuild[guildId]) ?? [];
  const selected = useStore((s) => s.selectedChannelByGuild[guildId]);
  const readStates = useStore((s) => s.readStates);
  const notif = useStore((s) => s.notif);
  const threadsByChannel = useStore((s) => s.threadsByChannel);
  const voiceStates = useStore((s) => s.voiceStatesByGuild[guildId]);
  const members = useStore((s) => s.membersByGuild[guildId]);
  const selectChannel = useStore((s) => s.selectChannel);
  const viewChannel = useStore((s) => s.viewChannel);
  const joinVoice = useStore((s) => s.joinVoice);
  const setVoiceTextOpen = useStore((s) => s.setVoiceTextOpen);
  const refreshGuilds = useStore((s) => s.refreshGuilds);
  const selectHome = useStore((s) => s.selectHome);
  const markRead = useStore((s) => s.markRead);
  const setMute = useStore((s) => s.setMute);
  const guildMuted = isMuted(notif, 0, guildId);
  const me = useStore((s) => s.me);
  const [createState, setCreateState] = useState<{ parentId: string | null; forceCategory?: boolean } | null>(null);
  const [inviting, setInviting] = useState(false);
  const [editing, setEditing] = useState<Channel | null>(null);
  const [webhooksChannel, setWebhooksChannel] = useState<Channel | null>(null);
  const [guildSettings, setGuildSettings] = useState(false);
  const [eventsOpen, setEventsOpen] = useState(false);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  // Glisser-déposer des salons (réordonnancement + déplacement entre catégories).
  const dragId = useRef<string | null>(null);
  const [over, setOver] = useState<{ id: string; mode: "before" | "into" | "root" } | null>(null);
  const isOwner = !!guild && guild.owner_id === me?.id;
  // Permissions de l'utilisateur courant : le menu n'affiche que les actions autorisées.
  const canInvite = useStore((s) => canIn(s, guildId, PERM.CREATE_INSTANT_INVITE));
  const canManageChannels = useStore((s) => canIn(s, guildId, PERM.MANAGE_CHANNELS));
  const canManageWebhooks = useStore((s) => canIn(s, guildId, PERM.MANAGE_WEBHOOKS));
  const canCreateEvents = useStore((s) => canIn(s, guildId, PERM.CREATE_EVENTS));
  const canOpenSettings = useStore((s) => {
    const p = permsIn(s, guildId);
    return [
      PERM.MANAGE_GUILD,
      PERM.MANAGE_ROLES,
      PERM.BAN_MEMBERS,
      PERM.VIEW_AUDIT_LOG,
      PERM.MANAGE_WEBHOOKS,
      PERM.MANAGE_EXPRESSIONS,
    ].some((b) => (p & b) === b);
  });

  // Bannière du serveur : image téléversée prioritaire, sinon dégradé de couleur vers le bas.
  const bannerImg = guild?.banner_id ? mediaUrl(`/api/guilds/${guildId}/banner?v=${guild.banner_id}`) : null;
  const hasBanner = !!(guild?.banner_id || guild?.banner_color != null);
  const bannerStyle: React.CSSProperties = bannerImg
    ? { backgroundImage: `url(${bannerImg})`, backgroundSize: "cover", backgroundPosition: "center" }
    : { background: `linear-gradient(180deg, ${roleColorHex(guild?.banner_color ?? 0x4e5058)}, #0c0c0e)` };

  const tree = channelTree(channels);

  const toggleCategory = (id: string) =>
    setCollapsed((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  async function leave() {
    try {
      await api.leaveGuild(guildId);
      await refreshGuilds();
      await selectHome();
    } catch {
      /* ignore */
    }
  }

  const draggedIsCat = () => {
    const id = dragId.current;
    return !!id && channels.find((c) => c.id === id)?.type === CH_CATEGORY;
  };

  // Applique un glisser-déposer : recalcul des positions/parents, mise à jour optimiste + serveur.
  function applyReorder(target: { id: string; mode: "before" | "into" | "root" }) {
    const d = dragId.current;
    dragId.current = null;
    setOver(null);
    if (!canManageChannels || !d || d === target.id) return;
    const plan = reorderChannelPlan(channels, d, target);
    if (!plan.length) return;
    useStore.setState((s) => {
      const map = new Map(plan.map((p) => [p.id, p]));
      const next = (s.channelsByGuild[guildId] ?? []).map((c) => {
        const p = map.get(c.id);
        return p ? { ...c, position: p.position, parent_id: p.parent_id } : c;
      });
      return { channelsByGuild: { ...s.channelsByGuild, [guildId]: sortChannels(next) } };
    });
    void api.reorderChannels(guildId, plan).catch(async () => {
      // En cas d'échec : resynchronise depuis le serveur.
      try {
        const chs = await api.listChannels(guildId);
        useStore.setState((s) => ({
          channelsByGuild: { ...s.channelsByGuild, [guildId]: sortChannels(chs) },
        }));
      } catch {
        /* ignore */
      }
    });
  }

  return (
    <>
      <Popover.Root>
        <Popover.Trigger
          className={`group relative w-full overflow-hidden border-b border-line text-left outline-none ${
            hasBanner
              ? "h-[124px]"
              : "flex h-12 items-center justify-between px-4 shadow-sm hover:bg-hover"
          }`}
        >
          {hasBanner ? (
            <>
              {/* Bannière (image ou dégradé) */}
              <div className="absolute inset-0" style={bannerStyle} />
              {/* Pastille « ovale » nom + flèche (haut-gauche) — adoucit le rendu. Flou LÉGER au
                  repos (lisible même sur fond clair), FORT au survol de la pastille uniquement. */}
              <div className="absolute left-3 top-3 inline-flex max-w-[calc(100%-1.5rem)] items-center gap-1 rounded-full bg-black/35 px-3 py-1.5 shadow-sm backdrop-blur-[2px] transition-all duration-200 hover:bg-black/55 hover:backdrop-blur-md">
                <h1 className="truncate text-[15px] font-semibold text-white drop-shadow-sm">
                  {guild?.name ?? "Guilde"}
                </h1>
                <ChevronDown
                  size={16}
                  className="shrink-0 text-white/90 transition-transform group-data-[state=open]:rotate-180"
                />
              </div>
              {/* Ancre le menu juste sous la pastille du nom. */}
              <Popover.Anchor asChild>
                <span className="pointer-events-none absolute left-3 top-[44px]" />
              </Popover.Anchor>
            </>
          ) : (
            <>
              <h1 className="truncate font-semibold text-header">{guild?.name ?? "Guilde"}</h1>
              <ChevronDown size={18} className="shrink-0 text-interactive-normal" />
            </>
          )}
        </Popover.Trigger>
        <Popover.Portal>
          <Popover.Content
            align="start"
            sideOffset={6}
            className={`z-[60] w-[220px] rounded-xl bg-floating p-1.5 shadow-pop ring-1 ring-line ${OVERLAY_ANIM}`}
          >
            {canInvite && (
              <Popover.Close
                onClick={() => setInviting(true)}
                className="flex w-full items-center justify-between rounded px-2 py-1.5 text-sm text-accent outline-none hover:bg-accent hover:text-white"
              >
                Inviter sur le serveur <UserPlus size={16} />
              </Popover.Close>
            )}
            {canOpenSettings && (
              <Popover.Close
                onClick={() => setGuildSettings(true)}
                className="flex w-full items-center justify-between rounded px-2 py-1.5 text-sm text-normal outline-none hover:bg-accent hover:text-white"
              >
                Paramètres du serveur <SettingsIcon size={16} />
              </Popover.Close>
            )}
            {canManageChannels && (
              <Popover.Close
                onClick={() => setCreateState({ parentId: null })}
                className="flex w-full items-center justify-between rounded px-2 py-1.5 text-sm text-normal outline-none hover:bg-accent hover:text-white"
              >
                Créer un salon <Plus size={16} />
              </Popover.Close>
            )}
            {canManageChannels && (
              <Popover.Close
                onClick={() => setCreateState({ parentId: null, forceCategory: true })}
                className="flex w-full items-center justify-between rounded px-2 py-1.5 text-sm text-normal outline-none hover:bg-accent hover:text-white"
              >
                Créer une catégorie <FolderPlus size={16} />
              </Popover.Close>
            )}
            {canCreateEvents && (
              <Popover.Close
                onClick={() => setEventsOpen(true)}
                className="flex w-full items-center justify-between rounded px-2 py-1.5 text-sm text-normal outline-none hover:bg-accent hover:text-white"
              >
                Créer un événement <CalendarDays size={16} />
              </Popover.Close>
            )}
            {(canInvite || canOpenSettings || canManageChannels || canCreateEvents) && (
              <div className="my-1 h-px bg-white/10" />
            )}
            <Popover.Close
              onClick={() => void setMute(0, guildId, !guildMuted)}
              className="flex w-full items-center justify-between rounded px-2 py-1.5 text-sm text-normal outline-none hover:bg-accent hover:text-white"
            >
              {guildMuted ? "Réactiver le serveur" : "Mettre en sourdine"}
              {guildMuted ? <Bell size={16} /> : <BellOff size={16} />}
            </Popover.Close>
            {!isOwner && (
              <>
                <div className="my-1 h-px bg-white/10" />
                <Popover.Close
                  onClick={() => void leave()}
                  className="flex w-full items-center justify-between rounded px-2 py-1.5 text-sm text-dnd outline-none hover:bg-dnd hover:text-white"
                >
                  Quitter le serveur <LogOut size={16} />
                </Popover.Close>
              </>
            )}
          </Popover.Content>
        </Popover.Portal>
      </Popover.Root>

      <div className="flex flex-1 flex-col px-2 py-3">
        {tree.map((group, i) => {
          const cat = group.category;
          const catId = cat?.id;
          const isCollapsed = catId ? collapsed.has(catId) : false;
          const intoActive = over?.id === (catId ?? "") && over.mode === "into";
          return (
            <div
              key={catId ?? `u${i}`}
              className={`mb-3 rounded-lg ${intoActive ? "bg-accent/10 ring-1 ring-accent/40" : ""}`}
              onDragOver={(e) => {
                if (!dragId.current) return;
                e.preventDefault();
                setOver({ id: catId ?? "", mode: cat ? (draggedIsCat() ? "before" : "into") : "root" });
              }}
              onDrop={(e) => {
                e.preventDefault();
                applyReorder({ id: catId ?? "", mode: cat ? (draggedIsCat() ? "before" : "into") : "root" });
              }}
            >
              {cat && (
                <CM.Root>
                  <CM.Trigger asChild>
                    <button
                      draggable={canManageChannels}
                      onDragStart={(e) => {
                        dragId.current = cat.id;
                        e.dataTransfer.effectAllowed = "move";
                      }}
                      onDragEnd={() => {
                        dragId.current = null;
                        setOver(null);
                      }}
                      onDragOver={(e) => {
                        if (!dragId.current || dragId.current === cat.id) return;
                        e.preventDefault();
                        e.stopPropagation();
                        setOver({ id: cat.id, mode: draggedIsCat() ? "before" : "into" });
                      }}
                      onDrop={(e) => {
                        e.preventDefault();
                        e.stopPropagation();
                        applyReorder({ id: cat.id, mode: draggedIsCat() ? "before" : "into" });
                      }}
                      onClick={() => toggleCategory(cat.id)}
                      className="group/cat relative flex w-full items-center gap-0.5 px-1 pb-1 pt-0.5 text-xs font-semibold uppercase tracking-wide text-channel hover:text-interactive-hover"
                    >
                      {over?.id === cat.id && over.mode === "before" && (
                        <span className="pointer-events-none absolute -top-px left-1 right-1 h-0.5 rounded-full bg-accent" />
                      )}
                      <ChevronDown
                        size={12}
                        className={`shrink-0 transition-transform ${isCollapsed ? "-rotate-90" : ""}`}
                      />
                      <span className="truncate">{cat.name}</span>
                      {canManageChannels && (
                        <span
                          role="button"
                          tabIndex={-1}
                          onClick={(e) => {
                            e.stopPropagation();
                            setCreateState({ parentId: cat.id });
                          }}
                          title="Créer un salon"
                          className="ml-auto opacity-0 transition-opacity hover:text-interactive-hover group-hover/cat:opacity-100"
                        >
                          <Plus size={15} />
                        </span>
                      )}
                    </button>
                  </CM.Trigger>
                  <CM.Portal>
                    <CM.Content
                      className={`z-[60] min-w-[200px] rounded-xl bg-floating p-1.5 shadow-pop ring-1 ring-line ${OVERLAY_ANIM}`}
                    >
                      <CatItem onSelect={() => toggleCategory(cat.id)}>
                        {isCollapsed ? "Étendre la catégorie" : "Réduire la catégorie"}
                      </CatItem>
                      {canManageChannels && (
                        <>
                          <CM.Separator className="my-1 h-px bg-white/10" />
                          <CatItem icon={<Plus size={15} />} onSelect={() => setCreateState({ parentId: cat.id })}>
                            Créer un salon
                          </CatItem>
                          <CatItem icon={<Pencil size={15} />} onSelect={() => setEditing(cat)}>
                            Modifier la catégorie
                          </CatItem>
                          <CM.Separator className="my-1 h-px bg-white/10" />
                          <CatItem
                            danger
                            icon={<Trash2 size={15} />}
                            onSelect={() => void api.deleteChannel(cat.id).catch(() => {})}
                          >
                            Supprimer la catégorie
                          </CatItem>
                        </>
                      )}
                    </CM.Content>
                  </CM.Portal>
                </CM.Root>
              )}
              {group.items
                .filter(
                  (c) =>
                    !isCollapsed ||
                    c.id === selected ||
                    isChannelUnread(c.last_message_id, readStates[c.id]),
                )
                .map((c) => (
                  <div key={c.id}>
                    <div
                      draggable={canManageChannels}
                      onDragStart={(e) => {
                        dragId.current = c.id;
                        e.dataTransfer.effectAllowed = "move";
                      }}
                      onDragEnd={() => {
                        dragId.current = null;
                        setOver(null);
                      }}
                      onDragOver={(e) => {
                        if (!dragId.current || dragId.current === c.id) return;
                        e.preventDefault();
                        e.stopPropagation();
                        setOver({ id: c.id, mode: "before" });
                      }}
                      onDrop={(e) => {
                        e.preventDefault();
                        e.stopPropagation();
                        applyReorder({ id: c.id, mode: "before" });
                      }}
                      className="relative"
                    >
                      {over?.id === c.id && over.mode === "before" && (
                        <span className="pointer-events-none absolute -top-px left-1 right-1 z-10 h-0.5 rounded-full bg-accent" />
                      )}
                      <ChannelContextMenu
                        channel={c}
                        muted={isMuted(notif, 1, c.id)}
                        canManage={canManageChannels}
                        canWebhooks={canManageWebhooks}
                        onMarkRead={() => markRead(c.id)}
                        onToggleMute={() => void setMute(1, c.id, !isMuted(notif, 1, c.id))}
                        onEdit={setEditing}
                        onWebhooks={setWebhooksChannel}
                        onDelete={(ch) => void api.deleteChannel(ch.id).catch(() => {})}
                      >
                        <ChannelRow
                          channel={c}
                          active={c.id === selected}
                          muted={isMuted(notif, 1, c.id)}
                          unread={c.id !== selected && isChannelUnread(c.last_message_id, readStates[c.id])}
                          mentions={readStates[c.id]?.mention_count ?? 0}
                          onClick={() => {
                            if (c.type === CH_VOICE) {
                              viewChannel(c.id);
                              void joinVoice(guildId, c.id);
                            } else {
                              void selectChannel(c.id);
                            }
                          }}
                          onOpenText={
                            c.type === CH_VOICE
                              ? () => {
                                  viewChannel(c.id);
                                  setVoiceTextOpen(true);
                                }
                              : undefined
                          }
                        />
                      </ChannelContextMenu>
                    </div>
                    {/* Fils actifs uniquement (les archivés restent accessibles via leur lien
                        ou en réécrivant ; on n'encombre pas la liste). */}
                    {(threadsByChannel[c.id] ?? [])
                      .filter((t) => !t.archived || t.id === selected)
                      .map((t) => (
                        <ThreadRow
                          key={t.id}
                          name={t.name}
                          active={t.id === selected}
                          locked={!!t.locked}
                          onClick={() => void selectChannel(t.id)}
                        />
                      ))}
                    {c.type === CH_VOICE &&
                      (voiceStates ?? [])
                        .filter((v) => v.channel_id === c.id)
                        .map((v) => (
                          <VoiceMember
                            key={v.user_id}
                            state={v}
                            member={members?.find((m) => m.user.id === v.user_id)}
                            guildId={guildId}
                          />
                        ))}
                  </div>
                ))}
            </div>
          );
        })}

        {/* Zone vide sous les salons : clic droit → menu de création (si permission).
            Sans permission, simple remplisseur (le menu navigateur est supprimé globalement). */}
        {canManageChannels ? (
          <CM.Root>
            <CM.Trigger asChild>
              <div className="min-h-8 flex-1" />
            </CM.Trigger>
            <CM.Portal>
              <CM.Content
                className={`z-[60] min-w-[200px] rounded-xl bg-floating p-1.5 shadow-pop ring-1 ring-line ${OVERLAY_ANIM}`}
              >
                <CatItem icon={<Plus size={15} />} onSelect={() => setCreateState({ parentId: null })}>
                  Créer un salon
                </CatItem>
                <CatItem
                  icon={<FolderPlus size={15} />}
                  onSelect={() => setCreateState({ parentId: null, forceCategory: true })}
                >
                  Créer une catégorie
                </CatItem>
              </CM.Content>
            </CM.Portal>
          </CM.Root>
        ) : (
          <div className="min-h-8 flex-1" />
        )}
      </div>

      {createState && (
        <CreateChannelDialog
          guildId={guildId}
          parentId={createState.parentId}
          forceCategory={createState.forceCategory}
          categories={channels.filter((c) => c.type === CH_CATEGORY)}
          onClose={() => setCreateState(null)}
        />
      )}
      {inviting && <InviteModal guildId={guildId} onClose={() => setInviting(false)} />}
      {editing && (
        <ChannelSettings channelId={editing.id} guildId={guildId} onClose={() => setEditing(null)} />
      )}
      {webhooksChannel && (
        <WebhooksModal
          channelId={webhooksChannel.id}
          channelName={webhooksChannel.name}
          onClose={() => setWebhooksChannel(null)}
        />
      )}
      {guildSettings && <ServerSettings guildId={guildId} onClose={() => setGuildSettings(false)} />}
      {eventsOpen && <EventsModal guildId={guildId} onClose={() => setEventsOpen(false)} />}
    </>
  );
}

// Entrée du menu contextuel d'une catégorie.
function CatItem({
  children,
  onSelect,
  icon,
  danger,
}: {
  children: React.ReactNode;
  onSelect: () => void;
  icon?: React.ReactNode;
  danger?: boolean;
}) {
  return (
    <CM.Item
      onSelect={onSelect}
      className={`flex cursor-pointer items-center justify-between gap-2 rounded px-2 py-1.5 text-sm outline-none ${
        danger
          ? "text-dnd data-[highlighted]:bg-dnd data-[highlighted]:text-white"
          : "text-normal data-[highlighted]:bg-accent data-[highlighted]:text-white"
      }`}
    >
      <span className="truncate">{children}</span>
      {icon && <span className="shrink-0">{icon}</span>}
    </CM.Item>
  );
}

function ChannelRow({
  channel,
  active,
  unread,
  mentions,
  muted,
  onClick,
  onOpenText,
}: {
  channel: Channel;
  active: boolean;
  unread: boolean;
  mentions: number;
  muted: boolean;
  onClick: () => void;
  onOpenText?: () => void; // salons vocaux : ouvrir la discussion textuelle intégrée
}) {
  const voice = channel.type === CH_VOICE;
  const Icon = voice ? Volume2 : Hash;
  const showUnread = unread && !active && !muted;
  return (
    <div className={`relative transition-opacity ${muted && !active ? "opacity-50" : ""}`}>
      {/* Pastille de bord gauche : barre accent qui grandit selon l'état (actif > non-lu > repos),
          transition fluide façon Discord. */}
      <span
        className={`absolute -left-2 top-1/2 w-1 -translate-y-1/2 rounded-r-full transition-all duration-200 ${
          active
            ? "h-5 bg-accent opacity-100"
            : showUnread
              ? "h-2 bg-white opacity-100"
              : "h-2 bg-white opacity-0"
        }`}
      />
      <button
        onClick={onClick}
        className={`pressable group mb-0.5 flex w-full items-center gap-1.5 rounded-lg px-2.5 py-1.5 text-[15px] transition-colors duration-150 ${
          active
            ? "bg-selected text-interactive-active"
            : showUnread
              ? "text-interactive-active hover:bg-hover"
              : "text-channel hover:bg-hover hover:text-interactive-hover"
        }`}
      >
        <Icon size={20} className="shrink-0 text-muted transition-colors group-hover:text-interactive-hover" />
        <span className={`truncate ${showUnread ? "font-semibold" : ""}`}>{channel.name}</span>
        <span className="ml-auto flex shrink-0 items-center gap-1.5">
          {/* Bulle de discussion (salons vocaux) — ouvre le chat sans rejoindre le vocal. */}
          {voice && onOpenText && (
            <span
              role="button"
              tabIndex={-1}
              onClick={(e) => {
                e.stopPropagation();
                onOpenText();
              }}
              title="Ouvrir la discussion"
              className="opacity-0 transition-opacity hover:text-interactive-hover group-hover:opacity-100"
            >
              <MessagesSquare size={16} />
            </span>
          )}
          {mentions > 0 && (
            <span className="rounded-full bg-dnd px-1.5 text-xs font-bold text-white">{mentions}</span>
          )}
        </span>
      </button>
    </div>
  );
}

export function CreateChannelDialog({
  guildId,
  parentId = null,
  forceCategory = false,
  categories = [],
  onClose,
}: {
  guildId: string;
  parentId?: string | null;
  forceCategory?: boolean;
  categories?: Channel[];
  onClose: () => void;
}) {
  const [name, setName] = useState("");
  const [voice, setVoice] = useState(false);
  const [parent, setParent] = useState<string | null>(parentId);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const selectChannel = useStore((s) => s.selectChannel);

  const parentName = categories.find((c) => c.id === parent)?.name;

  async function submit() {
    setBusy(true);
    setError(null);
    try {
      const type = forceCategory ? CH_CATEGORY : voice ? CH_VOICE : 0;
      const c = await api.createChannel(guildId, {
        name: name.trim(),
        type,
        // Une catégorie n'a pas de parent ; sinon on rattache à la catégorie choisie.
        parent_id: forceCategory ? null : parent,
      });
      // L'événement CHANNEL_CREATE mettra à jour la liste ; on l'ajoute tout de suite.
      useStore.setState((s) => {
        const list = s.channelsByGuild[guildId] ?? [];
        return list.some((x) => x.id === c.id)
          ? {}
          : { channelsByGuild: { ...s.channelsByGuild, [guildId]: sortChannels([...list, c]) } };
      });
      if (type === 0) await selectChannel(c.id);
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Échec.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal onClose={onClose}>
      <div className="w-[460px] rounded-xl bg-modal p-6 ring-1 ring-cardline surface-card">
        <h2 className="mb-1 text-xl font-bold text-header">
          {forceCategory ? "Créer une catégorie" : "Créer un salon"}
        </h2>
        {parentName && !forceCategory && (
          <p className="mb-4 text-sm text-muted">dans la catégorie {parentName}</p>
        )}
        {!forceCategory && (
          <div className="mb-4 mt-3 flex gap-2">
            <TypeOption label="Texte" icon={<Hash size={20} />} active={!voice} onClick={() => setVoice(false)} />
            <TypeOption label="Vocal" icon={<Volume2 size={20} />} active={voice} onClick={() => setVoice(true)} />
          </div>
        )}
        <label className="mb-1.5 mt-3 block text-xs font-bold uppercase tracking-wide text-subtext">
          {forceCategory ? "Nom de la catégorie" : "Nom du salon"}
        </label>
        <div className="mb-4 flex items-center gap-2 rounded-lg bg-deepest px-3 ring-1 ring-transparent focus-within:ring-accent">
          {!forceCategory && (
            <span className="text-muted">{voice ? <Volume2 size={18} /> : <Hash size={18} />}</span>
          )}
          <input
            autoFocus
            placeholder={forceCategory ? "Nouvelle catégorie" : "nouveau-salon"}
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && name.trim()) void submit();
            }}
            className="w-full bg-transparent py-2.5 text-normal outline-none placeholder:text-muted"
          />
        </div>

        {/* Choix de la catégorie (sauf si c'est une catégorie ou si le parent est imposé). */}
        {!forceCategory && parentId === null && categories.length > 0 && (
          <div className="mb-4">
            <label className="mb-1.5 block text-xs font-bold uppercase tracking-wide text-subtext">
              Catégorie
            </label>
            <div className="flex flex-wrap gap-1.5">
              <CatChip label="Aucune" active={parent === null} onClick={() => setParent(null)} />
              {categories.map((c) => (
                <CatChip key={c.id} label={c.name} active={parent === c.id} onClick={() => setParent(c.id)} />
              ))}
            </div>
          </div>
        )}

        {error && <p className="mb-3 text-sm text-dnd">{error}</p>}
        <div className="flex justify-end gap-3">
          <button onClick={onClose} className="px-4 py-2 text-sm text-normal hover:underline">
            Annuler
          </button>
          <button
            onClick={() => void submit()}
            disabled={busy || !name.trim()}
            className="pressable inline-flex items-center justify-center gap-2 rounded-lg btn-accent px-5 py-2 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-60"
          >
            {busy && <Spinner size={14} />}Créer
          </button>
        </div>
      </div>
    </Modal>
  );
}

function CatChip({ label, active, onClick }: { label: string; active: boolean; onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      className={`flex items-center gap-1.5 rounded-full px-3 py-1.5 text-sm transition-colors ${
        active ? "bg-accent text-white" : "bg-deepest text-normal hover:bg-white/10"
      }`}
    >
      <Folder size={13} className="shrink-0 opacity-80" />
      <span className="max-w-[140px] truncate">{label}</span>
    </button>
  );
}

function TypeOption({
  label,
  icon,
  active,
  onClick,
}: {
  label: string;
  icon: React.ReactNode;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={`flex flex-1 items-center gap-2 rounded border px-3 py-2 text-left ${
        active ? "border-blurple bg-white/5 text-interactive-active" : "border-transparent bg-sidebar text-muted"
      }`}
    >
      {icon}
      <span className="text-sm text-normal">{label}</span>
    </button>
  );
}

function ThreadRow({
  name,
  active,
  locked,
  onClick,
}: {
  name: string;
  active: boolean;
  locked?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={`mb-0.5 ml-6 flex w-[calc(100%-1.5rem)] items-center gap-1.5 rounded-lg px-2 py-1 text-sm ${
        active ? "bg-selected text-interactive-active" : "text-channel hover:bg-hover hover:text-interactive-hover"
      }`}
    >
      <MessagesSquare size={16} className="shrink-0 text-muted" />
      <span className="truncate">{name}</span>
      {locked && <Lock size={12} className="ml-auto shrink-0 text-muted" />}
    </button>
  );
}

function VoiceMember({
  state,
  member,
  guildId,
}: {
  state: VoiceState;
  member?: Member;
  guildId: string;
}) {
  const name = member ? member.nick || displayName(member.user) : `#${state.user_id}`;
  const muted = state.self_mute || state.mute;
  // Anneau vert quand le participant parle (même signal que la scène vocale).
  const talking = useStore((s) => !!s.speaking[state.user_id]) && !muted;
  // Sourdine LOCALE (volume 0) : pictogramme distinct du mute serveur.
  const locallyMuted = useStore((s) => (s.userVolumes[state.user_id] ?? 1) === 0);
  // Popover de profil CONTRÔLÉ : ouvert seulement au clic gauche explicite. Sans cela, la
  // sélection d'un item du menu contextuel (clic droit) rouvrait le profil par un événement
  // de pointeur retombant sur le déclencheur du popover.
  const [profileOpen, setProfileOpen] = useState(false);
  // Clic gauche → fiche profil ; clic droit → menu d'actions (VoiceMemberContextMenu).
  return (
    <UserPopover
      userId={state.user_id}
      guildId={guildId}
      open={profileOpen}
      onOpenChange={setProfileOpen}
    >
      <VoiceMemberContextMenu userId={state.user_id} guildId={guildId} state={state}>
        <div
          onClick={() => setProfileOpen(true)}
          className="ml-6 flex w-full items-center gap-2 rounded px-2 py-1 text-channel transition-colors hover:bg-hover"
        >
          {/* Anneau vert qui « respire » pendant la parole (speak-pulse), au lieu d'un ring figé. */}
          <span className={`rounded-full ${talking ? "speak-pulse ring-2 ring-online" : ""}`}>
            <Avatar name={name} id={state.user_id} size={24} avatarId={member?.user.avatar_id} />
          </span>
          <span className={`truncate text-sm transition-colors ${talking ? "text-interactive-active" : ""}`}>{name}</span>
          <span className="ml-auto flex gap-1 text-muted">
            {locallyMuted && <VolumeX size={14} />}
            {muted && <MicOff size={14} />}
            {(state.self_deaf || state.deaf) && <HeadphoneOff size={14} />}
          </span>
        </div>
      </VoiceMemberContextMenu>
    </UserPopover>
  );
}

// ───────────────────────────── Accueil (MP) ─────────────────────────────

// « écrit… » dans la liste des MP : trois points animés à droite du nom de la conversation.
function DmTypingDots({ channelId }: { channelId: string }) {
  const typing = useStore((s) => s.typing[channelId]);
  const [, force] = useState(0);
  useEffect(() => {
    if (!typing) return;
    const t = setInterval(() => force((n) => n + 1), 1000);
    return () => clearInterval(t);
  }, [typing]);
  if (!typing) return null;
  const now = performance.now();
  if (!Object.values(typing).some((exp) => exp > now)) return null;
  return (
    <span className="ml-auto flex shrink-0 items-center gap-0.5" title="Écrit…">
      {[0, 1, 2].map((i) => (
        <span
          key={i}
          className="h-1 w-1 animate-bounce rounded-full bg-current"
          style={{ animationDelay: `${i * 150}ms` }}
        />
      ))}
    </span>
  );
}

// Ligne d'une conversation MP : avatar + nom, avec en 2ᵉ ligne le statut perso (1:1) ou
// l'effectif (groupe). « écrit… » l'emporte sur le sous-texte quand quelqu'un tape.
function DmRow({
  dm,
  meId,
  active,
  onOpen,
}: {
  dm: DMChannel;
  meId?: string;
  active: boolean;
  onOpen: () => void;
}) {
  const others = dm.recipients.filter((u) => u.id !== meId);
  const name = dm.name || others.map(displayName).join(", ") || "Groupe";
  const first = others[0];
  const isGroup = others.length > 1;
  // Statut perso du correspondant (1:1) — sélection scalaire (pas d'objet neuf).
  const customStatus = useStore((s) => (first ? s.customStatus[first.id] : undefined));
  const subtitle = isGroup
    ? `${dm.recipients.length} membres`
    : customStatus || null;
  return (
    <button
      onClick={onOpen}
      className={`pressable mb-0.5 flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 transition-colors duration-150 ${
        active ? "bg-selected text-white" : "text-channel hover:bg-hover hover:text-normal"
      }`}
    >
      <Avatar name={name} id={first?.id ?? dm.id} size={32} avatarId={first?.avatar_id} />
      <span className="flex min-w-0 flex-1 flex-col text-left">
        <span className="truncate text-[15px] leading-tight">{name}</span>
        {subtitle && <span className="truncate text-xs text-muted leading-tight">{subtitle}</span>}
      </span>
      <DmTypingDots channelId={dm.id} />
    </button>
  );
}

function HomeSidebar() {
  const dms = useStore((s) => s.dms);
  const me = useStore((s) => s.me);
  const activeDM = useStore((s) => s.activeDM);
  const openDM = useStore((s) => s.openDM);
  const view = useStore((s) => s.view);
  const setHome = useStore((s) => s.selectHome);
  const relationships = useStore((s) => s.relationships);
  const refreshDMs = useStore((s) => s.refreshDMs);

  const friendsActive = view.kind === "home" && activeDM === null;
  const friends = relationships.filter((r) => r.type === "friend");

  async function startDM(userId: string) {
    try {
      const dm = await api.openDM({ recipients: [userId] });
      await refreshDMs();
      await openDM(dm.id);
    } catch {
      /* ignore */
    }
  }

  return (
    <>
      <header className="flex h-12 items-center border-b border-line px-3 shadow-sm">
        <Popover.Root>
          <Popover.Trigger className="w-full rounded bg-deepest px-2 py-1 text-left text-sm text-muted outline-none hover:text-normal">
            Trouver ou démarrer une conversation
          </Popover.Trigger>
          <Popover.Portal>
            <Popover.Content
              align="start"
              sideOffset={6}
              className={`z-[60] max-h-[360px] w-[280px] overflow-y-auto rounded-xl bg-floating p-2 shadow-pop ring-1 ring-line scroll-thin ${OVERLAY_ANIM}`}
            >
              <div className="px-2 pb-1 text-xs font-semibold uppercase tracking-wide text-channel">
                Amis
              </div>
              {friends.length === 0 ? (
                <p className="px-2 py-2 text-sm text-muted">Ajoute des amis pour discuter.</p>
              ) : (
                friends.map((r) => (
                  <Popover.Close
                    key={r.id}
                    onClick={() => void startDM(r.user.id)}
                    className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left outline-none hover:bg-hover"
                  >
                    <Avatar name={displayName(r.user)} id={r.user.id} size={28} avatarId={r.user.avatar_id} />
                    <span className="truncate text-sm text-normal">{displayName(r.user)}</span>
                  </Popover.Close>
                ))
              )}
            </Popover.Content>
          </Popover.Portal>
        </Popover.Root>
      </header>
      <div className="px-2 py-3">
        <button
          onClick={() => void setHome()}
          className={`pressable mb-2 flex w-full items-center gap-3 rounded-lg px-2.5 py-2 text-[15px] transition-colors duration-150 ${
            friendsActive ? "bg-selected text-white" : "text-channel hover:bg-hover hover:text-normal"
          }`}
        >
          <Users size={20} />
          <span className="font-medium">Amis</span>
        </button>

        <div className="mb-1 px-2 text-xs font-semibold uppercase tracking-wide text-channel">
          Messages privés
        </div>
        {dms.map((dm) => (
          <DmRow key={dm.id} dm={dm} meId={me?.id} active={activeDM === dm.id} onOpen={() => void openDM(dm.id)} />
        ))}
        {dms.length === 0 && (
          <p className="px-2 py-2 text-sm text-muted">Aucune conversation pour l'instant.</p>
        )}
      </div>
    </>
  );
}
