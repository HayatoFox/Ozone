//! Signalisation HTTP du SFU + **authentification du plan média** : chaque appel doit présenter
//! le jeton vocal (`kind = "voice"`, `sub = "<user_id>.<channel_id>"`) émis par l'API et diffusé
//! via `VOICE_SERVER_UPDATE`. Le SFU vérifie la signature (secret partagé `OZONE_VOICE_SECRET`),
//! l'expiration, et que le salon demandé correspond au jeton. **Fail-closed** : sans secret
//! configuré, toute connexion est refusée.

use crate::room::Sfu;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub sfu: Arc<Sfu>,
    /// Secret partagé avec l'API pour vérifier les jetons vocaux. `None` ⇒ plan média **fermé**.
    pub voice_secret: Arc<Option<Vec<u8>>>,
}

#[derive(Deserialize)]
struct OfferReq {
    sdp: String,
    #[serde(default)]
    token: String,
}

#[derive(Serialize)]
struct AnswerResp {
    peer_id: String,
    sdp: String,
}

#[derive(Deserialize)]
struct TokenQuery {
    #[serde(default)]
    token: String,
}

/// Vérifie le jeton vocal et que `room` correspond au salon autorisé. Renvoie l'`user_id`.
fn authorize(
    state: &AppState,
    room: &str,
    token: &str,
) -> Result<String, (StatusCode, &'static str)> {
    let Some(secret) = state.voice_secret.as_ref().as_ref() else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "plan média non configuré (OZONE_VOICE_SECRET requis)",
        ));
    };
    let claims = ozone_proto::token::verify(secret, token, "voice")
        .ok_or((StatusCode::UNAUTHORIZED, "jeton vocal invalide ou expiré"))?;
    // sub = "<user_id>.<channel_id>"
    let (uid, cid) = claims
        .sub
        .split_once('.')
        .ok_or((StatusCode::UNAUTHORIZED, "jeton vocal mal formé"))?;
    if cid != room {
        return Err((StatusCode::FORBIDDEN, "le jeton n'autorise pas ce salon"));
    }
    Ok(uid.to_string())
}

/// `POST /sfu/rooms/:room/peers` — offre SDP (avec jeton vocal) → réponse SDP.
async fn join(
    State(state): State<AppState>,
    Path(room): Path<String>,
    Json(req): Json<OfferReq>,
) -> Result<Json<AnswerResp>, (StatusCode, String)> {
    let uid = authorize(&state, &room, &req.token).map_err(|(c, m)| (c, m.to_string()))?;
    match state.sfu.join(&room, &uid, req.sdp).await {
        Ok((peer_id, sdp)) => Ok(Json(AnswerResp { peer_id, sdp })),
        Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
    }
}

/// `DELETE /sfu/rooms/:room/peers/:peer_id?token=…` — déconnecte **son** pair.
async fn leave(
    State(state): State<AppState>,
    Path((room, peer_id)): Path<(String, String)>,
    Query(q): Query<TokenQuery>,
) -> Result<StatusCode, (StatusCode, String)> {
    let uid = authorize(&state, &room, &q.token).map_err(|(c, m)| (c, m.to_string()))?;
    if state.sfu.leave(&room, &peer_id, &uid).await {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((StatusCode::FORBIDDEN, "pair inconnu ou non possédé".into()))
    }
}

async fn health() -> &'static str {
    "ok"
}

/// Construit le routeur de signalisation.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/sfu/rooms/:room/peers", post(join))
        .route("/sfu/rooms/:room/peers/:peer_id", delete(leave))
        .with_state(state)
}
