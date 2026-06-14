// Client REST typé pour le serveur Ozone (ozone-api).
// Base `/api` (origine, proxy Vite en dev) ou `<instance>/api` (build empaqueté). Auth Bearer.

import { apiBase } from "./lib/instance";
import type {
  AddRelationship,
  Attachment,
  AuditLogEntry,
  AuditLogQuery,
  AutomodRule,
  CreateAutomodRule,
  UpdateAutomodRule,
  Ban,
  ChangeEmail,
  ChangePassword,
  Channel,
  CreateChannel,
  DeleteAccount,
  CreateDM,
  CreateGuild,
  CreateInvite,
  CreateEvent,
  CreateInstanceInvite,
  CreateMessage,
  CreatePoll,
  CreateRole,
  CreateEmoji,
  CreateSound,
  CreateSticker,
  CreateWebhook,
  DiscoveryGuild,
  DMChannel,
  EditMessage,
  Emoji,
  Guild,
  InstanceAdminConfig,
  InstanceInfo,
  InstanceInvite,
  InstanceUserView,
  Invite,
  InvitePreview,
  LoginRequest,
  Member,
  Message,
  ModerateVoiceState,
  NotificationSetting,
  PermissionOverwrite,
  Poll,
  PresenceView,
  ReadState,
  RegisterRequest,
  SetNotificationSetting,
  SetOverwrite,
  Relationship,
  Role,
  ScheduledEvent,
  SearchResponse,
  SetPresence,
  Snowflake,
  SoundboardSound,
  Sticker,
  TokenPair,
  UpdateChannel,
  UpdateEmoji,
  UpdateEvent,
  UpdateGuild,
  UpdateProfile,
  UpdateRole,
  UpdateSound,
  UpdateSticker,
  UpdateVoiceState,
  UpdateWebhook,
  User,
  UserProfile,
  UserSettings,
  VoiceJoinResponse,
  VoiceState,
  Webhook,
} from "./types";

// Base des routes REST : `/api` (origine) ou `<instance>/api` (.exe). Résolue à CHAQUE appel
// (l'URL d'instance peut être définie après le chargement initial du module).
const BASE = (): string => apiBase();

// ───────────────────────────── État du token ─────────────────────────────

let accessToken: string | null = null;

const TOKEN_KEY = "ozone.tokens";

export function setTokens(tokens: TokenPair | null): void {
  accessToken = tokens?.access_token ?? null;
  if (tokens) localStorage.setItem(TOKEN_KEY, JSON.stringify(tokens));
  else localStorage.removeItem(TOKEN_KEY);
  scheduleProactiveRefresh(tokens);
}

export function loadTokens(): TokenPair | null {
  const raw = localStorage.getItem(TOKEN_KEY);
  if (!raw) return null;
  try {
    const t = JSON.parse(raw) as TokenPair;
    accessToken = t.access_token;
    scheduleProactiveRefresh(t); // amorce le renouvellement dès le chargement de page
    return t;
  } catch {
    return null;
  }
}

export function getAccessToken(): string | null {
  return accessToken;
}

// Appelé quand le rafraîchissement est DÉFINITIVEMENT rejeté (session morte) → déconnexion.
let onAuthLost: (() => void) | null = null;
export function setAuthLostHandler(fn: (() => void) | null): void {
  onAuthLost = fn;
}

// Renouvellement PROACTIF : on rafraîchit le jeton AVANT son expiration (~80 % du TTL) → la
// session reste vivante en continu, l'utilisateur ne retombe jamais sur l'écran de connexion.
let refreshTimer: ReturnType<typeof setTimeout> | null = null;
function scheduleProactiveRefresh(tokens: TokenPair | null): void {
  if (refreshTimer) {
    clearTimeout(refreshTimer);
    refreshTimer = null;
  }
  if (!tokens?.refresh_token || !tokens.expires_in) return;
  const secs = Math.max(15, Math.min(tokens.expires_in - 60, Math.floor(tokens.expires_in * 0.8)));
  refreshTimer = setTimeout(() => {
    void refreshTokens().then((ok) => {
      // Échec transitoire (réseau) sans session morte → court réessai (le jeton vaut peut-être encore).
      if (!ok && accessToken) {
        if (refreshTimer) clearTimeout(refreshTimer);
        refreshTimer = setTimeout(() => void refreshTokens(), 30_000);
      }
    });
  }, secs * 1000);
}

// Rafraîchissement **mutualisé** (un seul /refresh pour N appels concurrents) et **résilient** :
// un échec réseau NE déconnecte PAS (on garde la session pour réessayer) ; seul un rejet
// explicite du jeton de rafraîchissement (4xx — rotation, session révoquée) tue la session.
let refreshInFlight: Promise<boolean> | null = null;
export function refreshTokens(): Promise<boolean> {
  if (!refreshInFlight) {
    refreshInFlight = (async () => {
      const stored = loadTokens();
      if (!stored?.refresh_token) return false;
      try {
        const fresh = await api.refresh(stored.refresh_token);
        setTokens(fresh);
        return true;
      } catch (e) {
        if (e instanceof ApiError && [400, 401, 403].includes(e.status)) {
          setTokens(null);
          onAuthLost?.();
        }
        return false;
      }
    })();
    void refreshInFlight.finally(() => {
      refreshInFlight = null;
    });
  }
  return refreshInFlight;
}

// ───────────────────────────── Erreurs ─────────────────────────────

export class ApiError extends Error {
  status: number;
  body: unknown;
  constructor(status: number, message: string, body: unknown) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.body = body;
  }
}

// ───────────────────────────── Cœur HTTP ─────────────────────────────

interface ReqOpts {
  method?: string;
  body?: unknown;
  auth?: boolean; // défaut : true
  query?: Record<string, string | number | boolean | undefined>;
  retried?: boolean; // interne : empêche une 2ᵉ tentative après rafraîchissement
}

async function request<T>(path: string, opts: ReqOpts = {}): Promise<T> {
  const { method = "GET", body, auth = true, query } = opts;

  let url = BASE() + path;
  if (query) {
    const qs = new URLSearchParams();
    for (const [k, v] of Object.entries(query)) {
      if (v !== undefined) qs.set(k, String(v));
    }
    const s = qs.toString();
    if (s) url += "?" + s;
  }

  const headers: Record<string, string> = {};
  if (body !== undefined) headers["Content-Type"] = "application/json";
  if (auth && accessToken) headers["Authorization"] = `Bearer ${accessToken}`;

  const res = await fetch(url, {
    method,
    headers,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });

  // 401 sur une requête authentifiée → rafraîchir le jeton une fois et rejouer. Si le
  // rafraîchissement échoue, refreshTokens() gère la suite (déconnexion seulement si jeton mort).
  if (res.status === 401 && auth && !opts.retried) {
    if (await refreshTokens()) return request<T>(path, { ...opts, retried: true });
  }

  if (!res.ok) {
    let parsed: unknown = null;
    let msg = `${res.status} ${res.statusText}`;
    try {
      const text = await res.text();
      if (text) {
        try {
          parsed = JSON.parse(text);
          const m = (parsed as { message?: string; error?: string })?.message ??
            (parsed as { error?: string })?.error;
          if (m) msg = m;
        } catch {
          msg = text;
        }
      }
    } catch {
      /* ignore */
    }
    throw new ApiError(res.status, msg, parsed);
  }

  if (res.status === 204) return undefined as T;
  const text = await res.text();
  if (!text) return undefined as T;
  return JSON.parse(text) as T;
}

const get = <T>(p: string, query?: ReqOpts["query"]) => request<T>(p, { query });
const post = <T>(p: string, body?: unknown) => request<T>(p, { method: "POST", body });
const patch = <T>(p: string, body?: unknown) => request<T>(p, { method: "PATCH", body });
const put = <T>(p: string, body?: unknown) => request<T>(p, { method: "PUT", body });
const del = <T>(p: string, body?: unknown) => request<T>(p, { method: "DELETE", body });

// Envoi multipart (pièces jointes) — on laisse le navigateur poser le boundary.
async function requestForm<T>(path: string, form: FormData, retried = false): Promise<T> {
  const headers: Record<string, string> = {};
  if (accessToken) headers["Authorization"] = `Bearer ${accessToken}`;
  const res = await fetch(BASE() + path, { method: "POST", headers, body: form });
  if (res.status === 401 && !retried) {
    if (await refreshTokens()) return requestForm<T>(path, form, true);
  }
  if (!res.ok) {
    let msg = `${res.status} ${res.statusText}`;
    try {
      const text = await res.text();
      if (text) msg = text;
    } catch {
      /* ignore */
    }
    throw new ApiError(res.status, msg, null);
  }
  const text = await res.text();
  return (text ? JSON.parse(text) : undefined) as T;
}

// ───────────────────────────── API ─────────────────────────────

export const api = {
  // Instance & auth
  instance: () => get<InstanceInfo>("/instance", undefined),
  gate: (password: string) =>
    request<{ gate_token: string }>("/instance/gate", {
      method: "POST",
      body: { password },
      auth: false,
    }),
  register: (req: RegisterRequest) =>
    request<TokenPair>("/auth/register", { method: "POST", body: req, auth: false }),
  login: (req: LoginRequest) =>
    request<TokenPair>("/auth/login", { method: "POST", body: req, auth: false }),
  refresh: (refresh_token: string) =>
    request<TokenPair>("/auth/token/refresh", {
      method: "POST",
      body: { refresh_token },
      auth: false,
    }),
  me: () => get<User>("/users/@me"),
  updateProfile: (req: UpdateProfile) => patch<User>("/users/@me", req),
  // Réglages client synchronisés entre appareils (blob JSON libre, max 64 Ko).
  getMySettings: () => get<UserSettings>("/users/@me/settings"),
  putMySettings: (settings: UserSettings) => put<UserSettings>("/users/@me/settings", settings),
  // Administration d'instance (self-hoster) — 403 si non-admin (feature-détection côté client).
  adminConfig: () => get<InstanceAdminConfig>("/instance/admin/config"),
  adminListInvites: () => get<InstanceInvite[]>("/instance/admin/invites"),
  adminCreateInvite: (body: CreateInstanceInvite) =>
    post<InstanceInvite>("/instance/admin/invites", body),
  adminRevokeInvite: (code: string) => del<void>(`/instance/admin/invites/${code}`),
  adminListUsers: () => get<InstanceUserView[]>("/instance/admin/users"),
  adminSetSuspended: (userId: Snowflake, suspended: boolean) =>
    patch<void>(`/instance/admin/users/${userId}`, { suspended }),
  adminSetRole: (userId: Snowflake, role: string) =>
    put<void>(`/instance/admin/users/${userId}/role`, { role }),
  setPresence: (req: SetPresence) => put<void>("/users/@me/presence", req),
  changePassword: (req: ChangePassword) => patch<void>("/users/@me/password", req),
  changeEmail: (req: ChangeEmail) => patch<void>("/users/@me/email", req),
  deleteAccount: (req: DeleteAccount) => del<void>("/users/@me", req),
  userProfile: (userId: Snowflake) => get<UserProfile>(`/users/${userId}/profile`),
  // Téléverse une image de profil (avatar / bannière) → image_id à poser via updateProfile.
  uploadUserImage: (file: File) => {
    const form = new FormData();
    form.append("file", file);
    return requestForm<{ image_id: string }>(`/users/@me/images`, form);
  },
  // Guildes & amis en commun (panneau profil des MP).
  userMutual: (userId: Snowflake) =>
    get<{ guilds: { id: Snowflake; name: string; icon_id: string | null }[]; friends: User[] }>(
      `/users/${userId}/mutual`,
    ),

  // Guildes
  listGuilds: () => get<Guild[]>("/guilds"),
  createGuild: (req: CreateGuild) => post<Guild>("/guilds", req),
  getGuild: (id: Snowflake) => get<Guild>(`/guilds/${id}`),
  updateGuild: (id: Snowflake, req: UpdateGuild) => patch<Guild>(`/guilds/${id}`, req),
  // Transfert de propriété (réservé au propriétaire actuel ; la cible doit être membre).
  transferGuild: (id: Snowflake, newOwnerId: Snowflake) =>
    post<void>(`/guilds/${id}/transfer`, { new_owner_id: newOwnerId }),
  // Re-copie les surcharges de permission de la catégorie parente sur le salon.
  syncChannelPermissions: (channelId: Snowflake) =>
    post<void>(`/channels/${channelId}/sync-permissions`),
  deleteGuild: (id: Snowflake) => del<void>(`/guilds/${id}`),
  leaveGuild: (id: Snowflake) => del<void>(`/guilds/${id}/members/@me`),

  // Salons
  listChannels: (guildId: Snowflake) => get<Channel[]>(`/guilds/${guildId}/channels`),
  createChannel: (guildId: Snowflake, req: CreateChannel) =>
    post<Channel>(`/guilds/${guildId}/channels`, req),
  getChannel: (id: Snowflake) => get<Channel>(`/channels/${id}`),
  createThread: (channelId: Snowflake, name: string) =>
    post<Channel>(`/channels/${channelId}/threads`, { name }),
  listThreads: (channelId: Snowflake) => get<Channel[]>(`/channels/${channelId}/threads`),
  updateChannel: (id: Snowflake, req: UpdateChannel) => patch<Channel>(`/channels/${id}`, req),
  joinThread: (channelId: Snowflake) => put<void>(`/channels/${channelId}/thread-members/@me`),
  leaveThread: (channelId: Snowflake) => del<void>(`/channels/${channelId}/thread-members/@me`),
  deleteChannel: (id: Snowflake) => del<void>(`/channels/${id}`),
  // Réordonnancement / déplacement entre catégories. `parent_id` = "0" ⇒ racine (hors catégorie).
  reorderChannels: (
    guildId: Snowflake,
    items: { id: Snowflake; position: number; parent_id: Snowflake | null }[],
  ) =>
    patch<void>(
      `/guilds/${guildId}/channels`,
      items.map((i) => ({ id: i.id, position: i.position, parent_id: i.parent_id ?? "0" })),
    ),
  // Surcharges de permission de salon (page Permissions des paramètres de salon).
  listOverwrites: (channelId: Snowflake) =>
    get<PermissionOverwrite[]>(`/channels/${channelId}/permissions`),
  setOverwrite: (channelId: Snowflake, overwriteId: Snowflake, body: SetOverwrite) =>
    put<PermissionOverwrite>(`/channels/${channelId}/permissions/${overwriteId}`, body),
  deleteOverwrite: (channelId: Snowflake, overwriteId: Snowflake) =>
    del<void>(`/channels/${channelId}/permissions/${overwriteId}`),

  // Membres & modération
  listMembers: (guildId: Snowflake, query?: { after?: Snowflake; limit?: number; query?: string }) =>
    get<Member[]>(`/guilds/${guildId}/members`, query),
  // Surnom de serveur (soi : CHANGE_NICKNAME ; autrui : MANAGE_NICKNAMES) et/ou timeout
  // (MODERATE_MEMBERS ; instant de fin en ms — une valeur passée lève l'exclusion).
  updateMember: (
    guildId: Snowflake,
    userId: Snowflake,
    body: { nick?: string | null; communication_disabled_until?: number | null },
  ) => patch<void>(`/guilds/${guildId}/members/${userId}`, body),
  listAuditLogs: (guildId: Snowflake, query?: AuditLogQuery) =>
    get<AuditLogEntry[]>(`/guilds/${guildId}/audit-logs`, query),

  // Auto-modération
  listAutomodRules: (guildId: Snowflake) =>
    get<AutomodRule[]>(`/guilds/${guildId}/automod/rules`),
  createAutomodRule: (guildId: Snowflake, body: CreateAutomodRule) =>
    post<AutomodRule>(`/guilds/${guildId}/automod/rules`, body),
  updateAutomodRule: (guildId: Snowflake, ruleId: Snowflake, body: UpdateAutomodRule) =>
    patch<AutomodRule>(`/guilds/${guildId}/automod/rules/${ruleId}`, body),
  deleteAutomodRule: (guildId: Snowflake, ruleId: Snowflake) =>
    del<void>(`/guilds/${guildId}/automod/rules/${ruleId}`),
  kickMember: (guildId: Snowflake, userId: Snowflake) =>
    del<void>(`/guilds/${guildId}/members/${userId}`),
  banMember: (guildId: Snowflake, userId: Snowflake, reason?: string) =>
    put<void>(`/guilds/${guildId}/bans/${userId}`, {
      reason: reason ?? null,
      delete_message_seconds: 0,
    }),
  listBans: (guildId: Snowflake) => get<Ban[]>(`/guilds/${guildId}/bans`),
  unbanMember: (guildId: Snowflake, userId: Snowflake) =>
    del<void>(`/guilds/${guildId}/bans/${userId}`),

  // Rôles
  listRoles: (guildId: Snowflake) => get<Role[]>(`/guilds/${guildId}/roles`),
  createRole: (guildId: Snowflake, body: CreateRole) =>
    post<Role>(`/guilds/${guildId}/roles`, body),
  // Réordonne les rôles : `ids` = liste complète (hors @everyone), du plus haut au plus bas.
  reorderRoles: (guildId: Snowflake, ids: Snowflake[]) =>
    patch<Role[]>(`/guilds/${guildId}/roles`, { ids }),
  updateRole: (guildId: Snowflake, roleId: Snowflake, body: UpdateRole) =>
    patch<Role>(`/guilds/${guildId}/roles/${roleId}`, body),
  deleteRole: (guildId: Snowflake, roleId: Snowflake) =>
    del<void>(`/guilds/${guildId}/roles/${roleId}`),
  addMemberRole: (guildId: Snowflake, userId: Snowflake, roleId: Snowflake) =>
    put<void>(`/guilds/${guildId}/members/${userId}/roles/${roleId}`),
  removeMemberRole: (guildId: Snowflake, userId: Snowflake, roleId: Snowflake) =>
    del<void>(`/guilds/${guildId}/members/${userId}/roles/${roleId}`),

  // Messages
  listMessages: (
    channelId: Snowflake,
    opts?: { limit?: number; before?: Snowflake; after?: Snowflake },
  ) => get<Message[]>(`/channels/${channelId}/messages`, opts),
  sendMessage: (channelId: Snowflake, req: CreateMessage) =>
    post<Message>(`/channels/${channelId}/messages`, req),
  editMessage: (channelId: Snowflake, messageId: Snowflake, req: EditMessage) =>
    patch<Message>(`/channels/${channelId}/messages/${messageId}`, req),
  deleteMessage: (channelId: Snowflake, messageId: Snowflake) =>
    del<void>(`/channels/${channelId}/messages/${messageId}`),
  addReaction: (channelId: Snowflake, messageId: Snowflake, emoji: string) =>
    put<void>(
      `/channels/${channelId}/messages/${messageId}/reactions/${encodeURIComponent(emoji)}/@me`,
    ),
  removeReaction: (channelId: Snowflake, messageId: Snowflake, emoji: string) =>
    del<void>(
      `/channels/${channelId}/messages/${messageId}/reactions/${encodeURIComponent(emoji)}/@me`,
    ),
  typing: (channelId: Snowflake) => post<void>(`/channels/${channelId}/typing`),
  listPins: (channelId: Snowflake) => get<Message[]>(`/channels/${channelId}/pins`),
  pinMessage: (channelId: Snowflake, messageId: Snowflake) =>
    put<void>(`/channels/${channelId}/pins/${messageId}`),
  unpinMessage: (channelId: Snowflake, messageId: Snowflake) =>
    del<void>(`/channels/${channelId}/pins/${messageId}`),
  uploadAttachment: (channelId: Snowflake, file: File) => {
    const form = new FormData();
    form.append("file", file);
    return requestForm<Attachment>(`/channels/${channelId}/attachments`, form);
  },
  ackMessage: (channelId: Snowflake, messageId: Snowflake) =>
    post<void>(`/channels/${channelId}/messages/${messageId}/ack`),
  // Marque TOUTE la guilde comme lue (un appel serveur, tous les salons).
  ackGuild: (guildId: Snowflake) => post<void>(`/guilds/${guildId}/ack`),
  listReadStates: () => get<ReadState[]>("/users/@me/read-states"),
  // Boîte de réception : messages récents qui me mentionnent (filtrés aux salons lisibles).
  mentionsInbox: (limit = 25) => get<Message[]>("/users/@me/mentions", { limit }),
  // Note personnelle (privée) sur un utilisateur — max 256 caractères.
  getNote: (userId: Snowflake) => get<{ note: string | null }>(`/users/@me/notes/${userId}`),
  putNote: (userId: Snowflake, note: string) => put<void>(`/users/@me/notes/${userId}`, { note }),

  // Réglages de notification (mute)
  listNotificationSettings: () =>
    get<NotificationSetting[]>("/users/@me/notification-settings"),
  setGuildNotification: (guildId: Snowflake, body: SetNotificationSetting) =>
    put<void>(`/users/@me/notification-settings/guild/${guildId}`, body),
  setChannelNotification: (channelId: Snowflake, body: SetNotificationSetting) =>
    put<void>(`/users/@me/notification-settings/channel/${channelId}`, body),

  // Webhooks
  listChannelWebhooks: (channelId: Snowflake) =>
    get<Webhook[]>(`/channels/${channelId}/webhooks`),
  createWebhook: (channelId: Snowflake, body: CreateWebhook) =>
    post<Webhook>(`/channels/${channelId}/webhooks`, body),
  updateWebhook: (webhookId: Snowflake, body: UpdateWebhook) =>
    patch<Webhook>(`/webhooks/${webhookId}`, body),
  deleteWebhook: (webhookId: Snowflake) => del<void>(`/webhooks/${webhookId}`),
  regenerateWebhook: (webhookId: Snowflake) => post<Webhook>(`/webhooks/${webhookId}`),

  // Emojis personnalisés
  listEmojis: (guildId: Snowflake) => get<Emoji[]>(`/guilds/${guildId}/emojis`),
  uploadEmojiImage: (guildId: Snowflake, file: File) => {
    const form = new FormData();
    form.append("file", file);
    return requestForm<{ image_id: string }>(`/guilds/${guildId}/emojis/image`, form);
  },
  // Téléverse une image d'icône ou de bannière de serveur → renvoie son image_id.
  uploadGuildImage: (guildId: Snowflake, file: File) => {
    const form = new FormData();
    form.append("file", file);
    return requestForm<{ image_id: string }>(`/guilds/${guildId}/images`, form);
  },
  createEmoji: (guildId: Snowflake, body: CreateEmoji) =>
    post<Emoji>(`/guilds/${guildId}/emojis`, body),
  updateEmoji: (guildId: Snowflake, emojiId: Snowflake, body: UpdateEmoji) =>
    patch<Emoji>(`/guilds/${guildId}/emojis/${emojiId}`, body),
  deleteEmoji: (guildId: Snowflake, emojiId: Snowflake) =>
    del<void>(`/guilds/${guildId}/emojis/${emojiId}`),

  // Autocollants (stickers) — route d'image dédiée (limite 2 Mio, distincte des emojis 512 Kio).
  listStickers: (guildId: Snowflake) => get<Sticker[]>(`/guilds/${guildId}/stickers`),
  uploadStickerImage: (guildId: Snowflake, file: File) => {
    const form = new FormData();
    form.append("file", file);
    return requestForm<{ image_id: string }>(`/guilds/${guildId}/stickers/image`, form);
  },
  createSticker: (guildId: Snowflake, body: CreateSticker) =>
    post<Sticker>(`/guilds/${guildId}/stickers`, body),
  updateSticker: (guildId: Snowflake, stickerId: Snowflake, body: UpdateSticker) =>
    patch<Sticker>(`/guilds/${guildId}/stickers/${stickerId}`, body),
  deleteSticker: (guildId: Snowflake, stickerId: Snowflake) =>
    del<void>(`/guilds/${guildId}/stickers/${stickerId}`),

  // Soundboard
  listSounds: (guildId: Snowflake) => get<SoundboardSound[]>(`/guilds/${guildId}/soundboard`),
  uploadSoundAudio: (guildId: Snowflake, file: File) => {
    const form = new FormData();
    form.append("file", file);
    return requestForm<{ sound_id: string }>(`/guilds/${guildId}/soundboard/audio`, form);
  },
  createSound: (guildId: Snowflake, body: CreateSound) =>
    post<SoundboardSound>(`/guilds/${guildId}/soundboard`, body),
  updateSound: (guildId: Snowflake, soundId: Snowflake, body: UpdateSound) =>
    patch<SoundboardSound>(`/guilds/${guildId}/soundboard/${soundId}`, body),
  deleteSound: (guildId: Snowflake, soundId: Snowflake) =>
    del<void>(`/guilds/${guildId}/soundboard/${soundId}`),

  // Sondages
  createPoll: (channelId: Snowflake, body: CreatePoll) =>
    post<Poll>(`/channels/${channelId}/polls`, body),
  getPoll: (channelId: Snowflake, messageId: Snowflake) =>
    get<Poll>(`/channels/${channelId}/polls/${messageId}`),
  castVote: (channelId: Snowflake, messageId: Snowflake, answerIds: number[]) =>
    put<Poll>(`/channels/${channelId}/polls/${messageId}/votes`, { answer_ids: answerIds }),

  // Recherche
  searchChannel: (channelId: Snowflake, q: string) =>
    get<SearchResponse>(`/channels/${channelId}/messages/search`, { q }),
  searchGuild: (guildId: Snowflake, q: string) =>
    get<SearchResponse>(`/guilds/${guildId}/messages/search`, { q }),

  // MP / groupes
  listDMs: () => get<DMChannel[]>("/users/@me/channels"),
  openDM: (req: CreateDM) => post<DMChannel>("/users/@me/channels", req),
  // Gestion d'un groupe EXISTANT : ajouter / retirer un membre (se retirer soi-même = quitter).
  addRecipient: (channelId: Snowflake, userId: Snowflake) =>
    put<void>(`/channels/${channelId}/recipients/${userId}`),
  removeRecipient: (channelId: Snowflake, userId: Snowflake) =>
    del<void>(`/channels/${channelId}/recipients/${userId}`),

  // Relations
  listRelationships: () => get<Relationship[]>("/users/@me/relationships"),
  addRelationship: (req: AddRelationship) => post<void>("/users/@me/relationships", req),
  acceptRelationship: (userId: Snowflake) => put<void>(`/users/@me/relationships/${userId}`),
  removeRelationship: (userId: Snowflake) => del<void>(`/users/@me/relationships/${userId}`),

  // Présence
  listPresences: (guildId: Snowflake) => get<PresenceView[]>(`/guilds/${guildId}/presences`),

  // Vocal (signalisation — présence ; le transport média relève du nœud SFU)
  listVoiceStates: (guildId: Snowflake) =>
    get<VoiceState[]>(`/guilds/${guildId}/voice-states`),
  updateVoiceState: (guildId: Snowflake, body: UpdateVoiceState) =>
    patch<VoiceJoinResponse>(`/guilds/${guildId}/voice-states/@me`, body),
  leaveVoice: (guildId: Snowflake) => del<void>(`/guilds/${guildId}/voice-states/@me`),
  // Modération vocale d'un AUTRE membre (mute/sourdine serveur, déplacement, déconnexion).
  moderateVoiceState: (guildId: Snowflake, userId: Snowflake, body: ModerateVoiceState) =>
    patch<VoiceState>(`/guilds/${guildId}/voice-states/${userId}`, body),

  // Invitations
  listInvites: (guildId: Snowflake) => get<Invite[]>(`/guilds/${guildId}/invites`),
  createInvite: (guildId: Snowflake, req: CreateInvite) =>
    post<Invite>(`/guilds/${guildId}/invites`, req),
  previewInvite: (code: string) => get<InvitePreview>(`/invites/${code}`),
  joinInvite: (code: string) => post<Guild>(`/invites/${code}`),
  revokeInvite: (code: string) => del<void>(`/invites/${code}`),

  // Événements programmés
  listEvents: (guildId: Snowflake) => get<ScheduledEvent[]>(`/guilds/${guildId}/events`),
  createEvent: (guildId: Snowflake, body: CreateEvent) =>
    post<ScheduledEvent>(`/guilds/${guildId}/events`, body),
  updateEvent: (guildId: Snowflake, eventId: Snowflake, body: UpdateEvent) =>
    patch<ScheduledEvent>(`/guilds/${guildId}/events/${eventId}`, body),
  deleteEvent: (guildId: Snowflake, eventId: Snowflake) =>
    del<void>(`/guilds/${guildId}/events/${eventId}`),
  rsvpEvent: (guildId: Snowflake, eventId: Snowflake) =>
    put<void>(`/guilds/${guildId}/events/${eventId}/interested`),
  unrsvpEvent: (guildId: Snowflake, eventId: Snowflake) =>
    del<void>(`/guilds/${guildId}/events/${eventId}/interested`),

  // Découverte
  listDiscovery: () => get<DiscoveryGuild[]>("/discovery/guilds"),
  joinDiscovery: (guildId: Snowflake) => post<Guild>(`/discovery/guilds/${guildId}/join`),
};
