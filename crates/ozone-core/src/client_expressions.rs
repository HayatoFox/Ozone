//! Bindings `ApiClient` — **expressions** d'une guilde : emojis, stickers et sons de soundboard
//! (CRUD pour chacun). Cf. routes `routes_emojis`, `routes_stickers`, `routes_soundboard`.
//! Suit le patron de `client_guild`.

use crate::proto::dto::{
    CreateEmoji, CreateSound, CreateSticker, Emoji, SoundboardSound, Sticker, UpdateEmoji,
    UpdateSound, UpdateSticker,
};
use crate::proto::Snowflake;
use crate::ApiClient;
use anyhow::Result;

impl ApiClient {
    // ─────────────────────────────── Emojis ───────────────────────────────

    /// `GET /guilds/:guild_id/emojis` — liste tous les emojis d'une guilde.
    pub async fn list_emojis(&self, guild_id: Snowflake) -> Result<Vec<Emoji>> {
        self.get(&format!("/guilds/{guild_id}/emojis")).await
    }

    /// `POST /guilds/:guild_id/emojis` — crée un emoji (renvoie l'emoji créé).
    pub async fn create_emoji(&self, guild_id: Snowflake, emoji: &CreateEmoji) -> Result<Emoji> {
        self.post(&format!("/guilds/{guild_id}/emojis"), emoji)
            .await
    }

    /// `PATCH /guilds/:guild_id/emojis/:emoji_id` — modifie un emoji (champs optionnels ;
    /// renvoie l'emoji mis à jour).
    pub async fn update_emoji(
        &self,
        guild_id: Snowflake,
        emoji_id: Snowflake,
        update: &UpdateEmoji,
    ) -> Result<Emoji> {
        self.patch(&format!("/guilds/{guild_id}/emojis/{emoji_id}"), update)
            .await
    }

    /// `DELETE /guilds/:guild_id/emojis/:emoji_id` — supprime un emoji.
    pub async fn delete_emoji(&self, guild_id: Snowflake, emoji_id: Snowflake) -> Result<()> {
        self.delete_unit(&format!("/guilds/{guild_id}/emojis/{emoji_id}"))
            .await
    }

    // ─────────────────────────────── Stickers ───────────────────────────────

    /// `GET /guilds/:guild_id/stickers` — liste tous les stickers d'une guilde.
    pub async fn list_stickers(&self, guild_id: Snowflake) -> Result<Vec<Sticker>> {
        self.get(&format!("/guilds/{guild_id}/stickers")).await
    }

    /// `POST /guilds/:guild_id/stickers` — crée un sticker (renvoie le sticker créé).
    pub async fn create_sticker(
        &self,
        guild_id: Snowflake,
        sticker: &CreateSticker,
    ) -> Result<Sticker> {
        self.post(&format!("/guilds/{guild_id}/stickers"), sticker)
            .await
    }

    /// `PATCH /guilds/:guild_id/stickers/:sticker_id` — modifie un sticker (champs optionnels ;
    /// renvoie le sticker mis à jour).
    pub async fn update_sticker(
        &self,
        guild_id: Snowflake,
        sticker_id: Snowflake,
        update: &UpdateSticker,
    ) -> Result<Sticker> {
        self.patch(&format!("/guilds/{guild_id}/stickers/{sticker_id}"), update)
            .await
    }

    /// `DELETE /guilds/:guild_id/stickers/:sticker_id` — supprime un sticker.
    pub async fn delete_sticker(&self, guild_id: Snowflake, sticker_id: Snowflake) -> Result<()> {
        self.delete_unit(&format!("/guilds/{guild_id}/stickers/{sticker_id}"))
            .await
    }

    // ─────────────────────────────── Soundboard ───────────────────────────────

    /// `GET /guilds/:guild_id/soundboard` — liste tous les sons de la soundboard d'une guilde.
    pub async fn list_sounds(&self, guild_id: Snowflake) -> Result<Vec<SoundboardSound>> {
        self.get(&format!("/guilds/{guild_id}/soundboard")).await
    }

    /// `POST /guilds/:guild_id/soundboard` — ajoute un son à la soundboard (renvoie le son créé).
    pub async fn create_sound(
        &self,
        guild_id: Snowflake,
        sound: &CreateSound,
    ) -> Result<SoundboardSound> {
        self.post(&format!("/guilds/{guild_id}/soundboard"), sound)
            .await
    }

    /// `PATCH /guilds/:guild_id/soundboard/:sound_id` — modifie un son de la soundboard (champs
    /// optionnels ; renvoie le son mis à jour).
    pub async fn update_sound(
        &self,
        guild_id: Snowflake,
        sound_id: Snowflake,
        update: &UpdateSound,
    ) -> Result<SoundboardSound> {
        self.patch(&format!("/guilds/{guild_id}/soundboard/{sound_id}"), update)
            .await
    }

    /// `DELETE /guilds/:guild_id/soundboard/:sound_id` — supprime un son de la soundboard.
    pub async fn delete_sound(&self, guild_id: Snowflake, sound_id: Snowflake) -> Result<()> {
        self.delete_unit(&format!("/guilds/{guild_id}/soundboard/{sound_id}"))
            .await
    }
}
