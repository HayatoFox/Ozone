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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateGuild {
    pub name: String,
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
