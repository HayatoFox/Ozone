//! `ozone-core` — cœur client partagé (multiplateforme).
//!
//! Phase 1 : registre d'instances + résolution d'URL d'API. Les clients REST/Gateway,
//! le store normalisé, le cache SQLite et le moteur voix viennent ensuite
//! (cf. `docs/01-architecture.md`, `docs/02-stack-technique.md`).

pub mod cache;
pub mod client;
pub mod client_account;
pub mod client_discovery;
pub mod client_dms;
pub mod client_events;
pub mod client_expressions;
pub mod client_guild;
pub mod client_instance_admin;
pub mod client_invites;
pub mod client_members;
pub mod client_messaging;
pub mod client_notifications;
pub mod client_polls;
pub mod client_relationships;
pub mod client_roles;
pub mod client_search;
pub mod client_webhooks;
pub mod gateway;
pub mod instances;
pub mod session;
pub mod store;
pub use cache::Cache;
pub use client::ApiClient;
pub use gateway::{connect as gateway_connect, GatewayConnection};
pub use instances::{InstanceRegistry, PersistedInstance};
pub use ozone_proto as proto;
pub use session::{EventOutcome, Session};
pub use store::Store;

use ozone_proto::Snowflake;

/// Référence locale à une **instance** enregistrée côté client.
///
/// Le client est multi-instances : chaque instance porte sa propre identité/session,
/// stockée séparément (cf. `docs/features/00-instances.md`).
#[derive(Clone, Debug, Default)]
pub struct InstanceRef {
    pub address: String,
    pub instance_id: Option<Snowflake>,
    pub display_name: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
}

impl InstanceRef {
    pub fn new(address: impl Into<String>) -> Self {
        InstanceRef {
            address: address.into(),
            ..Default::default()
        }
    }

    /// Base de l'API REST de l'instance (force HTTPS si aucun schéma n'est fourni).
    ///
    /// Le binaire tout-en-un sert l'API à la **racine** (cf. `docs/04-api-rest.md` : `/auth/...`,
    /// `/guilds/...`). Un reverse-proxy peut l'exposer sous un préfixe ; dans ce cas, inclure le
    /// préfixe dans `address`.
    pub fn api_base(&self) -> String {
        let a = self.address.trim().trim_end_matches('/');
        if a.starts_with("http://") || a.starts_with("https://") {
            a.to_string()
        } else {
            format!("https://{a}")
        }
    }

    pub fn is_authenticated(&self) -> bool {
        self.access_token.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_base_forces_https_and_keeps_root() {
        // L'API est servie à la racine ; un schéma explicite est respecté.
        assert_eq!(
            InstanceRef::new("ozone.exemple.fr").api_base(),
            "https://ozone.exemple.fr"
        );
        assert_eq!(
            InstanceRef::new("http://127.0.0.1:8080/").api_base(),
            "http://127.0.0.1:8080"
        );
    }
}
