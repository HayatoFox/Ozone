//! Orchestrateur de session client : **une instance connectée**, vue de haut niveau.
//!
//! Réunit les briques de `ozone-core` derrière une API simple que l'UI pilote sans connaître
//! les détails REST/WS/SQL :
//! - [`ApiClient`] (REST typé) + jetons,
//! - [`Store`] (état normalisé en mémoire),
//! - [`Cache`] optionnel (persistance locale, démarrage hors-ligne),
//! - [`GatewayConnection`] optionnelle (temps réel).
//!
//! Réconciliation : `hydrate_from_cache()` (instantané hors-ligne) puis `bootstrap()` (rafraîchit
//! via REST) ; ensuite `poll_event()` applique le flux Gateway au `Store` **et** au cache.

use crate::cache::Cache;
use crate::gateway::{self, GatewayConnection};
use crate::store::Store;
use crate::{ApiClient, InstanceRef};
use anyhow::{anyhow, Result};
use ozone_proto::dto::{Guild, TokenPair};
use ozone_proto::gateway::GatewayFrame;
use ozone_proto::Snowflake;
use serde_json::Value;

/// Résultat de l'application d'un événement Gateway au `Store` (et au cache).
pub struct EventOutcome {
    /// `true` si l'événement a modifié l'état en mémoire.
    pub changed: bool,
    /// La trame brute (l'UI peut lire `frame.t` pour savoir quoi rafraîchir).
    pub frame: GatewayFrame,
}

impl EventOutcome {
    /// Type d'événement (`MESSAGE_CREATE`, `PRESENCE_UPDATE`…), s'il s'agit d'un dispatch.
    pub fn kind(&self) -> Option<&str> {
        self.frame.t.as_deref()
    }
}

/// Session vers **une** instance Ozone.
pub struct Session {
    pub instance: InstanceRef,
    pub api: ApiClient,
    pub store: Store,
    /// Payload `READY` reçu à la connexion Gateway (utilisateur, guildes initiales…).
    pub ready: Option<Value>,
    cache: Option<Cache>,
    gateway: Option<GatewayConnection>,
    access: Option<String>,
    refresh: Option<String>,
}

impl Session {
    /// Crée une session (non authentifiée) pour une instance.
    pub fn new(instance: InstanceRef) -> Self {
        let api = ApiClient::from_instance(&instance);
        let access = instance.access_token.clone();
        let refresh = instance.refresh_token.clone();
        Self {
            instance,
            api,
            store: Store::new(),
            ready: None,
            cache: None,
            gateway: None,
            access,
            refresh,
        }
    }

    /// Attache un cache local (persistance + démarrage hors-ligne).
    pub fn with_cache(mut self, cache: Cache) -> Self {
        self.cache = Some(cache);
        self
    }

    pub fn is_authenticated(&self) -> bool {
        self.access.is_some()
    }

    pub fn access_token(&self) -> Option<&str> {
        self.access.as_deref()
    }

    pub fn refresh_token(&self) -> Option<&str> {
        self.refresh.as_deref()
    }

    /// Mémorise une paire de jetons (porte le jeton d'accès sur le client REST et l'`InstanceRef`).
    fn apply_tokens(&mut self, tp: TokenPair) {
        self.api.set_token(Some(tp.access_token.clone()));
        self.instance.access_token = Some(tp.access_token.clone());
        self.instance.refresh_token = Some(tp.refresh_token.clone());
        self.access = Some(tp.access_token);
        self.refresh = Some(tp.refresh_token);
    }

    // ─────────────── Authentification ───────────────

    /// Inscription puis authentification de la session.
    pub async fn register(&mut self, username: &str, email: &str, password: &str) -> Result<()> {
        let tp = self.api.register(username, email, password).await?;
        self.apply_tokens(tp);
        Ok(())
    }

    /// Connexion (login + mot de passe).
    pub async fn login(&mut self, login: &str, password: &str) -> Result<()> {
        let tp = self.api.login(login, password).await?;
        self.apply_tokens(tp);
        Ok(())
    }

    /// Rafraîchit la session à partir du refresh token mémorisé (rotation).
    pub async fn refresh_session(&mut self) -> Result<()> {
        let rt = self
            .refresh
            .clone()
            .ok_or_else(|| anyhow!("aucun refresh token"))?;
        let tp = self.api.refresh(&rt).await?;
        self.apply_tokens(tp);
        Ok(())
    }

    // ─────────────── Temps réel ───────────────

    /// Ouvre la connexion Gateway (nécessite d'être authentifié) et mémorise le `READY`.
    pub async fn connect_gateway(&mut self) -> Result<()> {
        let access = self
            .access
            .as_deref()
            .ok_or_else(|| anyhow!("non authentifié"))?;
        let conn = gateway::connect(&self.instance.api_base(), access).await?;
        self.ready = Some(conn.ready.clone());
        self.gateway = Some(conn);
        Ok(())
    }

    pub fn is_realtime(&self) -> bool {
        self.gateway.is_some()
    }

    /// Dernière séquence Gateway consommée (`0` si pas de connexion). Utile pour l'UI / le RESUME.
    pub fn gateway_seq(&self) -> u64 {
        self.gateway.as_ref().map(|g| g.last_seq()).unwrap_or(0)
    }

    /// Reprend la Gateway après une coupure : tente un **RESUME** (rejeu des événements manqués
    /// depuis le dernier `seq`) ; si le serveur refuse (session expirée/tampon dépassé), bascule
    /// sur une connexion complète (`IDENTIFY`). Renvoie `true` si le RESUME a réussi.
    pub async fn reconnect(&mut self) -> Result<bool> {
        let access = self
            .access
            .clone()
            .ok_or_else(|| anyhow!("non authentifié"))?;
        // Récupère (et libère le borrow de) l'id de session + dernier seq de la connexion en cours.
        let prev = self
            .gateway
            .as_ref()
            .and_then(|c| c.session_id().map(|s| (s.to_string(), c.last_seq())));
        if let Some((session_id, seq)) = prev {
            match gateway::connect_resume(&self.instance.api_base(), &access, &session_id, seq)
                .await?
            {
                gateway::Resumed::Ok(conn) => {
                    self.ready = Some(conn.ready.clone());
                    self.gateway = Some(conn);
                    return Ok(true);
                }
                gateway::Resumed::Invalid => {} // → reconnexion complète ci-dessous
            }
        }
        self.connect_gateway().await?;
        Ok(false)
    }

    /// Coupe le socket Gateway courant (perte réseau simulée / gestion d'une connexion morte).
    /// La session reste résumable via [`Session::reconnect`].
    pub fn abort_gateway(&self) {
        if let Some(g) = self.gateway.as_ref() {
            g.abort();
        }
    }

    /// Attend le prochain événement Gateway, l'applique au `Store` **et** au cache (best-effort).
    /// Renvoie `None` si aucune Gateway n'est connectée ou si le flux est fermé.
    pub async fn poll_event(&mut self) -> Option<EventOutcome> {
        // On récupère d'abord la trame (le borrow de `gateway` se termine ici), puis on mute.
        let frame = match self.gateway.as_mut() {
            Some(conn) => conn.next_event().await?,
            None => return None,
        };
        let changed = self.store.apply(&frame);
        if let Some(c) = self.cache.as_ref() {
            // La persistance ne doit jamais faire échouer la boucle d'événements de l'UI.
            let _ = c.apply(&frame).await;
        }
        Some(EventOutcome { changed, frame })
    }

    // ─────────────── Chargement REST + cache ───────────────

    /// Hydrate le `Store` depuis le cache local (démarrage hors-ligne). Sans cache : no-op.
    pub async fn hydrate_from_cache(&mut self, msgs_per_channel: i64) -> Result<()> {
        if let Some(c) = self.cache.as_ref() {
            c.load_into(&mut self.store, msgs_per_channel).await?;
        }
        Ok(())
    }

    /// Charge guildes + salons via REST dans le `Store` (et les persiste si un cache est attaché).
    pub async fn bootstrap(&mut self) -> Result<()> {
        let guilds: Vec<Guild> = self.api.list_guilds().await?;
        if let Some(c) = self.cache.as_ref() {
            c.save_guilds(&guilds).await?;
        }
        let guild_ids: Vec<Snowflake> = guilds.iter().map(|g| g.id).collect();
        self.store.set_guilds(guilds);
        for gid in guild_ids {
            let channels = self.api.list_channels(gid).await?;
            if let Some(c) = self.cache.as_ref() {
                c.save_channels(&channels).await?;
            }
            self.store.set_channels(channels);
        }
        Ok(())
    }

    /// Charge l'historique d'un salon dans le `Store` (et le persiste si un cache est attaché).
    pub async fn open_channel(&mut self, channel_id: Snowflake) -> Result<()> {
        let msgs = self.api.list_messages(channel_id).await?;
        if let Some(c) = self.cache.as_ref() {
            c.replace_channel_messages(channel_id, &msgs).await?;
        }
        self.store.set_messages(channel_id, msgs);
        Ok(())
    }

    /// Envoie un message. Le `Store` sera mis à jour par l'écho `MESSAGE_CREATE` de la Gateway
    /// (via `poll_event`) ; renvoie tout de même le message créé pour un retour immédiat.
    pub async fn send_message(
        &self,
        channel_id: Snowflake,
        content: &str,
    ) -> Result<ozone_proto::dto::Message> {
        self.api.send_message(channel_id, content).await
    }
}
