//! Bindings `ApiClient` — **présence, profil, réglages & compte** de l'utilisateur courant.
//! Cf. routes `routes_presence`, `routes_users`, `routes_auth`. Suit le patron de `client_guild`.
//!
//! Note : `me()` (`GET /users/@me`) et `user_profile()` (`GET /users/:id/profile`) sont définis
//! ailleurs (respectivement `client_dms` et `client`) ; ils ne sont pas redéfinis ici.

use crate::proto::dto::{
    ChangeEmail, ChangePassword, DeleteAccount, PresenceView, SetPresence, UpdateProfile, User,
    UserProfile, UserSettings,
};
use crate::proto::Snowflake;
use crate::ApiClient;
use anyhow::Result;

impl ApiClient {
    // ─────────────────────────── Présence & statut ───────────────────────────

    /// `PUT /users/@me/presence` — définit son statut (`online` | `idle` | `dnd` | `invisible`)
    /// et un statut personnalisé optionnel. Renvoie le statut **effectif** appliqué.
    pub async fn set_presence(&self, presence: &SetPresence) -> Result<PresenceView> {
        self.put("/users/@me/presence", presence).await
    }

    /// `GET /guilds/:guild_id/presences` — présences (non hors-ligne) des membres d'une guilde.
    pub async fn list_presences(&self, guild_id: Snowflake) -> Result<Vec<PresenceView>> {
        self.get(&format!("/guilds/{guild_id}/presences")).await
    }

    // ─────────────────────────── Profil (soi-même) ───────────────────────────

    /// `PATCH /users/@me` — édite son propre profil (champs absents = inchangés, chaîne vide =
    /// effacé). Renvoie le profil mis à jour.
    pub async fn update_profile(&self, update: &UpdateProfile) -> Result<UserProfile> {
        self.patch("/users/@me", update).await
    }

    // ─────────────────────────── Réglages client ───────────────────────────

    /// `GET /users/@me/settings` — blob JSON libre des réglages client de l'utilisateur courant.
    pub async fn get_settings(&self) -> Result<UserSettings> {
        self.get("/users/@me/settings").await
    }

    /// `PUT /users/@me/settings` — remplace le blob de réglages client. Renvoie le blob enregistré.
    pub async fn put_settings(&self, settings: &UserSettings) -> Result<UserSettings> {
        self.put("/users/@me/settings", settings).await
    }

    // ───────────────────────────── Compte (auth) ─────────────────────────────

    /// `PATCH /users/@me/password` — change le mot de passe (ré-authentification par le mot de
    /// passe actuel côté serveur). **Toutes les sessions sont révoquées** : il faut se reconnecter.
    pub async fn change_password(&self, change: &ChangePassword) -> Result<()> {
        self.patch_unit("/users/@me/password", change).await
    }

    /// `PATCH /users/@me/email` — change l'e-mail (ré-authentification par le mot de passe côté
    /// serveur, unicité vérifiée). Renvoie l'utilisateur mis à jour.
    pub async fn change_email(&self, change: &ChangeEmail) -> Result<User> {
        self.patch("/users/@me/email", change).await
    }

    /// `DELETE /users/@me` — supprime (anonymise) son propre compte. Exige le mot de passe dans le
    /// corps (ré-authentification côté serveur) ; impossible si l'on possède encore des guildes.
    pub async fn delete_account(&self, delete: &DeleteAccount) -> Result<()> {
        // `delete_unit` ne porte pas de corps ; on émet donc la requête DELETE avec un corps JSON.
        self.send_unit(self.http().delete(self.url("/users/@me")).json(delete))
            .await
    }
}
