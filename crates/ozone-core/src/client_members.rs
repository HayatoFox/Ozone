//! Bindings `ApiClient` — **membres & modération** (liste des membres, expulsion, départ,
//! pseudo/timeout, bannissements, journal d'audit).
//! Cf. routes `routes_guild` (membres) et `routes_moderation` (modération). Suit le patron de
//! `client_guild`.

use crate::proto::dto::{AuditLogEntry, Ban, CreateBan, Member, UpdateMember};
use crate::proto::Snowflake;
use crate::ApiClient;
use anyhow::Result;

impl ApiClient {
    // ───────────────────────────── Membres ─────────────────────────────

    /// `GET /guilds/:guild_id/members` — liste les membres d'une guilde (avec pseudos et rôles).
    pub async fn list_members(&self, guild_id: Snowflake) -> Result<Vec<Member>> {
        self.get(&format!("/guilds/{guild_id}/members")).await
    }

    /// `DELETE /guilds/:guild_id/members/@me` — quitter soi-même une guilde.
    pub async fn leave_guild(&self, guild_id: Snowflake) -> Result<()> {
        self.delete_unit(&format!("/guilds/{guild_id}/members/@me"))
            .await
    }

    /// `PATCH /guilds/:guild_id/members/:user_id` — met à jour un membre (pseudo de serveur
    /// et/ou timeout). Champs absents = inchangés.
    pub async fn update_member(
        &self,
        guild_id: Snowflake,
        user_id: Snowflake,
        update: &UpdateMember,
    ) -> Result<()> {
        self.patch_unit(&format!("/guilds/{guild_id}/members/{user_id}"), update)
            .await
    }

    /// `DELETE /guilds/:guild_id/members/:user_id` — expulse un membre de la guilde.
    pub async fn kick_member(&self, guild_id: Snowflake, user_id: Snowflake) -> Result<()> {
        self.delete_unit(&format!("/guilds/{guild_id}/members/{user_id}"))
            .await
    }

    // ───────────────────────────── Bannissements ─────────────────────────────

    /// `PUT /guilds/:guild_id/bans/:user_id` — bannit un membre (avec motif et purge de
    /// messages optionnels via `CreateBan`).
    pub async fn ban_member(
        &self,
        guild_id: Snowflake,
        user_id: Snowflake,
        ban: &CreateBan,
    ) -> Result<()> {
        self.put_unit(&format!("/guilds/{guild_id}/bans/{user_id}"), ban)
            .await
    }

    /// `DELETE /guilds/:guild_id/bans/:user_id` — lève le bannissement d'un utilisateur.
    pub async fn unban_member(&self, guild_id: Snowflake, user_id: Snowflake) -> Result<()> {
        self.delete_unit(&format!("/guilds/{guild_id}/bans/{user_id}"))
            .await
    }

    /// `GET /guilds/:guild_id/bans` — liste les bannissements de la guilde.
    pub async fn list_bans(&self, guild_id: Snowflake) -> Result<Vec<Ban>> {
        self.get(&format!("/guilds/{guild_id}/bans")).await
    }

    // ───────────────────────────── Journal d'audit ─────────────────────────────

    /// `GET /guilds/:guild_id/audit-logs` — entrées du journal d'audit (50 plus récentes).
    pub async fn list_audit_logs(&self, guild_id: Snowflake) -> Result<Vec<AuditLogEntry>> {
        self.get(&format!("/guilds/{guild_id}/audit-logs")).await
    }
}
