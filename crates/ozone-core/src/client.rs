//! Client REST typé de l'API Ozone. Construit les corps de requête en JSON et **désérialise des
//! types partagés** (`ozone_proto::dto`). Côté client : `reqwest` + `rustls` (pas d'OpenSSL).

use crate::InstanceRef;
use anyhow::{anyhow, Result};
use ozone_proto::dto::{
    Channel, GateResponse, Guild, InstanceInfo, Message, TokenPair, UserProfile,
};
use ozone_proto::Snowflake;
use serde::de::DeserializeOwned;

/// Client HTTP d'une instance Ozone (porte le jeton d'accès une fois authentifié).
#[derive(Clone)]
pub struct ApiClient {
    base: String,
    http: reqwest::Client,
    token: Option<String>,
    /// Jeton de **gate d'instance** (si l'instance est protégée par mot de passe), joint à
    /// l'inscription/connexion. Obtenu via [`ApiClient::gate`].
    gate_token: Option<String>,
}

impl ApiClient {
    /// Crée un client pour une base d'API (racine, p. ex. `https://ozone.exemple.fr`).
    pub fn new(base: impl Into<String>) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
            token: None,
            gate_token: None,
        }
    }

    /// Crée un client depuis une `InstanceRef` (reprend son jeton d'accès s'il existe).
    pub fn from_instance(inst: &InstanceRef) -> Self {
        let mut c = Self::new(inst.api_base());
        c.token = inst.access_token.clone();
        c
    }

    /// Définit (ou efface) le jeton d'accès porté par le client.
    pub fn set_token(&mut self, token: Option<String>) {
        self.token = token;
    }

    /// Définit (ou efface) le jeton de gate d'instance joint à l'inscription/connexion.
    pub fn set_gate_token(&mut self, gate_token: Option<String>) {
        self.gate_token = gate_token;
    }

    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    /// Référence au client HTTP (pour les requêtes multipart spécifiques, p. ex. pièces jointes).
    pub(crate) fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// Ajoute le bearer si un jeton est présent.
    pub(crate) fn auth(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.token {
            Some(t) => rb.bearer_auth(t),
            None => rb,
        }
    }

    /// Exécute une requête et désérialise la réponse JSON (erreur explicite sur statut non-2xx).
    pub(crate) async fn send_json<T: DeserializeOwned>(
        &self,
        rb: reqwest::RequestBuilder,
    ) -> Result<T> {
        let resp = self.auth(rb).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("HTTP {status} : {body}"));
        }
        Ok(resp.json::<T>().await?)
    }

    /// Exécute une requête en attendant un **succès sans corps** (204/200 ignoré).
    pub(crate) async fn send_unit(&self, rb: reqwest::RequestBuilder) -> Result<()> {
        let resp = self.auth(rb).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("HTTP {status} : {body}"));
        }
        Ok(())
    }

    // ── Verbes typés (réponse JSON) ──
    pub(crate) async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.send_json(self.http.get(self.url(path))).await
    }
    pub(crate) async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        body: impl serde::Serialize,
    ) -> Result<T> {
        self.send_json(self.http.post(self.url(path)).json(&body))
            .await
    }
    pub(crate) async fn put<T: DeserializeOwned>(
        &self,
        path: &str,
        body: impl serde::Serialize,
    ) -> Result<T> {
        self.send_json(self.http.put(self.url(path)).json(&body))
            .await
    }
    pub(crate) async fn patch<T: DeserializeOwned>(
        &self,
        path: &str,
        body: impl serde::Serialize,
    ) -> Result<T> {
        self.send_json(self.http.patch(self.url(path)).json(&body))
            .await
    }

    // ── Verbes sans corps de réponse ──
    pub(crate) async fn post_unit(&self, path: &str, body: impl serde::Serialize) -> Result<()> {
        self.send_unit(self.http.post(self.url(path)).json(&body))
            .await
    }
    pub(crate) async fn put_unit(&self, path: &str, body: impl serde::Serialize) -> Result<()> {
        self.send_unit(self.http.put(self.url(path)).json(&body))
            .await
    }
    pub(crate) async fn patch_unit(&self, path: &str, body: impl serde::Serialize) -> Result<()> {
        self.send_unit(self.http.patch(self.url(path)).json(&body))
            .await
    }
    pub(crate) async fn delete_unit(&self, path: &str) -> Result<()> {
        self.send_unit(self.http.delete(self.url(path))).await
    }

    // ─────────────── Instance & authentification ───────────────

    /// `GET /instance` — métadonnées publiques de l'instance.
    pub async fn instance_info(&self) -> Result<InstanceInfo> {
        self.get("/instance").await
    }

    /// `GET /instance/health` — sonde de disponibilité (`{status, instance_id, version}`).
    pub async fn health(&self) -> Result<serde_json::Value> {
        self.get("/instance/health").await
    }

    /// `POST /instance/gate` — échange le **mot de passe d'instance** contre un jeton de gate
    /// court. À stocker via [`ApiClient::set_gate_token`] avant l'inscription/connexion.
    pub async fn gate(&self, password: &str) -> Result<String> {
        let resp: GateResponse = self
            .post(
                "/instance/gate",
                serde_json::json!({ "password": password }),
            )
            .await?;
        Ok(resp.gate_token)
    }

    /// `POST /auth/register` — crée un compte, renvoie la paire de jetons. Joint le jeton de gate
    /// s'il est défini.
    pub async fn register(&self, username: &str, email: &str, password: &str) -> Result<TokenPair> {
        self.post(
            "/auth/register",
            serde_json::json!({
                "username": username,
                "email": email,
                "password": password,
                "gate_token": self.gate_token,
            }),
        )
        .await
    }

    /// `POST /auth/login` — connexion, renvoie la paire de jetons. Joint le jeton de gate s'il
    /// est défini.
    pub async fn login(&self, login: &str, password: &str) -> Result<TokenPair> {
        self.post(
            "/auth/login",
            serde_json::json!({
                "login": login,
                "password": password,
                "gate_token": self.gate_token,
            }),
        )
        .await
    }

    /// `POST /auth/token/refresh` — rotation du refresh token.
    pub async fn refresh(&self, refresh_token: &str) -> Result<TokenPair> {
        self.post(
            "/auth/token/refresh",
            serde_json::json!({ "refresh_token": refresh_token }),
        )
        .await
    }

    /// `GET /users/:id/profile` — profil public d'un utilisateur.
    pub async fn user_profile(&self, user_id: Snowflake) -> Result<UserProfile> {
        self.get(&format!("/users/{}/profile", user_id)).await
    }

    // ─────────────── Guildes / salons / messages ───────────────

    /// `POST /guilds` — crée une guilde.
    pub async fn create_guild(&self, name: &str) -> Result<Guild> {
        self.post("/guilds", serde_json::json!({ "name": name }))
            .await
    }

    /// `GET /guilds` — guildes dont l'utilisateur est membre.
    pub async fn list_guilds(&self) -> Result<Vec<Guild>> {
        self.get("/guilds").await
    }

    /// `GET /guilds/:id/channels` — salons visibles d'une guilde.
    pub async fn list_channels(&self, guild_id: Snowflake) -> Result<Vec<Channel>> {
        self.get(&format!("/guilds/{}/channels", guild_id)).await
    }

    /// `GET /channels/:id/messages` — messages d'un salon.
    pub async fn list_messages(&self, channel_id: Snowflake) -> Result<Vec<Message>> {
        self.get(&format!("/channels/{}/messages", channel_id))
            .await
    }

    /// `POST /channels/:id/messages` — envoie un message.
    pub async fn send_message(&self, channel_id: Snowflake, content: &str) -> Result<Message> {
        self.post(
            &format!("/channels/{}/messages", channel_id),
            serde_json::json!({ "content": content }),
        )
        .await
    }
}
