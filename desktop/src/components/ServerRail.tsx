import { useCallback, useEffect, useState } from "react";
import * as CM from "@radix-ui/react-context-menu";
import { Compass, Plus } from "lucide-react";
import { api } from "../api";
import {
  guildHasUnread,
  guildMentionCount,
  isChannelUnread,
  isMuted,
  unreadDmCount,
  useStore,
} from "../store";
import { colorFor, initials } from "../lib/format";
import { mediaUrl } from "../lib/instance";
import { OVERLAY_ANIM } from "../lib/anim";
import { Tip } from "./ui/Tooltip";
import { DiscoveryModal } from "./DiscoveryModal";
import { GuildContextMenu } from "./GuildContextMenu";
import { Spinner } from "./ui/Spinner";

export function ServerRail() {
  const guilds = useStore((s) => s.guilds);
  const view = useStore((s) => s.view);
  const channelsByGuild = useStore((s) => s.channelsByGuild);
  const readStates = useStore((s) => s.readStates);
  const dms = useStore((s) => s.dms);
  const notif = useStore((s) => s.notif);
  const selectGuild = useStore((s) => s.selectGuild);
  const selectHome = useStore((s) => s.selectHome);
  const refreshGuilds = useStore((s) => s.refreshGuilds);
  const [adding, setAdding] = useState(false);
  const [discovering, setDiscovering] = useState(false);

  const homeActive = view.kind === "home";
  // Badge MP : nombre de conversations non-lues (le bouton Accueil agrège tous les MP).
  const dmUnread = unreadDmCount(dms, readStates);
  const me = useStore((s) => s.me);
  const activeDM = useStore((s) => s.activeDM);
  const openDM = useStore((s) => s.openDM);
  // Pile d'avatars des MP non-lus (façon Discord), tout en haut du rail — disparaissent à la lecture.
  const unreadDms = dms.filter(
    (d) => d.id !== activeDM && isChannelUnread(d.last_message_id, readStates[d.id]),
  );

  return (
    <div className="flex w-[72px] shrink-0 flex-col items-center gap-2 bg-deepest py-3">
      <RailButton
        active={homeActive}
        onClick={() => void selectHome()}
        label="Accueil"
        squircle
        brand
        unread={!homeActive && dmUnread > 0}
        mentions={dmUnread}
      >
        <OzoneMark />
      </RailButton>

      {/* MP non-lus : avatar de la personne + compteur, cliquable, au-dessus des serveurs. */}
      {unreadDms.length > 0 && (
        <>
          <div className="h-0.5 w-8 rounded-full bg-white/10" />
          <div className="flex w-full flex-col items-center gap-2">
            {unreadDms.map((d) => {
              const other = d.recipients.find((u) => u.id !== me?.id);
              const dmName =
                d.name ||
                d.recipients
                  .filter((u) => u.id !== me?.id)
                  .map((u) => u.display_name || u.username)
                  .join(", ") ||
                "MP";
              // Badge numérique = nombre de messages non lus de CE MP (calculé serveur + temps réel),
              // pas seulement les mentions — fiable même pour un MP jamais ouvert.
              const count = d.unread_count ?? 0;
              return (
                <div key={d.id} className="group relative flex w-full items-center justify-center">
                  {/* Pastille blanche « non-lu » (demi-cercle qui s'allonge au survol), à gauche de l'avatar. */}
                  <span className="absolute left-0 w-1 rounded-r-full bg-white transition-all h-2 group-hover:h-5" />
                  <Tip label={d.name || (other ? other.display_name || other.username : "Message privé")} side="right">
                    <button
                      onClick={() => void openDM(d.id)}
                      className="relative h-12 w-12 overflow-hidden rounded-full ring-2 ring-transparent transition hover:ring-white/25"
                    >
                      {other?.avatar_id ? (
                        <img
                          src={mediaUrl(`/api/users/${other.id}/avatar?v=${other.avatar_id}`)}
                          alt=""
                          className="h-full w-full object-cover"
                          draggable={false}
                        />
                      ) : (
                        <span
                          className="flex h-full w-full items-center justify-center text-sm font-semibold text-white"
                          style={{ backgroundColor: colorFor(other?.id ?? d.id) }}
                        >
                          {initials(dmName)}
                        </span>
                      )}
                      <span className="absolute -bottom-0.5 -right-0.5 min-w-[18px] rounded-full border-2 border-deepest bg-dnd px-1 text-center text-xs font-bold leading-tight text-white">
                        {count > 99 ? "99+" : count > 0 ? count : "•"}
                      </span>
                    </button>
                  </Tip>
                </div>
              );
            })}
          </div>
        </>
      )}

      <div className="h-0.5 w-8 rounded-full bg-white/10" />

      <div className="flex flex-1 flex-col items-center gap-2 overflow-y-auto scroll-thin">
        {guilds.map((g) => {
          const active = view.kind === "guild" && view.guildId === g.id;
          const chs = channelsByGuild[g.id];
          const muted = isMuted(notif, 0, g.id);
          return (
            // Clic droit sur l'icône → menu déroulant du serveur (adapté aux permissions).
            <GuildContextMenu key={g.id} guildId={g.id}>
              <div className="w-full">
                <RailButton
                  active={active}
                  muted={muted}
                  unread={!active && !muted && guildHasUnread(chs, readStates)}
                  mentions={guildMentionCount(chs, readStates)}
                  onClick={() => void selectGuild(g.id)}
                  label={g.name}
                >
                  {g.icon_id ? (
                    // `?v=` = cache-buster sur l'id d'image → l'icône se rafraîchit en direct au
                    // changement (via l'événement GUILD_UPDATE qui met à jour g.icon_id).
                    <img
                      src={mediaUrl(`/api/guilds/${g.id}/icon?v=${g.icon_id}`)}
                      alt=""
                      className="h-full w-full object-cover"
                    />
                  ) : (
                    <span className="text-[15px] font-semibold">{initials(g.name)}</span>
                  )}
                </RailButton>
              </div>
            </GuildContextMenu>
          );
        })}

        <RailButton
          active={false}
          onClick={() => setAdding(true)}
          label="Ajouter une guilde"
          green
        >
          <Plus size={24} strokeWidth={2.5} />
        </RailButton>

        <RailButton active={false} onClick={() => setDiscovering(true)} label="Découvrir" green>
          <Compass size={22} strokeWidth={2} />
        </RailButton>

        {/* Zone vide du rail : clic droit → ajouter / découvrir un serveur. */}
        <CM.Root>
          <CM.Trigger asChild>
            <div className="min-h-6 w-full flex-1" />
          </CM.Trigger>
          <CM.Portal>
            <CM.Content
              className={`z-[60] min-w-[210px] rounded-xl bg-floating p-1.5 shadow-pop ring-1 ring-line ${OVERLAY_ANIM}`}
            >
              <CM.Item
                onSelect={() => setAdding(true)}
                className="flex cursor-pointer items-center justify-between gap-2 rounded px-2 py-1.5 text-sm text-normal outline-none data-[highlighted]:bg-accent data-[highlighted]:text-white"
              >
                Ajouter un serveur <Plus size={15} />
              </CM.Item>
              <CM.Item
                onSelect={() => setDiscovering(true)}
                className="flex cursor-pointer items-center justify-between gap-2 rounded px-2 py-1.5 text-sm text-normal outline-none data-[highlighted]:bg-accent data-[highlighted]:text-white"
              >
                Découvrir des serveurs <Compass size={15} />
              </CM.Item>
            </CM.Content>
          </CM.Portal>
        </CM.Root>
      </div>

      {adding && (
        <AddGuildDialog
          onClose={() => setAdding(false)}
          onDone={async () => {
            setAdding(false);
            await refreshGuilds();
          }}
        />
      )}
      {discovering && <DiscoveryModal onClose={() => setDiscovering(false)} />}
    </div>
  );
}

function RailButton({
  active,
  onClick,
  label,
  children,
  green,
  squircle,
  brand,
  unread,
  mentions,
  muted,
}: {
  active: boolean;
  onClick: () => void;
  label: string;
  children: React.ReactNode;
  green?: boolean;
  squircle?: boolean;
  brand?: boolean; // bouton « marque » (Oz) : dégradé aurora quand actif
  unread?: boolean;
  mentions?: number;
  muted?: boolean;
}) {
  // Hauteur de la pastille gauche selon l'état : actif > survol > non-lu > repos.
  const pill = active ? "h-10" : unread ? "h-2 group-hover:h-5" : "h-0 group-hover:h-5";
  return (
    <div
      className={`group relative flex w-full items-center justify-center ${
        muted && !active ? "opacity-50" : ""
      }`}
    >
      <span className={`absolute left-0 w-1 rounded-r-full bg-white transition-all ${pill}`} />
      <Tip label={label} side="right">
        <button
          onClick={onClick}
          className={`flex h-12 w-12 items-center justify-center overflow-hidden transition-all duration-200 active:scale-90 ${
            active || squircle
              ? "rounded-2xl"
              : "rounded-[24px] group-hover:rounded-2xl"
          } ${
            active
              ? `${brand ? "bg-aurora" : "bg-accent"} text-white shadow-md`
              : green
                ? "bg-active text-online hover:bg-online hover:text-white"
                : "bg-active text-normal hover:bg-selected"
          }`}
        >
          {children}
        </button>
      </Tip>
      {mentions ? (
        <span
          key={mentions}
          className="absolute -bottom-0.5 right-1 min-w-[18px] animate-pop-in rounded-full border-2 border-deepest bg-dnd px-1 text-center text-xs font-bold leading-tight text-white shadow-[0_0_8px_rgb(218_62_68/0.6)]"
        >
          {mentions > 99 ? "99+" : mentions}
        </span>
      ) : null}
    </div>
  );
}

function OzoneMark() {
  return <span className="text-lg font-bold">Oz</span>;
}

function AddGuildDialog({
  onClose,
  onDone,
}: {
  onClose: () => void;
  onDone: () => void | Promise<void>;
}) {
  const [tab, setTab] = useState<"create" | "join">("create");
  const [name, setName] = useState("");
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const selectGuild = useStore((s) => s.selectGuild);

  async function submit() {
    setBusy(true);
    setError(null);
    try {
      if (tab === "create") {
        const g = await api.createGuild({ name });
        await onDone();
        await selectGuild(g.id);
      } else {
        const g = await api.joinInvite(code.trim());
        await onDone();
        await selectGuild(g.id);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Échec.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal onClose={onClose}>
      <div className="w-[440px] rounded-xl bg-modal p-6 ring-1 ring-cardline surface-card">
        <h2 className="mb-1 text-center text-2xl font-bold text-header">
          {tab === "create" ? "Crée ta guilde" : "Rejoindre une guilde"}
        </h2>
        {/* Onglets avec indicateur d'accent qui glisse sous l'onglet actif. */}
        <div className="mb-4 flex justify-center">
          <div className="relative flex gap-1 rounded-lg bg-deepest p-1 text-sm">
            <span
              className="absolute inset-y-1 w-[calc(50%-0.25rem)] rounded-md bg-accent/15 ring-1 ring-accent/40 transition-transform duration-300 ease-[cubic-bezier(0.32,0.72,0,1)]"
              style={{ transform: tab === "create" ? "translateX(0)" : "translateX(100%)" }}
            />
            <button
              className={`pressable relative z-10 rounded-md px-4 py-1.5 font-medium transition-colors ${tab === "create" ? "text-header" : "text-muted hover:text-normal"}`}
              onClick={() => setTab("create")}
            >
              Créer
            </button>
            <button
              className={`pressable relative z-10 rounded-md px-4 py-1.5 font-medium transition-colors ${tab === "join" ? "text-header" : "text-muted hover:text-normal"}`}
              onClick={() => setTab("join")}
            >
              Rejoindre
            </button>
          </div>
        </div>

        {tab === "create" ? (
          <input
            autoFocus
            placeholder="Nom de la guilde"
            value={name}
            onChange={(e) => setName(e.target.value)}
            className="mb-4 w-full rounded-lg bg-deepest px-3 py-2.5 text-normal outline-none ring-1 ring-transparent focus:ring-accent"
          />
        ) : (
          <input
            autoFocus
            placeholder="Code d'invitation"
            value={code}
            onChange={(e) => setCode(e.target.value)}
            className="mb-4 w-full rounded-lg bg-deepest px-3 py-2.5 text-normal outline-none ring-1 ring-transparent focus:ring-accent"
          />
        )}

        {error && <p className="mb-3 text-sm text-dnd">{error}</p>}

        <div className="flex justify-end gap-2">
          <button
            onClick={onClose}
            className="pressable rounded-lg px-4 py-2 text-sm text-normal transition-colors hover:bg-hover"
          >
            Annuler
          </button>
          <button
            onClick={() => void submit()}
            disabled={busy || (tab === "create" ? !name : !code)}
            className="pressable inline-flex items-center justify-center gap-2 rounded-lg btn-accent px-5 py-2 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-60"
          >
            {busy && <Spinner size={14} />}
            {tab === "create" ? "Créer" : "Rejoindre"}
          </button>
        </div>
      </div>
    </Modal>
  );
}

export function Modal({
  children,
  onClose,
}: {
  children: React.ReactNode;
  onClose: () => void;
}) {
  // Fermeture DIFFÉRÉE : on joue d'abord l'animation de sortie (voile + carte), puis on démonte.
  // `closing` déclenche les classes -out ; à la fin de l'animation, onClose() démonte réellement.
  const [closing, setClosing] = useState(false);
  const requestClose = useCallback(() => setClosing(true), []);

  // ESC ferme CETTE modale uniquement : capture + stopImmediatePropagation pour passer avant
  // un éventuel gestionnaire ESC parent (p. ex. la modale Paramètres qui l'héberge).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopImmediatePropagation();
        requestClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [requestClose]);

  return (
    <div
      className={`fixed inset-0 z-50 flex items-center justify-center bg-black/60 ${
        closing ? "animate-overlay-out" : "animate-overlay-in"
      }`}
      onClick={requestClose}
      // Le démontage réel n'a lieu qu'à la fin de l'animation de SORTIE du voile.
      onAnimationEnd={() => {
        if (closing) onClose();
      }}
    >
      <div
        className={closing ? "animate-pop-out" : "animate-pop-in"}
        onClick={(e) => e.stopPropagation()}
      >
        {children}
      </div>
    </div>
  );
}
