//! DTOs de l'API REST : instance, authentification, entités. Cf. `docs/04-api-rest.md`.

use crate::ids::Snowflake;
use serde::{Deserialize, Serialize};

// ───────────────────────────── Instance ─────────────────────────────

/// Politique d'inscription d'une instance (cf. `docs/features/00-instances.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RegistrationPolicy {
    Open,
    Invite,
    Closed,
}

impl RegistrationPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            RegistrationPolicy::Open => "open",
            RegistrationPolicy::Invite => "invite",
            RegistrationPolicy::Closed => "closed",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "invite" => RegistrationPolicy::Invite,
            "closed" => RegistrationPolicy::Closed,
            _ => RegistrationPolicy::Open,
        }
    }
}

/// Indique si l'instance est protégée par un mot de passe d'instance.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct AccessGate {
    pub required: bool,
}

/// Métadonnées **publiques** d'une instance (réponse de `GET /instance`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstanceInfo {
    pub instance_id: Snowflake,
    pub name: String,
    pub description: Option<String>,
    pub accent_color: Option<u32>,
    pub version: String,
    pub registration_policy: RegistrationPolicy,
    pub access_gate: AccessGate,
}

// ──────────────────────────── Authentification ───────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub display_name: Option<String>,
    /// Jeton de gate (si l'instance est protégée par mot de passe).
    #[serde(default)]
    pub gate_token: Option<String>,
    /// Code d'invitation d'instance (si politique `invite`).
    #[serde(default)]
    pub invite_code: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoginRequest {
    /// E-mail ou pseudo.
    pub login: String,
    pub password: String,
    #[serde(default)]
    pub gate_token: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GateRequest {
    pub password: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GateResponse {
    pub gate_token: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: u64,
}

// ──────────────────────────────── Entités ────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub id: Snowflake,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_id: Option<String>,
    /// Présent uniquement pour `users/@me`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Guild {
    pub id: Snowflake,
    pub name: String,
    pub owner_id: Snowflake,
    pub icon_id: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub discoverable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateGuild {
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateGuild {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub icon_id: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Inscrire/retirer la guilde de l'annuaire de découverte.
    #[serde(default)]
    pub discoverable: Option<bool>,
}

/// Entrée de l'annuaire de découverte (guilde publique).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiscoveryGuild {
    pub id: Snowflake,
    pub name: String,
    pub icon_id: Option<String>,
    pub description: Option<String>,
    pub member_count: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Channel {
    pub id: Snowflake,
    pub guild_id: Option<Snowflake>,
    /// Type de salon (0 = texte, 2 = vocal, 4 = catégorie… cf. `docs/features/03-salons.md`).
    #[serde(rename = "type")]
    pub kind: u8,
    pub name: String,
    pub topic: Option<String>,
    pub position: i32,
    pub parent_id: Option<Snowflake>,
    pub nsfw: bool,
    /// Slowmode en secondes (0 = désactivé).
    pub rate_limit_per_user: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateChannel {
    pub name: String,
    #[serde(rename = "type", default)]
    pub kind: u8,
    #[serde(default)]
    pub topic: Option<String>,
    #[serde(default)]
    pub parent_id: Option<Snowflake>,
    #[serde(default)]
    pub nsfw: Option<bool>,
    #[serde(default)]
    pub rate_limit_per_user: Option<i32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateChannel {
    pub name: Option<String>,
    pub topic: Option<String>,
    pub nsfw: Option<bool>,
    pub rate_limit_per_user: Option<i32>,
    pub position: Option<i32>,
    pub parent_id: Option<Snowflake>,
}

/// Élément du tableau de réordonnancement des salons.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelPosition {
    pub id: Snowflake,
    pub position: i32,
    #[serde(default)]
    pub parent_id: Option<Snowflake>,
}

/// Agrégat de réaction sur un message.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reaction {
    pub emoji: String,
    pub count: i64,
    /// `true` si l'utilisateur courant a réagi.
    pub me: bool,
}

/// Pièce jointe d'un message.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Attachment {
    pub id: Snowflake,
    pub filename: String,
    pub content_type: String,
    pub size: i64,
    /// Chemin de téléchargement (`/attachments/<id>/<filename>`).
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub id: Snowflake,
    pub channel_id: Snowflake,
    pub author: User,
    pub content: String,
    #[serde(rename = "type")]
    pub kind: u8,
    pub created_at: u64,
    pub edited_at: Option<u64>,
    pub pinned: bool,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referenced_message: Option<Box<Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
    /// Présent si le message a été émis par un webhook (l'`author` porte alors le nom/avatar du webhook).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_id: Option<Snowflake>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateMessage {
    pub content: String,
    /// Déduplication du rendu optimiste côté client.
    #[serde(default)]
    pub nonce: Option<String>,
    /// Identifiant du message auquel on répond.
    #[serde(default)]
    pub reply_to: Option<Snowflake>,
    /// Pièces jointes (déjà téléversées) à attacher à ce message.
    #[serde(default)]
    pub attachments: Vec<Snowflake>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditMessage {
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BulkDelete {
    pub messages: Vec<Snowflake>,
}

// ──────────────────────────── Rôles & permissions ────────────────────────────

/// Bitfields de permission sérialisés en **chaîne** (un `u64` dépasse la précision JS).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Role {
    pub id: Snowflake,
    pub guild_id: Snowflake,
    pub name: String,
    pub color: u32,
    pub hoist: bool,
    pub position: i32,
    pub permissions: String,
    pub mentionable: bool,
    pub managed: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CreateRole {
    pub name: Option<String>,
    pub color: Option<u32>,
    pub hoist: Option<bool>,
    pub permissions: Option<String>,
    pub mentionable: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateRole {
    pub name: Option<String>,
    pub color: Option<u32>,
    pub hoist: Option<bool>,
    pub permissions: Option<String>,
    pub mentionable: Option<bool>,
}

/// Surcharge de permissions au niveau d'un salon (`type` : 0 = rôle, 1 = membre).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PermissionOverwrite {
    pub id: Snowflake,
    #[serde(rename = "type")]
    pub kind: u8,
    pub allow: String,
    pub deny: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetOverwrite {
    #[serde(rename = "type")]
    pub kind: u8,
    #[serde(default)]
    pub allow: Option<String>,
    #[serde(default)]
    pub deny: Option<String>,
}

// ──────────────────────────────── Membres ────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Member {
    pub user: User,
    pub nick: Option<String>,
    pub roles: Vec<Snowflake>,
    pub joined_at: u64,
}

// ──────────────────────────────── Invitations ────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Invite {
    pub code: String,
    pub guild_id: Snowflake,
    pub channel_id: Option<Snowflake>,
    pub inviter_id: Snowflake,
    pub uses: i32,
    pub max_uses: i32,
    pub max_age: i64,
    pub created_at: u64,
    pub expires_at: Option<u64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CreateInvite {
    /// 0 = illimité.
    #[serde(default)]
    pub max_uses: i32,
    /// Durée de validité en secondes (0 = jamais).
    #[serde(default)]
    pub max_age: i64,
}

/// Aperçu d'une invitation (avant de rejoindre) : infos de guilde sans la rejoindre.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InvitePreview {
    pub code: String,
    pub guild_id: Snowflake,
    pub guild_name: String,
    pub guild_icon: Option<String>,
    pub inviter_id: Snowflake,
    pub member_count: i64,
    pub expires_at: Option<u64>,
}

// ──────────────────────────── Relations (amis / blocages) ────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipType {
    Friend,
    Incoming,
    Outgoing,
    Blocked,
}

impl RelationshipType {
    pub fn as_str(self) -> &'static str {
        match self {
            RelationshipType::Friend => "friend",
            RelationshipType::Incoming => "incoming",
            RelationshipType::Outgoing => "outgoing",
            RelationshipType::Blocked => "blocked",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "incoming" => RelationshipType::Incoming,
            "outgoing" => RelationshipType::Outgoing,
            "blocked" => RelationshipType::Blocked,
            _ => RelationshipType::Friend,
        }
    }
}

/// `id` = identifiant de l'autre utilisateur.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Relationship {
    pub id: Snowflake,
    #[serde(rename = "type")]
    pub kind: RelationshipType,
    pub user: User,
    pub since: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AddRelationship {
    pub username: String,
    /// `true` = bloquer au lieu d'envoyer une demande d'ami.
    #[serde(default)]
    pub block: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateNote {
    pub note: String,
}

// ──────────────────────────── Messages privés / groupes ────────────────────────────

/// Salon de MP : `type` 1 = privé (1:1), 3 = groupe.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DMChannel {
    pub id: Snowflake,
    #[serde(rename = "type")]
    pub kind: u8,
    pub name: Option<String>,
    pub owner_id: Option<Snowflake>,
    pub recipients: Vec<User>,
    pub last_message_id: Option<Snowflake>,
}

/// Ouvre un MP (1 destinataire) ou crée un groupe (2 à 9 destinataires).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateDM {
    pub recipients: Vec<Snowflake>,
}

// ──────────────────────────── Expressions (emojis / stickers / sons) ────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Emoji {
    pub id: Snowflake,
    pub guild_id: Snowflake,
    pub name: String,
    pub animated: bool,
    pub image_id: String,
    pub created_by: Snowflake,
    pub available: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateEmoji {
    pub name: String,
    #[serde(default)]
    pub animated: bool,
    /// Référence de l'asset image (pipeline de stockage à venir).
    pub image_id: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateEmoji {
    pub name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sticker {
    pub id: Snowflake,
    pub guild_id: Snowflake,
    pub name: String,
    pub description: Option<String>,
    pub tags: Option<String>,
    /// 1 png · 2 apng · 3 lottie · 4 gif.
    pub format_type: u8,
    pub asset_id: String,
    pub created_by: Snowflake,
    pub available: bool,
}

fn default_sticker_format() -> u8 {
    1
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateSticker {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Option<String>,
    #[serde(default = "default_sticker_format")]
    pub format_type: u8,
    pub asset_id: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateSticker {
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SoundboardSound {
    pub id: Snowflake,
    pub guild_id: Snowflake,
    pub name: String,
    pub sound_id: String,
    pub volume: f64,
    pub emoji: Option<String>,
    pub created_by: Snowflake,
    pub available: bool,
}

fn default_volume() -> f64 {
    1.0
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateSound {
    pub name: String,
    pub sound_id: String,
    #[serde(default = "default_volume")]
    pub volume: f64,
    #[serde(default)]
    pub emoji: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateSound {
    pub name: Option<String>,
    pub volume: Option<f64>,
    pub emoji: Option<String>,
}

// ──────────────────────────── Modération ────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ban {
    pub user: User,
    pub reason: Option<String>,
    pub moderator_id: Snowflake,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CreateBan {
    #[serde(default)]
    pub reason: Option<String>,
    /// Purge des messages de la cible sur les N dernières secondes (0 = aucune).
    #[serde(default)]
    pub delete_message_seconds: i64,
}

/// Mise à jour d'un membre : pseudo de serveur et/ou timeout.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateMember {
    #[serde(default)]
    pub nick: Option<String>,
    /// Instant (ms epoch) de fin de timeout ; valeur passée = lever le timeout. `None` = inchangé.
    #[serde(default)]
    pub communication_disabled_until: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub id: Snowflake,
    pub user_id: Snowflake,
    pub target_id: Option<Snowflake>,
    pub action_type: String,
    pub reason: Option<String>,
    pub created_at: u64,
}

// ──────────────────────────── Administration d'instance ────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstanceInvite {
    pub code: String,
    pub created_by: Snowflake,
    pub uses: i32,
    pub max_uses: i32,
    pub expires_at: Option<u64>,
    pub created_at: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CreateInstanceInvite {
    #[serde(default)]
    pub max_uses: i32,
    /// Durée de validité en secondes (0 = jamais).
    #[serde(default)]
    pub max_age: i64,
}

/// Vue admin d'un compte de l'instance.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstanceUserView {
    pub user: User,
    pub role: String,
    pub suspended: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetInstanceRole {
    pub role: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetSuspended {
    pub suspended: bool,
}

// ──────────────────────────────── Webhooks ────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Webhook {
    pub id: Snowflake,
    pub channel_id: Snowflake,
    pub guild_id: Snowflake,
    pub name: String,
    pub avatar_id: Option<String>,
    pub created_by: Snowflake,
    pub created_at: u64,
    /// Jeton secret d'exécution — présent **uniquement** à la création / régénération.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateWebhook {
    pub name: String,
    #[serde(default)]
    pub avatar_id: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateWebhook {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub avatar_id: Option<String>,
    /// Déplacer le webhook vers un autre salon de la même guilde.
    #[serde(default)]
    pub channel_id: Option<Snowflake>,
}

/// Corps d'exécution d'un webhook (auth par jeton dans l'URL, pas de session).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ExecuteWebhook {
    pub content: String,
    /// Nom d'affichage de remplacement pour ce message.
    #[serde(default)]
    pub username: Option<String>,
    /// Avatar de remplacement pour ce message.
    #[serde(default)]
    pub avatar_id: Option<String>,
}

// ──────────────────────────── Événements programmés ────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScheduledEvent {
    pub id: Snowflake,
    pub guild_id: Snowflake,
    pub channel_id: Option<Snowflake>,
    pub creator_id: Snowflake,
    pub name: String,
    pub description: Option<String>,
    pub cover_id: Option<String>,
    /// 1 = stage, 2 = vocal, 3 = externe.
    pub entity_type: u8,
    pub location: Option<String>,
    pub scheduled_start: i64,
    pub scheduled_end: Option<i64>,
    /// 1 = programmé, 2 = actif, 3 = terminé, 4 = annulé.
    pub status: u8,
    pub interested_count: i64,
    pub created_at: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CreateEvent {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub cover_id: Option<String>,
    pub entity_type: u8,
    #[serde(default)]
    pub channel_id: Option<Snowflake>,
    #[serde(default)]
    pub location: Option<String>,
    pub scheduled_start: i64,
    #[serde(default)]
    pub scheduled_end: Option<i64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateEvent {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub cover_id: Option<String>,
    #[serde(default)]
    pub channel_id: Option<Snowflake>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub scheduled_start: Option<i64>,
    #[serde(default)]
    pub scheduled_end: Option<i64>,
    /// Transition de statut (2 = démarrer, 3 = terminer, 4 = annuler).
    #[serde(default)]
    pub status: Option<u8>,
}

// ──────────────────────────────── Recherche ────────────────────────────────

/// Réponse de recherche de messages : messages + total estimé.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchResponse {
    pub total: i64,
    pub messages: Vec<Message>,
}

// ──────────────────────── Marqueurs de lecture & notifications ────────────────────────

/// État de lecture d'un salon pour l'utilisateur courant.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReadState {
    pub channel_id: Snowflake,
    pub last_read_id: Snowflake,
    pub mention_count: i64,
}

/// Réglage de notification d'une portée (`scope_type` : 0 = guilde, 1 = salon).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NotificationSetting {
    pub scope_type: u8,
    pub scope_id: Snowflake,
    /// 0 = tous, 1 = @mentions, 2 = rien, 3 = hériter (salon).
    pub level: u8,
    /// Instant (epoch ms) de fin de mute ; `None` = non mute.
    pub muted_until: Option<i64>,
}

// ──────────────────────────── Présence & statut ────────────────────────────

/// Statut effectif d'un utilisateur (`online` | `idle` | `dnd` | `offline`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PresenceView {
    pub user_id: Snowflake,
    pub status: String,
    pub custom_status: Option<String>,
}

/// Mise à jour de son statut de présence.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SetPresence {
    /// `online` | `idle` | `dnd` | `invisible`.
    pub status: String,
    #[serde(default)]
    pub custom_status: Option<String>,
}

// ──────────────────────────── Profil & réglages ────────────────────────────

/// Profil public d'un utilisateur (sans e-mail).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserProfile {
    pub id: Snowflake,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_id: Option<String>,
    pub bio: Option<String>,
    pub pronouns: Option<String>,
    pub banner_id: Option<String>,
    pub accent_color: Option<u32>,
    pub created_at: u64,
}

/// Mise à jour de son propre profil. Champ absent = inchangé ; chaîne vide = effacé.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateProfile {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub avatar_id: Option<String>,
    #[serde(default)]
    pub bio: Option<String>,
    #[serde(default)]
    pub pronouns: Option<String>,
    #[serde(default)]
    pub banner_id: Option<String>,
    #[serde(default)]
    pub accent_color: Option<u32>,
}

/// Réglages client (blob JSON libre géré par le client).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserSettings {
    pub data: serde_json::Value,
}

/// Mise à jour d'un réglage de notification.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SetNotificationSetting {
    /// 0 = tous, 1 = @mentions, 2 = rien, 3 = hériter (salon). `None` = inchangé.
    #[serde(default)]
    pub level: Option<u8>,
    /// Mute : `0` = réactiver, `> 0` = durée en secondes, `< 0` = jusqu'à réactivation. `None` = inchangé.
    #[serde(default)]
    pub mute_seconds: Option<i64>,
}

// ──────────────────────────────── Sondages ────────────────────────────────

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CreatePoll {
    pub question: String,
    pub answers: Vec<String>,
    #[serde(default)]
    pub multiselect: bool,
    /// Durée en heures (défaut 24, max 768 = 32 jours). 0 = sans expiration.
    #[serde(default)]
    pub duration_hours: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PollAnswer {
    pub answer_id: i32,
    pub text: String,
    pub vote_count: i64,
    /// `true` si l'utilisateur courant a voté pour cette réponse.
    pub me_voted: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Poll {
    pub message_id: Snowflake,
    pub channel_id: Snowflake,
    pub question: String,
    pub multiselect: bool,
    pub expires_at: Option<i64>,
    pub finished: bool,
    pub answers: Vec<PollAnswer>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CastVote {
    pub answer_ids: Vec<i32>,
}

// ──────────────────────────── Vocal (signalisation) ────────────────────────────

/// État vocal d'un utilisateur dans un salon vocal.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VoiceState {
    pub user_id: Snowflake,
    pub guild_id: Snowflake,
    pub channel_id: Option<Snowflake>,
    pub session_id: String,
    pub self_mute: bool,
    pub self_deaf: bool,
    pub self_video: bool,
    pub self_stream: bool,
    pub mute: bool,
    pub deaf: bool,
    pub suppress: bool,
}

/// Mise à jour de son propre état vocal. `channel_id` présent ⇒ rejoindre / se déplacer.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateVoiceState {
    /// Salon vocal cible (rejoindre/déplacer). Absent = simple mise à jour des indicateurs.
    #[serde(default)]
    pub channel_id: Option<Snowflake>,
    #[serde(default)]
    pub self_mute: Option<bool>,
    #[serde(default)]
    pub self_deaf: Option<bool>,
    #[serde(default)]
    pub self_video: Option<bool>,
    #[serde(default)]
    pub self_stream: Option<bool>,
}

/// Action de modération vocale sur un membre.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ModerateVoiceState {
    #[serde(default)]
    pub mute: Option<bool>,
    #[serde(default)]
    pub deaf: Option<bool>,
    /// Déplacer le membre vers ce salon vocal.
    #[serde(default)]
    pub channel_id: Option<Snowflake>,
    /// Déconnecter le membre du vocal.
    #[serde(default)]
    pub disconnect: Option<bool>,
}

/// Informations de connexion au nœud média (équivalent `VOICE_SERVER_UPDATE`).
/// Le transport média (SFU) est un sous-projet séparé : `endpoint` est un emplacement à configurer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VoiceConnectionInfo {
    pub token: String,
    pub endpoint: String,
    pub guild_id: Snowflake,
    pub channel_id: Snowflake,
    pub session_id: String,
}

/// Réponse à la connexion vocale : état + informations du nœud média.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VoiceJoinResponse {
    pub voice_state: VoiceState,
    pub connection: VoiceConnectionInfo,
}

/// Région vocale disponible (sélection de nœud SFU).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VoiceRegion {
    pub id: String,
    pub name: String,
    pub optimal: bool,
}
