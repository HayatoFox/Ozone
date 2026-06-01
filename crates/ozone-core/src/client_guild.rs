//! Bindings `ApiClient` — **guildes, salons, fils** (CRUD de la structure d'une guilde).
//! Cf. routes `routes_chat`. Sert aussi de **patron** aux autres modules `client_*`.

use crate::proto::dto::{
    Channel, ChannelPosition, CreateChannel, CreateThread, Guild, UpdateChannel, UpdateGuild,
};
use crate::proto::Snowflake;
use crate::ApiClient;
use anyhow::Result;

impl ApiClient {
    /// `GET /guilds/:id` — détails d'une guilde.
    pub async fn get_guild(&self, guild_id: Snowflake) -> Result<Guild> {
        self.get(&format!("/guilds/{guild_id}")).await
    }

    /// `PATCH /guilds/:id` — met à jour une guilde (champs optionnels).
    pub async fn update_guild(&self, guild_id: Snowflake, update: &UpdateGuild) -> Result<Guild> {
        self.patch(&format!("/guilds/{guild_id}"), update).await
    }

    /// `DELETE /guilds/:id` — supprime une guilde (propriétaire requis côté serveur).
    pub async fn delete_guild(&self, guild_id: Snowflake) -> Result<()> {
        self.delete_unit(&format!("/guilds/{guild_id}")).await
    }

    /// `POST /guilds/:id/channels` — crée un salon.
    pub async fn create_channel(
        &self,
        guild_id: Snowflake,
        channel: &CreateChannel,
    ) -> Result<Channel> {
        self.post(&format!("/guilds/{guild_id}/channels"), channel)
            .await
    }

    /// `PATCH /guilds/:id/channels` — réordonne (positions/catégories) plusieurs salons.
    pub async fn reorder_channels(
        &self,
        guild_id: Snowflake,
        positions: &[ChannelPosition],
    ) -> Result<()> {
        self.patch_unit(&format!("/guilds/{guild_id}/channels"), positions)
            .await
    }

    /// `GET /channels/:id` — détails d'un salon.
    pub async fn get_channel(&self, channel_id: Snowflake) -> Result<Channel> {
        self.get(&format!("/channels/{channel_id}")).await
    }

    /// `PATCH /channels/:id` — met à jour un salon.
    pub async fn update_channel(
        &self,
        channel_id: Snowflake,
        update: &UpdateChannel,
    ) -> Result<Channel> {
        self.patch(&format!("/channels/{channel_id}"), update).await
    }

    /// `DELETE /channels/:id` — supprime un salon.
    pub async fn delete_channel(&self, channel_id: Snowflake) -> Result<()> {
        self.delete_unit(&format!("/channels/{channel_id}")).await
    }

    /// `POST /channels/:id/threads` — démarre un fil sous un salon texte.
    pub async fn create_thread(&self, channel_id: Snowflake, name: &str) -> Result<Channel> {
        self.post(
            &format!("/channels/{channel_id}/threads"),
            CreateThread {
                name: name.to_string(),
            },
        )
        .await
    }

    /// `GET /channels/:id/threads` — liste les fils visibles d'un salon.
    pub async fn list_threads(&self, channel_id: Snowflake) -> Result<Vec<Channel>> {
        self.get(&format!("/channels/{channel_id}/threads")).await
    }
}
