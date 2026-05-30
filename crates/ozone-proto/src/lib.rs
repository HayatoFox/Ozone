//! `ozone-proto` — source de vérité des types échangés entre le client et le serveur Ozone.
//!
//! Voir la conception : `docs/03-modele-de-donnees.md`, `docs/04-api-rest.md`,
//! `docs/05-gateway-temps-reel.md`.

pub mod dto;
pub mod gateway;
pub mod ids;
pub mod perms;

pub use dto::*;
pub use ids::{Snowflake, SnowflakeGenerator, OZONE_EPOCH_MS};
