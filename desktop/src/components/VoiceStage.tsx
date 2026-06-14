import { useEffect, useRef, useState } from "react";
import * as Popover from "@radix-ui/react-popover";
import {
  ChevronDown,
  HeadphoneOff,
  Headphones,
  Maximize,
  Maximize2,
  MessageSquare,
  Mic,
  MicOff,
  Minimize,
  Minimize2,
  MonitorUp,
  Music2,
  PhoneOff,
  RotateCw,
  ScreenShare,
  Signal,
  Video,
  VideoOff,
  Volume2,
  VolumeX,
} from "lucide-react";
import { useStore } from "../store";
import { colorFor, displayName } from "../lib/format";
import { OVERLAY_ANIM } from "../lib/anim";
import type { Channel, SoundboardSound } from "../types";
import { Avatar } from "./Avatar";
import { Spinner } from "./ui/Spinner";
import { StreamQualityMenu, heightLabel } from "./ui/StreamQualityMenu";

// Attache un MediaStream à un <video> (les flux ne sont pas sérialisables → via ref).
function VideoSurface({
  stream,
  mirror,
  muted,
  contain,
}: {
  stream: MediaStream;
  mirror?: boolean;
  muted?: boolean;
  contain?: boolean; // écran → letterbox (pas de rognage) ; caméra → remplit le cadre
}) {
  const ref = useRef<HTMLVideoElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (el && el.srcObject !== stream) el.srcObject = stream;
  }, [stream]);
  return (
    <video
      ref={ref}
      autoPlay
      playsInline
      muted={muted}
      className={`h-full w-full bg-black ${contain ? "object-contain" : "object-cover"}`}
      style={mirror ? { transform: "scaleX(-1)" } : undefined}
    />
  );
}

type TileVariant = "grid" | "stage" | "strip";

// Tuile générique : cadre 16:9, anneau « parle », pastille de nom + badges.
// `variant` pilote la taille : grille (défaut), scène (focus plein cadre) ou pellicule (miniature).
function Tile({
  children,
  name,
  speaking,
  badges,
  topBadge,
  variant = "grid",
  active,
  onClick,
}: {
  children: React.ReactNode;
  name: string;
  speaking?: boolean;
  badges?: React.ReactNode;
  topBadge?: React.ReactNode;
  variant?: TileVariant;
  active?: boolean;
  onClick?: () => void;
}) {
  const sizeCls =
    variant === "stage"
      ? "h-full w-full animate-spot-in"
      : variant === "strip"
        ? "aspect-video h-[84px] shrink-0"
        : "aspect-video w-[min(100%,23rem)]";
  return (
    <div
      onClick={onClick}
      className={`group relative flex ${sizeCls} items-center justify-center overflow-hidden rounded-xl bg-black/50 shadow-lg ring-2 transition-[box-shadow,border-color] duration-150 ${
        active ? "ring-accent" : speaking ? "ring-online speak-pulse" : "ring-white/5"
      } ${onClick ? "cursor-pointer hover:ring-white/25" : ""}`}
    >
      {children}
      {/* Affordance focus : agrandir (grille/pellicule) ou réduire (scène), au survol. */}
      {onClick && (
        <div className="absolute left-2 top-2 flex h-7 w-7 items-center justify-center rounded-md bg-black/55 text-white opacity-0 backdrop-blur-sm transition-opacity group-hover:opacity-100">
          {variant === "stage" ? <Minimize2 size={14} /> : <Maximize2 size={14} />}
        </div>
      )}
      {topBadge && <div className="absolute right-2 top-2 flex items-center gap-1">{topBadge}</div>}
      <div className="absolute bottom-2 left-2 flex max-w-[calc(100%-1rem)] items-center gap-1.5 rounded-md bg-black/60 px-2 py-1 text-sm font-medium text-white backdrop-blur-sm">
        {badges}
        <span className="truncate">{name}</span>
      </div>
    </div>
  );
}

// Tuile d'un participant : sa caméra si active, sinon un grand avatar sur fond teinté.
function ParticipantTile({
  uid,
  name,
  cam,
  mirror,
  speaking,
  muted,
  deaf,
  you,
  avatarId,
  variant = "grid",
  active,
  onClick,
}: {
  uid: string;
  name: string;
  cam: MediaStream | null;
  mirror?: boolean;
  speaking?: boolean;
  muted?: boolean;
  deaf?: boolean;
  you?: boolean;
  avatarId?: string | null;
  variant?: TileVariant;
  active?: boolean;
  onClick?: () => void;
}) {
  const color = colorFor(uid);
  // Avatar plus grand en scène (focus), plus petit en pellicule.
  const avatarSize = variant === "stage" ? 160 : variant === "strip" ? 40 : 96;
  return (
    <Tile
      variant={variant}
      active={active}
      onClick={onClick}
      name={you ? `${name} (vous)` : name}
      speaking={speaking}
      badges={
        <>
          {muted && <MicOff size={14} className="text-dnd" />}
          {deaf && <HeadphoneOff size={14} className="text-dnd" />}
        </>
      }
    >
      {cam ? (
        <VideoSurface stream={cam} mirror={mirror} muted />
      ) : (
        <div
          className="flex h-full w-full items-center justify-center"
          style={{ background: `radial-gradient(circle at 50% 38%, ${color}2e, #0a0a0a 72%)` }}
        >
          <div className={`rounded-full ${speaking ? "ring-4 ring-online/70 speak-pulse" : ""}`}>
            <Avatar name={name} id={uid} size={avatarSize} ring="transparent" avatarId={avatarId} />
          </div>
        </div>
      )}
    </Tile>
  );
}

export function VoiceStage({ channel, guildId }: { channel: Channel; guildId: string }) {
  const me = useStore((s) => s.me);
  const myVoice = useStore((s) => s.myVoice);
  const connecting = useStore((s) => s.voiceConnecting);
  const localVideo = useStore((s) => s.localVideo);
  const localScreen = useStore((s) => s.localScreen);
  const voiceVideos = useStore((s) => s.voiceVideos);
  const speaking = useStore((s) => s.speaking);
  const states = useStore((s) => s.voiceStatesByGuild[guildId]);
  const members = useStore((s) => s.membersByGuild[guildId]);
  const mediaPrefs = useStore((s) => s.mediaPrefs);

  const joinVoice = useStore((s) => s.joinVoice);
  const leave = useStore((s) => s.leaveVoiceChannel);
  const toggleMute = useStore((s) => s.toggleSelfMute);
  const toggleDeaf = useStore((s) => s.toggleSelfDeaf);
  const toggleVideo = useStore((s) => s.toggleSelfVideo);
  const toggleScreen = useStore((s) => s.toggleScreenShare);
  const reconnect = useStore((s) => s.reconnectVoice);
  const voiceTextOpen = useStore((s) => s.voiceTextOpen);
  const toggleVoiceText = useStore((s) => s.toggleVoiceText);

  const rootRef = useRef<HTMLDivElement>(null);
  const [fullscreen, setFullscreen] = useState(false);
  // Tuile mise en avant (focus plein cadre de la modale vocale). `null` = grille.
  const [spot, setSpot] = useState<string | null>(null);
  useEffect(() => {
    const h = () => setFullscreen(!!document.fullscreenElement);
    document.addEventListener("fullscreenchange", h);
    return () => document.removeEventListener("fullscreenchange", h);
  }, []);
  function toggleFullscreen() {
    if (document.fullscreenElement) void document.exitFullscreen().catch(() => {});
    else void rootRef.current?.requestFullscreen().catch(() => {});
  }
  const toggleSpot = (key: string) => setSpot((cur) => (cur === key ? null : key));

  const connectedHere = myVoice?.channelId === channel.id;
  const participants = (states ?? []).filter((v) => v.channel_id === channel.id);

  function nameFor(uid: string): string {
    const m = members?.find((x) => x.user.id === uid);
    if (m) return m.nick || displayName(m.user);
    return me?.id === uid ? displayName(me) : "Membre";
  }

  function avatarFor(uid: string): string | null {
    const m = members?.find((x) => x.user.id === uid);
    if (m) return m.user.avatar_id;
    return me?.id === uid ? me.avatar_id : null;
  }

  // ── Pas (encore) connecté à CE salon → écran d'accueil de l'appel. ──
  if (!connectedHere) {
    return (
      <div className="aurora-halo flex flex-1 flex-col items-center justify-center gap-5 bg-chat text-center">
        <div className="flex h-20 w-20 items-center justify-center rounded-3xl bg-hover ring-1 ring-line surface-card">
          <Volume2 size={38} style={{ color: "var(--aurora-a)" }} />
        </div>
        <div>
          <h2 className="text-xl font-bold text-header">{channel.name}</h2>
          <p className="mt-1 text-sm text-muted">
            {participants.length > 0
              ? `${participants.length} personne${participants.length > 1 ? "s" : ""} dans le salon`
              : "Personne pour l'instant"}
          </p>
        </div>
        {participants.length > 0 && (
          <div className="flex -space-x-2">
            {participants.slice(0, 8).map((v) => (
              <div key={v.user_id} className="rounded-full ring-2 ring-deepest">
                <Avatar name={nameFor(v.user_id)} id={v.user_id} size={36} ring="var(--bg-deepest)" avatarId={avatarFor(v.user_id)} />
              </div>
            ))}
          </div>
        )}
        <button
          onClick={() => void joinVoice(guildId, channel.id)}
          disabled={connecting}
          className="pressable flex items-center gap-2 rounded-full btn-success px-6 py-2.5 font-semibold text-white transition disabled:opacity-60"
        >
          {connecting && <Spinner size={16} />}
          {connecting ? "Connexion…" : "Rejoindre le salon vocal"}
        </button>
      </div>
    );
  }

  // Caméra d'un participant : mon flux local pour moi, sinon le flux distant attribué.
  const camFor = (uid: string): MediaStream | null => {
    if (uid === me?.id) return myVoice?.selfVideo ? localVideo : null;
    return voiceVideos.find((v) => v.kind === "cam" && v.userId === uid)?.stream ?? null;
  };
  const screens: { id: string; userId: string; stream: MediaStream; mine: boolean }[] = [
    ...(localScreen ? [{ id: "local-screen", userId: me?.id ?? "", stream: localScreen, mine: true }] : []),
    ...voiceVideos
      .filter((v) => v.kind === "screen")
      .map((v) => ({ id: v.trackId, userId: v.userId, stream: v.stream, mine: false })),
  ];

  // Liste unifiée des tuiles (partages d'écran puis participants). Chaque tuile a une `key`
  // stable ; la mise en avant (`spot`) ne fait que changer sa taille/place → aucun flux n'est
  // rechargé (les MediaStream vivent dans le store, on ne fait que ré-attacher le même objet).
  const showSelfFallback = !participants.some((v) => v.user_id === me?.id) && !!me;
  const descs: { key: string; render: (variant: TileVariant) => React.ReactNode }[] = [];
  for (const sc of screens) {
    const key = `screen:${sc.id}`;
    descs.push({
      key,
      render: (variant) => (
        <Tile
          key={key}
          variant={variant}
          active={spot === key}
          onClick={() => toggleSpot(key)}
          name={`${sc.mine ? "Vous" : nameFor(sc.userId)} — écran`}
          topBadge={
            <>
              {/* Qualité courante : pertinente uniquement pour MON flux sortant. */}
              {sc.mine && (
                <span className="rounded bg-black/60 px-1.5 py-0.5 text-[10px] font-semibold text-white backdrop-blur-sm">
                  {heightLabel(mediaPrefs.streamHeight)} · {mediaPrefs.streamFps} FPS
                </span>
              )}
              <span className="rounded bg-dnd px-1.5 py-0.5 text-[10px] font-bold uppercase tracking-wide text-white">
                En direct
              </span>
            </>
          }
          badges={<ScreenShare size={14} className="text-online" />}
        >
          <VideoSurface stream={sc.stream} muted contain />
          {/* Volume du flux distant (côté spectateur) — n'apparaît qu'au survol de la tuile. */}
          {!sc.mine && <StreamVolumeOverlay userId={sc.userId} />}
        </Tile>
      ),
    });
  }
  for (const v of participants) {
    const you = v.user_id === me?.id;
    // Pour MOI : état optimiste local (myVoice) + mute/sourdine serveur imposés ; pour les autres :
    // état serveur. Refléter serverMute/serverDeaf évite qu'un mute de modération passe inaperçu.
    const muted = you ? !!myVoice?.selfMute || !!myVoice?.serverMute : v.self_mute || v.mute;
    const deaf = you ? !!myVoice?.selfDeaf || !!myVoice?.serverDeaf : v.self_deaf || v.deaf;
    const key = `cam:${v.user_id}`;
    descs.push({
      key,
      render: (variant) => (
        <ParticipantTile
          key={key}
          variant={variant}
          active={spot === key}
          onClick={() => toggleSpot(key)}
          uid={v.user_id}
          name={nameFor(v.user_id)}
          avatarId={avatarFor(v.user_id)}
          cam={camFor(v.user_id)}
          mirror={you}
          speaking={!!speaking[v.user_id] && !muted}
          muted={muted}
          deaf={deaf}
          you={you}
        />
      ),
    });
  }
  if (showSelfFallback && me) {
    const key = `cam:${me.id}`;
    descs.push({
      key,
      render: (variant) => (
        <ParticipantTile
          key={key}
          variant={variant}
          active={spot === key}
          onClick={() => toggleSpot(key)}
          uid={me.id}
          name={displayName(me)}
          avatarId={me.avatar_id}
          cam={camFor(me.id)}
          mirror
          speaking={!!speaking[me.id] && !myVoice?.selfMute}
          muted={!!myVoice?.selfMute}
          deaf={!!myVoice?.selfDeaf}
          you
        />
      ),
    });
  }
  // Tuile mise en avant si elle existe encore (sinon on retombe sur la grille).
  const spotDesc = spot ? descs.find((d) => d.key === spot) : undefined;

  return (
    <div
      ref={rootRef}
      className={`flex flex-1 flex-col bg-deepest ${fullscreen ? "animate-fs-zoom" : ""}`}
    >
      {/* En-tête */}
      <header className="flex h-12 shrink-0 items-center gap-2 border-b border-line px-4">
        <Volume2 size={20} className="text-muted" />
        <h2 className="font-semibold text-header">{channel.name}</h2>
        <span className="ml-1 flex items-center gap-1 text-xs text-online">
          <Signal size={13} /> {connecting ? "Resynchronisation…" : "Connecté"}
        </span>
        <span className="ml-auto mr-1 text-xs text-muted">{participants.length} dans le salon</span>
        <button
          onClick={toggleVoiceText}
          title={voiceTextOpen ? "Masquer la discussion" : "Afficher la discussion"}
          className={`rounded p-1.5 hover:bg-white/5 ${
            voiceTextOpen ? "text-interactive-active" : "text-interactive-normal hover:text-header"
          }`}
        >
          <MessageSquare size={18} />
        </button>
        <button
          onClick={toggleFullscreen}
          title={fullscreen ? "Quitter le plein écran" : "Plein écran"}
          className="rounded p-1.5 text-interactive-normal hover:bg-white/5 hover:text-header"
        >
          {fullscreen ? <Minimize size={18} /> : <Maximize size={18} />}
        </button>
      </header>

      {/* Scène : grille de tuiles, ou tuile mise en avant + pellicule des autres. */}
      {spotDesc ? (
        <div className="flex min-h-0 flex-1 flex-col gap-3 p-4">
          <div className="flex min-h-0 flex-1 items-center justify-center">
            {spotDesc.render("stage")}
          </div>
          {descs.length > 1 && (
            <div className="flex shrink-0 items-center gap-2 overflow-x-auto pb-1 scroll-thin">
              {descs.filter((d) => d.key !== spot).map((d) => d.render("strip"))}
            </div>
          )}
        </div>
      ) : (
        <div className="flex flex-1 flex-wrap content-center items-center justify-center gap-3 overflow-y-auto p-4 scroll-thin">
          {descs.map((d) => d.render("grid"))}
        </div>
      )}

      {/* Barre de contrôle — capsule flottante en verre dépoli (façon Control Center). */}
      <div className="flex shrink-0 items-center justify-center pb-6 pt-2">
        <div className="flex items-center gap-2 rounded-full bg-black/30 px-3 py-2 shadow-lg ring-1 ring-white/[0.06] backdrop-blur-md">
        <CtrlButton
          active={!myVoice?.selfMute}
          danger={myVoice?.selfMute}
          title={myVoice?.selfMute ? "Réactiver le micro" : "Couper le micro"}
          onClick={() => void toggleMute()}
        >
          {myVoice?.selfMute ? <MicOff size={20} /> : <Mic size={20} />}
        </CtrlButton>
        <CtrlButton
          active={!myVoice?.selfDeaf}
          danger={myVoice?.selfDeaf}
          title={myVoice?.selfDeaf ? "Réactiver le son" : "Se rendre sourd"}
          onClick={() => void toggleDeaf()}
        >
          {myVoice?.selfDeaf ? <HeadphoneOff size={20} /> : <Headphones size={20} />}
        </CtrlButton>
        <CtrlButton
          active={myVoice?.selfVideo}
          title={myVoice?.selfVideo ? "Couper la caméra" : "Activer la caméra"}
          onClick={() => void toggleVideo()}
          disabled={connecting}
        >
          {myVoice?.selfVideo ? <Video size={20} /> : <VideoOff size={20} />}
        </CtrlButton>
        <CtrlButton
          active={!!localScreen}
          title={localScreen ? "Arrêter le partage d'écran" : "Partager un écran"}
          onClick={() => void toggleScreen()}
          disabled={connecting}
        >
          <MonitorUp size={20} />
        </CtrlButton>
        <StreamQualityMenu>
          <button
            type="button"
            title="Qualité du partage d'écran"
            className="pressable flex h-7 w-7 items-center justify-center rounded-full bg-white/5 text-interactive-normal transition-colors hover:bg-white/10"
          >
            <ChevronDown size={16} />
          </button>
        </StreamQualityMenu>
        <SoundboardButton guildId={guildId} disabled={connecting} />
        <CtrlButton
          active={false}
          title="Resynchroniser le flux"
          onClick={() => void reconnect()}
          disabled={connecting}
        >
          <RotateCw size={20} className={connecting ? "animate-spin" : undefined} />
        </CtrlButton>
        <span className="mx-0.5 h-7 w-px bg-white/10" />
        <button
          onClick={() => void leave()}
          title="Se déconnecter"
          className="pressable flex h-12 w-12 items-center justify-center rounded-full bg-dnd text-white transition-transform hover:scale-105"
        >
          <PhoneOff size={22} />
        </button>
        </div>
      </div>
    </div>
  );
}

// Soundboard : grille des sons du serveur, joués dans le mix publié (tout le salon les entend).
function SoundboardButton({ guildId, disabled }: { guildId: string; disabled?: boolean }) {
  const sounds = useStore((s) => s.soundsByGuild[guildId]);
  const playSoundboard = useStore((s) => s.playSoundboard);
  const list: SoundboardSound[] = sounds ?? [];
  return (
    <Popover.Root>
      <Popover.Trigger asChild>
        <button
          title="Soundboard"
          disabled={disabled}
          className="pressable flex h-12 w-12 items-center justify-center rounded-full bg-white/5 text-interactive-normal transition-colors hover:bg-white/10 disabled:opacity-40"
        >
          <Music2 size={20} />
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          side="top"
          align="center"
          sideOffset={10}
          className={`z-[60] max-h-[300px] w-[280px] overflow-y-auto rounded-xl bg-floating p-2 shadow-pop ring-1 ring-line scroll-thin ${OVERLAY_ANIM}`}
        >
          <div className="px-1 pb-1 text-xs font-semibold uppercase text-muted">Soundboard</div>
          {list.length === 0 ? (
            <div className="flex flex-col items-center gap-2 py-5 text-center text-sm text-muted">
              <Music2 size={20} />
              Aucun son sur ce serveur.
            </div>
          ) : (
            <div className="grid grid-cols-2 gap-1.5">
              {list.map((s) => (
                <button
                  key={s.id}
                  onClick={() => void playSoundboard(s)}
                  title={s.name}
                  className="pressable flex items-center gap-2 rounded-lg bg-white/5 px-2.5 py-2 text-left hover:bg-white/10"
                >
                  {/* Contenu utilisateur : émoji décoratif du son. */}
                  <span className="w-5 shrink-0 text-center">{s.emoji ?? ""}</span>
                  <span className="truncate text-sm text-normal">{s.name}</span>
                </button>
              ))}
            </div>
          )}
          <Popover.Arrow className="fill-floating" />
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

// Contrôle de volume du flux distant (côté spectateur). Agit sur l'audio du STREAM via
// setStreamVolume (séparé du volume micro de la personne). Le <video> de la tuile reste muet :
// l'audio passe par un <audio> caché géré dans voice.ts — on ne touche jamais l'élément vidéo.
// Discret : n'apparaît qu'au survol de la tuile (le parent porte la classe `group`).
function StreamVolumeOverlay({ userId }: { userId: string }) {
  const volume = useStore((s) => s.streamVolumes[userId]);
  const setStreamVolume = useStore((s) => s.setStreamVolume);
  const v = volume ?? 1;
  const muted = v === 0;
  // Clic sur l'icône : bascule muet (0) / plein (1). Évite de mettre la tuile en avant.
  function toggleMute(e: React.MouseEvent) {
    e.stopPropagation();
    setStreamVolume(userId, muted ? 1 : 0);
  }
  return (
    <div
      onClick={(e) => e.stopPropagation()}
      className="absolute bottom-2 right-2 flex items-center gap-1.5 rounded-md bg-black/60 px-1.5 py-1 opacity-0 backdrop-blur-sm transition-opacity group-hover:opacity-100"
    >
      <button
        type="button"
        onClick={toggleMute}
        title={muted ? "Réactiver le son du stream" : "Couper le son du stream"}
        className="pressable flex h-6 w-6 shrink-0 items-center justify-center rounded text-white hover:bg-white/10"
      >
        {muted ? <VolumeX size={15} /> : <Volume2 size={15} />}
      </button>
      <input
        type="range"
        min={0}
        max={100}
        value={Math.round(v * 100)}
        onChange={(e) => setStreamVolume(userId, Number(e.target.value) / 100)}
        title="Volume du stream"
        className="w-20 accent-[var(--accent)]"
      />
    </div>
  );
}

function CtrlButton({
  children,
  onClick,
  title,
  active,
  danger,
  disabled,
}: {
  children: React.ReactNode;
  onClick: () => void;
  title: string;
  active?: boolean;
  danger?: boolean;
  disabled?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      title={title}
      disabled={disabled}
      className={`pressable flex h-12 w-12 items-center justify-center rounded-full transition-colors disabled:opacity-40 ${
        danger
          ? "bg-dnd/20 text-dnd hover:bg-dnd/30"
          : active
            ? "bg-white/10 text-header hover:bg-white/15"
            : "bg-white/5 text-interactive-normal hover:bg-white/10"
      }`}
    >
      {children}
    </button>
  );
}
