//! Extracteurs axum : `AuthUser` (jeton Bearer → identité) et `ClientIp` (clé de rate-limiting).

use crate::crypto;
use crate::error::AppError;
use crate::state::AppState;
use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use ozone_proto::Snowflake;
use std::net::SocketAddr;

pub struct AuthUser {
    pub id: Snowflake,
}

/// IP cliente pour le rate-limiting des routes non authentifiées (login/register/gate).
///
/// **Sécurité** : l'IP socket réelle (`ConnectInfo`) est la source par défaut. `X-Forwarded-For`
/// n'est cru **que** si l'instance est déclarée derrière un reverse-proxy de confiance
/// (`OZONE_TRUSTED_PROXY=1` → `state.trust_proxy`). Sinon un client en accès direct usurperait
/// trivialement son IP (un XFF différent par requête) et annulerait le rate-limit par IP.
/// Repli `"unknown"` pour les tests `oneshot` (sans socket). Jamais d'échec d'extraction.
pub struct ClientIp(pub String);

#[axum::async_trait]
impl FromRequestParts<AppState> for ClientIp {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Derrière un proxy de confiance UNIQUEMENT : honorer X-Forwarded-For (dernier hop ajouté
        // par le proxy = client réel ; on prend le premier élément, l'origine de la chaîne).
        if state.trust_proxy {
            if let Some(xff) = parts
                .headers
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.split(',').next())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                return Ok(ClientIp(xff.to_string()));
            }
        }
        // Source de confiance : adresse de connexion réelle (présente avec ConnectInfo).
        if let Some(ConnectInfo(addr)) = parts.extensions.get::<ConnectInfo<SocketAddr>>() {
            return Ok(ClientIp(addr.ip().to_string()));
        }
        Ok(ClientIp("unknown".into()))
    }
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
