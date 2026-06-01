//! Bindings `ApiClient` — **relations** (amis, blocages, notes personnelles).
//! Cf. routes `routes_relationships`. Suit le patron de `client_guild`.

use crate::proto::dto::{AddRelationship, Relationship, UpdateNote};
use crate::proto::Snowflake;
use crate::ApiClient;
use anyhow::Result;

impl ApiClient {
    /// `GET /users/@me/relationships` — liste toutes les relations de l'utilisateur courant.
    pub async fn list_relationships(&self) -> Result<Vec<Relationship>> {
        self.get("/users/@me/relationships").await
    }

    /// `POST /users/@me/relationships` — envoie une demande d'ami (ou bloque) par nom d'utilisateur.
    pub async fn add_relationship(&self, req: &AddRelationship) -> Result<()> {
        self.post_unit("/users/@me/relationships", req).await
    }

    /// `PUT /users/@me/relationships/:user_id` — accepte une demande entrante (ou l'envoie si absente).
    pub async fn accept_relationship(&self, user_id: Snowflake) -> Result<()> {
        self.put_unit(
            &format!("/users/@me/relationships/{user_id}"),
            serde_json::json!({}),
        )
        .await
    }

    /// `DELETE /users/@me/relationships/:user_id` — supprime une relation dans les deux sens.
    pub async fn remove_relationship(&self, user_id: Snowflake) -> Result<()> {
        self.delete_unit(&format!("/users/@me/relationships/{user_id}"))
            .await
    }

    /// `GET /users/@me/notes/:user_id` — lit la note personnelle sur un utilisateur (`None` si absente).
    pub async fn get_note(&self, user_id: Snowflake) -> Result<Option<String>> {
        let resp: serde_json::Value = self.get(&format!("/users/@me/notes/{user_id}")).await?;
        Ok(resp
            .get("note")
            .and_then(|n| n.as_str())
            .map(|s| s.to_string()))
    }

    /// `PUT /users/@me/notes/:user_id` — crée ou remplace la note personnelle sur un utilisateur.
    pub async fn put_note(&self, user_id: Snowflake, note: &str) -> Result<()> {
        self.put_unit(
            &format!("/users/@me/notes/{user_id}"),
            UpdateNote {
                note: note.to_string(),
            },
        )
        .await
    }
}
