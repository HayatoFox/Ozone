//! Bindings `ApiClient` — **événements programmés** d'une guilde.
//! Cf. routes `routes_events`. Suit le patron de `client_guild`.

use crate::proto::dto::{CreateEvent, ScheduledEvent, UpdateEvent};
use crate::proto::Snowflake;
use crate::ApiClient;
use anyhow::Result;

impl ApiClient {
    /// `GET /guilds/:guild_id/events` — liste les événements programmés d'une guilde.
    pub async fn list_events(&self, guild_id: Snowflake) -> Result<Vec<ScheduledEvent>> {
        self.get(&format!("/guilds/{guild_id}/events")).await
    }

    /// `POST /guilds/:guild_id/events` — crée un événement programmé.
    pub async fn create_event(
        &self,
        guild_id: Snowflake,
        event: &CreateEvent,
    ) -> Result<ScheduledEvent> {
        self.post(&format!("/guilds/{guild_id}/events"), event)
            .await
    }

    /// `GET /guilds/:guild_id/events/:event_id` — détail d'un événement.
    pub async fn get_event(
        &self,
        guild_id: Snowflake,
        event_id: Snowflake,
    ) -> Result<ScheduledEvent> {
        self.get(&format!("/guilds/{guild_id}/events/{event_id}"))
            .await
    }

    /// `PATCH /guilds/:guild_id/events/:event_id` — modifie un événement (champs optionnels).
    pub async fn update_event(
        &self,
        guild_id: Snowflake,
        event_id: Snowflake,
        update: &UpdateEvent,
    ) -> Result<ScheduledEvent> {
        self.patch(&format!("/guilds/{guild_id}/events/{event_id}"), update)
            .await
    }

    /// `DELETE /guilds/:guild_id/events/:event_id` — supprime un événement.
    pub async fn delete_event(&self, guild_id: Snowflake, event_id: Snowflake) -> Result<()> {
        self.delete_unit(&format!("/guilds/{guild_id}/events/{event_id}"))
            .await
    }

    /// `PUT /guilds/:guild_id/events/:event_id/interested` — marquer son intérêt (idempotent).
    pub async fn rsvp_event(&self, guild_id: Snowflake, event_id: Snowflake) -> Result<()> {
        self.put_unit(
            &format!("/guilds/{guild_id}/events/{event_id}/interested"),
            serde_json::json!({}),
        )
        .await
    }

    /// `DELETE /guilds/:guild_id/events/:event_id/interested` — retirer son intérêt (idempotent).
    pub async fn unrsvp_event(&self, guild_id: Snowflake, event_id: Snowflake) -> Result<()> {
        self.delete_unit(&format!("/guilds/{guild_id}/events/{event_id}/interested"))
            .await
    }
}
