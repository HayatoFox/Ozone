//! `ozone-core` — cœur client partagé (multiplateforme).
//!
//! Phase 1 : registre d'instances + résolution d'URL d'API. Les clients REST/Gateway,
//! le store normalisé, le cache SQLite et le moteur voix viennent ensuite
//! (cf. `docs/01-architecture.md`, `docs/02-stack-technique.md`).

pub use ozone_proto as proto;

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
    pub fn api_base(&self) -> String {
        let a = self.address.trim().trim_end_matches('/');
        if a.starts_with("http://") || a.starts_with("https://") {
            format!("{a}/api/v1")
        } else {
            format!("https://{a}/api/v1")
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
    fn api_base_forces_https() {
        assert_eq!(
            InstanceRef::new("ozone.exemple.fr").api_base(),
            "https://ozone.exemple.fr/api/v1"
        );
        assert_eq!(
            InstanceRef::new("http://127.0.0.1:8080/").api_base(),
            "http://127.0.0.1:8080/api/v1"
        );
    }
}
