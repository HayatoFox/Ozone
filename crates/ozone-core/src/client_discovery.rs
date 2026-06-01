//! Bindings `ApiClient` — **découverte de guildes** (annuaire public opt-in + adhésion directe).
//! Cf. routes `routes_discovery`. Suit le patron de `client_guild`.

use crate::proto::dto::{DiscoveryGuild, Guild};
use crate::proto::Snowflake;
use crate::ApiClient;
use anyhow::Result;

impl ApiClient {
    /// `GET /discovery/guilds` — liste les guildes publiques (ayant opté pour la découverte),
    /// triées par taille côté serveur.
    pub async fn list_discovery(&self) -> Result<Vec<DiscoveryGuild>> {
        self.get("/discovery/guilds").await
    }

    /// `POST /discovery/guilds/:guild_id/join` — rejoint directement une guilde publique et
    /// renvoie la guilde rejointe.
    ///
    /// La jonction n'a pas de corps de requête ; on envoie un objet JSON vide (cf. `client_invites`).
    pub async fn join_discovery(&self, guild_id: Snowflake) -> Result<Guild> {
        self.post(
            &format!("/discovery/guilds/{guild_id}/join"),
            serde_json::json!({}),
        )
        .await
    }
}
