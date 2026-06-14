//! Signalisation HTTP du SFU + **authentification du plan média** : chaque appel doit présenter
//! le jeton vocal (`kind = "voice"`, `sub = "<user_id>.<channel_id>"`) émis par l'API et diffusé
//! via `VOICE_SERVER_UPDATE`. Le SFU vérifie la signature (secret partagé `OZONE_VOICE_SECRET`),
//! l'expiration, et que le salon demandé correspond au jeton. **Fail-closed** : sans secret
//! configuré, toute connexion est refusée.

use crate::room::{PeerCmd, Sfu};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;

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
    /// Manifeste optionnel `id_de_piste → nature` (`mic`/`cam`/`screen`). Étiquette d'affichage
    /// uniquement ; filtrée par liste blanche côté SFU. L'identité reste imposée par le jeton.
    #[serde(default)]
    tracks: std::collections::HashMap<String, String>,
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
    match state.sfu.join(&room, &uid, req.sdp, req.tracks).await {
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

#[derive(Deserialize)]
struct EvictReq {
    /// Jeton `kind = "evict"`, `sub = "<uid_cible>.<room>"`, signé par l'API (secret partagé).
    #[serde(default)]
    token: String,
}

/// `POST /sfu/rooms/:room/evict` — déconnexion de MODÉRATION initiée par l'API (pas par le pair
/// lui-même). Authentifiée par un jeton `kind = "evict"` signé avec le secret partagé : l'API
/// l'émet quand un modérateur déconnecte/déplace un membre, ou à la fermeture de sa session.
/// Retire TOUS les pairs de l'uid ciblé dans la salle (coupe réellement son média).
async fn evict(
    State(state): State<AppState>,
    Path(room): Path<String>,
    Json(req): Json<EvictReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let Some(secret) = state.voice_secret.as_ref().as_ref() else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "plan média non configuré".into(),
        ));
    };
    let claims = ozone_proto::token::verify(secret, &req.token, "evict")
        .ok_or((StatusCode::UNAUTHORIZED, "jeton d'éviction invalide".to_string()))?;
    // sub = "<uid_cible>.<room>"
    let (uid, cid) = claims
        .sub
        .split_once('.')
        .ok_or((StatusCode::UNAUTHORIZED, "jeton d'éviction mal formé".to_string()))?;
    if cid != room {
        return Err((StatusCode::FORBIDDEN, "le jeton ne vise pas ce salon".into()));
    }
    let removed = state.sfu.evict_uid(&room, uid).await;
    Ok(Json(serde_json::json!({ "removed": removed })))
}

/// `GET /sfu/rooms/:room/peers/:peer_id/signal?token=…` — canal WebSocket de **renégociation**.
/// Authentifié par le jeton vocal ; on vérifie que le `uid` du jeton **possède** le `peer_id`.
async fn signal(
    State(state): State<AppState>,
    Path((room, peer_id)): Path<(String, String)>,
    Query(q): Query<TokenQuery>,
    ws: WebSocketUpgrade,
) -> axum::response::Response {
    let uid = match authorize(&state, &room, &q.token) {
        Ok(u) => u,
        Err((c, m)) => return (c, m).into_response(),
    };
    match state.sfu.signal_handle(&room, &peer_id, &uid).await {
        Some(cmd_tx) => ws.on_upgrade(move |socket| signal_socket(socket, cmd_tx)),
        None => (StatusCode::FORBIDDEN, "pair inconnu ou non possédé").into_response(),
    }
}

/// Pont WebSocket ↔ acteur du pair : transmet les messages client vers l'acteur, et les messages
/// de l'acteur (offres/réponses) vers le client.
async fn signal_socket(socket: WebSocket, cmd_tx: mpsc::UnboundedSender<PeerCmd>) {
    let (mut sink, mut stream) = socket.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();
    let _ = cmd_tx.send(PeerCmd::WsConnected(out_tx));

    // Sortant : messages de l'acteur → WebSocket.
    let send_task = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            if sink.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // Entrant : messages du client → acteur.
    while let Some(Ok(msg)) = stream.next().await {
        match msg {
            Message::Text(t) => {
                if cmd_tx.send(PeerCmd::ClientMsg(t)).is_err() {
                    break;
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
    let _ = cmd_tx.send(PeerCmd::WsClosed);
    send_task.abort();
}

async fn health() -> &'static str {
    "ok"
}

/// Construit le routeur de signalisation.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/sfu/rooms/:room/peers", post(join))
        .route("/sfu/rooms/:room/evict", post(evict))
        .route("/sfu/rooms/:room/peers/:peer_id", delete(leave))
        .route("/sfu/rooms/:room/peers/:peer_id/signal", get(signal))
        .with_state(state)
}
