//! Bindings `ApiClient` — **rôles & surcharges de permission** (gestion des rôles d'une guilde,
//! attribution aux membres, surcharges au niveau d'un salon).
//! Cf. routes `routes_roles`. Suit le patron de `client_guild`.

use crate::proto::dto::{CreateRole, PermissionOverwrite, Role, SetOverwrite, UpdateRole};
use crate::proto::Snowflake;
use crate::ApiClient;
use anyhow::Result;

impl ApiClient {
    /// `GET /guilds/:guild_id/roles` — liste les rôles d'une guilde (du plus haut au plus bas).
    pub async fn list_roles(&self, guild_id: Snowflake) -> Result<Vec<Role>> {
        self.get(&format!("/guilds/{guild_id}/roles")).await
    }

    /// `POST /guilds/:guild_id/roles` — crée un rôle (renvoie le rôle créé).
    ///
    /// Anti-escalade côté serveur : seules les permissions que l'on possède sont accordées.
    pub async fn create_role(&self, guild_id: Snowflake, role: &CreateRole) -> Result<Role> {
        self.post(&format!("/guilds/{guild_id}/roles"), role).await
    }

    /// `PATCH /guilds/:guild_id/roles/:role_id` — met à jour un rôle (champs optionnels ;
    /// renvoie le rôle mis à jour).
    pub async fn update_role(
        &self,
        guild_id: Snowflake,
        role_id: Snowflake,
        update: &UpdateRole,
    ) -> Result<Role> {
        self.patch(&format!("/guilds/{guild_id}/roles/{role_id}"), update)
            .await
    }

    /// `DELETE /guilds/:guild_id/roles/:role_id` — supprime un rôle (le rôle `@everyone` ne peut
    /// pas être supprimé côté serveur).
    pub async fn delete_role(&self, guild_id: Snowflake, role_id: Snowflake) -> Result<()> {
        self.delete_unit(&format!("/guilds/{guild_id}/roles/{role_id}"))
            .await
    }

    /// `PUT /guilds/:guild_id/members/:user_id/roles/:role_id` — attribue un rôle à un membre.
    pub async fn add_member_role(
        &self,
        guild_id: Snowflake,
        user_id: Snowflake,
        role_id: Snowflake,
    ) -> Result<()> {
        self.put_unit(
            &format!("/guilds/{guild_id}/members/{user_id}/roles/{role_id}"),
            serde_json::json!({}),
        )
        .await
    }

    /// `DELETE /guilds/:guild_id/members/:user_id/roles/:role_id` — retire un rôle d'un membre.
    pub async fn remove_member_role(
        &self,
        guild_id: Snowflake,
        user_id: Snowflake,
        role_id: Snowflake,
    ) -> Result<()> {
        self.delete_unit(&format!(
            "/guilds/{guild_id}/members/{user_id}/roles/{role_id}"
        ))
        .await
    }

    /// `PUT /channels/:channel_id/permissions/:overwrite_id` — pose (ou remplace) une surcharge de
    /// permission sur un salon (renvoie la surcharge).
    ///
    /// `overwrite_id` est l'identifiant de la **cible** de la surcharge : un identifiant de rôle
    /// lorsque `SetOverwrite.kind == 0`, ou un identifiant de membre lorsque `kind == 1`. Il est
    /// repris tel quel dans `PermissionOverwrite.id`.
    pub async fn set_overwrite(
        &self,
        channel_id: Snowflake,
        overwrite_id: Snowflake,
        overwrite: &SetOverwrite,
    ) -> Result<PermissionOverwrite> {
        self.put(
            &format!("/channels/{channel_id}/permissions/{overwrite_id}"),
            overwrite,
        )
        .await
    }

    /// `DELETE /channels/:channel_id/permissions/:overwrite_id` — supprime une surcharge de
    /// permission d'un salon. Cf. [`ApiClient::set_overwrite`] pour le sens de `overwrite_id`.
    pub async fn delete_overwrite(
        &self,
        channel_id: Snowflake,
        overwrite_id: Snowflake,
    ) -> Result<()> {
        self.delete_unit(&format!(
            "/channels/{channel_id}/permissions/{overwrite_id}"
        ))
        .await
    }
}
