//! Petits utilitaires partagés.

use crate::error::{AppError, AppResult};
use ozone_proto::dto::NameStyle;

/// Parse un identifiant Snowflake (chaîne décimale `u64`) en `i64` (motif binaire stocké en base).
pub fn parse_i64(s: &str) -> AppResult<i64> {
    s.parse::<u64>()
        .map(|v| v as i64)
        .map_err(|_| AppError::bad_request("identifiant invalide"))
}

/// Désérialise le style de pseudonyme (colonne `users.name_style`, JSON) — `None` si absent/invalide.
pub fn parse_name_style(raw: Option<String>) -> Option<NameStyle> {
    raw.and_then(|s| serde_json::from_str(&s).ok())
}
