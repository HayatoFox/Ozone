//! État partagé de l'application (injecté dans chaque handler axum).

use ozone_proto::dto::RegistrationPolicy;
use ozone_proto::{Snowflake, SnowflakeGenerator};
use serde_json::Value;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub ids: Arc<SnowflakeGenerator>,
    pub jwt_secret: Arc<Vec<u8>>,
    pub instance: Arc<InstanceRuntime>,
    /// Bus de diffusion en mémoire (fan-out gateway en mode tout-en-un).
    pub hub: broadcast::Sender<HubEvent>,
    /// Registre de présence (connexions actives + statut désiré).
    pub presence: Arc<crate::presence::Registry>,
    /// Registre des sessions Gateway résumables (RESUME : rejeu des événements manqués).
    pub sessions: Arc<crate::gateway_session::SessionRegistry>,
    /// Secret de signature des **jetons vocaux** (partagé avec le nœud média SFU via
    /// `OZONE_VOICE_SECRET` ; à défaut, le secret JWT de l'instance).
    pub voice_secret: Arc<Vec<u8>>,
    /// Base HTTP du nœud média SFU (`OZONE_SFU_URL`, déf. `http://127.0.0.1:8081`). Sert à l'API
    /// pour évincer un pair du média (déconnexion de modération) — sinon le SFU continuerait de
    /// relayer son flux. HTTP clair (co-localisé / derrière reverse-proxy) : aucune dépendance TLS.
    pub sfu_url: Arc<String>,
    /// Répertoire de stockage des pièces jointes (`OZONE_UPLOAD_DIR`).
    pub upload_dir: Arc<std::path::PathBuf>,
    /// Limiteur de débit en mémoire (token bucket par clé) — R1/R6.
    pub rate: Arc<crate::ratelimit::RateLimiter>,
    /// Faire confiance à l'en-tête `X-Forwarded-For` (déploiement DERRIÈRE un reverse-proxy
    /// de confiance, `OZONE_TRUSTED_PROXY=1`). Faux par défaut : sinon un client en accès direct
    /// pourrait usurper son IP et contourner le rate-limiting par IP. Cf. `extract::ClientIp`.
    pub trust_proxy: bool,
}

/// Runtime de l'instance (chargé au bootstrap).
pub struct InstanceRuntime {
    pub instance_id: Snowflake,
    pub name: String,
    pub description: Option<String>,
    pub version: String,
    pub registration_policy: RegistrationPolicy,
    pub gate_enabled: bool,
    pub gate_hash: Option<String>,
}

/// Portée d'un événement = qui a le droit de le recevoir (routage pub/sub, pas de paywall).
#[derive(Clone, Debug)]
pub enum EventScope {
    /// Les membres d'une guilde.
    Guild(i64),
    /// Les membres qui peuvent voir ce salon de guilde.
    Channel { guild_id: i64, channel_id: i64 },
    /// Les destinataires d'un MP / groupe.
    Dm(i64),
    /// Un utilisateur précis.
    User(i64),
}

impl EventScope {
    /// Portée d'un salon : MP si `guild_id == 0`, sinon salon de guilde.
    pub fn channel(guild_id: i64, channel_id: i64) -> Self {
        if guild_id == 0 {
            EventScope::Dm(channel_id)
        } else {
            EventScope::Channel {
                guild_id,
                channel_id,
            }
        }
    }
}

/// Événement diffusé aux sessions Gateway connectées (avec sa portée de routage).
#[derive(Clone, Debug)]
pub struct HubEvent {
    pub t: String,
    pub d: Value,
    pub scope: EventScope,
}

impl AppState {
    /// Publie un événement sur le bus Gateway. Le routage (qui le reçoit) est ensuite
    /// appliqué par `gateway::should_deliver` selon la **portée** — aucune fuite possible.
    /// `Err` ignorée : absence d'abonné (aucune session connectée) n'est pas une erreur.
    pub fn publish(&self, scope: EventScope, t: &str, d: Value) {
        let _ = self.hub.send(HubEvent {
            t: t.to_string(),
            d,
            scope,
        });
    }
}
