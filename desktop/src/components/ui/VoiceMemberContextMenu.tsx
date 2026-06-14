import * as CM from "@radix-ui/react-context-menu";
import { type ReactNode } from "react";
import {
  ArrowRightLeft,
  ChevronRight,
  Copy,
  HeadphoneOff,
  MessageCircle,
  MicOff,
  PhoneOff,
  Volume2,
  VolumeX,
} from "lucide-react";
import { api } from "../../api";
import { canIn, useStore } from "../../store";
import { PERM } from "../../lib/permissions";
import { OVERLAY_ANIM } from "../../lib/anim";
import { CH_VOICE, type ModerateVoiceState, type VoiceState } from "../../types";

// Menu contextuel (clic droit) sur un participant d'un salon vocal — façon Discord :
// volume local, sourdine locale, modération serveur (mute/sourdine/déplacer/déconnecter),
// message privé, copier l'identifiant. Les actions de modération n'apparaissent que selon les
// permissions et seulement pour les autres membres (jamais soi-même ni le propriétaire).
export function VoiceMemberContextMenu({
  userId,
  guildId,
  state,
  children,
}: {
  userId: string;
  guildId: string;
  state: VoiceState;
  children: ReactNode;
}) {
  const me = useStore((s) => s.me);
  const isSelf = me?.id === userId;

  const targetIsOwner = useStore(
    (s) => s.guilds.find((g) => g.id === guildId)?.owner_id === userId,
  );
  const canVoiceMute = useStore((s) => canIn(s, guildId, PERM.MUTE_MEMBERS));
  const canVoiceDeafen = useStore((s) => canIn(s, guildId, PERM.DEAFEN_MEMBERS));
  const canVoiceMove = useStore((s) => canIn(s, guildId, PERM.MOVE_MEMBERS));

  const userVolume = useStore((s) => s.userVolumes[userId]);
  const setUserVolume = useStore((s) => s.setUserVolume);
  const toggleLocalMute = useStore((s) => s.toggleLocalMute);
  const refreshDMs = useStore((s) => s.refreshDMs);
  const openDM = useStore((s) => s.openDM);

  // RÈGLE getSnapshot : on sélectionne la référence stable, on filtre en dehors du sélecteur.
  const guildChannels = useStore((s) => s.channelsByGuild[guildId]);
  const voiceChannels = (guildChannels ?? []).filter((c) => c.type === CH_VOICE);

  // Sourdine LOCALE (volume 0) — la mémorisation/restauration est gérée et persistée par le store.
  const locallyMuted = (userVolume ?? 1) === 0;

  async function moderateVoice(body: ModerateVoiceState) {
    try {
      await api.moderateVoiceState(guildId, userId, body);
      const states = await api.listVoiceStates(guildId);
      useStore.setState((s) => ({
        voiceStatesByGuild: { ...s.voiceStatesByGuild, [guildId]: states },
      }));
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

  function copyId() {
    void navigator.clipboard?.writeText(userId).catch(() => {});
  }

  const showServerMod =
    !isSelf && !targetIsOwner && (canVoiceMute || canVoiceDeafen || canVoiceMove);

  return (
    <CM.Root>
      <CM.Trigger asChild>{children}</CM.Trigger>
      <CM.Portal>
        <CM.Content
          // Empêche Radix de renvoyer le focus/un évènement de pointeur au déclencheur en
          // refermant : sinon la sélection d'un item rouvrait le popover de profil sous-jacent.
          onCloseAutoFocus={(e) => e.preventDefault()}
          className={`z-[70] min-w-[220px] rounded-xl bg-floating p-1.5 shadow-pop ring-1 ring-line ${OVERLAY_ANIM}`}
        >
          {!isSelf && (
            <Item onSelect={() => void startDM()} icon={<MessageCircle size={15} />}>
              Envoyer un message
            </Item>
          )}

          {/* Volume local — sous-menu avec curseur (réglage personnel persisté). */}
          {!isSelf && (
            <CM.Sub>
              <SubTrigger icon={<Volume2 size={15} />}>Volume de l'utilisateur</SubTrigger>
              <CM.Portal>
                <CM.SubContent
                  sideOffset={6}
                  className={`z-[80] w-[210px] rounded-xl bg-floating p-3 shadow-pop ring-1 ring-line ${OVERLAY_ANIM}`}
                >
                  <div className="mb-2 flex items-center justify-between text-xs text-muted">
                    <span>Volume</span>
                    <span>{Math.round((userVolume ?? 1) * 100)} %</span>
                  </div>
                  <input
                    type="range"
                    min={0}
                    max={100}
                    value={Math.round((userVolume ?? 1) * 100)}
                    onChange={(e) => setUserVolume(userId, Number(e.target.value) / 100)}
                    className="w-full accent-[var(--accent)]"
                  />
                </CM.SubContent>
              </CM.Portal>
            </CM.Sub>
          )}

          {/* Sourdine locale (mon écoute de cette personne) — distincte du mute serveur. */}
          {!isSelf && (
            <Item
              onSelect={() => toggleLocalMute(userId)}
              icon={locallyMuted ? <Volume2 size={15} /> : <VolumeX size={15} />}
            >
              {locallyMuted ? "Réactiver le son (local)" : "Rendre muet (local)"}
            </Item>
          )}

          {showServerMod && (
            <>
              <CM.Separator className="my-1 h-px bg-white/10" />
              <div className="px-2 py-1 text-[11px] font-bold uppercase tracking-wide text-subtext">
                Modération
              </div>
              {canVoiceMute && (
                <Item
                  onSelect={() => void moderateVoice({ mute: !state.mute })}
                  icon={<MicOff size={15} />}
                  danger={!state.mute}
                >
                  {state.mute ? "Démuter sur le serveur" : "Rendre muet sur le serveur"}
                </Item>
              )}
              {canVoiceDeafen && (
                <Item
                  onSelect={() => void moderateVoice({ deaf: !state.deaf })}
                  icon={<HeadphoneOff size={15} />}
                  danger={!state.deaf}
                >
                  {state.deaf ? "Réactiver l'audio serveur" : "Mettre en sourdine serveur"}
                </Item>
              )}
              {canVoiceMove && voiceChannels.length > 1 && (
                <CM.Sub>
                  <SubTrigger icon={<ArrowRightLeft size={15} />}>Déplacer vers</SubTrigger>
                  <CM.Portal>
                    <CM.SubContent
                      sideOffset={6}
                      className={`z-[80] max-h-[260px] w-[210px] overflow-y-auto rounded-xl bg-floating p-1.5 shadow-pop ring-1 ring-line scroll-thin ${OVERLAY_ANIM}`}
                    >
                      {voiceChannels
                        .filter((c) => c.id !== state.channel_id)
                        .map((c) => (
                          <Item
                            key={c.id}
                            onSelect={() => void moderateVoice({ channel_id: c.id })}
                            icon={<ArrowRightLeft size={14} />}
                          >
                            {c.name}
                          </Item>
                        ))}
                    </CM.SubContent>
                  </CM.Portal>
                </CM.Sub>
              )}
              {canVoiceMove && (
                <Item
                  onSelect={() => void moderateVoice({ disconnect: true })}
                  icon={<PhoneOff size={15} />}
                  danger
                >
                  Déconnecter du vocal
                </Item>
              )}
            </>
          )}

          <CM.Separator className="my-1 h-px bg-white/10" />
          <Item onSelect={copyId} icon={<Copy size={15} />}>
            Copier l'identifiant
          </Item>
        </CM.Content>
      </CM.Portal>
    </CM.Root>
  );
}

function Item({
  children,
  onSelect,
  icon,
  danger,
}: {
  children: ReactNode;
  onSelect: () => void;
  icon?: ReactNode;
  danger?: boolean;
}) {
  return (
    <CM.Item
      onSelect={onSelect}
      className={`flex cursor-pointer items-center gap-2.5 rounded px-2 py-1.5 text-sm outline-none transition-colors duration-150 data-[highlighted]:translate-x-0.5 ${
        danger
          ? "text-dnd data-[highlighted]:bg-dnd data-[highlighted]:text-white"
          : "text-normal data-[highlighted]:bg-accent data-[highlighted]:text-white"
      }`}
    >
      {icon && <span className="shrink-0">{icon}</span>}
      <span className="truncate">{children}</span>
    </CM.Item>
  );
}

function SubTrigger({ children, icon }: { children: ReactNode; icon?: ReactNode }) {
  return (
    <CM.SubTrigger className="flex w-full cursor-pointer items-center gap-2.5 rounded px-2 py-1.5 text-sm text-normal outline-none data-[highlighted]:bg-accent data-[highlighted]:text-white data-[state=open]:bg-accent data-[state=open]:text-white">
      {icon && <span className="shrink-0">{icon}</span>}
      <span className="flex-1 truncate text-left">{children}</span>
      <ChevronRight size={15} className="shrink-0" />
    </CM.SubTrigger>
  );
}
