//! Bindings `ApiClient` — **invitations de guilde** (création, liste, aperçu, jonction, révocation).
//! Cf. routes `routes_guild` (handlers `list_invites`/`create_invite`/`join_invite`/
//! `preview_invite`/`revoke_invite`). Suit le patron de `client_guild`.
//!
//! Le **code** d'invitation est une chaîne opaque (généré côté serveur), pas un `Snowflake` :
//! les méthodes prennent donc `code: &str`.

use crate::proto::dto::{CreateInvite, Guild, Invite, InvitePreview};
use crate::proto::Snowflake;
use crate::ApiClient;
use anyhow::Result;

impl ApiClient {
    /// `GET /guilds/:guild_id/invites` — liste les invitations d'une guilde (requiert `MANAGE_GUILD`).
    pub async fn list_invites(&self, guild_id: Snowflake) -> Result<Vec<Invite>> {
        self.get(&format!("/guilds/{guild_id}/invites")).await
    }

    /// `POST /guilds/:guild_id/invites` — crée une invitation (requiert `CREATE_INSTANT_INVITE`).
    pub async fn create_invite(&self, guild_id: Snowflake, req: &CreateInvite) -> Result<Invite> {
        self.post(&format!("/guilds/{guild_id}/invites"), req).await
    }

    /// `POST /invites/:code` — rejoint la guilde via une invitation ; renvoie la guilde rejointe.
    /// La jonction n'a pas de corps de requête ; on envoie un objet JSON vide (cf. `client_roles`).
    pub async fn join_invite(&self, code: &str) -> Result<Guild> {
        self.post(&format!("/invites/{code}"), serde_json::json!({}))
            .await
    }

    /// `GET /invites/:code` — aperçu d'une invitation **sans** rejoindre la guilde.
    pub async fn preview_invite(&self, code: &str) -> Result<InvitePreview> {
        self.get(&format!("/invites/{code}")).await
    }

    /// `DELETE /invites/:code` — révoque une invitation (son créateur ou `MANAGE_GUILD`).
    pub async fn revoke_invite(&self, code: &str) -> Result<()> {
        self.delete_unit(&format!("/invites/{code}")).await
    }
}
