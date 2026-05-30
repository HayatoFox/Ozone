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

/// Événement diffusé aux sessions Gateway connectées.
#[derive(Clone, Debug)]
pub struct HubEvent {
    pub t: String,
    pub d: Value,
}
