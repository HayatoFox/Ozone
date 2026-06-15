// État applicatif global (Zustand) : auth, instance, guildes, salons, messages,
// MP, amis, présence, et intégration de la Gateway temps réel.

import type { CSSProperties } from "react";
import { create } from "zustand";
import { api, getAccessToken, loadTokens, refreshTokens, setAuthLostHandler, setTokens } from "./api";
import { Gateway, type GatewayEvent, type GatewayStatus } from "./gateway";
import { VoiceConnection, type RemoteVideo } from "./lib/voice";
import {
  comboMatches,
  loadMediaPrefs,
  saveMediaPrefs,
  screenVideoConstraints,
  type MediaPrefs,
} from "./lib/mediaPrefs";
import { mediaUrl } from "./lib/instance";
import {
  decryptFromUser,
  encryptForUser,
  ensureKeypair,
  hasPublicKey,
} from "./lib/e2ee";
import { PERM, PERM_ALL } from "./lib/permissions";
import {
  CH_CATEGORY,
  CH_TEXT,
  CH_THREAD_PRIVATE,
  CH_THREAD_PUBLIC,
  CH_VOICE,
  type Channel,
  type CreateMessage,
  type DMChannel,
  type Guild,
  type InstanceInfo,
  type Emoji,
  type Member,
  type Message,
  type NotificationSetting,
  type Poll,
  type PresenceView,
  type Reaction,
  type ReadState,
  type Relationship,
  type Role,
  type Snowflake,
  type SoundboardSound,
  type Sticker,
  type User,
  type VoiceState,
} from "./types";

// Vue de navigation principale.
export type View =
  | { kind: "home" } // accueil : amis + MP
  | { kind: "guild"; guildId: Snowflake };

// Mode d'affichage des messages (réglage d'apparence).
export type MessageDisplay = "cozy" | "compact";

const DISPLAY_KEY = "ozone.messageDisplay";
function loadDisplay(): MessageDisplay {
  if (typeof localStorage === "undefined") return "cozy";
  return localStorage.getItem(DISPLAY_KEY) === "compact" ? "compact" : "cozy";
}

const EMBEDS_KEY = "ozone.mediaEmbeds";
function loadMediaEmbeds(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(EMBEDS_KEY) === "1";
}

const NOTIF_KEY = "ozone.desktopNotifications";
function loadDesktopNotifications(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(NOTIF_KEY) === "1";
}

// Son de notification (activé par défaut — joué pour les mentions et MP pertinents).
const NOTIF_SOUND_KEY = "ozone.notifSounds";
function loadNotifSounds(): boolean {
  if (typeof localStorage === "undefined") return true;
  return localStorage.getItem(NOTIF_SOUND_KEY) !== "0";
}

// Bip de notification discret en WebAudio (deux harmoniques courtes, pas de fichier embarqué).
let beepCtx: AudioContext | null = null;
function playNotifBeep(): void {
  try {
    beepCtx = beepCtx ?? new AudioContext();
    const ctx = beepCtx;
    void ctx.resume().catch(() => {});
    const t0 = ctx.currentTime;
    for (const [freq, start, dur] of [
      [880, 0, 0.09],
      [1318.5, 0.09, 0.14],
    ] as const) {
      const osc = ctx.createOscillator();
      osc.type = "sine";
      osc.frequency.value = freq;
      const g = ctx.createGain();
      g.gain.setValueAtTime(0, t0 + start);
      g.gain.linearRampToValueAtTime(0.12, t0 + start + 0.015);
      g.gain.exponentialRampToValueAtTime(0.001, t0 + start + dur);
      osc.connect(g);
      g.connect(ctx.destination);
      osc.start(t0 + start);
      osc.stop(t0 + start + dur + 0.05);
    }
  } catch {
    /* audio indisponible */
  }
}

// Thème (réglage d'apparence).
export type Theme = "dark" | "light" | "midnight" | "custom";

// Réglage du thème personnalisé (dégradé de fond + couleur d'accent).
export interface CustomTheme {
  gradient: [string, string];
  accent: number;
}
const DEFAULT_CUSTOM: CustomTheme = { gradient: ["#5865f2", "#c850c0"], accent: 0x5865f2 };

const THEME_KEY = "ozone.theme";
const CUSTOM_KEY = "ozone.customTheme";

function loadTheme(): Theme {
  if (typeof localStorage === "undefined") return "dark";
  const t = localStorage.getItem(THEME_KEY);
  return t === "light" || t === "midnight" || t === "custom" ? t : "dark";
}
function loadCustom(): CustomTheme {
  if (typeof localStorage === "undefined") return DEFAULT_CUSTOM;
  try {
    const c = JSON.parse(localStorage.getItem(CUSTOM_KEY) || "");
    if (Array.isArray(c?.gradient) && c.gradient.length === 2 && typeof c.accent === "number") return c;
  } catch {
    /* défaut */
  }
  return DEFAULT_CUSTOM;
}

export function applyTheme(theme: Theme, custom?: CustomTheme): void {
  if (typeof document === "undefined") return;
  const el = document.documentElement;
  el.className = `theme-${theme}`;
  if (theme === "custom") {
    const c = custom ?? DEFAULT_CUSTOM;
    const accentHex = "#" + (c.accent & 0xffffff).toString(16).padStart(6, "0");
    el.style.setProperty("--app-gradient", `linear-gradient(135deg, ${c.gradient[0]}, ${c.gradient[1]})`);
    el.style.setProperty("--custom-accent", accentHex);
  } else {
    el.style.removeProperty("--app-gradient");
    el.style.removeProperty("--custom-accent");
  }
}

// ───────────────────────── Sync des réglages entre appareils ─────────────────────────
// Les réglages d'apparence (thème, affichage des messages, intégrations, son de notif) sont
// poussés vers le serveur (PUT /users/@me/settings, débouncé) et tirés au démarrage de session.
// Les préférences de périphériques (mediaPrefs) restent locales : elles sont propres à la machine.

let settingsPushTimer: ReturnType<typeof setTimeout> | null = null;
function schedulePushSettings(get: () => State): void {
  if (settingsPushTimer) clearTimeout(settingsPushTimer);
  settingsPushTimer = setTimeout(() => {
    settingsPushTimer = null;
    const s = get();
    if (!s.authed) return;
    void api
      .putMySettings({
        data: {
          theme: s.theme,
          customTheme: s.customTheme,
          messageDisplay: s.messageDisplay,
          mediaEmbeds: s.mediaEmbeds,
          notifSounds: s.notifSounds,
        },
      })
      .catch(() => {});
  }, 1500);
}

// Applique le blob distant SANS re-pousser (écrit localStorage + état + thème).
function applyRemoteSettings(
  data: Record<string, unknown>,
  set: (partial: Partial<State>) => void,
  get: () => State,
): void {
  const patch: Partial<State> = {};
  const t = data.theme;
  if (t === "dark" || t === "light" || t === "midnight" || t === "custom") {
    localStorage.setItem(THEME_KEY, t);
    patch.theme = t;
  }
  const c = data.customTheme as CustomTheme | undefined;
  if (c && Array.isArray(c.gradient) && c.gradient.length === 2 && typeof c.accent === "number") {
    localStorage.setItem(CUSTOM_KEY, JSON.stringify(c));
    patch.customTheme = c;
  }
  if (data.messageDisplay === "cozy" || data.messageDisplay === "compact") {
    localStorage.setItem(DISPLAY_KEY, data.messageDisplay);
    patch.messageDisplay = data.messageDisplay;
  }
  if (typeof data.mediaEmbeds === "boolean") {
    localStorage.setItem(EMBEDS_KEY, data.mediaEmbeds ? "1" : "0");
    patch.mediaEmbeds = data.mediaEmbeds;
  }
  if (typeof data.notifSounds === "boolean") {
    localStorage.setItem(NOTIF_SOUND_KEY, data.notifSounds ? "1" : "0");
    patch.notifSounds = data.notifSounds;
  }
  if (Object.keys(patch).length === 0) return;
  set(patch);
  const next = get();
  applyTheme(next.theme, next.customTheme);
}

interface State {
  // Démarrage / auth
  ready: boolean;
  authed: boolean;
  instance: InstanceInfo | null;
  me: User | null;
  gatewayStatus: GatewayStatus;

  // Données
  guilds: Guild[];
  channelsByGuild: Record<Snowflake, Channel[]>;
  threadsByChannel: Record<Snowflake, Channel[]>; // salon parent -> fils
  messagesByChannel: Record<Snowflake, Message[]>;
  membersByGuild: Record<Snowflake, Member[]>;
  rolesByGuild: Record<Snowflake, Role[]>;
  emojisByGuild: Record<Snowflake, Emoji[]>;
  stickersByGuild: Record<Snowflake, Sticker[]>;
  soundsByGuild: Record<Snowflake, SoundboardSound[]>;
  voiceStatesByGuild: Record<Snowflake, VoiceState[]>;
  myVoice: {
    guildId: Snowflake;
    channelId: Snowflake;
    selfMute: boolean;
    selfDeaf: boolean;
    selfVideo: boolean;
    // Mute/sourdine SERVEUR imposés par un modérateur (distincts du self) : appliqués au gate et
    // affichés sur ma propre tuile. Le micro est coupé tant que serverMute est vrai.
    serverMute: boolean;
    serverDeaf: boolean;
  } | null;
  voiceConnecting: boolean;
  localVideo: MediaStream | null; // ma caméra (tuile locale)
  localScreen: MediaStream | null; // mon partage d'écran (tuile locale)
  voiceVideos: RemoteVideo[]; // pistes vidéo distantes, attribuées par uid + nature
  speaking: Record<Snowflake, boolean>; // uid → parle en ce moment (anneau vert)
  presences: Record<Snowflake, string>; // user_id -> status
  dms: DMChannel[];
  relationships: Relationship[];
  typing: Record<Snowflake, Record<Snowflake, number>>; // channel -> user -> expiry ms
  readStates: Record<Snowflake, ReadState>; // channel_id -> état de lecture
  unreadAnchor: Record<Snowflake, Snowflake>; // channel_id -> dernier lu au moment de l'ouverture
  notif: Record<string, NotificationSetting>; // `${scope_type}:${scope_id}` -> réglage

  // Navigation
  view: View;
  selectedChannelByGuild: Record<Snowflake, Snowflake>;
  activeDM: Snowflake | null;

  // Réglages d'apparence
  messageDisplay: MessageDisplay;
  setMessageDisplay: (mode: MessageDisplay) => void;
  dmProfileOpen: boolean; // panneau profil à droite des MP (toggle « Masquer le profil »)
  toggleDmProfile: () => void;
  voiceTextOpen: boolean; // discussion textuelle intégrée d'un salon vocal (panneau de droite)
  setVoiceTextOpen: (open: boolean) => void;
  toggleVoiceText: () => void;
  theme: Theme;
  setTheme: (theme: Theme) => void;
  customTheme: CustomTheme;
  setCustomTheme: (c: CustomTheme) => void;
  mediaEmbeds: boolean;
  setMediaEmbeds: (on: boolean) => void;
  desktopNotifications: boolean;
  setDesktopNotifications: (on: boolean) => Promise<void>;
  notifSounds: boolean;
  setNotifSounds: (on: boolean) => void;
  // Périphériques média (micro / caméra / sortie) + traitement du son.
  mediaPrefs: MediaPrefs;
  setMediaPrefs: (p: MediaPrefs) => void;
  // Volume local par participant (0..1, persisté) — appliqué aux flux vocaux distants.
  userVolumes: Record<Snowflake, number>;
  setUserVolume: (userId: Snowflake, volume: number) => void;
  // Sourdine LOCALE d'un participant : bascule volume → 0, en mémorisant le volume précédent
  // (persisté) pour le restaurer ensuite — robuste au rechargement de l'app.
  toggleLocalMute: (userId: Snowflake) => void;
  settingsOpen: boolean;
  setSettingsOpen: (open: boolean) => void;

  // Erreurs transitoires
  error: string | null;

  // Actions
  boot: () => Promise<void>;
  afterAuth: () => Promise<void>;
  logout: () => void;
  selectGuild: (guildId: Snowflake) => Promise<void>;
  selectHome: () => Promise<void>;
  selectChannel: (channelId: Snowflake) => Promise<void>;
  viewChannel: (channelId: Snowflake) => void;
  createThread: (parentId: Snowflake, name: string) => Promise<void>;
  joinVoice: (guildId: Snowflake, channelId: Snowflake) => Promise<void>;
  leaveVoiceChannel: () => Promise<void>;
  toggleSelfMute: () => Promise<void>;
  toggleSelfDeaf: () => Promise<void>;
  toggleSelfVideo: () => Promise<void>;
  toggleScreenShare: () => Promise<void>;
  // Recapture le partage d'écran aux nouvelles contraintes (qualité/fps/audio) en direct, ou
  // change de source. Sans argument, applique les préférences courantes. `pickSource` rouvre le
  // sélecteur de fenêtre/écran. Hot-swap sans coupure quand c'est possible.
  restreamWithCurrentQuality: (pickSource?: boolean) => Promise<void>;
  reconnectVoice: () => Promise<void>;
  // Volume du SON DU STREAM regardé, par utilisateur (0..1, persisté) — côté spectateur.
  streamVolumes: Record<Snowflake, number>;
  setStreamVolume: (userId: Snowflake, volume: number) => void;
  openDM: (channelId: Snowflake) => Promise<void>;
  loadMessages: (channelId: Snowflake) => Promise<void>;
  sendMessage: (
    channelId: Snowflake,
    content: string,
    opts?: { attachments?: Snowflake[]; replyTo?: Snowflake; stickerId?: Snowflake },
  ) => Promise<boolean>;
  // Joue un son du soundboard dans le vocal courant (mixé dans la piste publiée).
  playSoundboard: (sound: SoundboardSound) => Promise<void>;
  editMessage: (channelId: Snowflake, messageId: Snowflake, content: string) => Promise<void>;
  deleteMessage: (channelId: Snowflake, messageId: Snowflake) => Promise<void>;
  toggleReaction: (
    channelId: Snowflake,
    messageId: Snowflake,
    emoji: string,
    has: boolean,
  ) => Promise<void>;
  castVote: (channelId: Snowflake, messageId: Snowflake, answerIds: number[]) => Promise<void>;
  refreshGuilds: () => Promise<void>;
  refreshRoles: (guildId: Snowflake) => Promise<void>;
  refreshChannels: (guildId: Snowflake) => Promise<void>;
  refreshMembers: (guildId: Snowflake) => Promise<void>;
  refreshDMs: () => Promise<void>;
  refreshRelationships: () => Promise<void>;
  markRead: (channelId: Snowflake) => void;
  markGuildRead: (guildId: Snowflake) => void;
  setMute: (scopeType: number, scopeId: Snowflake, muted: boolean) => Promise<void>;
  // Niveau de notification : 0 tous · 1 @mentions · 2 rien · 3 hériter (salon).
  setNotifLevel: (scopeType: number, scopeId: Snowflake, level: number) => Promise<void>;
  setPresenceStatus: (status: string) => Promise<void>;
  // Statut personnalisé (texte libre, null = effacer) — affiché sous le pseudo.
  customStatus: Record<Snowflake, string | null>;
  setCustomStatus: (text: string | null) => Promise<void>;
  acceptRelationship: (userId: Snowflake) => Promise<void>;
  removeRelationship: (userId: Snowflake) => Promise<void>;
  setError: (msg: string | null) => void;
}

let gateway: Gateway | null = null;
let voiceConn: VoiceConnection | null = null;

// ───────────────────────── Push-to-talk & gate micro ─────────────────────────
// L'état EFFECTIF du micro = !selfMute ET (mode voix OU touche PTT maintenue).
// Centralisé ici : tous les chemins (toggle mute/sourdine, [re]connexion, changement de
// préférences, touche) repassent par applyMicGate().
let pttHeld = false;
// Gate de détection de voix : en mode « voix », le micro ne transmet que quand on parle (VAD).
// `vadOpen` suit l'état parle/se-tait de MON flux local, fourni par le détecteur (onSpeaking).
let vadOpen = true;
function applyMicGate(): void {
  const s = useStore.getState();
  const mv = s.myVoice;
  if (!voiceConn || !mv) return;
  let gated: boolean;
  if (s.mediaPrefs.inputMode === "ptt") {
    gated = !pttHeld; // PTT : fermé sauf touche maintenue
  } else {
    gated = !vadOpen; // Détection de voix : fermé tant qu'on ne parle pas
  }
  // Le mute SERVEUR (modération) coupe réellement le micro, comme le self-mute (sinon un membre
  // « rendu muet » continuerait d'émettre, contournant la modération).
  voiceConn.setMuted(mv.selfMute || mv.serverMute || gated);
}

// Volumes par participant (réglage LOCAL, persisté) : uid -> 0..1.
const USER_VOLUMES_KEY = "ozone.userVolumes";
// Volume d'avant la sourdine locale (pour restauration fidèle après reload) : uid -> 0..1.
const PRE_MUTE_KEY = "ozone.preMuteVolumes";
// Volume du SON DU STREAM regardé, par participant (côté spectateur) : uid -> 0..1.
const STREAM_VOLUMES_KEY = "ozone.streamVolumes";
function loadJsonMap(key: string): Record<string, number> {
  if (typeof localStorage === "undefined") return {};
  try {
    const v = JSON.parse(localStorage.getItem(key) || "{}");
    return typeof v === "object" && v !== null ? v : {};
  } catch {
    return {};
  }
}
function loadUserVolumes(): Record<string, number> {
  return loadJsonMap(USER_VOLUMES_KEY);
}

// Code de touche RÉELLEMENT enfoncé pour le PTT (mémorisé au keydown). On relâche sur CE code,
// pas sur la pref courante : sinon rebinder la touche pendant qu'on l'appuie laisserait pttHeld
// coincé à true (micro ouvert en permanence).
let pttCode: string | null = null;
function releasePtt(): void {
  if (pttHeld) {
    pttHeld = false;
    pttCode = null;
    applyMicGate();
  }
}
if (typeof window !== "undefined") {
  window.addEventListener("keydown", (e) => {
    const s = useStore.getState();
    if (s.mediaPrefs.inputMode !== "ptt" || e.code !== s.mediaPrefs.pttKey) return;
    if (!pttHeld) {
      pttHeld = true;
      pttCode = e.code; // on retiendra ce code pour le relâchement
      applyMicGate();
    }
  });
  window.addEventListener("keyup", (e) => {
    // Relâche sur le code effectivement appuyé (robuste à un rebind en cours d'appui).
    if (e.code !== pttCode) return;
    releasePtt();
  });
  // Perte de focus : on relâche (sinon micro resté ouvert après Alt+Tab touche enfoncée).
  window.addEventListener("blur", () => releasePtt());
  // Raccourcis vocaux (fenêtre au premier plan) : combos rebindables (défaut Ctrl+Maj+M / +D).
  // Inactifs quand le focus est dans un champ de saisie pour ne pas voler les frappes.
  window.addEventListener("keydown", (e) => {
    if (e.repeat) return;
    const el = e.target as HTMLElement | null;
    if (el && (el.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName))) return;
    const prefs = useStore.getState().mediaPrefs;
    if (comboMatches(e, prefs.muteKey)) {
      e.preventDefault();
      void useStore.getState().toggleSelfMute();
    } else if (comboMatches(e, prefs.deafenKey)) {
      e.preventDefault();
      void useStore.getState().toggleSelfDeaf();
    }
  });
}

// Tri des salons : catégories puis position.
export function sortChannels(chs: Channel[]): Channel[] {
  // Départage par id en **comparaison de snowflake** (BigInt), jamais en Number (précision 64-bit).
  return [...chs].sort(
    (a, b) => a.position - b.position || (a.id === b.id ? 0 : idGt(a.id, b.id) ? 1 : -1),
  );
}

// Tri chronologique DÉTERMINISTE des messages par snowflake (id) croissant + déduplication.
// Garantit un ordre d'affichage stable, identique à chaque chargement et quel que soit l'ordre
// d'arrivée (REST initial ou événements Gateway).
export function sortMessages(msgs: Message[]): Message[] {
  const seen = new Set<Snowflake>();
  const out: Message[] = [];
  for (const m of msgs) {
    if (seen.has(m.id)) continue;
    seen.add(m.id);
    out.push(m);
  }
  return out.sort((a, b) => (a.id === b.id ? 0 : idGt(a.id, b.id) ? 1 : -1));
}

// ───────────────────────── Chiffrement de bout en bout (MP) ─────────────────────────

/** L'autre participant d'un MP **1:1** (exactement 2 membres), ou null (groupe / salon). */
function dmPeer(get: () => State, channelId: Snowflake): User | null {
  const dm = get().dms.find((d) => d.id === channelId);
  if (!dm || dm.recipients.length !== 2) return null;
  const meId = get().me?.id;
  return dm.recipients.find((u) => u.id !== meId) ?? null;
}

// Sentinelle affichée si le déchiffrement échoue (clé locale différente de celle d'émission).
export const E2EE_UNDECRYPTABLE = " e2ee-undecryptable ";

/** Déchiffre le contenu d'un message MP chiffré (et son message cité). No-op hors MP 1:1. */
async function decryptMessage(get: () => State, channelId: Snowflake, m: Message): Promise<Message> {
  const peer = dmPeer(get, channelId);
  if (!peer) return m;
  const decode = async (cipher: string) => {
    try {
      return await decryptFromUser(peer.id, cipher);
    } catch {
      return E2EE_UNDECRYPTABLE;
    }
  };
  let out = m;
  if (m.cipher) out = { ...out, content: await decode(m.cipher) };
  // Aperçu de réponse : le message cité peut lui aussi être chiffré.
  if (out.referenced_message?.cipher) {
    out = {
      ...out,
      referenced_message: {
        ...out.referenced_message,
        content: await decode(out.referenced_message.cipher),
      },
    };
  }
  return out;
}

export const useStore = create<State>((set, get) => ({
  ready: false,
  authed: false,
  instance: null,
  me: null,
  gatewayStatus: "disconnected",

  guilds: [],
  channelsByGuild: {},
  threadsByChannel: {},
  messagesByChannel: {},
  membersByGuild: {},
  rolesByGuild: {},
  emojisByGuild: {},
  stickersByGuild: {},
  soundsByGuild: {},
  voiceStatesByGuild: {},
  myVoice: null,
  voiceConnecting: false,
  localVideo: null,
  localScreen: null,
  voiceVideos: [],
  speaking: {},
  presences: {},
  dms: [],
  relationships: [],
  typing: {},
  readStates: {},
  unreadAnchor: {},
  notif: {},

  view: { kind: "home" },
  selectedChannelByGuild: {},
  activeDM: null,

  messageDisplay: loadDisplay(),
  setMessageDisplay: (mode) => {
    localStorage.setItem(DISPLAY_KEY, mode);
    set({ messageDisplay: mode });
    schedulePushSettings(get);
  },
  dmProfileOpen:
    typeof localStorage === "undefined" || localStorage.getItem("ozone.dmProfile") !== "0",
  toggleDmProfile: () =>
    set((s) => {
      const next = !s.dmProfileOpen;
      if (typeof localStorage !== "undefined") localStorage.setItem("ozone.dmProfile", next ? "1" : "0");
      return { dmProfileOpen: next };
    }),
  // Discussion textuelle des salons vocaux : fermée par défaut (ouverte via la bulle / l'en-tête).
  voiceTextOpen: typeof localStorage !== "undefined" && localStorage.getItem("ozone.voiceText") === "1",
  setVoiceTextOpen: (open) => {
    if (typeof localStorage !== "undefined") localStorage.setItem("ozone.voiceText", open ? "1" : "0");
    set({ voiceTextOpen: open });
  },
  toggleVoiceText: () =>
    set((s) => {
      const next = !s.voiceTextOpen;
      if (typeof localStorage !== "undefined") localStorage.setItem("ozone.voiceText", next ? "1" : "0");
      return { voiceTextOpen: next };
    }),
  theme: loadTheme(),
  setTheme: (theme) => {
    localStorage.setItem(THEME_KEY, theme);
    applyTheme(theme, get().customTheme);
    set({ theme });
    schedulePushSettings(get);
  },
  customTheme: loadCustom(),
  setCustomTheme: (c) => {
    localStorage.setItem(CUSTOM_KEY, JSON.stringify(c));
    set({ customTheme: c });
    if (get().theme === "custom") applyTheme("custom", c);
    schedulePushSettings(get);
  },
  mediaEmbeds: loadMediaEmbeds(),
  setMediaEmbeds: (on) => {
    localStorage.setItem(EMBEDS_KEY, on ? "1" : "0");
    set({ mediaEmbeds: on });
    schedulePushSettings(get);
  },
  notifSounds: loadNotifSounds(),
  setNotifSounds: (on) => {
    localStorage.setItem(NOTIF_SOUND_KEY, on ? "1" : "0");
    set({ notifSounds: on });
    if (on) playNotifBeep(); // aperçu immédiat du son
    schedulePushSettings(get);
  },
  desktopNotifications: loadDesktopNotifications(),
  setDesktopNotifications: async (on) => {
    if (on && typeof Notification !== "undefined" && Notification.permission !== "granted") {
      const perm = await Notification.requestPermission();
      if (perm !== "granted") {
        set({ error: "Permission de notification refusée par le navigateur." });
        return;
      }
    }
    localStorage.setItem(NOTIF_KEY, on ? "1" : "0");
    set({ desktopNotifications: on });
  },
  mediaPrefs: loadMediaPrefs(),
  setMediaPrefs: (p) => {
    const prev = get().mediaPrefs;
    saveMediaPrefs(p);
    set({ mediaPrefs: p });
    // La sortie audio se reroute EN DIRECT ; micro/caméra s'appliquent à la prochaine acquisition.
    voiceConn?.applyOutputDevice();
    voiceConn?.setSensitivity({ auto: p.vadAuto, threshold: p.vadThreshold });
    // N'annule l'appui PTT en cours QUE si le mode/la touche PTT change réellement (sinon régler
    // un volume ou un périphérique pendant qu'on tient la touche couperait le micro à tort).
    if (p.inputMode !== prev.inputMode || p.pttKey !== prev.pttKey) releasePtt();
    applyMicGate(); // changement de mode voix ⇄ PTT (ou sensibilité) appliqué immédiatement
  },
  userVolumes: loadUserVolumes(),
  setUserVolume: (userId, volume) => {
    const v = Math.min(Math.max(volume, 0), 1);
    set((s) => {
      const userVolumes = { ...s.userVolumes, [userId]: v };
      if (typeof localStorage !== "undefined") {
        localStorage.setItem(USER_VOLUMES_KEY, JSON.stringify(userVolumes));
      }
      return { userVolumes };
    });
    voiceConn?.setUserVolume(userId, v);
  },
  streamVolumes: loadJsonMap(STREAM_VOLUMES_KEY),
  setStreamVolume: (userId, volume) => {
    const v = Math.min(Math.max(volume, 0), 1);
    set((s) => {
      const streamVolumes = { ...s.streamVolumes, [userId]: v };
      if (typeof localStorage !== "undefined") {
        localStorage.setItem(STREAM_VOLUMES_KEY, JSON.stringify(streamVolumes));
      }
      return { streamVolumes };
    });
    voiceConn?.setStreamVolume(userId, v);
  },
  toggleLocalMute: (userId) => {
    const cur = get().userVolumes[userId] ?? 1;
    const preMute = loadJsonMap(PRE_MUTE_KEY);
    if (cur === 0) {
      // Réactivation : restaure le volume mémorisé (défaut 1 si jamais sauvegardé).
      const restore = preMute[userId] ?? 1;
      get().setUserVolume(userId, restore > 0 ? restore : 1);
    } else {
      // Sourdine : mémorise le volume courant (persisté) puis coupe.
      preMute[userId] = cur;
      if (typeof localStorage !== "undefined") {
        localStorage.setItem(PRE_MUTE_KEY, JSON.stringify(preMute));
      }
      get().setUserVolume(userId, 0);
    }
  },
  settingsOpen: false,
  setSettingsOpen: (open) => set({ settingsOpen: open }),

  error: null,

  setError: (msg) => set({ error: msg }),

  boot: async () => {
    applyTheme(get().theme, get().customTheme);
    // Échec de rafraîchissement sur une requête REST authentifiée → déconnexion propre.
    setAuthLostHandler(() => {
      if (get().authed) get().logout();
    });
    let instance: InstanceInfo | null = null;
    try {
      instance = await api.instance();
    } catch {
      /* serveur indisponible : on laisse l'écran de connexion gérer */
    }
    if (loadTokens()) {
      // api.me() rafraîchit AUTOMATIQUEMENT le jeton si l'accès a expiré (request → refreshTokens).
      // Plusieurs tentatives : un échec TRANSITOIRE (serveur momentanément indisponible, ex. un
      // redémarrage) ne doit PAS déconnecter. On n'abandonne que si refreshTokens a effacé les
      // jetons (rejet définitif du refresh) ou après épuisement des tentatives.
      for (let attempt = 0; attempt < 3; attempt += 1) {
        try {
          const me = await api.me();
          set({ me, authed: true, instance, ready: true });
          await get().afterAuth();
          return;
        } catch {
          if (!loadTokens()) break; // jeton mort (rejet définitif) → écran de connexion
          await new Promise((r) => setTimeout(r, 700 * (attempt + 1)));
        }
      }
    }
    set({ instance, ready: true, authed: false });
  },

  afterAuth: async () => {
    const me = get().me ?? (await api.me());
    set({ me });
    await Promise.all([
      get().refreshGuilds(),
      get().refreshDMs(),
      get().refreshRelationships(),
      api
        .listReadStates()
        .then((rs) => set({ readStates: Object.fromEntries(rs.map((r) => [r.channel_id, r])) }))
        .catch(() => {}),
      api
        .listNotificationSettings()
        .then((ns) =>
          set({ notif: Object.fromEntries(ns.map((n) => [`${n.scope_type}:${n.scope_id}`, n])) }),
        )
        .catch(() => {}),
      // Réglages synchronisés entre appareils : le serveur fait foi au démarrage de session.
      api
        .getMySettings()
        .then((s) => applyRemoteSettings(s.data ?? {}, set, get))
        .catch(() => {}),
      // Chiffrement E2EE des MP : génère/recharge la paire de clés locale et publie la clé publique.
      ensureKeypair().catch(() => {}),
    ]);
    startGateway(set, get);
  },

  logout: () => {
    gateway?.close();
    gateway = null;
    // Couper le média : sinon micro/caméra/partage d'écran restent actifs après déconnexion.
    if (voiceResyncTimer) {
      clearTimeout(voiceResyncTimer);
      voiceResyncTimer = null;
    }
    if (voiceConn) {
      void voiceConn.close();
      voiceConn = null;
    }
    get().localScreen?.getTracks().forEach((t) => t.stop());
    setTokens(null);
    set({
      authed: false,
      me: null,
      guilds: [],
      channelsByGuild: {},
      threadsByChannel: {},
      messagesByChannel: {},
      membersByGuild: {},
      rolesByGuild: {},
      emojisByGuild: {},
      stickersByGuild: {},
      soundsByGuild: {},
      voiceStatesByGuild: {},
      myVoice: null,
      voiceConnecting: false,
      localVideo: null,
      localScreen: null,
      voiceVideos: [],
      speaking: {},
      dms: [],
      relationships: [],
      readStates: {},
      unreadAnchor: {},
      notif: {},
      view: { kind: "home" },
      gatewayStatus: "disconnected",
    });
  },

  refreshGuilds: async () => {
    const guilds = await api.listGuilds();
    set({ guilds });
  },

  refreshRoles: async (guildId) => {
    try {
      const roles = await api.listRoles(guildId);
      set((s) => ({ rolesByGuild: { ...s.rolesByGuild, [guildId]: roles } }));
    } catch {
      /* ignore */
    }
  },

  // Recharge la liste de salons depuis le serveur (qui filtre par VIEW_CHANNEL — autorité de
  // visibilité, overwrites compris). Appelée en direct quand une permission change : les salons
  // devenus inaccessibles disparaissent, les nouveaux apparaissent, sans reboot.
  refreshChannels: async (guildId) => {
    if (!get().channelsByGuild[guildId]) return; // guilde non chargée → rien à rafraîchir
    try {
      const channels = sortChannels(await api.listChannels(guildId));
      set((s) => ({ channelsByGuild: { ...s.channelsByGuild, [guildId]: channels } }));
      // Si le salon ouvert n'est plus visible (permission retirée), bascule sur un salon texte.
      const sel = get().selectedChannelByGuild[guildId];
      if (sel && !channels.some((c) => c.id === sel)) {
        const fallback = channels.find((c) => c.type === CH_TEXT);
        if (fallback) await get().selectChannel(fallback.id);
      }
    } catch {
      /* ignore */
    }
  },

  refreshMembers: async (guildId) => {
    try {
      const members = await api.listMembers(guildId);
      set((s) => ({ membersByGuild: { ...s.membersByGuild, [guildId]: members } }));
    } catch {
      /* ignore */
    }
  },

  refreshDMs: async () => {
    try {
      const dms = await api.listDMs();
      set({ dms });
    } catch {
      /* ignore */
    }
  },

  refreshRelationships: async () => {
    try {
      const relationships = await api.listRelationships();
      set({ relationships });
    } catch {
      /* ignore */
    }
  },

  acceptRelationship: async (userId) => {
    try {
      await api.acceptRelationship(userId);
      await get().refreshRelationships();
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  removeRelationship: async (userId) => {
    try {
      await api.removeRelationship(userId);
      await get().refreshRelationships();
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  setMute: async (scopeType, scopeId, muted) => {
    const key = `${scopeType}:${scopeId}`;
    const mute_seconds = muted ? -1 : 0;
    // Optimiste : reflète immédiatement l'état.
    set((s) => ({
      notif: {
        ...s.notif,
        [key]: {
          scope_type: scopeType,
          scope_id: scopeId,
          level: s.notif[key]?.level ?? 0,
          muted_until: muted ? -1 : null,
        },
      },
    }));
    try {
      if (scopeType === 0) await api.setGuildNotification(scopeId, { mute_seconds });
      else await api.setChannelNotification(scopeId, { mute_seconds });
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  setPresenceStatus: async (status) => {
    const me = get().me;
    try {
      // On N'ENVOIE PAS custom_status : champ absent ⇒ le serveur PRÉSERVE le statut perso
      // existant (corrige la perte de statut perso quand le cache local est vide, ex. au boot
      // sur un 2ᵉ appareil). Cf. sémantique 3 états de SetPresence.
      await api.setPresence({ status });
      // Affiche localement le statut choisi (invisible apparaît « hors ligne » pour les autres).
      if (me) {
        const shown = status === "invisible" ? "offline" : status;
        set((s) => ({ presences: { ...s.presences, [me.id]: shown } }));
      }
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  customStatus: {},
  setCustomStatus: async (text) => {
    const me = get().me;
    if (!me) return;
    // Statut de base actuel (le « offline » local correspond à « invisible » côté serveur).
    const shown = get().presences[me.id] ?? "online";
    const status = shown === "offline" ? "invisible" : shown;
    const clean = text?.trim() || null;
    try {
      await api.setPresence({ status, custom_status: clean });
      set((s) => ({ customStatus: { ...s.customStatus, [me.id]: clean } }));
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  markGuildRead: (guildId) => {
    // Optimiste : tous les salons de la guilde passent lus localement, puis UN appel serveur.
    set((s) => {
      const channels = s.channelsByGuild[guildId] ?? [];
      const readStates = { ...s.readStates };
      for (const c of channels) {
        if (c.last_message_id) {
          readStates[c.id] = { channel_id: c.id, last_read_id: c.last_message_id, mention_count: 0 };
        }
      }
      return { readStates };
    });
    void api.ackGuild(guildId).catch(() => {});
  },

  setNotifLevel: async (scopeType, scopeId, level) => {
    const key = `${scopeType}:${scopeId}`;
    set((s) => ({
      notif: {
        ...s.notif,
        [key]: {
          scope_type: scopeType,
          scope_id: scopeId,
          level,
          muted_until: s.notif[key]?.muted_until ?? null,
        },
      },
    }));
    try {
      if (scopeType === 0) await api.setGuildNotification(scopeId, { level });
      else await api.setChannelNotification(scopeId, { level });
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  markRead: (channelId) => {
    const last = latestMessageId(get(), channelId);
    if (!last) return;
    const cur = get().readStates[channelId];
    if (cur && cur.last_read_id === last && cur.mention_count === 0) return;
    set((s) => ({
      readStates: {
        ...s.readStates,
        [channelId]: { channel_id: channelId, last_read_id: last, mention_count: 0 },
      },
    }));
    void api.ackMessage(channelId, last).catch(() => {});
  },

  selectHome: async () => {
    // `activeDM: null` ⇒ on revient au MENU Amis (et pas à un MP resté ouvert) : sans ça, cliquer
    // « Amis » depuis une conversation gardait le MP affiché (view=home mais activeDM non nul).
    set({ view: { kind: "home" }, activeDM: null });
    await get().refreshDMs();
  },

  selectGuild: async (guildId) => {
    set({ view: { kind: "guild", guildId } });
    // Charge salons + membres + présences si pas en cache.
    if (!get().channelsByGuild[guildId]) {
      try {
        const [channels, members, presences, roles, emojis, stickers, sounds] = await Promise.all([
          api.listChannels(guildId),
          api.listMembers(guildId).catch(() => [] as Member[]),
          api.listPresences(guildId).catch(() => [] as PresenceView[]),
          api.listRoles(guildId).catch(() => [] as Role[]),
          api.listEmojis(guildId).catch(() => [] as Emoji[]),
          api.listStickers(guildId).catch(() => [] as Sticker[]),
          api.listSounds(guildId).catch(() => [] as SoundboardSound[]),
        ]);
        const voiceStates = await api.listVoiceStates(guildId).catch(() => [] as VoiceState[]);
        set((s) => ({
          channelsByGuild: { ...s.channelsByGuild, [guildId]: sortChannels(channels) },
          membersByGuild: { ...s.membersByGuild, [guildId]: members },
          rolesByGuild: { ...s.rolesByGuild, [guildId]: roles },
          emojisByGuild: { ...s.emojisByGuild, [guildId]: emojis },
          stickersByGuild: { ...s.stickersByGuild, [guildId]: stickers },
          soundsByGuild: { ...s.soundsByGuild, [guildId]: sounds },
          voiceStatesByGuild: { ...s.voiceStatesByGuild, [guildId]: voiceStates },
          presences: mergePresences(s.presences, presences),
          customStatus: mergeCustomStatus(s.customStatus, presences),
        }));
      } catch (e) {
        set({ error: errMsg(e) });
        return;
      }
    }
    // Sélectionne un salon texte par défaut.
    const chs = get().channelsByGuild[guildId] ?? [];
    const current = get().selectedChannelByGuild[guildId];
    const target = current ?? chs.find((c) => c.type === CH_TEXT)?.id;
    if (target) await get().selectChannel(target);
  },

  selectChannel: async (channelId) => {
    const view = get().view;
    if (view.kind === "guild") {
      set((s) => ({
        selectedChannelByGuild: { ...s.selectedChannelByGuild, [view.guildId]: channelId },
      }));
    }
    captureUnreadAnchor(get, set, channelId);
    await get().loadMessages(channelId);
    get().markRead(channelId);

    // Charge les fils du salon (si c'est un salon texte parent, pas déjà chargés).
    if (view.kind === "guild") {
      const ch = get().channelsByGuild[view.guildId]?.find((c) => c.id === channelId);
      if (ch && ch.type === CH_TEXT && !get().threadsByChannel[channelId]) {
        api
          .listThreads(channelId)
          .then((threads) =>
            set((s) => ({ threadsByChannel: { ...s.threadsByChannel, [channelId]: threads } })),
          )
          .catch(() => {});
      }
    }
  },

  // Sélectionne un salon pour l'affichage sans charger de messages (salons vocaux).
  viewChannel: (channelId) => {
    const view = get().view;
    if (view.kind === "guild") {
      set((s) => ({
        selectedChannelByGuild: { ...s.selectedChannelByGuild, [view.guildId]: channelId },
      }));
    }
  },

  createThread: async (parentId, name) => {
    try {
      const thread = await api.createThread(parentId, name.slice(0, 90) || "Nouveau fil");
      set((s) => ({
        threadsByChannel: {
          ...s.threadsByChannel,
          [parentId]: [...(s.threadsByChannel[parentId] ?? []), thread],
        },
      }));
      await get().selectChannel(thread.id);
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  joinVoice: async (guildId, channelId) => {
    // Garde anti-double-connexion : un clic pendant la connexion ne doit pas relancer un
    // 2ᵉ pair (qui orphelinerait micro/caméra du premier). On pose le verrou AVANT tout `await`
    // (notamment avant le leave) : sinon un 2ᵉ joinVoice se faufile pendant le leave et deux
    // connexions se créent, dont l'une devient orpheline (micro/caméra jamais coupés).
    if (get().voiceConnecting) return;
    const cur = get().myVoice;
    if (cur?.channelId === channelId) return; // déjà dans ce salon
    set({ voiceConnecting: true, voiceVideos: [] });
    if (cur) await get().leaveVoiceChannel();
    try {
      const resp = await api.updateVoiceState(guildId, { channel_id: channelId });
      const token = resp?.connection?.token;
      if (!token) throw new Error("jeton vocal absent (serveur)");
      voiceConn = newVoiceConn(get, set);
      voiceConn.seedUserVolumes(get().userVolumes);
      voiceConn.seedStreamVolumes(get().streamVolumes);
      await voiceConn.connect(channelId, token, { selfId: get().me?.id ?? "" });
      set({
        myVoice: {
          guildId,
          channelId,
          selfMute: false,
          selfDeaf: false,
          selfVideo: false,
          serverMute: false,
          serverDeaf: false,
        },
        voiceConnecting: false,
      });
      applyMicGate(); // mode PTT : micro fermé tant que la touche n'est pas maintenue
    } catch (e) {
      if (voiceConn) {
        await voiceConn.close();
        voiceConn = null;
      }
      try {
        await api.leaveVoice(guildId);
      } catch {
        /* ignore */
      }
      get().localScreen?.getTracks().forEach((t) => t.stop());
      set({
        myVoice: null,
        voiceConnecting: false,
        localVideo: null,
        localScreen: null,
        voiceVideos: [],
        speaking: {},
        error: voiceErr(e),
      });
    }
  },

  leaveVoiceChannel: async () => {
    const mv = get().myVoice;
    // Annule toute reconnexion différée en attente : sinon un resync fantôme pourrait tirer après
    // le départ (et reconstruire/raccrocher la connexion d'un salon qu'on vient de rejoindre).
    if (voiceResyncTimer) {
      clearTimeout(voiceResyncTimer);
      voiceResyncTimer = null;
    }
    if (voiceResyncOkTimer) {
      clearTimeout(voiceResyncOkTimer);
      voiceResyncOkTimer = null;
    }
    voiceResyncFails = 0;
    pttHeld = false;
    pttCode = null;
    if (voiceConn) {
      await voiceConn.close();
      voiceConn = null;
    }
    if (mv) {
      try {
        await api.leaveVoice(mv.guildId);
      } catch {
        /* ignore */
      }
    }
    get().localScreen?.getTracks().forEach((t) => t.stop());
    // Retire MON entrée de la liste du salon localement : ne pas attendre le VOICE_STATE_UPDATE
    // serveur (je viens de quitter → je ne suis peut-être plus destinataire de l'event), sinon mon
    // avatar/nom resterait affiché en fantôme dans la liste vocale.
    const myId = get().me?.id;
    set((s) => {
      const patch: Partial<State> = {
        myVoice: null,
        localVideo: null,
        localScreen: null,
        voiceVideos: [],
        speaking: {},
      };
      if (myId && mv) {
        const list = (s.voiceStatesByGuild[mv.guildId] ?? []).filter((x) => x.user_id !== myId);
        patch.voiceStatesByGuild = { ...s.voiceStatesByGuild, [mv.guildId]: list };
      }
      return patch;
    });
  },

  toggleSelfMute: async () => {
    const mv = get().myVoice;
    if (!mv) return;
    const selfMute = !mv.selfMute;
    // Démuter pendant la sourdine LÈVE aussi la sourdine (comportement Discord) — sinon on
    // resterait sourd avec le micro ouvert (état incohérent), et la restauration de preDeafMute
    // écraserait ensuite ce démutage explicite.
    const selfDeaf = !selfMute ? false : mv.selfDeaf;
    if (selfDeaf !== mv.selfDeaf) voiceConn?.setDeafened(selfDeaf);
    set({ myVoice: { ...mv, selfMute, selfDeaf } });
    applyMicGate();
    try {
      await api.updateVoiceState(mv.guildId, { self_mute: selfMute, self_deaf: selfDeaf });
    } catch {
      /* ignore */
    }
  },

  toggleSelfDeaf: async () => {
    const mv = get().myVoice;
    if (!mv) return;
    const selfDeaf = !mv.selfDeaf;
    // Se rendre sourd implique d'être muet ; à la réactivation, on RESTAURE l'état micro
    // d'avant la sourdine (comportement Discord — on ne reste pas muet par effet de bord).
    let selfMute: boolean;
    if (selfDeaf) {
      preDeafMute = mv.selfMute;
      selfMute = true;
    } else {
      selfMute = preDeafMute;
    }
    voiceConn?.setDeafened(selfDeaf);
    set({ myVoice: { ...mv, selfDeaf, selfMute } });
    applyMicGate();
    try {
      await api.updateVoiceState(mv.guildId, { self_deaf: selfDeaf, self_mute: selfMute });
    } catch {
      /* ignore */
    }
  },

  toggleSelfVideo: async () => {
    const mv = get().myVoice;
    if (!mv) return;
    const want = !mv.selfVideo;
    // Voie « flawless » : (dés)activer la caméra par renégociation, sans recharger le flux.
    // Repli : si la signalisation est indisponible/échoue, reconnexion complète (historique).
    if (voiceConn && voiceConn.signalingHealthy()) {
      try {
        if (want) {
          const local = await voiceConn.enableCamera();
          set({ localVideo: local });
        } else {
          await voiceConn.disableCamera();
          set({ localVideo: null });
        }
        set({ myVoice: { ...mv, selfVideo: want } });
        try {
          await api.updateVoiceState(mv.guildId, { self_video: want });
        } catch {
          /* ignore */
        }
        return;
      } catch {
        /* échec renégociation → repli reconnexion */
      }
    }
    await rejoinMedia(get, set, want);
  },

  toggleScreenShare: async () => {
    const mv = get().myVoice;
    if (!mv) return;
    // Déjà en partage → arrêter (renégociation, sinon repli reconnexion).
    if (get().localScreen) {
      const scr = get().localScreen;
      if (voiceConn && voiceConn.signalingHealthy()) {
        try {
          await voiceConn.removeScreen();
          scr?.getTracks().forEach((t) => t.stop());
          set({ localScreen: null });
          return;
        } catch {
          /* repli */
        }
      }
      scr?.getTracks().forEach((t) => t.stop());
      set({ localScreen: null });
      await rejoinMedia(get, set, mv.selfVideo);
      return;
    }
    // Sinon, demander une source (le navigateur ouvre le sélecteur d'écran/fenêtre).
    // Contraintes de qualité/fps selon les préférences ; audio = son de la source si activé.
    const prefs = get().mediaPrefs;
    let screen: MediaStream;
    try {
      screen = await navigator.mediaDevices.getDisplayMedia({
        video: screenVideoConstraints(prefs),
        audio: prefs.streamAudio,
      });
    } catch {
      return; // l'utilisateur a annulé / refusé
    }
    // Arrêt depuis l'UI native du navigateur (« Arrêter le partage ») → on coupe proprement.
    screen.getVideoTracks().forEach((t) => {
      t.onended = () => {
        if (get().localScreen) void get().toggleScreenShare();
      };
    });
    set({ localScreen: screen });
    // Voie « flawless » : publier l'écran par renégociation ; repli reconnexion sinon.
    if (voiceConn && voiceConn.signalingHealthy()) {
      try {
        await voiceConn.addScreen(screen);
        return;
      } catch {
        /* repli */
      }
    }
    await rejoinMedia(get, set, mv.selfVideo);
  },

  restreamWithCurrentQuality: async (pickSource = false) => {
    const mv = get().myVoice;
    const current = get().localScreen;
    if (!mv || !current) return; // rien à re-streamer
    const prefs = get().mediaPrefs;
    // L'audio de la source est-il déjà capturé ? Activer/désactiver l'audio impose une
    // renégociation (ajout/retrait de piste), donc un cycle complet ; sinon hot-swap vidéo.
    const hadAudio = current.getAudioTracks().length > 0;
    const audioChanged = hadAudio !== prefs.streamAudio;

    let next: MediaStream;
    try {
      next = await navigator.mediaDevices.getDisplayMedia({
        video: screenVideoConstraints(prefs),
        audio: prefs.streamAudio,
      });
    } catch {
      return; // annulé / refusé : on garde le partage en cours
    }
    next.getVideoTracks().forEach((t) => {
      t.onended = () => {
        if (get().localScreen) void get().toggleScreenShare();
      };
    });

    // Hot-swap vidéo possible uniquement si l'audio ne change pas ET qu'on ne touche pas l'audio
    // (replaceTrack ne gère que la vidéo). On tente le remplacement direct (zéro coupure).
    if (!audioChanged && !pickSource && next.getAudioTracks().length === 0) {
      const vt = next.getVideoTracks()[0];
      if (vt && voiceConn && voiceConn.signalingHealthy()) {
        const ok = await voiceConn.replaceScreenVideo(vt);
        if (ok) {
          // L'ancien flux conserve son audio éventuel ; ici pas d'audio → on stoppe la vidéo
          // remplacée (déjà gérée par replaceScreenVideo) et on adopte le nouveau flux comme local.
          set({ localScreen: next });
          return;
        }
      }
    }

    // Sinon : cycle propre (retrait + ajout) qui renégocie. On stoppe l'ancien flux.
    if (voiceConn && voiceConn.signalingHealthy()) {
      try {
        await voiceConn.removeScreen();
        current.getTracks().forEach((t) => t.stop());
        await voiceConn.addScreen(next);
        set({ localScreen: next });
        return;
      } catch {
        /* repli reconnexion */
      }
    }
    current.getTracks().forEach((t) => t.stop());
    set({ localScreen: next });
    await rejoinMedia(get, set, mv.selfVideo);
  },

  reconnectVoice: async () => {
    const mv = get().myVoice;
    if (!mv) return;
    // Reconnexion explicite de l'utilisateur (bouton « Resynchroniser ») : on remet le compteur
    // d'échecs à zéro, sinon l'auto-resync resterait saturé (plafond atteint) et ne reprendrait
    // jamais — alors que le message d'erreur invite justement à « réessayer via Resynchroniser ».
    voiceResyncFails = 0;
    if (voiceResyncOkTimer) {
      clearTimeout(voiceResyncOkTimer);
      voiceResyncOkTimer = null;
    }
    await rejoinMedia(get, set, mv.selfVideo);
  },

  openDM: async (channelId) => {
    set({ activeDM: channelId, view: { kind: "home" } });
    captureUnreadAnchor(get, set, channelId);
    await get().loadMessages(channelId);
    get().markRead(channelId);
  },

  loadMessages: async (channelId) => {
    if (get().messagesByChannel[channelId]) return;
    try {
      const msgs = await api.listMessages(channelId, { limit: 50 });
      // Déchiffrement E2EE des MP (avant affichage → pas de scintillement « cadenas → texte »).
      const decrypted = msgs.some((m) => m.cipher || m.referenced_message?.cipher)
        ? await Promise.all(msgs.map((m) => decryptMessage(get, channelId, m)))
        : msgs;
      // Tri DÉTERMINISTE par snowflake (ordre chronologique stable, indépendant de l'ordre
      // renvoyé par le serveur) → l'affichage est identique à chaque (re)chargement.
      set((s) => ({
        messagesByChannel: { ...s.messagesByChannel, [channelId]: sortMessages(decrypted) },
      }));
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  sendMessage: async (channelId, content, opts) => {
    const nonce = `${get().me?.id ?? "0"}-${performance.now()}`;
    try {
      let body: CreateMessage = {
        content,
        nonce,
        attachments: opts?.attachments,
        reply_to: opts?.replyTo,
        sticker_id: opts?.stickerId,
      };
      // MP 1:1 : si le pair a publié une clé, on chiffre le texte de bout en bout. Le serveur (et
      // l'admin de l'instance) ne voit qu'un blob opaque ; `content` part vide. Repli en clair
      // uniquement si le pair n'a pas (encore) de clé publique — sans quoi l'échange serait bloqué.
      const peer = dmPeer(get, channelId);
      if (peer && content.trim() && (await hasPublicKey(peer.id))) {
        try {
          const cipher = await encryptForUser(peer.id, content);
          body = { ...body, content: "", cipher };
        } catch {
          set({ error: "Échec du chiffrement du message." });
          return false;
        }
      }
      await api.sendMessage(channelId, body);
      // Le message réel arrivera par la Gateway (MESSAGE_CREATE).
      return true;
    } catch (e) {
      set({ error: errMsg(e) });
      return false;
    }
  },

  playSoundboard: async (sound) => {
    if (!voiceConn) {
      set({ error: "Rejoins un salon vocal pour jouer un son." });
      return;
    }
    try {
      await voiceConn.playSound(mediaUrl(`/api/soundboard-sounds/${sound.id}/audio`), sound.volume);
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  editMessage: async (channelId, messageId, content) => {
    try {
      // MP 1:1 chiffré : on ré-chiffre l'édition (sinon le texte clair fuirait côté serveur et
      // l'ancien blob réécraserait l'affichage au rechargement).
      const peer = dmPeer(get, channelId);
      if (peer && content.trim() && (await hasPublicKey(peer.id))) {
        try {
          const cipher = await encryptForUser(peer.id, content);
          await api.editMessage(channelId, messageId, { content: "", cipher });
          return;
        } catch {
          set({ error: "Échec du chiffrement du message." });
          return;
        }
      }
      await api.editMessage(channelId, messageId, { content });
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  deleteMessage: async (channelId, messageId) => {
    try {
      await api.deleteMessage(channelId, messageId);
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  toggleReaction: async (channelId, messageId, emoji, has) => {
    try {
      if (has) await api.removeReaction(channelId, messageId, emoji);
      else await api.addReaction(channelId, messageId, emoji);
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  castVote: async (channelId, messageId, answerIds) => {
    try {
      const poll = await api.castVote(channelId, messageId, answerIds);
      set((s) => setMessagePoll(s, channelId, messageId, poll));
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },
}));

// ───────────────────────────── Gateway ─────────────────────────────

function startGateway(
  set: (partial: Partial<State> | ((s: State) => Partial<State>)) => void,
  get: () => State,
): void {
  if (!getAccessToken()) return;
  gateway?.close();
  gateway = new Gateway(
    () => getAccessToken(),
    (ev) => applyEvent(ev, set, get),
    (status: GatewayStatus) => set({ gatewayStatus: status }),
    {
      refreshAuth: () => refreshTokens(),
      onAuthLost: () => get().logout(),
    },
  );
  gateway.connect();
}

// Refetch des salons débouncé par guilde : une mutation de permission peut changer la visibilité
// (overwrites) — on laisse le serveur retrancher, sans marteler à chaque événement.
const channelRefreshTimers = new Map<Snowflake, ReturnType<typeof setTimeout>>();
function scheduleChannelRefresh(get: () => State, guildId: Snowflake): void {
  if (!get().channelsByGuild[guildId]) return;
  const existing = channelRefreshTimers.get(guildId);
  if (existing) clearTimeout(existing);
  channelRefreshTimers.set(
    guildId,
    setTimeout(() => {
      channelRefreshTimers.delete(guildId);
      void get().refreshChannels(guildId);
    }, 250),
  );
}

// Applique un MESSAGE_CREATE (déjà déchiffré le cas échéant) : non-lus, typing, insertion triée,
// ack et notification. Extrait pour permettre l'attente du déchiffrement E2EE avant application.
function applyMessageCreate(
  m: Message,
  set: (partial: Partial<State> | ((s: State) => Partial<State>)) => void,
  get: () => State,
): void {
  const me = get().me;
  const active = activeChannelId(get()) === m.channel_id;
  const mine = m.author.id === me?.id;
  const mentionsMe =
    !!me &&
    (m.content.includes(`<@${me.id}>`) ||
      m.content.includes(`<@!${me.id}>`) ||
      m.content.includes("@everyone") ||
      m.content.includes("@here"));

  set((s) => {
    // Met à jour le dernier message du salon (pour le calcul des non-lus).
    const channelsByGuild = bumpChannelLastMessage(s.channelsByGuild, m.channel_id, m.id);
    // Marqueur de lecture : auto-lu si actif, sinon comptage des mentions.
    let readStates = s.readStates;
    if (active || mine) {
      readStates = {
        ...readStates,
        [m.channel_id]: { channel_id: m.channel_id, last_read_id: m.id, mention_count: 0 },
      };
    } else if (mentionsMe) {
      const cur = readStates[m.channel_id];
      readStates = {
        ...readStates,
        [m.channel_id]: {
          channel_id: m.channel_id,
          last_read_id: cur?.last_read_id ?? "0",
          mention_count: (cur?.mention_count ?? 0) + 1,
        },
      };
    }

    // Le message arrivé → l'indicateur « écrit… » de l'auteur n'a plus lieu d'être.
    let typing = s.typing;
    const chTyping = typing[m.channel_id];
    if (chTyping && m.author.id in chTyping) {
      const next = { ...chTyping };
      delete next[m.author.id];
      typing = { ...typing, [m.channel_id]: next };
    }

    // Bump aussi le dernier message des MP (non couverts par channelsByGuild).
    let dms = s.dms;
    const di = dms.findIndex((d) => d.id === m.channel_id);
    if (di !== -1 && dms[di].last_message_id !== m.id) {
      dms = [...dms];
      dms[di] = { ...dms[di], last_message_id: m.id };
    }

    const list = s.messagesByChannel[m.channel_id];
    if (!list || list.some((x) => x.id === m.id)) {
      return { channelsByGuild, readStates, typing, dms };
    }
    // Insertion triée par snowflake : un message arrivé hors-ordre (latence Gateway) se range
    // à sa place chronologique au lieu d'être collé en fin.
    return {
      channelsByGuild,
      readStates,
      typing,
      dms,
      messagesByChannel: { ...s.messagesByChannel, [m.channel_id]: sortMessages([...list, m]) },
    };
  });

  // Si le salon est actif, persiste la lecture côté serveur.
  if (active && !mine) void api.ackMessage(m.channel_id, m.id).catch(() => {});

  // Notification bureau (mentions + MP), si activée et pertinent.
  if (!mine) maybeNotify(get(), m, { mentionsMe, active });
}

function applyEvent(
  ev: GatewayEvent,
  set: (partial: Partial<State> | ((s: State) => Partial<State>)) => void,
  get: () => State,
): void {
  switch (ev.t) {
    case "MESSAGE_CREATE": {
      const raw = ev.d as Message;
      // MP chiffré : on déchiffre AVANT d'appliquer → notif/affichage cohérents, jamais de blob brut.
      if (raw.cipher || raw.referenced_message?.cipher) {
        void decryptMessage(get, raw.channel_id, raw).then((m) => applyMessageCreate(m, set, get));
      } else {
        applyMessageCreate(raw, set, get);
      }
      break;
    }
    case "MESSAGE_UPDATE": {
      const raw = ev.d as Message;
      const apply = (m: Message) =>
        set((s) => {
          const list = s.messagesByChannel[m.channel_id];
          if (!list) return {};
          return {
            messagesByChannel: {
              ...s.messagesByChannel,
              [m.channel_id]: list.map((x) => (x.id === m.id ? m : x)),
            },
          };
        });
      // Édition d'un MP chiffré : déchiffrer avant d'appliquer.
      if (raw.cipher || raw.referenced_message?.cipher) {
        void decryptMessage(get, raw.channel_id, raw).then(apply);
      } else apply(raw);
      break;
    }
    case "MESSAGE_DELETE": {
      const d = ev.d as { id: Snowflake; channel_id: Snowflake };
      set((s) => {
        const list = s.messagesByChannel[d.channel_id];
        if (!list) return {};
        return {
          messagesByChannel: {
            ...s.messagesByChannel,
            [d.channel_id]: list.filter((x) => x.id !== d.id),
          },
        };
      });
      break;
    }
    case "MESSAGE_POLL_VOTE": {
      const d = ev.d as { channel_id: Snowflake; message_id: Snowflake };
      // Recharge le sondage (les décomptes + `me_voted` sont propres au spectateur).
      void api
        .getPoll(d.channel_id, d.message_id)
        .then((poll) => set((s) => setMessagePoll(s, d.channel_id, d.message_id, poll)))
        .catch(() => {});
      break;
    }
    case "MESSAGE_REACTION_ADD":
    case "MESSAGE_REACTION_REMOVE": {
      const d = ev.d as {
        channel_id: Snowflake;
        message_id: Snowflake;
        user_id: Snowflake;
        emoji: string;
      };
      const isAdd = ev.t === "MESSAGE_REACTION_ADD";
      const mine = d.user_id === get().me?.id;
      set((s) => {
        const list = s.messagesByChannel[d.channel_id];
        if (!list) return {};
        return {
          messagesByChannel: {
            ...s.messagesByChannel,
            [d.channel_id]: list.map((m) =>
              m.id === d.message_id
                ? { ...m, reactions: applyReaction(m.reactions, d.emoji, isAdd, mine) }
                : m,
            ),
          },
        };
      });
      break;
    }
    case "TYPING_START": {
      const d = ev.d as { channel_id: Snowflake; user_id: Snowflake };
      if (d.user_id === get().me?.id) break;
      const expiry = performance.now() + 8000;
      set((s) => ({
        typing: {
          ...s.typing,
          [d.channel_id]: { ...(s.typing[d.channel_id] ?? {}), [d.user_id]: expiry },
        },
      }));
      break;
    }
    case "PRESENCE_UPDATE": {
      const d = ev.d as { user_id: Snowflake; status: string; custom_status?: string | null };
      set((s) => ({
        presences: { ...s.presences, [d.user_id]: d.status },
        customStatus: { ...s.customStatus, [d.user_id]: d.custom_status ?? null },
      }));
      break;
    }
    case "VOICE_STATE_UPDATE": {
      const vs = ev.d as VoiceState;
      const gid = vs.guild_id;
      const mv = get().myVoice;
      const myId = get().me?.id;

      // ── Mon propre état vocal (modération serveur appliquée sur MOI) ──
      // Le serveur émet aussi MON VOICE_STATE_UPDATE. Sans traitement, une déconnexion/déplacement
      // de modérateur me laisserait en connexion fantôme et un force-mute serveur ne couperait pas
      // mon micro (contournement de modération).
      if (mv && vs.user_id === myId && vs.guild_id === mv.guildId) {
        if (!vs.channel_id) {
          // Déconnexion forcée par un modérateur : on quitte proprement (ferme voiceConn + SFU).
          void get().leaveVoiceChannel();
          break;
        }
        if (vs.channel_id !== mv.channelId) {
          // Déplacement forcé par un modérateur : on rejoint le nouveau salon (façon Discord).
          void get().joinVoice(mv.guildId, vs.channel_id);
          break;
        }
        // Toujours dans mon salon : refléter le mute/sourdine serveur sur le gate + l'affichage.
        const serverMute = !!vs.mute;
        const serverDeaf = !!vs.deaf;
        if (serverMute !== mv.serverMute || serverDeaf !== mv.serverDeaf) {
          set({ myVoice: { ...mv, serverMute, serverDeaf } });
          if (serverDeaf !== mv.serverDeaf) voiceConn?.setDeafened(serverDeaf || mv.selfDeaf);
          applyMicGate();
        }
        // On ne sort pas : on laisse aussi le set ci-dessous tenir voiceStatesByGuild à jour.
      }

      const wasInMyChannel =
        !!mv &&
        (get().voiceStatesByGuild[gid] ?? []).some(
          (x) => x.user_id === vs.user_id && x.channel_id === mv.channelId,
        );
      // L'utilisateur a-t-il quitté MON salon (déconnexion vocale ou déplacement ailleurs) ?
      // Dans ce cas on purge proactivement ses tuiles vidéo et son état « parle » : ne pas
      // attendre `track.onended` (qui ne se déclenche pas toujours sur une coupure brutale),
      // sinon une tuile noire fantôme persiste après le départ du streamer.
      const leftMyChannel =
        !!mv && vs.user_id !== myId && wasInMyChannel && vs.channel_id !== mv.channelId;
      set((s) => {
        const list = (s.voiceStatesByGuild[gid] ?? []).filter((x) => x.user_id !== vs.user_id);
        // channel_id nul ⇒ l'utilisateur a quitté le vocal.
        const next = vs.channel_id ? [...list, vs] : list;
        const patch: Partial<State> = {
          voiceStatesByGuild: { ...s.voiceStatesByGuild, [gid]: next },
        };
        if (leftMyChannel) {
          patch.voiceVideos = s.voiceVideos.filter((v) => v.userId !== vs.user_id);
          if (s.speaking[vs.user_id]) {
            const speaking = { ...s.speaking };
            delete speaking[vs.user_id];
            patch.speaking = speaking;
          }
        }
        return patch;
      });
      // Un AUTRE membre vient de rejoindre mon salon. Avec la renégociation poussée (WS sain),
      // le SFU nous envoie ses pistes en direct → AUCUNE reconnexion. On ne resync (repli
      // historique) que si la signalisation est indisponible.
      if (
        mv &&
        vs.user_id !== myId &&
        vs.channel_id === mv.channelId &&
        !wasInMyChannel &&
        !get().voiceConnecting &&
        !(voiceConn && voiceConn.signalingHealthy())
      ) {
        scheduleVoiceResync(get);
      }
      break;
    }
    case "VOICE_SPEAKING": {
      // Indicateur « parle » d'un AUTRE membre, diffusé par le serveur. Source autoritative pour
      // tout le monde sauf moi (mon propre indicateur est piloté localement par onSpeaking).
      const d = ev.d as { user_id: Snowflake; speaking: boolean };
      if (d.user_id === get().me?.id) break;
      set((s) =>
        !!s.speaking[d.user_id] === d.speaking
          ? {}
          : { speaking: { ...s.speaking, [d.user_id]: d.speaking } },
      );
      break;
    }
    case "GUILD_CREATE": {
      const g = ev.d as Guild;
      set((s) =>
        s.guilds.some((x) => x.id === g.id) ? {} : { guilds: [...s.guilds, g] },
      );
      break;
    }
    case "GUILD_DELETE": {
      const d = ev.d as { id: Snowflake };
      set((s) => ({ guilds: s.guilds.filter((x) => x.id !== d.id) }));
      break;
    }
    case "GUILD_UPDATE": {
      const g = ev.d as Guild;
      set((s) => ({ guilds: s.guilds.map((x) => (x.id === g.id ? g : x)) }));
      break;
    }
    // ── Rôles & appartenance : mises à jour EN DIRECT (hot-swap) → les permissions effectives
    // (permsIn/canIn) et tout l'affichage gated se recalculent immédiatement. ──
    case "GUILD_ROLE_CREATE":
    case "GUILD_ROLE_UPDATE": {
      const r = ev.d as Role;
      const gid = r.guild_id;
      set((s) => {
        const list = s.rolesByGuild[gid];
        if (!list) return {};
        const next = list.some((x) => x.id === r.id)
          ? list.map((x) => (x.id === r.id ? r : x))
          : [...list, r];
        return { rolesByGuild: { ...s.rolesByGuild, [gid]: next } };
      });
      // Un changement de permissions de rôle peut modifier ma visibilité des salons (overwrites).
      scheduleChannelRefresh(get, gid);
      break;
    }
    case "GUILD_ROLE_DELETE": {
      const d = ev.d as { role_id: Snowflake; guild_id: Snowflake };
      set((s) => {
        const out: Partial<State> = {};
        const list = s.rolesByGuild[d.guild_id];
        if (list) {
          out.rolesByGuild = {
            ...s.rolesByGuild,
            [d.guild_id]: list.filter((x) => x.id !== d.role_id),
          };
        }
        const members = s.membersByGuild[d.guild_id];
        if (members) {
          out.membersByGuild = {
            ...s.membersByGuild,
            [d.guild_id]: members.map((m) => ({
              ...m,
              roles: m.roles.filter((rid) => rid !== d.role_id),
            })),
          };
        }
        return out;
      });
      scheduleChannelRefresh(get, d.guild_id);
      break;
    }
    case "GUILD_MEMBER_UPDATE": {
      const d = ev.d as {
        guild_id: Snowflake;
        user_id: Snowflake;
        role_id?: Snowflake;
        added?: boolean;
      };
      // Forme « modération » (changement de surnom / timeout) : pas de role_id → re-fetch.
      const rid = d.role_id;
      if (!rid) {
        if (get().membersByGuild[d.guild_id]) void get().refreshMembers(d.guild_id);
        break;
      }
      set((s) => {
        const members = s.membersByGuild[d.guild_id];
        if (!members) return {};
        return {
          membersByGuild: {
            ...s.membersByGuild,
            [d.guild_id]: members.map((m) =>
              m.user.id === d.user_id
                ? {
                    ...m,
                    roles: d.added
                      ? [...new Set([...m.roles, rid])]
                      : m.roles.filter((r) => r !== rid),
                  }
                : m,
            ),
          },
        };
      });
      // Si c'est MON rôle qui change, ma visibilité des salons peut changer → refetch serveur.
      if (d.user_id === get().me?.id) scheduleChannelRefresh(get, d.guild_id);
      break;
    }
    case "GUILD_MEMBER_ADD":
    case "GUILD_MEMBER_REMOVE": {
      const d = ev.d as { guild_id: Snowflake };
      if (get().membersByGuild[d.guild_id]) void get().refreshMembers(d.guild_id);
      break;
    }
    // Visibilité d'un salon potentiellement modifiée (overwrites) : refetch de la liste filtrée
    // par le serveur (autorité). Pas de payload de salon → on ne touche pas au cache directement.
    case "CHANNEL_PERMISSIONS_UPDATE": {
      const d = ev.d as { guild_id: Snowflake };
      scheduleChannelRefresh(get, d.guild_id);
      break;
    }
    case "CHANNEL_CREATE":
    case "CHANNEL_UPDATE": {
      const c = ev.d as Channel;
      // Garde anti-stub : un payload sans nom/type n'est pas un vrai salon (ne pas écraser le cache).
      if (c.guild_id && (c.name === undefined || c.type === undefined)) {
        scheduleChannelRefresh(get, c.guild_id);
        break;
      }
      if (!c.guild_id) {
        // MP / groupe : quelqu'un vient d'ouvrir une conversation avec nous → elle doit
        // apparaître EN DIRECT dans la liste (sinon le premier message est invisible).
        const dm = ev.d as DMChannel;
        set((s) =>
          s.dms.some((x) => x.id === dm.id)
            ? { dms: s.dms.map((x) => (x.id === dm.id ? { ...x, ...dm } : x)) }
            : { dms: [dm, ...s.dms] },
        );
        break;
      }
      const gid = c.guild_id;
      // Fil (type 11/12) : il vit dans threadsByChannel, pas channelsByGuild → on patche là.
      if (c.type === CH_THREAD_PUBLIC || c.type === CH_THREAD_PRIVATE) {
        set((s) => {
          let changed = false;
          const next: Record<Snowflake, Channel[]> = {};
          for (const [pid, list] of Object.entries(s.threadsByChannel)) {
            if (list.some((t) => t.id === c.id)) {
              next[pid] = list.map((t) => (t.id === c.id ? c : t));
              changed = true;
            } else {
              next[pid] = list;
            }
          }
          return changed ? { threadsByChannel: next } : {};
        });
        break;
      }
      set((s) => {
        const list = s.channelsByGuild[gid];
        if (!list) return {};
        const next = ev.t === "CHANNEL_CREATE"
          ? [...list.filter((x) => x.id !== c.id), c]
          : list.map((x) => (x.id === c.id ? c : x));
        return { channelsByGuild: { ...s.channelsByGuild, [gid]: sortChannels(next) } };
      });
      break;
    }
    case "CHANNEL_DELETE": {
      const c = ev.d as Channel;
      if (!c.guild_id) break;
      const gid = c.guild_id;
      set((s) => {
        const list = s.channelsByGuild[gid];
        if (!list) return {};
        return {
          channelsByGuild: { ...s.channelsByGuild, [gid]: list.filter((x) => x.id !== c.id) },
        };
      });
      break;
    }
    // Réordonnancement / déplacement entre catégories (positions + parents) — en direct.
    case "CHANNELS_REORDER": {
      const d = ev.d as {
        guild_id: Snowflake;
        positions: { id: Snowflake; position: number; parent_id: Snowflake | null }[];
      };
      const gid = d.guild_id;
      set((s) => {
        const list = s.channelsByGuild[gid];
        if (!list) return {};
        const map = new Map(d.positions.map((p) => [p.id, p]));
        const next = list.map((c) => {
          const p = map.get(c.id);
          return p ? { ...c, position: p.position, parent_id: p.parent_id } : c;
        });
        return { channelsByGuild: { ...s.channelsByGuild, [gid]: sortChannels(next) } };
      });
      break;
    }
    case "RELATIONSHIP_ADD":
    case "RELATIONSHIP_REMOVE": {
      void get().refreshRelationships();
      break;
    }
    // Un fil créé (par quiconque) apparaît en direct sous son salon parent.
    case "THREAD_CREATE": {
      const c = ev.d as Channel;
      if (!c.parent_id) break;
      const pid = c.parent_id;
      set((s) => {
        const list = s.threadsByChannel[pid];
        if (!list || list.some((t) => t.id === c.id)) return {};
        return { threadsByChannel: { ...s.threadsByChannel, [pid]: [c, ...list] } };
      });
      break;
    }
    // Suppression en masse (modération) : retire d'un coup tous les messages visés.
    case "MESSAGE_DELETE_BULK": {
      const d = ev.d as { channel_id: Snowflake; ids: Snowflake[] };
      set((s) => {
        const list = s.messagesByChannel[d.channel_id];
        if (!list) return {};
        const gone = new Set(d.ids);
        return {
          messagesByChannel: {
            ...s.messagesByChannel,
            [d.channel_id]: list.filter((m) => !gone.has(m.id)),
          },
        };
      });
      break;
    }
    // Épinglage : met à jour le drapeau du message chargé (indicateur en direct).
    case "CHANNEL_PINS_UPDATE": {
      const d = ev.d as { channel_id: Snowflake; message_id?: Snowflake; pinned?: boolean };
      if (!d.message_id) break;
      set((s) => {
        const list = s.messagesByChannel[d.channel_id];
        if (!list) return {};
        return {
          messagesByChannel: {
            ...s.messagesByChannel,
            [d.channel_id]: list.map((m) =>
              m.id === d.message_id ? { ...m, pinned: !!d.pinned } : m,
            ),
          },
        };
      });
      break;
    }
    // Composition d'un groupe MP modifiée : resynchronise la liste des conversations.
    case "CHANNEL_RECIPIENT_ADD":
    case "CHANNEL_RECIPIENT_REMOVE": {
      const d = ev.d as { channel_id: Snowflake; user_id: Snowflake };
      const me = get().me;
      if (ev.t === "CHANNEL_RECIPIENT_REMOVE" && me && d.user_id === me.id) {
        // C'est NOUS qu'on retire : la conversation disparaît (et on la quitte si active).
        set((s) => ({
          dms: s.dms.filter((x) => x.id !== d.channel_id),
          activeDM: s.activeDM === d.channel_id ? null : s.activeDM,
        }));
        break;
      }
      void get().refreshDMs();
      break;
    }
    // Émojis personnalisés modifiés : rafraîchit le picker en direct.
    case "GUILD_EMOJIS_UPDATE": {
      const d = ev.d as { guild_id: Snowflake };
      if (!get().emojisByGuild[d.guild_id]) break;
      void api
        .listEmojis(d.guild_id)
        .then((emojis) =>
          set((s) => ({ emojisByGuild: { ...s.emojisByGuild, [d.guild_id]: emojis } })),
        )
        .catch(() => {});
      break;
    }
    // Autocollants modifiés : rafraîchit le picker / l'onglet de gestion en direct.
    case "GUILD_STICKERS_UPDATE": {
      const d = ev.d as { guild_id: Snowflake };
      if (!get().stickersByGuild[d.guild_id]) break;
      void api
        .listStickers(d.guild_id)
        .then((stickers) =>
          set((s) => ({ stickersByGuild: { ...s.stickersByGuild, [d.guild_id]: stickers } })),
        )
        .catch(() => {});
      break;
    }
    // Soundboard modifié : rafraîchit la grille de sons en direct.
    case "GUILD_SOUNDBOARD_UPDATE": {
      const d = ev.d as { guild_id: Snowflake };
      if (!get().soundsByGuild[d.guild_id]) break;
      void api
        .listSounds(d.guild_id)
        .then((sounds) =>
          set((s) => ({ soundsByGuild: { ...s.soundsByGuild, [d.guild_id]: sounds } })),
        )
        .catch(() => {});
      break;
    }
    // Lecture synchronisée depuis une AUTRE session du même compte : efface le badge ici aussi.
    case "MESSAGE_ACK": {
      const d = ev.d as { channel_id: Snowflake; last_read_id: Snowflake };
      set((s) => ({
        readStates: {
          ...s.readStates,
          [d.channel_id]: {
            channel_id: d.channel_id,
            last_read_id: d.last_read_id,
            mention_count: 0,
          },
        },
      }));
      break;
    }
    case "GUILD_ACK": {
      const d = ev.d as { guild_id: Snowflake };
      set((s) => {
        const channels = s.channelsByGuild[d.guild_id];
        if (!channels) return {};
        const readStates = { ...s.readStates };
        for (const c of channels) {
          if (c.last_message_id) {
            readStates[c.id] = {
              channel_id: c.id,
              last_read_id: c.last_message_id,
              mention_count: 0,
            };
          }
        }
        return { readStates };
      });
      break;
    }
    // Profil public (pseudo/avatar) mis à jour EN DIRECT partout où l'utilisateur est affiché :
    // soi, amis, destinataires de MP, membres de guilde, et auteurs des messages déjà chargés.
    case "USER_UPDATE": {
      const u = ev.d as User;
      set((s) => {
        const patch: Partial<State> = {};
        if (s.me?.id === u.id) patch.me = { ...s.me, ...u };
        if (s.relationships.some((r) => r.user.id === u.id)) {
          patch.relationships = s.relationships.map((r) =>
            r.user.id === u.id ? { ...r, user: { ...r.user, ...u } } : r,
          );
        }
        if (s.dms.some((d) => d.recipients.some((r) => r.id === u.id))) {
          patch.dms = s.dms.map((d) =>
            d.recipients.some((r) => r.id === u.id)
              ? { ...d, recipients: d.recipients.map((r) => (r.id === u.id ? { ...r, ...u } : r)) }
              : d,
          );
        }
        let membersChanged = false;
        const mbg: Record<Snowflake, Member[]> = {};
        for (const gid in s.membersByGuild) {
          const list = s.membersByGuild[gid];
          if (list.some((m) => m.user.id === u.id)) {
            mbg[gid] = list.map((m) =>
              m.user.id === u.id ? { ...m, user: { ...m.user, ...u } } : m,
            );
            membersChanged = true;
          } else {
            mbg[gid] = list;
          }
        }
        if (membersChanged) patch.membersByGuild = mbg;
        let msgsChanged = false;
        const mbc: Record<Snowflake, Message[]> = {};
        for (const cid in s.messagesByChannel) {
          const list = s.messagesByChannel[cid];
          if (list.some((m) => m.author.id === u.id)) {
            mbc[cid] = list.map((m) =>
              m.author.id === u.id ? { ...m, author: { ...m.author, ...u } } : m,
            );
            msgsChanged = true;
          } else {
            mbc[cid] = list;
          }
        }
        if (msgsChanged) patch.messagesByChannel = mbc;
        return patch;
      });
      break;
    }
  }
}

// ───────────────────────────── Utilitaires ─────────────────────────────

function mergePresences(
  current: Record<Snowflake, string>,
  list: PresenceView[],
): Record<Snowflake, string> {
  const next = { ...current };
  for (const p of list) next[p.user_id] = p.status;
  return next;
}

function mergeCustomStatus(
  current: Record<Snowflake, string | null>,
  list: PresenceView[],
): Record<Snowflake, string | null> {
  const next = { ...current };
  for (const p of list) next[p.user_id] = p.custom_status ?? null;
  return next;
}

function errMsg(e: unknown): string {
  if (e instanceof Error) return e.message;
  return String(e);
}

type SetFn = (partial: Partial<State> | ((s: State) => Partial<State>)) => void;
type GetFn = () => State;

// Resync débounce : le SFU ne pousse pas les nouvelles pistes, donc quand un
// membre rejoint notre salon on se reconnecte (coalescé) pour récupérer son flux.
let voiceResyncTimer: ReturnType<typeof setTimeout> | null = null;
// Back-off de reconnexion : si la signalisation échoue en boucle (ex. upgrade WS systématiquement
// rejeté par un proxy), on espace les tentatives et on abandonne après un plafond, au lieu de
// reconnecter toutes les ~700 ms à l'infini (re-getUserMedia + churn SFU). Le compteur retombe à
// zéro dès qu'une fenêtre sans nouvel échec s'écoule (cf. resetVoiceResyncOnSuccess).
let voiceResyncFails = 0;
let voiceResyncOkTimer: ReturnType<typeof setTimeout> | null = null;
const VOICE_RESYNC_MAX = 6;
// Mémorise l'état micro avant la sourdine, pour le restaurer à la réactivation.
let preDeafMute = false;
function scheduleVoiceResync(get: GetFn): void {
  if (voiceResyncTimer) clearTimeout(voiceResyncTimer);
  if (voiceResyncFails >= VOICE_RESYNC_MAX) {
    // Trop d'échecs rapprochés : on cesse de reconnecter et on signale l'état dégradé.
    useStore.setState({
      error: "Connexion vocale instable — reconnexion abandonnée. Réessaie via Resynchroniser.",
    });
    return;
  }
  // 700 ms, 1.4 s, 2.8 s… plafonné à ~8 s.
  const delay = Math.min(700 * 2 ** voiceResyncFails, 8000);
  voiceResyncFails += 1;
  voiceResyncTimer = setTimeout(() => {
    voiceResyncTimer = null;
    // Re-vérifie au déclenchement : un toggle vidéo/écran a pu lancer un rejoin entre-temps.
    if (!get().voiceConnecting) void get().reconnectVoice();
    // Si aucune nouvelle tentative n'est planifiée dans les 10 s qui suivent, c'est que la
    // connexion tient : on remet le compteur d'échecs à zéro.
    if (voiceResyncOkTimer) clearTimeout(voiceResyncOkTimer);
    voiceResyncOkTimer = setTimeout(() => {
      voiceResyncFails = 0;
      voiceResyncOkTimer = null;
    }, 10000);
  }, delay);
}

// Fabrique une connexion vocale dont les pistes vidéo distantes alimentent le store.
// Applique d'emblée la sensibilité VAD courante et réinitialise le gate : les DEUX chemins de
// connexion (joinVoice / rejoinMedia) partagent ainsi exactement le même setup.
function newVoiceConn(get: GetFn, set: SetFn): VoiceConnection {
  const conn = new VoiceConnection({
    onVideoTrack: (v) =>
      set((s) => ({ voiceVideos: [...s.voiceVideos.filter((x) => x.trackId !== v.trackId), v] })),
    onVideoEnded: (trackId) =>
      set((s) => ({ voiceVideos: s.voiceVideos.filter((x) => x.trackId !== trackId) })),
    onSpeaking: (userId, on) => {
      const myId = get().me?.id;
      // L'état « parle » des AUTRES est désormais autoritatif via le broadcast Gateway
      // (VOICE_SPEAKING) — il marche même quand on ne reçoit pas leur audio (sourd, volume 0).
      // La détection locale ne pilote donc QUE mon propre indicateur (+ le gate de voix).
      if (userId !== myId) return;

      vadOpen = on;
      applyMicGate();
      // N'allume l'anneau « parle » que si mon micro est RÉELLEMENT ouvert : en PTT sans tenir la
      // touche, le détecteur (clone non gaté) capte ma voix mais rien n'est transmis.
      let effective = on;
      if (get().mediaPrefs.inputMode === "ptt" && !pttHeld) effective = false;
      const mute = get().myVoice?.selfMute || get().myVoice?.serverMute;
      if (mute) effective = false; // muet ⇒ jamais « parle »

      set((s) =>
        !!s.speaking[userId] === effective
          ? {}
          : { speaking: { ...s.speaking, [userId]: effective } },
      );
      // Diffuse mon état aux autres membres du salon (ne fire que sur transition → throttlé).
      gateway?.sendVoiceSpeaking(effective);
    },
    // Repli : si la renégociation/WS échoue, on reconnecte complètement (comportement historique).
    onNeedsReconnect: () => {
      if (!get().voiceConnecting) scheduleVoiceResync(get);
    },
  });
  // En mode voix, on démarre gate FERMÉ (vadOpen=false) : le micro ne s'ouvre qu'au 1er onSpeaking,
  // évitant une brève fenêtre d'émission de silence/bruit après chaque (re)connexion. (En PTT,
  // vadOpen est ignoré par applyMicGate, donc la valeur importe peu.)
  vadOpen = false;
  conn.setSensitivity({
    auto: get().mediaPrefs.vadAuto,
    threshold: get().mediaPrefs.vadThreshold,
  });
  return conn;
}

// Message d'erreur vocal lisible (permission média, SFU indisponible…).
function voiceErr(e: unknown): string {
  if (typeof DOMException !== "undefined" && e instanceof DOMException) {
    if (e.name === "NotAllowedError" || e.name === "SecurityError")
      return "Accès au micro/caméra refusé.";
    if (e.name === "NotFoundError" || e.name === "OverconstrainedError")
      return "Aucun micro/caméra détecté.";
  }
  const m = errMsg(e);
  if (m.includes("SFU 401") || m.includes("SFU 403")) return "Jeton vocal rejeté par le SFU.";
  if (/SFU 5\d\d/.test(m) || m.includes("Failed to fetch"))
    return "Nœud vocal indisponible (lance ozone-sfu).";
  return `Connexion vocale échouée : ${m}`;
}

// Reconnecte le média en cours avec/sans vidéo. Le SFU ne renégocie pas : pour
// (dés)activer la caméra ou récupérer les pistes d'un nouvel arrivant, on relance
// la connexion (le salon vocal logique reste inchangé côté serveur).
async function rejoinMedia(get: GetFn, set: SetFn, withVideo: boolean): Promise<void> {
  const mv = get().myVoice;
  if (!mv) return;
  set({ voiceConnecting: true });
  try {
    if (voiceConn) await voiceConn.close();
    set({ voiceVideos: [], localVideo: null });
    const resp = await api.updateVoiceState(mv.guildId, {
      channel_id: mv.channelId,
      self_video: withVideo,
    });
    const token = resp?.connection?.token ?? "";
    if (!token) throw new Error("jeton vocal absent (serveur)");
    voiceConn = newVoiceConn(get, set);
    voiceConn.seedUserVolumes(get().userVolumes);
    voiceConn.seedStreamVolumes(get().streamVolumes);
    await voiceConn.connect(mv.channelId, token, {
      withVideo,
      selfId: get().me?.id ?? "",
      screen: get().localScreen,
    });
    voiceConn.setDeafened(mv.selfDeaf);
    set({
      myVoice: { ...mv, selfVideo: withVideo },
      localVideo: withVideo ? voiceConn.localStream : null,
      voiceConnecting: false,
    });
    applyMicGate();
  } catch (e) {
    set({ voiceConnecting: false, error: voiceErr(e) });
  }
}

// Notification pour un message entrant (mentions + MP), si activée et pertinent :
// son (WebAudio) et/ou notification bureau, selon les deux réglages indépendants.
function maybeNotify(s: State, m: Message, opts: { mentionsMe: boolean; active: boolean }): void {
  if (!s.desktopNotifications && !s.notifSounds) return;
  const isDM = s.dms.some((d) => d.id === m.channel_id);
  if (!opts.mentionsMe && !isDM) return;
  // Ne pas notifier le salon qu'on regarde déjà (fenêtre au premier plan).
  if (opts.active && typeof document !== "undefined" && !document.hidden) return;
  // Respect de la sourdine (salon, puis guilde).
  if (isMuted(s.notif, 1, m.channel_id)) return;
  let guildId: Snowflake | undefined;
  let channelName = "";
  for (const [gid, list] of Object.entries(s.channelsByGuild)) {
    const c = list.find((x) => x.id === m.channel_id);
    if (c) {
      guildId = gid;
      channelName = c.name;
      break;
    }
  }
  if (guildId && isMuted(s.notif, 0, guildId)) return;

  if (s.notifSounds) playNotifBeep();
  if (!s.desktopNotifications) return;
  if (typeof Notification === "undefined" || Notification.permission !== "granted") return;

  const author = m.author.display_name || m.author.username;
  const guildName = s.guilds.find((g) => g.id === guildId)?.name ?? "";
  const title = isDM ? author : `#${channelName}${guildName ? ` — ${guildName}` : ""}`;
  const body = (isDM ? m.content : `${author} : ${m.content}`).slice(0, 160) || "Nouveau message";
  try {
    const n = new Notification(title, { body, tag: m.channel_id });
    n.onclick = () => {
      window.focus();
      const st = useStore.getState();
      if (isDM) void st.openDM(m.channel_id);
      else if (guildId) void st.selectGuild(guildId).then(() => st.selectChannel(m.channel_id));
      n.close();
    };
  } catch {
    /* ignore */
  }
}

// Salon actuellement affiché (guilde sélectionnée ou MP actif).
function activeChannelId(s: State): Snowflake | null {
  if (s.view.kind === "guild") return s.selectedChannelByGuild[s.view.guildId] ?? null;
  return s.activeDM;
}

// Dernier message connu d'un salon (messages chargés, sinon last_message_id).
function latestMessageId(s: State, channelId: Snowflake): Snowflake | null {
  const msgs = s.messagesByChannel[channelId];
  if (msgs && msgs.length) return msgs[msgs.length - 1].id;
  for (const list of Object.values(s.channelsByGuild)) {
    const c = list.find((x) => x.id === channelId);
    if (c?.last_message_id) return c.last_message_id;
  }
  return s.dms.find((d) => d.id === channelId)?.last_message_id ?? null;
}

// Met à jour le last_message_id d'un salon dans channelsByGuild.
function bumpChannelLastMessage(
  channelsByGuild: Record<Snowflake, Channel[]>,
  channelId: Snowflake,
  msgId: Snowflake,
): Record<Snowflake, Channel[]> {
  for (const [gid, list] of Object.entries(channelsByGuild)) {
    if (list.some((c) => c.id === channelId)) {
      return {
        ...channelsByGuild,
        [gid]: list.map((c) => (c.id === channelId ? { ...c, last_message_id: msgId } : c)),
      };
    }
  }
  return channelsByGuild;
}

// Capture la frontière de non-lu à l'ouverture d'un salon (pour la barre « nouveaux messages »).
function captureUnreadAnchor(
  get: () => State,
  set: (partial: Partial<State> | ((s: State) => Partial<State>)) => void,
  channelId: Snowflake,
): void {
  const rs = get().readStates[channelId];
  const last = latestMessageId(get(), channelId);
  if (rs && last && idGt(last, rs.last_read_id)) {
    set((s) => ({ unreadAnchor: { ...s.unreadAnchor, [channelId]: rs.last_read_id } }));
  } else {
    set((s) => {
      if (!(channelId in s.unreadAnchor)) return {};
      const rest = { ...s.unreadAnchor };
      delete rest[channelId];
      return { unreadAnchor: rest };
    });
  }
}

// Remplace le sondage d'un message (vote / mise à jour).
function setMessagePoll(
  s: State,
  channelId: Snowflake,
  messageId: Snowflake,
  poll: Poll,
): Partial<State> {
  const list = s.messagesByChannel[channelId];
  if (!list) return {};
  return {
    messagesByChannel: {
      ...s.messagesByChannel,
      [channelId]: list.map((m) => (m.id === messageId ? { ...m, poll } : m)),
    },
  };
}

// Met à jour l'agrégat de réactions d'un message (ajout/retrait d'un emoji).
function applyReaction(
  reactions: Reaction[],
  emoji: string,
  isAdd: boolean,
  mine: boolean,
): Reaction[] {
  const arr = reactions.map((r) => ({ ...r }));
  const idx = arr.findIndex((r) => r.emoji === emoji);
  if (isAdd) {
    if (idx >= 0) {
      arr[idx].count += 1;
      if (mine) arr[idx].me = true;
    } else {
      arr.push({ emoji, count: 1, me: mine });
    }
  } else if (idx >= 0) {
    arr[idx].count -= 1;
    if (mine) arr[idx].me = false;
    if (arr[idx].count <= 0) arr.splice(idx, 1);
  }
  return arr;
}

// Helpers de présentation réutilisables.
export function channelTree(channels: Channel[]): {
  category: Channel | null;
  items: Channel[];
}[] {
  const groups: { category: Channel | null; items: Channel[] }[] = [];
  const uncategorized: Channel[] = [];
  const cats = channels.filter((c) => c.type === CH_CATEGORY);
  const others = channels.filter((c) => c.type !== CH_CATEGORY);

  for (const c of others) {
    if (!c.parent_id) uncategorized.push(c);
  }
  if (uncategorized.length) groups.push({ category: null, items: uncategorized });
  for (const cat of cats) {
    groups.push({
      category: cat,
      items: others.filter((c) => c.parent_id === cat.id),
    });
  }
  return groups;
}

// Plan de réordonnancement après un glisser-déposer : recalcule positions + parents pour TOUS
// les salons (catégories incluses), en ordre d'affichage (non-catégorisés, puis chaque catégorie
// et ses enfants). Fonction pure → facile à tester et à raisonner. `parent_id` null = racine.
export function reorderChannelPlan(
  channels: Channel[],
  dragId: Snowflake,
  target: { id: Snowflake; mode: "before" | "into" | "root" },
): { id: Snowflake; position: number; parent_id: Snowflake | null }[] {
  const isCat = (c: Channel) => c.type === CH_CATEGORY;
  const dragged = channels.find((c) => c.id === dragId);
  if (!dragged || dragId === target.id) return [];
  // Les salons sont déjà triés (position, id) dans le store ; on conserve cet ordre.
  const draggingCat = isCat(dragged);

  const rootChans = channels.filter((c) => !isCat(c) && !c.parent_id && c.id !== dragId);
  let cats = channels.filter((c) => isCat(c));
  const children: Record<string, Channel[]> = {};
  for (const cat of cats) {
    children[cat.id] = channels.filter((c) => !isCat(c) && c.parent_id === cat.id && c.id !== dragId);
  }

  if (draggingCat) {
    // Déplacement d'une catégorie : on conserve ses enfants attachés.
    cats = cats.filter((c) => c.id !== dragId);
    const idx = cats.findIndex((c) => c.id === target.id);
    if (target.mode === "before" && idx >= 0) cats.splice(idx, 0, dragged);
    else cats.push(dragged);
    if (!children[dragged.id])
      children[dragged.id] = channels.filter((c) => !isCat(c) && c.parent_id === dragId);
  } else if (target.mode === "root") {
    rootChans.push(dragged);
  } else if (target.mode === "into") {
    (children[target.id] ??= []).push(dragged);
  } else {
    // « before » un salon cible : on adopte le parent de la cible.
    const tgt = channels.find((c) => c.id === target.id);
    const parent = tgt?.parent_id ?? null;
    const list = parent ? (children[parent] ??= []) : rootChans;
    const i = list.findIndex((c) => c.id === target.id);
    if (i >= 0) list.splice(i, 0, dragged);
    else list.push(dragged);
  }

  const flat: { id: Snowflake; parent: Snowflake | null }[] = [];
  for (const c of rootChans) flat.push({ id: c.id, parent: null });
  for (const cat of cats) {
    flat.push({ id: cat.id, parent: null });
    for (const child of children[cat.id] ?? []) flat.push({ id: child.id, parent: cat.id });
  }
  return flat.map((x, i) => ({ id: x.id, position: i, parent_id: x.parent }));
}

export function isVoice(c: Channel): boolean {
  return c.type === CH_VOICE;
}

// Une portée (guilde/salon) est-elle en sourdine ?
export function isMuted(
  notif: Record<string, NotificationSetting>,
  scopeType: number,
  scopeId: Snowflake,
): boolean {
  const s = notif[`${scopeType}:${scopeId}`];
  if (!s || s.muted_until == null) return false;
  return s.muted_until < 0 || s.muted_until > Date.now();
}

// Comparaison de Snowflakes (chaînes numériques) par valeur.
export function idGt(a: Snowflake, b: Snowflake): boolean {
  try {
    return BigInt(a) > BigInt(b);
  } catch {
    return a.length === b.length ? a > b : a.length > b.length;
  }
}

// Un salon est non-lu si son dernier message dépasse le marqueur de lecture.
export function isChannelUnread(
  lastMessageId: Snowflake | null | undefined,
  rs?: ReadState,
): boolean {
  if (!lastMessageId) return false;
  if (!rs) return true;
  return idGt(lastMessageId, rs.last_read_id);
}

// Nombre de conversations MP non-lues (pour le badge du bouton Accueil du rail), plafonné à 99+.
export function unreadDmCount(
  dms: DMChannel[],
  readStates: Record<Snowflake, ReadState>,
): number {
  return dms.filter((d) => isChannelUnread(d.last_message_id, readStates[d.id])).length;
}

// Une guilde a-t-elle des salons non-lus (parmi ses salons chargés) ?
export function guildHasUnread(
  channels: Channel[] | undefined,
  readStates: Record<Snowflake, ReadState>,
): boolean {
  if (!channels) return false;
  return channels.some(
    (c) => c.type !== CH_CATEGORY && isChannelUnread(c.last_message_id, readStates[c.id]),
  );
}

// Total des mentions sur les salons (chargés) d'une guilde.
export function guildMentionCount(
  channels: Channel[] | undefined,
  readStates: Record<Snowflake, ReadState>,
): number {
  if (!channels) return 0;
  return channels.reduce((n, c) => n + (readStates[c.id]?.mention_count ?? 0), 0);
}

// Couleur d'un rôle (u32 décimal) en hex CSS ; 0 = pas de couleur.
export function roleColorHex(color: number): string {
  return "#" + (color & 0xffffff).toString(16).padStart(6, "0");
}

// Couleur du rôle le plus haut (position) ayant une couleur, pour un membre donné.
export function memberTopRoleColor(roles: Role[], member?: Member): string | null {
  if (!member || !roles.length) return null;
  const colored = roles.filter((r) => member.roles.includes(r.id) && r.color !== 0);
  if (!colored.length) return null;
  colored.sort((a, b) => b.position - a.position);
  return roleColorHex(colored[0].color);
}

// Rôle coloré le plus haut (objet complet) d'un membre — pour appliquer le style (dégradé, néon, vague).
export function memberTopColorRole(roles: Role[], member?: Member): Role | null {
  if (!member || !roles.length) return null;
  const colored = roles.filter((r) => member.roles.includes(r.id) && r.color !== 0);
  if (!colored.length) return null;
  colored.sort((a, b) => b.position - a.position);
  return colored[0];
}

// Style CSS d'un **nom coloré** selon le style du rôle (solid/gradient/neon/wave).
export interface RoleNameStyle {
  style: CSSProperties;
  className: string; // classe d'animation éventuelle (".role-name-wave")
  solid: string; // repli couleur unie (mentions, pastilles)
}

// Calcule le style d'affichage d'un nom à partir d'un rôle. `null` si aucune couleur.
export function roleNameStyle(role?: Role | null): RoleNameStyle | null {
  if (!role || !role.color) return null;
  const a = roleColorHex(role.color);
  const b = role.secondary_color ? roleColorHex(role.secondary_color) : a;
  const kind = role.color_style ?? "solid";
  if (kind === "gradient") {
    return { solid: a, className: "", style: gradientText(a, b, "90deg") };
  }
  if (kind === "wave") {
    return {
      solid: a,
      className: "role-name-wave",
      style: { ...gradientText(`${a}, ${b}, ${a}`, undefined, "90deg"), backgroundSize: "200% auto" },
    };
  }
  if (kind === "neon") {
    // Néon : lettres pleines (couleur vive, lisibles) + lueur **serrée** près des glyphes.
    // Petits rayons ⇒ ça « éclaire » le texte sans former de bloc ni de halo rectangulaire.
    return {
      solid: a,
      className: "",
      style: { color: a, textShadow: `0 0 2px ${a}, 0 0 5px ${a}b3` },
    };
  }
  return { solid: a, className: "", style: { color: a } };
}

// Style d'une **pastille** de couleur de rôle (point) — dégradé visible si applicable.
export function roleDotStyle(role?: Role | null): CSSProperties {
  if (!role || !role.color) return { backgroundColor: "#99aab5" };
  const a = roleColorHex(role.color);
  const kind = role.color_style ?? "solid";
  if ((kind === "gradient" || kind === "wave") && role.secondary_color) {
    return { backgroundImage: `linear-gradient(135deg, ${a}, ${roleColorHex(role.secondary_color)})` };
  }
  if (kind === "neon") return { backgroundColor: a, boxShadow: `0 0 5px ${a}` };
  return { backgroundColor: a };
}

// Fabrique un style de texte « rempli par dégradé » (background-clip: text).
function gradientText(colors: string, second: string | undefined, angle: string): CSSProperties {
  const stops = second ? `${colors}, ${second}` : colors;
  return {
    backgroundImage: `linear-gradient(${angle}, ${stops})`,
    WebkitBackgroundClip: "text",
    backgroundClip: "text",
    color: "transparent",
    WebkitTextFillColor: "transparent",
  };
}

// Permissions EFFECTIVES de l'utilisateur courant dans une guilde (bitfield).
// Propriétaire ou ADMINISTRATEUR ⇒ toutes les permissions. Sinon : OU des bits de @everyone
// (rôle dont l'id == guildId) et des rôles du membre.
export function permsIn(s: State, guildId: Snowflake): bigint {
  const me = s.me;
  const guild = s.guilds.find((g) => g.id === guildId);
  if (!me || !guild) return 0n;
  if (guild.owner_id === me.id) return PERM_ALL;
  const roles = s.rolesByGuild[guildId] ?? [];
  const member = s.membersByGuild[guildId]?.find((m) => m.user.id === me.id);
  const ids = new Set<string>([guildId, ...(member?.roles ?? [])]); // @everyone + rôles du membre
  let bits = 0n;
  for (const r of roles) {
    if (!ids.has(r.id)) continue;
    try {
      bits |= BigInt(r.permissions || "0");
    } catch {
      /* perms illisibles */
    }
  }
  return (bits & PERM.ADMINISTRATOR) !== 0n ? PERM_ALL : bits;
}

// L'utilisateur courant possède-t-il `bit` dans la guilde ?
export function canIn(s: State, guildId: Snowflake, bit: bigint): boolean {
  return (permsIn(s, guildId) & bit) === bit;
}
