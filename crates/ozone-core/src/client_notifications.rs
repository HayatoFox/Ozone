//! Bindings `ApiClient` — **marqueurs de lecture, notifications, mentions**.
//! Cf. routes `routes_notifications` (read states + réglages) et `routes_messages`
//! (boîte de mentions). Suit le patron de `client_guild`.

use crate::proto::dto::{Message, NotificationSetting, ReadState, SetNotificationSetting};
use crate::proto::Snowflake;
use crate::ApiClient;
use anyhow::Result;

impl ApiClient {
    // ───────────────────────────── Marqueurs de lecture ─────────────────────────────

    /// `POST /channels/:channel_id/messages/:message_id/ack` — marque le salon lu jusqu'à ce
    /// message ; renvoie l'état de lecture mis à jour. (Le serveur n'attend aucun corps.)
    pub async fn ack_message(
        &self,
        channel_id: Snowflake,
        message_id: Snowflake,
    ) -> Result<ReadState> {
        self.post(
            &format!("/channels/{channel_id}/messages/{message_id}/ack"),
            serde_json::json!({}),
        )
        .await
    }

    /// `POST /guilds/:guild_id/ack` — marque toute la guilde comme lue. (Aucun corps attendu.)
    pub async fn ack_guild(&self, guild_id: Snowflake) -> Result<()> {
        self.post_unit(&format!("/guilds/{guild_id}/ack"), serde_json::json!({}))
            .await
    }

    /// `GET /users/@me/read-states` — états de lecture (synchronisation multi-appareils).
    pub async fn list_read_states(&self) -> Result<Vec<ReadState>> {
        self.get("/users/@me/read-states").await
    }

    // ───────────────────────────── Boîte de mentions ─────────────────────────────

    /// `GET /users/@me/mentions` — messages récents qui mentionnent l'utilisateur courant,
    /// filtrés aux salons encore lisibles.
    pub async fn mentions_inbox(&self) -> Result<Vec<Message>> {
        self.get("/users/@me/mentions").await
    }

    // ───────────────────────────── Réglages de notification ─────────────────────────────

    /// `GET /users/@me/notification-settings` — réglages de notification par portée.
    pub async fn list_notification_settings(&self) -> Result<Vec<NotificationSetting>> {
        self.get("/users/@me/notification-settings").await
    }

    /// `PUT /users/@me/notification-settings/guild/:guild_id` — règle les notifications d'une guilde.
    pub async fn set_guild_notification(
        &self,
        guild_id: Snowflake,
        setting: &SetNotificationSetting,
    ) -> Result<NotificationSetting> {
        self.put(
            &format!("/users/@me/notification-settings/guild/{guild_id}"),
            setting,
        )
        .await
    }

    /// `PUT /users/@me/notification-settings/channel/:channel_id` — règle les notifications d'un salon.
    pub async fn set_channel_notification(
        &self,
        channel_id: Snowflake,
        setting: &SetNotificationSetting,
    ) -> Result<NotificationSetting> {
        self.put(
            &format!("/users/@me/notification-settings/channel/{channel_id}"),
            setting,
        )
        .await
    }
}
