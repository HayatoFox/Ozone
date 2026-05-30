//! Configuration du serveur, lue depuis l'environnement (mode tout-en-un).

use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    /// Adresse d'écoute HTTP (ex. `127.0.0.1:8080`).
    pub bind: String,
    /// Chemin du fichier SQLite (mode tout-en-un).
    pub db_path: String,
    pub instance_name: String,
    pub instance_description: Option<String>,
    /// `open` | `invite` | `closed`.
    pub registration_policy: String,
    /// Mot de passe d'instance (en clair, seulement au bootstrap). `None` = pas de gate.
    pub instance_password: Option<String>,
    pub version: String,
}

impl Config {
    pub fn from_env() -> Self {
        Config {
            bind: env::var("OZONE_BIND").unwrap_or_else(|_| "127.0.0.1:8080".into()),
            db_path: env::var("OZONE_DB_PATH").unwrap_or_else(|_| "ozone.db".into()),
            instance_name: env::var("OZONE_INSTANCE_NAME").unwrap_or_else(|_| "Ozone".into()),
            instance_description: env::var("OZONE_INSTANCE_DESCRIPTION").ok(),
            registration_policy: env::var("OZONE_REGISTRATION").unwrap_or_else(|_| "open".into()),
            instance_password: env::var("OZONE_INSTANCE_PASSWORD")
                .ok()
                .filter(|s| !s.is_empty()),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}
