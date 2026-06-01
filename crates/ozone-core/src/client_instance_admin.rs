//! Bindings `ApiClient` — **administration d'instance** (tableau de bord du self-hoster) :
//! configuration, invitations d'instance, comptes (rôles d'instance, suspension).
//! Cf. routes `routes_instance_admin`.
//!
//! Toutes ces routes sont **privilégiées** : le serveur exige le rôle d'instance `owner`/`admin`
//! (et `owner` seul pour [`ApiClient::set_instance_role`]). Le client se contente de porter le
//! bearer de l'utilisateur courant ; l'autorisation est vérifiée côté serveur.

use crate::proto::dto::{
    CreateInstanceInvite, InstanceInvite, InstanceUserView, SetInstanceRole, SetSuspended,
};
use crate::proto::Snowflake;
use crate::ApiClient;
use anyhow::Result;
use serde_json::Value;

impl ApiClient {
    /// `GET /instance/admin/config` — configuration de l'instance (vue admin).
    ///
    /// Le serveur renvoie un objet JSON ad hoc (`instance_id`, `name`, `description`, `version`,
    /// `registration_policy`, `gate_enabled`) ; d'où le type de retour `serde_json::Value`.
    pub async fn get_config(&self) -> Result<Value> {
        self.get("/instance/admin/config").await
    }

    // ───────────────────────────── Invitations d'instance ─────────────────────────────

    /// `GET /instance/admin/invites` — liste les invitations d'instance.
    pub async fn list_instance_invites(&self) -> Result<Vec<InstanceInvite>> {
        self.get("/instance/admin/invites").await
    }

    /// `POST /instance/admin/invites` — crée une invitation d'instance.
    pub async fn create_instance_invite(
        &self,
        invite: &CreateInstanceInvite,
    ) -> Result<InstanceInvite> {
        self.post("/instance/admin/invites", invite).await
    }

    /// `DELETE /instance/admin/invites/:code` — révoque une invitation d'instance.
    pub async fn revoke_instance_invite(&self, code: &str) -> Result<()> {
        self.delete_unit(&format!("/instance/admin/invites/{code}"))
            .await
    }

    // ───────────────────────────── Comptes ─────────────────────────────

    /// `GET /instance/admin/users` — liste les comptes de l'instance (vue admin).
    pub async fn list_instance_users(&self) -> Result<Vec<InstanceUserView>> {
        self.get("/instance/admin/users").await
    }

    /// `PATCH /instance/admin/users/:user_id` — suspend / réactive un compte.
    pub async fn set_suspended(&self, user_id: Snowflake, body: &SetSuspended) -> Result<()> {
        self.patch_unit(&format!("/instance/admin/users/{user_id}"), body)
            .await
    }

    /// `PUT /instance/admin/users/:user_id/role` — change le rôle d'instance d'un compte
    /// (réservé au propriétaire côté serveur).
    pub async fn set_instance_role(
        &self,
        user_id: Snowflake,
        body: &SetInstanceRole,
    ) -> Result<()> {
        self.put_unit(&format!("/instance/admin/users/{user_id}/role"), body)
            .await
    }
}
