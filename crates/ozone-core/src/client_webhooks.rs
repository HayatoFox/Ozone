//! Bindings `ApiClient` — **webhooks** entrants (gestion + exécution par jeton).
//! Cf. routes `routes_webhooks`. Suit le patron de `client_guild`.
//!
//! Deux familles d'appels :
//! - **gestion** (création, liste, mise à jour, régénération, suppression) : requiert une session
//!   authentifiée et la permission `MANAGE_WEBHOOKS` côté serveur ; passe par les aides standard
//!   qui attachent le bearer.
//! - **exécution** (`execute_webhook`) : **non authentifiée** — le jeton secret présent dans l'URL
//!   *est* l'authentification. Le handler ne lit aucun en-tête `Authorization` ; ce binding
//!   construit donc la requête **sans** bearer (cf. note en tête de méthode).

use crate::proto::dto::{CreateWebhook, ExecuteWebhook, Message, UpdateWebhook, Webhook};
use crate::proto::Snowflake;
use crate::ApiClient;
use anyhow::{anyhow, Result};

impl ApiClient {
    /// `GET /channels/:channel_id/webhooks` — liste les webhooks d'un salon (jetons masqués).
    pub async fn list_channel_webhooks(&self, channel_id: Snowflake) -> Result<Vec<Webhook>> {
        self.get(&format!("/channels/{channel_id}/webhooks")).await
    }

    /// `POST /channels/:channel_id/webhooks` — crée un webhook dans un salon.
    /// La réponse porte le **jeton secret** (`token: Some(..)`), unique occasion de le lire.
    pub async fn create_webhook(
        &self,
        channel_id: Snowflake,
        body: &CreateWebhook,
    ) -> Result<Webhook> {
        self.post(&format!("/channels/{channel_id}/webhooks"), body)
            .await
    }

    /// `GET /guilds/:guild_id/webhooks` — liste tous les webhooks d'une guilde (jetons masqués).
    pub async fn list_guild_webhooks(&self, guild_id: Snowflake) -> Result<Vec<Webhook>> {
        self.get(&format!("/guilds/{guild_id}/webhooks")).await
    }

    /// `PATCH /webhooks/:webhook_id` — met à jour un webhook (renommage, avatar, déplacement de
    /// salon). Champs `None` = inchangés. Le jeton n'est pas renvoyé ici.
    pub async fn update_webhook(
        &self,
        webhook_id: Snowflake,
        body: &UpdateWebhook,
    ) -> Result<Webhook> {
        self.patch(&format!("/webhooks/{webhook_id}"), body).await
    }

    /// `DELETE /webhooks/:webhook_id` — supprime un webhook.
    pub async fn delete_webhook(&self, webhook_id: Snowflake) -> Result<()> {
        self.delete_unit(&format!("/webhooks/{webhook_id}")).await
    }

    /// `POST /webhooks/:webhook_id` — régénère le jeton secret du webhook.
    /// La réponse porte le **nouveau** jeton (`token: Some(..)`) ; l'ancien est invalidé.
    pub async fn regenerate_token(&self, webhook_id: Snowflake) -> Result<Webhook> {
        // Corps vide : le handler ne lit aucun JSON, mais `post` exige un corps sérialisable.
        self.post(&format!("/webhooks/{webhook_id}"), serde_json::json!({}))
            .await
    }

    /// `POST /webhooks/:webhook_id/:token` — **exécute** le webhook (poste un message).
    ///
    /// Endpoint **volontairement non authentifié** : le jeton du chemin *est* l'authentification,
    /// et le handler serveur ne lit aucun en-tête `Authorization`. On contourne donc les aides
    /// `post`/`send_json` (qui attacheraient le bearer via `auth()`) et on émet la requête en
    /// direct, **sans** bearer — cela reflète fidèlement la sémantique de l'endpoint et permet de
    /// l'appeler depuis un client sans session.
    ///
    /// Retour : avec `wait = true`, le serveur renvoie le **`Message`** créé (désérialisé ici) ;
    /// avec `wait = false`, il répond `{"ok": true}`. Ce binding force `wait=true` pour obtenir le
    /// message ; en cas de réponse sans corps de message, l'erreur de désérialisation est explicite.
    ///
    /// Sécurité du chemin : `webhook_id` est un [`Snowflake`] (numérique, non injectable) et le
    /// jeton est ajouté comme **segment de chemin unique** via `Url::path_segments_mut`, qui
    /// percent-encode tout caractère réservé (`/`, `?`, `#`, espace…). Un jeton hostile ne peut donc
    /// pas s'échapper du segment ni greffer de sous-chemin/paramètre. (Les jetons légitimes sont du
    /// base64 URL-safe sans padding, déjà sûrs tels quels.)
    pub async fn execute_webhook(
        &self,
        webhook_id: Snowflake,
        token: &str,
        body: &ExecuteWebhook,
    ) -> Result<Message> {
        // Construit l'URL via le type `Url` (réexporté par reqwest) pour encoder proprement le
        // segment de jeton ; `wait=true` afin de récupérer le message créé.
        let mut url = reqwest::Url::parse(&self.url(&format!("/webhooks/{webhook_id}")))?;
        url.path_segments_mut()
            .map_err(|_| anyhow!("base d'URL invalide pour l'exécution du webhook"))?
            .push(token);
        url.query_pairs_mut().append_pair("wait", "true");
        // Requête directe SANS `auth()` : on n'attache jamais le bearer pour cet endpoint.
        let resp = self.http().post(url).json(body).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let detail = resp.text().await.unwrap_or_default();
            return Err(anyhow!("HTTP {status} : {detail}"));
        }
        Ok(resp.json::<Message>().await?)
    }
}
