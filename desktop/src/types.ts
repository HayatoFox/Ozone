// Types miroir des DTOs de `ozone-proto` (cf. crates/ozone-proto/src/dto.rs).
// IMPORTANT : tous les Snowflake sont sérialisés en **chaîne** dans le JSON (précision JS).

export type Snowflake = string;

// ───────────────────────────── Instance ─────────────────────────────

export type RegistrationPolicy = "open" | "invite" | "closed";

export interface AccessGate {
  required: boolean;
}

export interface InstanceInfo {
  instance_id: Snowflake;
  name: string;
  description: string | null;
  accent_color: number | null;
  version: string;
  registration_policy: RegistrationPolicy;
  access_gate: AccessGate;
}

// ──────────────────────────── Authentification ───────────────────────────

export interface RegisterRequest {
  username: string;
  email: string;
  password: string;
  display_name?: string | null;
  gate_token?: string | null;
  invite_code?: string | null;
}

export interface LoginRequest {
  login: string;
  password: string;
  gate_token?: string | null;
}

export interface GateRequest {
  password: string;
}
export interface GateResponse {
  gate_token: string;
}

export interface ChangePassword {
  current_password: string;
  new_password: string;
}
export interface ChangeEmail {
  password: string;
  new_email: string;
}
export interface DeleteAccount {
  password: string;
}

export interface TokenPair {
  access_token: string;
  refresh_token: string;
  token_type: string;
  expires_in: number;
}

// ──────────────────────────────── Entités ────────────────────────────────

export interface User {
  id: Snowflake;
  username: string;
  display_name: string | null;
  avatar_id: string | null;
  email?: string | null;
}

export interface Guild {
  id: Snowflake;
  name: string;
  owner_id: Snowflake;
  icon_id: string | null;
  description?: string | null;
  discoverable?: boolean;
  banner_color?: number | null;
  banner_id?: string | null;
  games?: string[];
  private_profile?: boolean;
  system_channel_id?: Snowflake | null; // salon des messages système (null = désactivé)
  default_message_notifications?: number; // 0 = tous, 1 = mentions
  afk_channel_id?: Snowflake | null;
  afk_timeout?: number; // secondes
  vanity_code?: string | null;
}

export interface CreateGuild {
  name: string;
}

export interface UpdateGuild {
  name?: string | null;
  icon_id?: string | null;
  description?: string | null;
  discoverable?: boolean | null;
  banner_color?: number | null;
  banner_id?: string | null;
  games?: string[];
  private_profile?: boolean;
  system_channel_id?: Snowflake; // "0" = désactiver
  default_message_notifications?: number;
  afk_channel_id?: Snowflake; // "0" = désactiver
  afk_timeout?: number;
  vanity_code?: string; // "" = retirer
}

// Types de message (sous-ensemble Discord) : 0 = normal, 7 = arrivée d'un membre.
export const MSG_MEMBER_JOIN = 7;

export interface DiscoveryGuild {
  id: Snowflake;
  name: string;
  icon_id: string | null;
  description: string | null;
  member_count: number;
}

// Types de salon.
export const CH_TEXT = 0;
export const CH_DM = 1;
export const CH_VOICE = 2;
export const CH_GROUP = 3;
export const CH_CATEGORY = 4;
export const CH_THREAD_PUBLIC = 11;
export const CH_THREAD_PRIVATE = 12;

export function isThreadType(t: number): boolean {
  return t === CH_THREAD_PUBLIC || t === CH_THREAD_PRIVATE;
}

export interface Channel {
  id: Snowflake;
  guild_id: Snowflake | null;
  type: number;
  name: string;
  topic: string | null;
  position: number;
  parent_id: Snowflake | null;
  nsfw: boolean;
  rate_limit_per_user: number;
  bitrate?: number; // vocal : débit audio (bps)
  user_limit?: number; // vocal : limite d'utilisateurs (0 = illimité)
  rtc_region?: string | null; // vocal : région imposée (null = auto)
  video_quality_mode?: number; // vocal : 1 = auto, 2 = 720p
  default_auto_archive?: number; // texte : masquage des fils inactifs (min)
  last_message_id?: Snowflake | null;
  archived?: boolean; // fil archivé
  locked?: boolean; // fil verrouillé
}

export interface CreateChannel {
  name: string;
  type?: number;
  topic?: string | null;
  parent_id?: Snowflake | null;
  nsfw?: boolean | null;
  rate_limit_per_user?: number | null;
}

export interface UpdateChannel {
  name?: string | null;
  topic?: string | null;
  nsfw?: boolean | null;
  rate_limit_per_user?: number | null;
  position?: number | null;
  parent_id?: Snowflake | null;
  bitrate?: number | null;
  user_limit?: number | null;
  rtc_region?: string | null; // "" => auto
  video_quality_mode?: number | null;
  default_auto_archive?: number | null;
  archived?: boolean | null;
  locked?: boolean | null;
}

// Surcharge de permission de salon (rôle ou membre).
export interface PermissionOverwrite {
  id: Snowflake;
  type: number; // 0 = rôle, 1 = membre
  allow: string; // bitfield décimal
  deny: string;
}

export interface SetOverwrite {
  type: number;
  allow?: string | null;
  deny?: string | null;
}

export interface Reaction {
  emoji: string;
  count: number;
  me: boolean;
}

export interface Attachment {
  id: Snowflake;
  filename: string;
  content_type: string;
  size: number;
  url: string;
}

export interface PollAnswer {
  answer_id: number;
  text: string;
  vote_count: number;
  me_voted: boolean;
}

export interface Poll {
  message_id: Snowflake;
  channel_id: Snowflake;
  question: string;
  multiselect: boolean;
  expires_at: number | null;
  finished: boolean;
  answers: PollAnswer[];
}

export interface CreatePoll {
  question: string;
  answers: string[];
  multiselect?: boolean;
  duration_hours?: number | null;
}

// Sticker attaché à un message (vue allégée — l'asset se sert via /stickers/:id).
export interface MessageSticker {
  id: Snowflake;
  name: string;
  format_type: number;
}

export interface EmbedField {
  name: string;
  value: string;
  inline?: boolean;
}

export interface MessageEmbed {
  title?: string | null;
  description?: string | null;
  url?: string | null;
  color?: number | null;
  fields?: EmbedField[];
  image_url?: string | null;
  footer?: string | null;
}

export interface Message {
  id: Snowflake;
  channel_id: Snowflake;
  author: User;
  content: string;
  type: number;
  created_at: number;
  edited_at: number | null;
  pinned: boolean;
  reactions: Reaction[];
  reference_id?: Snowflake;
  referenced_message?: Message;
  nonce?: string;
  webhook_id?: Snowflake;
  attachments: Attachment[];
  poll?: Poll | null;
  sticker?: MessageSticker | null;
  embeds?: MessageEmbed[];
}

export interface CreateMessage {
  content: string;
  nonce?: string | null;
  reply_to?: Snowflake | null;
  attachments?: Snowflake[];
  sticker_id?: Snowflake | null;
}

export interface EditMessage {
  content: string;
}

// ──────────────────────────── Rôles & permissions ────────────────────────────

// Style de couleur d'un rôle.
export type RoleColorStyleKind = "solid" | "gradient" | "neon" | "wave";

export interface Role {
  id: Snowflake;
  guild_id: Snowflake;
  name: string;
  color: number;
  secondary_color?: number | null; // couleur secondaire (dégradé / vague)
  color_style?: string; // "solid" | "gradient" | "neon" | "wave"
  hoist: boolean;
  position: number;
  permissions: string;
  mentionable: boolean;
  managed: boolean;
}

export interface CreateRole {
  name?: string | null;
  color?: number | null;
  secondary_color?: number | null;
  color_style?: string | null;
  hoist?: boolean | null;
  permissions?: string | null;
  mentionable?: boolean | null;
}

export interface UpdateRole {
  name?: string | null;
  color?: number | null;
  secondary_color?: number | null;
  color_style?: string | null;
  hoist?: boolean | null;
  permissions?: string | null;
  mentionable?: boolean | null;
}

// ──────────────────────────────── Membres ────────────────────────────────

export interface Member {
  user: User;
  nick: string | null;
  roles: Snowflake[];
  joined_at: number;
  joined_via?: string | null; // code d'invitation utilisé (méthode d'adhésion)
}

// ──────────────────────────────── Invitations ────────────────────────────────

export interface Invite {
  code: string;
  guild_id: Snowflake;
  channel_id: Snowflake | null;
  inviter_id: Snowflake;
  uses: number;
  max_uses: number;
  max_age: number;
  created_at: number;
  expires_at: number | null;
}

export interface CreateInvite {
  max_uses?: number;
  max_age?: number;
  code?: string; // code personnalisé optionnel
}

export interface InvitePreview {
  code: string;
  guild_id: Snowflake;
  guild_name: string;
  guild_icon: string | null;
  inviter_id: Snowflake;
  member_count: number;
  expires_at: number | null;
}

// ──────────────────────────── Relations ────────────────────────────

export type RelationshipType = "friend" | "incoming" | "outgoing" | "blocked";

export interface Relationship {
  id: Snowflake;
  type: RelationshipType;
  user: User;
  since: number;
}

export interface AddRelationship {
  username: string;
  block?: boolean;
}

// ──────────────────────────── MP / groupes ────────────────────────────

export interface DMChannel {
  id: Snowflake;
  type: number;
  name: string | null;
  owner_id: Snowflake | null;
  recipients: User[];
  last_message_id: Snowflake | null;
}

export interface CreateDM {
  recipients: Snowflake[];
}

// ──────────────────────────── AutoMod ────────────────────────────

export interface AutomodRule {
  id: Snowflake;
  guild_id: Snowflake;
  name: string;
  trigger_type: "keyword" | "mention_spam";
  keywords: string[];
  mention_limit: number;
  action: "block" | "alert";
  alert_channel_id: Snowflake | null;
  exempt_roles: Snowflake[];
  enabled: boolean;
}

export interface CreateAutomodRule {
  name: string;
  trigger_type: "keyword" | "mention_spam";
  keywords?: string[];
  mention_limit?: number;
  action?: "block" | "alert";
  alert_channel_id?: Snowflake | null;
  exempt_roles?: Snowflake[];
}

export interface UpdateAutomodRule {
  name?: string | null;
  keywords?: string[];
  mention_limit?: number;
  action?: "block" | "alert";
  alert_channel_id?: Snowflake | null;
  exempt_roles?: Snowflake[];
  enabled?: boolean;
}

// ──────────────────────────── Modération ────────────────────────────

export interface Ban {
  user: User;
  reason: string | null;
  moderator_id: Snowflake;
}

export interface AuditLogEntry {
  id: Snowflake;
  user_id: Snowflake;
  target_id: Snowflake | null;
  action_type: string;
  reason: string | null;
  changes?: { name?: string; [k: string]: unknown } | null;
  created_at: number;
}

export type AuditLogQuery = {
  before?: Snowflake;
  limit?: number;
  action_type?: string;
  user_id?: Snowflake;
} & Record<string, string | number | boolean | undefined>;

// ──────────────────────────── Vocal (signalisation) ────────────────────────────

export interface VoiceState {
  user_id: Snowflake;
  guild_id: Snowflake;
  channel_id: Snowflake | null;
  session_id: string;
  self_mute: boolean;
  self_deaf: boolean;
  self_video: boolean;
  self_stream: boolean;
  mute: boolean;
  deaf: boolean;
  suppress: boolean;
}

export interface UpdateVoiceState {
  channel_id?: Snowflake | null;
  self_mute?: boolean;
  self_deaf?: boolean;
  self_video?: boolean;
  self_stream?: boolean;
}

// Modération vocale d'un membre (mute/sourdine serveur, déplacement, déconnexion).
export interface ModerateVoiceState {
  mute?: boolean;
  deaf?: boolean;
  channel_id?: Snowflake; // déplacer vers ce salon vocal
  disconnect?: boolean;
}

export interface VoiceConnectionInfo {
  token: string;
  endpoint: string;
  guild_id: Snowflake;
  channel_id: Snowflake;
  session_id: string;
}

export interface VoiceJoinResponse {
  voice_state: VoiceState;
  connection: VoiceConnectionInfo;
}

// ──────────────────────────── Présence ────────────────────────────

export type PresenceStatus = "online" | "idle" | "dnd" | "offline" | "invisible";

export interface PresenceView {
  user_id: Snowflake;
  status: string;
  custom_status: string | null;
}

export interface SetPresence {
  status: string;
  // 3 états : champ absent (omis) = préserver le statut perso ; null = effacer ; string = définir.
  custom_status?: string | null;
}

// ──────────────────────────── Profil & réglages ────────────────────────────

export interface UserProfile {
  id: Snowflake;
  username: string;
  display_name: string | null;
  avatar_id: string | null;
  bio: string | null;
  pronouns: string | null;
  banner_id: string | null;
  accent_color: number | null;
  created_at: number;
}

export interface UpdateProfile {
  display_name?: string | null;
  avatar_id?: string | null;
  bio?: string | null;
  pronouns?: string | null;
  banner_id?: string | null;
  accent_color?: number | null;
}

// ──────────────────────────── Webhooks ────────────────────────────

export interface Webhook {
  id: Snowflake;
  channel_id: Snowflake;
  guild_id: Snowflake;
  name: string;
  avatar_id: string | null;
  created_by: Snowflake;
  created_at: number;
  token?: string; // présent uniquement à la création / régénération
}

export interface CreateWebhook {
  name: string;
  avatar_id?: string | null;
}

export interface UpdateWebhook {
  name?: string | null;
  avatar_id?: string | null;
  channel_id?: Snowflake; // déplacer vers un autre salon de la même guilde
}

// ──────────────────────────── Expressions (emojis) ────────────────────────────

export interface Emoji {
  id: Snowflake;
  guild_id: Snowflake;
  name: string;
  animated: boolean;
  image_id: string;
  created_by: Snowflake;
  available: boolean;
}

export interface CreateEmoji {
  name: string;
  animated?: boolean;
  image_id: string;
}

export interface UpdateEmoji {
  name?: string | null;
}

// ──────────────────────────── Expressions (stickers) ────────────────────────────

export interface Sticker {
  id: Snowflake;
  guild_id: Snowflake;
  name: string;
  description: string | null;
  tags: string | null;
  format_type: number; // 1 png · 2 apng · 3 lottie · 4 gif
  asset_id: string;
  created_by: Snowflake;
  available: boolean;
}

export interface CreateSticker {
  name: string;
  description?: string | null;
  tags?: string | null;
  format_type?: number;
  asset_id: string;
}

export interface UpdateSticker {
  name?: string | null;
  description?: string | null;
  tags?: string | null;
}

// ──────────────────────────── Expressions (soundboard) ────────────────────────────

export interface SoundboardSound {
  id: Snowflake;
  guild_id: Snowflake;
  name: string;
  sound_id: string;
  volume: number; // [0, 1]
  emoji: string | null;
  created_by: Snowflake;
  available: boolean;
}

export interface CreateSound {
  name: string;
  sound_id: string;
  volume?: number;
  emoji?: string | null;
}

export interface UpdateSound {
  name?: string | null;
  volume?: number | null;
  emoji?: string | null;
}

// ──────────────────────────── Notifications ────────────────────────────

export interface ReadState {
  channel_id: Snowflake;
  last_read_id: Snowflake;
  mention_count: number;
}

export interface NotificationSetting {
  scope_type: number; // 0 = guilde, 1 = salon
  scope_id: Snowflake;
  level: number; // 0 tous · 1 @mentions · 2 rien · 3 hériter
  muted_until: number | null; // epoch ms ; null = non muet
}

export interface SetNotificationSetting {
  level?: number | null;
  mute_seconds?: number | null; // 0 réactiver · >0 durée · <0 jusqu'à réactivation
}

// ──────────────────────────── Événements programmés ────────────────────────────

export interface ScheduledEvent {
  id: Snowflake;
  guild_id: Snowflake;
  channel_id: Snowflake | null;
  creator_id: Snowflake;
  name: string;
  description: string | null;
  cover_id: string | null;
  entity_type: number; // 1 stage · 2 vocal · 3 externe
  location: string | null;
  scheduled_start: number;
  scheduled_end: number | null;
  status: number; // 1 programmé · 2 actif · 3 terminé · 4 annulé
  interested_count: number;
  created_at: number;
}

export interface CreateEvent {
  name: string;
  description?: string | null;
  entity_type: number;
  channel_id?: Snowflake | null;
  location?: string | null;
  scheduled_start: number;
  scheduled_end?: number | null;
}

export interface UpdateEvent {
  name?: string | null;
  description?: string | null;
  cover_id?: string | null;
  channel_id?: Snowflake;
  location?: string | null;
  scheduled_start?: number | null;
  scheduled_end?: number | null;
  status?: number; // 2 démarrer · 3 terminer · 4 annuler
}

// ──────────────────────────── Administration d'instance ────────────────────────────

export interface InstanceAdminConfig {
  instance_id: Snowflake;
  name: string;
  description: string | null;
  version: string;
  registration_policy: string;
  gate_enabled: boolean;
}

export interface InstanceInvite {
  code: string;
  created_by: Snowflake;
  uses: number;
  max_uses: number;
  expires_at: number | null;
  created_at: number;
}

export interface CreateInstanceInvite {
  max_uses?: number;
  max_age?: number; // secondes (0 = jamais)
}

export interface InstanceUserView {
  user: User;
  role: string; // owner | admin | moderator | user
  suspended: boolean;
}

// Blob de réglages client synchronisé entre appareils (forme libre côté serveur).
export interface UserSettings {
  data: Record<string, unknown>;
}

// ──────────────────────────── Recherche ────────────────────────────

export interface SearchResponse {
  total: number;
  messages: Message[];
}

// ──────────────────────────── Gateway ────────────────────────────

export interface ReadyPayload {
  session_id: string;
  user: User;
  instance: { id: Snowflake; name: string };
  guilds: { id: Snowflake; name: string }[];
}

export interface GatewayFrame<T = unknown> {
  op: number;
  d?: T;
  s?: number;
  t?: string;
}
