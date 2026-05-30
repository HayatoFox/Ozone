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
    /// Tout le monde (rare).
    Global,
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
