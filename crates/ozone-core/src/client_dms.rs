//! Bindings `ApiClient` — **messages privés (1:1) et groupes de MP**.
//! Cf. routes `routes_dms`. Suit le patron de `client_guild`.
//!
//! Les salons de MP réutilisent l'entité `channels` (sans guilde) : `type` 1 = MP 1:1,
//! 3 = groupe. La messagerie elle-même (envoi/lecture) passe par les bindings de salon.

use crate::proto::dto::{CreateDM, DMChannel, User};
use crate::proto::Snowflake;
use crate::ApiClient;
use anyhow::Result;

impl ApiClient {
    /// `GET /users/@me` — utilisateur courant (porte l'`id` et l'e-mail).
    ///
    /// Pratique pour résoudre son propre identifiant, p. ex. distinguer « soi » parmi les
    /// destinataires (`recipients`) d'un MP.
    pub async fn me(&self) -> Result<User> {
        self.get("/users/@me").await
    }

    /// `GET /users/@me/channels` — liste les MP et groupes de l'utilisateur courant.
    pub async fn list_dm_channels(&self) -> Result<Vec<DMChannel>> {
        self.get("/users/@me/channels").await
    }

    /// `POST /users/@me/channels` — ouvre un MP (1 destinataire) ou crée un groupe (2 à 9).
    ///
    /// Le MP 1:1 est **dédupliqué** côté serveur : rappeler avec le même destinataire renvoie
    /// le salon existant.
    pub async fn open_or_create_dm(&self, req: &CreateDM) -> Result<DMChannel> {
        self.post("/users/@me/channels", req).await
    }

    /// `PUT /channels/:channel_id/recipients/:user_id` — ajoute un membre à un groupe de MP.
    pub async fn add_recipient(&self, channel_id: Snowflake, user_id: Snowflake) -> Result<()> {
        self.put_unit(
            &format!("/channels/{channel_id}/recipients/{user_id}"),
            serde_json::json!({}),
        )
        .await
    }

    /// `DELETE /channels/:channel_id/recipients/:user_id` — retire un membre d'un groupe de MP
    /// (soi-même pour quitter, ou un autre membre si l'on est propriétaire).
    pub async fn remove_recipient(&self, channel_id: Snowflake, user_id: Snowflake) -> Result<()> {
        self.delete_unit(&format!("/channels/{channel_id}/recipients/{user_id}"))
            .await
    }
}
