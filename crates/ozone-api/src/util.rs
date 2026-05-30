//! Petits utilitaires partagés.

use crate::error::{AppError, AppResult};

/// Parse un identifiant Snowflake (chaîne décimale `u64`) en `i64` (motif binaire stocké en base).
pub fn parse_i64(s: &str) -> AppResult<i64> {
    s.parse::<u64>()
        .map(|v| v as i64)
        .map_err(|_| AppError::bad_request("identifiant invalide"))
}
