//! Extracteur axum `AuthUser` : valide le jeton d'accès (Bearer) et fournit l'identité.

use crate::crypto;
use crate::error::AppError;
use crate::state::AppState;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use ozone_proto::Snowflake;

pub struct AuthUser {
    pub id: Snowflake,
}

#[axum::async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| AppError::unauthorized("jeton d'autorisation manquant"))?;
        let token = header
            .strip_prefix("Bearer ")
            .ok_or_else(|| AppError::unauthorized("schéma d'autorisation invalide"))?;
        let claims = crypto::jwt_verify(&state.jwt_secret, token, "access")
            .ok_or_else(|| AppError::unauthorized("jeton invalide ou expiré"))?;
        let id = claims
            .sub
            .parse::<u64>()
            .map_err(|_| AppError::unauthorized("sujet de jeton invalide"))?;
        Ok(AuthUser {
            id: Snowflake::new(id),
        })
    }
}
