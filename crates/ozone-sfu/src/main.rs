//! Binaire du nœud média SFU Ozone : signalisation HTTP (offre/réponse SDP) au-dessus du cœur SFU.
//!
//! Processus séparé de l'API. L'API émet `VOICE_SERVER_UPDATE { token, endpoint }` ; le client
//! présente son offre SDP ici et reçoit la réponse, puis le média RTP/SRTP circule en UDP.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use ozone_sfu::room::Sfu;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize)]
struct OfferReq {
    sdp: String,
}

#[derive(Serialize)]
struct AnswerResp {
    peer_id: String,
    sdp: String,
}

/// `POST /sfu/rooms/:room/peers` — soumet une offre SDP, reçoit la réponse + un identifiant de pair.
async fn join(
    State(sfu): State<Arc<Sfu>>,
    Path(room): Path<String>,
    Json(req): Json<OfferReq>,
) -> Result<Json<AnswerResp>, (StatusCode, String)> {
    match sfu.join(&room, req.sdp).await {
        Ok((peer_id, sdp)) => Ok(Json(AnswerResp { peer_id, sdp })),
        Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
    }
}

/// `DELETE /sfu/rooms/:room/peers/:peer_id` — déconnecte un pair.
async fn leave(
    State(sfu): State<Arc<Sfu>>,
    Path((room, peer_id)): Path<(String, String)>,
) -> StatusCode {
    sfu.leave(&room, &peer_id).await;
    StatusCode::NO_CONTENT
}

async fn health() -> &'static str {
    "ok"
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let sfu = Sfu::new()?;
    let app = Router::new()
        .route("/health", get(health))
        .route("/sfu/rooms/:room/peers", post(join))
        .route("/sfu/rooms/:room/peers/:peer_id", delete(leave))
        .with_state(sfu);

    let bind = std::env::var("OZONE_SFU_BIND").unwrap_or_else(|_| "127.0.0.1:8081".to_string());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!("Nœud média SFU Ozone à l'écoute sur http://{bind}");
    axum::serve(listener, app).await?;
    Ok(())
}
