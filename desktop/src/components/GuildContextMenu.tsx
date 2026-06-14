import { useState, type ReactNode } from "react";
import * as CM from "@radix-ui/react-context-menu";
import {
  Bell,
  BellOff,
  CalendarDays,
  Check,
  CheckCheck,
  ChevronRight,
  Hash as IdIcon,
  LogOut,
  Plus,
  Settings as SettingsIcon,
  UserPlus,
} from "lucide-react";
import { api } from "../api";
import { canIn, isMuted, permsIn, useStore } from "../store";
import { PERM } from "../lib/permissions";
import { OVERLAY_ANIM } from "../lib/anim";
import { InviteModal } from "./InviteModal";
import { EventsModal } from "./EventsModal";
import { ServerSettings } from "./ServerSettings";
import { CreateChannelDialog } from "./ChannelSidebar";

type Modal = "invite" | "settings" | "events" | "create" | null;

// Menu contextuel (clic droit) sur l'icône d'un serveur dans le rail — reprend le menu déroulant
// du serveur, **adapté aux permissions** de l'utilisateur courant.
export function GuildContextMenu({ guildId, children }: { guildId: string; children: ReactNode }) {
  const guild = useStore((s) => s.guilds.find((g) => g.id === guildId));
  const me = useStore((s) => s.me);
  const notif = useStore((s) => s.notif);
  const setMute = useStore((s) => s.setMute);
  const setNotifLevel = useStore((s) => s.setNotifLevel);
  const markGuildRead = useStore((s) => s.markGuildRead);
  const refreshGuilds = useStore((s) => s.refreshGuilds);
  const selectHome = useStore((s) => s.selectHome);
  const notifLevel = notif[`0:${guildId}`]?.level ?? 0;

  const isOwner = !!guild && guild.owner_id === me?.id;
  const muted = isMuted(notif, 0, guildId);
  const canInvite = useStore((s) => canIn(s, guildId, PERM.CREATE_INSTANT_INVITE));
  const canManageChannels = useStore((s) => canIn(s, guildId, PERM.MANAGE_CHANNELS));
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

  const [modal, setModal] = useState<Modal>(null);

  function markAllRead() {
    markGuildRead(guildId); // un seul appel serveur (ack guilde) + mise à jour locale
  }
  function copyId() {
    void navigator.clipboard?.writeText(guildId).catch(() => {});
  }
  async function leave() {
    try {
      await api.leaveGuild(guildId);
      await refreshGuilds();
      await selectHome();
    } catch {
      /* ignore */
    }
  }

  const hasCreate = canOpenSettings || canManageChannels || canCreateEvents;

  return (
    <>
      <CM.Root>
        <CM.Trigger asChild>{children}</CM.Trigger>
        <CM.Portal>
          <CM.Content
            className={`z-[70] w-[240px] rounded-xl bg-floating p-1.5 shadow-pop ring-1 ring-line ${OVERLAY_ANIM}`}
          >
            <Item onSelect={markAllRead} icon={<CheckCheck size={16} />}>
              Marquer comme lu
            </Item>
            {canInvite && (
              <Item accent onSelect={() => setModal("invite")} icon={<UserPlus size={16} />}>
                Inviter sur le serveur
              </Item>
            )}
            <CM.Separator className="my-1 h-px bg-white/10" />
            <Item onSelect={() => void setMute(0, guildId, !muted)} icon={muted ? <Bell size={16} /> : <BellOff size={16} />}>
              {muted ? "Réactiver le serveur" : "Mettre en sourdine"}
            </Item>
            {/* Niveau de notification : Tous / @mentions / Rien. */}
            <CM.Sub>
              <CM.SubTrigger className="flex w-full cursor-pointer items-center justify-between gap-3 rounded px-2 py-1.5 text-sm text-normal outline-none data-[highlighted]:bg-accent data-[highlighted]:text-white data-[state=open]:bg-accent data-[state=open]:text-white">
                <span className="truncate">Notifications</span>
                <ChevronRight size={15} className="shrink-0" />
              </CM.SubTrigger>
              <CM.Portal>
                <CM.SubContent
                  sideOffset={6}
                  className={`z-[70] w-[200px] rounded-xl bg-floating p-1.5 shadow-pop ring-1 ring-line ${OVERLAY_ANIM}`}
                >
                  {[
                    { v: 0, label: "Tous les messages" },
                    { v: 1, label: "@mentions uniquement" },
                    { v: 2, label: "Rien" },
                  ].map((o) => (
                    <CM.Item
                      key={o.v}
                      onSelect={() => void setNotifLevel(0, guildId, o.v)}
                      className="flex w-full cursor-pointer items-center justify-between gap-2 rounded px-2 py-1.5 text-sm text-normal outline-none data-[highlighted]:bg-accent data-[highlighted]:text-white"
                    >
                      <span className="truncate">{o.label}</span>
                      {notifLevel === o.v && <Check size={15} className="shrink-0" />}
                    </CM.Item>
                  ))}
                </CM.SubContent>
              </CM.Portal>
            </CM.Sub>
            {hasCreate && <CM.Separator className="my-1 h-px bg-white/10" />}
            {canOpenSettings && (
              <Item onSelect={() => setModal("settings")} icon={<SettingsIcon size={16} />}>
                Paramètres du serveur
              </Item>
            )}
            {canManageChannels && (
              <Item onSelect={() => setModal("create")} icon={<Plus size={16} />}>
                Créer un salon
              </Item>
            )}
            {canCreateEvents && (
              <Item onSelect={() => setModal("events")} icon={<CalendarDays size={16} />}>
                Créer un événement
              </Item>
            )}
            <CM.Separator className="my-1 h-px bg-white/10" />
            <Item onSelect={copyId} icon={<IdIcon size={16} />}>
              Copier l'identifiant du serveur
            </Item>
            {!isOwner && (
              <>
                <CM.Separator className="my-1 h-px bg-white/10" />
                <Item danger onSelect={() => void leave()} icon={<LogOut size={16} />}>
                  Quitter le serveur
                </Item>
              </>
            )}
          </CM.Content>
        </CM.Portal>
      </CM.Root>

      {modal === "invite" && <InviteModal guildId={guildId} onClose={() => setModal(null)} />}
      {modal === "settings" && <ServerSettings guildId={guildId} onClose={() => setModal(null)} />}
      {modal === "events" && <EventsModal guildId={guildId} onClose={() => setModal(null)} />}
      {modal === "create" && <CreateChannelDialog guildId={guildId} onClose={() => setModal(null)} />}
    </>
  );
}

function Item({
  children,
  onSelect,
  icon,
  accent,
  danger,
}: {
  children: ReactNode;
  onSelect: () => void;
  icon?: ReactNode;
  accent?: boolean;
  danger?: boolean;
}) {
  return (
    <CM.Item
      onSelect={onSelect}
      className={`flex w-full cursor-pointer items-center justify-between gap-3 rounded px-2 py-1.5 text-sm outline-none ${
        danger
          ? "text-dnd data-[highlighted]:bg-dnd data-[highlighted]:text-white"
          : accent
            ? "text-accent data-[highlighted]:bg-accent data-[highlighted]:text-white"
            : "text-normal data-[highlighted]:bg-accent data-[highlighted]:text-white"
      }`}
    >
      <span className="truncate">{children}</span>
      {icon && <span className="shrink-0">{icon}</span>}
    </CM.Item>
  );
}
