import * as CM from "@radix-ui/react-context-menu";
import type { ReactNode } from "react";
import { Check, ChevronRight } from "lucide-react";
import { useStore } from "../../store";
import { CH_VOICE, type Channel } from "../../types";
import { OVERLAY_ANIM } from "../../lib/anim";

// Menu contextuel (clic droit) sur un salon de la sidebar. Adapté aux permissions.
export function ChannelContextMenu({
  channel,
  muted,
  canManage,
  canWebhooks,
  onMarkRead,
  onToggleMute,
  onEdit,
  onWebhooks,
  onDelete,
  children,
}: {
  channel: Channel;
  muted: boolean;
  canManage: boolean;
  canWebhooks: boolean;
  onMarkRead: () => void;
  onToggleMute: () => void;
  onEdit: (c: Channel) => void;
  onWebhooks: (c: Channel) => void;
  onDelete: (c: Channel) => void;
  children: ReactNode;
}) {
  const isVoice = channel.type === CH_VOICE;
  const notifLevel = useStore((s) => s.notif[`1:${channel.id}`]?.level ?? 3);
  const setNotifLevel = useStore((s) => s.setNotifLevel);
  return (
    <CM.Root>
      <CM.Trigger>{children}</CM.Trigger>
      <CM.Portal>
        <CM.Content className={`z-[60] min-w-[200px] rounded-xl bg-floating p-1.5 shadow-pop ring-1 ring-line ${OVERLAY_ANIM}`}>
          <Item onSelect={onMarkRead}>Marquer comme lu</Item>
          <Item onSelect={onToggleMute}>
            {muted ? "Réactiver le salon" : "Mettre le salon en sourdine"}
          </Item>
          {/* Niveau de notification du salon (3 = hériter du réglage serveur). */}
          <CM.Sub>
            <CM.SubTrigger className="flex w-full cursor-pointer items-center justify-between gap-3 rounded px-2 py-1.5 text-sm text-normal outline-none data-[highlighted]:bg-accent data-[highlighted]:text-white data-[state=open]:bg-accent data-[state=open]:text-white">
              <span className="truncate">Notifications</span>
              <ChevronRight size={15} className="shrink-0" />
            </CM.SubTrigger>
            <CM.Portal>
              <CM.SubContent
                sideOffset={6}
                className={`z-[70] w-[210px] rounded-xl bg-floating p-1.5 shadow-pop ring-1 ring-line ${OVERLAY_ANIM}`}
              >
                {[
                  { v: 3, label: "Hériter du serveur" },
                  { v: 0, label: "Tous les messages" },
                  { v: 1, label: "@mentions uniquement" },
                  { v: 2, label: "Rien" },
                ].map((o) => (
                  <CM.Item
                    key={o.v}
                    onSelect={() => void setNotifLevel(1, channel.id, o.v)}
                    className="flex w-full cursor-pointer items-center justify-between gap-2 rounded px-2 py-1.5 text-sm text-normal outline-none data-[highlighted]:bg-accent data-[highlighted]:text-white"
                  >
                    <span className="truncate">{o.label}</span>
                    {notifLevel === o.v && <Check size={15} className="shrink-0" />}
                  </CM.Item>
                ))}
              </CM.SubContent>
            </CM.Portal>
          </CM.Sub>
          {(canManage || (canWebhooks && !isVoice)) && (
            <CM.Separator className="my-1 h-px bg-white/10" />
          )}
          {canManage && <Item onSelect={() => onEdit(channel)}>Modifier le salon</Item>}
          {canWebhooks && !isVoice && <Item onSelect={() => onWebhooks(channel)}>Webhooks</Item>}
          {canManage && (
            <>
              <CM.Separator className="my-1 h-px bg-white/10" />
              <Item danger onSelect={() => onDelete(channel)}>
                Supprimer le salon
              </Item>
            </>
          )}
        </CM.Content>
      </CM.Portal>
    </CM.Root>
  );
}

function Item({
  children,
  onSelect,
  danger,
}: {
  children: ReactNode;
  onSelect: () => void;
  danger?: boolean;
}) {
  return (
    <CM.Item
      onSelect={onSelect}
      className={`cursor-pointer rounded px-2 py-1.5 text-sm outline-none transition-colors duration-150 data-[highlighted]:translate-x-0.5 ${
        danger
          ? "text-dnd data-[highlighted]:bg-dnd data-[highlighted]:text-white"
          : "text-normal data-[highlighted]:bg-accent data-[highlighted]:text-white"
      }`}
    >
      {children}
    </CM.Item>
  );
}
