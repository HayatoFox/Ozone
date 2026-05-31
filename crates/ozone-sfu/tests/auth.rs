//! Vérification cybersécurité du plan média (R7) : la signalisation SFU exige un jeton vocal
//! valide (signature + `kind` + salon correspondant), et le départ exige la propriété du pair.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use ozone_sfu::room::Sfu;
use ozone_sfu::server::{build_router, AppState};
use std::sync::Arc;
use tower::ServiceExt;

const SECRET: &[u8] = b"secret-vocal-partage-de-test";

fn state(secret: Option<&[u8]>) -> AppState {
    AppState {
        sfu: Sfu::new().expect("sfu"),
        voice_secret: Arc::new(secret.map(|s| s.to_vec())),
    }
}

/// Jeton vocal comme l'émet l'API : `sub = "<user>.<channel>"`, `kind = "voice"`.
fn voice_token(secret: &[u8], user: &str, channel: &str) -> String {
    ozone_proto::token::encode(secret, &format!("{user}.{channel}"), "voice", 60)
}

async fn post(state: AppState, uri: &str, body: serde_json::Value) -> StatusCode {
    let app = build_router(state);
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    app.oneshot(req).await.unwrap().status()
}

async fn delete(state: AppState, uri: &str) -> StatusCode {
    let app = build_router(state);
    let req = Request::builder()
        .method("DELETE")
        .uri(uri)
        .body(Body::empty())
        .unwrap();
    app.oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn fail_closed_without_secret() {
    // Aucun secret configuré → plan média fermé.
    let s = post(
        state(None),
        "/sfu/rooms/100/peers",
        serde_json::json!({"sdp":"x","token":"x"}),
    )
    .await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn rejects_invalid_token() {
    let s = post(
        state(Some(SECRET)),
        "/sfu/rooms/100/peers",
        serde_json::json!({"sdp":"x","token":"pas-un-jeton"}),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // Jeton signé avec un AUTRE secret → invalide.
    let foreign = voice_token(b"mauvais-secret", "42", "100");
    let s = post(
        state(Some(SECRET)),
        "/sfu/rooms/100/peers",
        serde_json::json!({"sdp":"x","token":foreign}),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn rejects_wrong_room() {
    // Jeton pour le salon 100, mais on tente de rejoindre le salon 999.
    let tok = voice_token(SECRET, "42", "100");
    let s = post(
        state(Some(SECRET)),
        "/sfu/rooms/999/peers",
        serde_json::json!({"sdp":"x","token":tok}),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "le jeton n'autorise pas ce salon");
}

#[tokio::test]
async fn valid_token_passes_auth_then_sfu_rejects_bad_sdp() {
    // Jeton valide + salon correspondant → l'auth passe ; l'offre SDP invalide est rejetée par le SFU.
    let tok = voice_token(SECRET, "42", "100");
    let s = post(
        state(Some(SECRET)),
        "/sfu/rooms/100/peers",
        serde_json::json!({"sdp":"offre-sdp-invalide","token":tok}),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::BAD_REQUEST,
        "auth franchie, SDP invalide → 400"
    );
}

#[tokio::test]
async fn leave_requires_token_and_ownership() {
    // Sans secret → fermé.
    assert_eq!(
        delete(state(None), "/sfu/rooms/100/peers/p1?token=x").await,
        StatusCode::SERVICE_UNAVAILABLE
    );
    // Jeton invalide → 401.
    assert_eq!(
        delete(state(Some(SECRET)), "/sfu/rooms/100/peers/p1?token=mauvais").await,
        StatusCode::UNAUTHORIZED
    );
    // Jeton valide mais pair inexistant / non possédé → 403.
    let tok = voice_token(SECRET, "42", "100");
    assert_eq!(
        delete(
            state(Some(SECRET)),
            &format!("/sfu/rooms/100/peers/p1?token={tok}")
        )
        .await,
        StatusCode::FORBIDDEN
    );
}
