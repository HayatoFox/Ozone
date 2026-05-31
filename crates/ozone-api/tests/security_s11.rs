//! Vérification cybersécurité S11 : présence.
//! Auth requise, validation du statut, présences réservées aux membres, invisible masqué.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use ozone_api::config::Config;
use ozone_api::state::AppState;
use ozone_api::{bootstrap_state, build_app};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn build() -> (Router, AppState) {
    let unique = format!(
        "{}_{:?}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        std::thread::current().id()
    );
    let path = std::env::temp_dir().join(format!("ozone-test-secs11-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "SecS11".into(),
        instance_description: None,
        registration_policy: "open".into(),
        instance_password: None,
        version: "0.1.0-test".into(),
    };
    let state = bootstrap_state(&cfg).await.expect("bootstrap");
    (build_app(state.clone()), state)
}

async fn send(
    app: &Router,
    m: &str,
    uri: &str,
    body: Option<Value>,
    tok: Option<&str>,
) -> (StatusCode, Value) {
    let mut b = Request::builder().method(m).uri(uri);
    if let Some(t) = tok {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    let body = match body {
        Some(v) => {
            b = b.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let resp = app.clone().oneshot(b.body(body).unwrap()).await.unwrap();
    let st = resp.status();
    let by = resp.into_body().collect().await.unwrap().to_bytes();
    (st, serde_json::from_slice(&by).unwrap_or(Value::Null))
}

async fn token(app: &Router, u: &str, e: &str) -> String {
    send(
        app,
        "POST",
        "/auth/register",
        Some(json!({"username":u,"email":e,"password":"motdepasse"})),
        None,
    )
    .await
    .1["access_token"]
        .as_str()
        .unwrap()
        .to_string()
}
async fn uid(app: &Router, t: &str) -> i64 {
    send(app, "GET", "/users/@me", None, Some(t)).await.1["id"]
        .as_str()
        .unwrap()
        .parse::<u64>()
        .unwrap() as i64
}

#[tokio::test]
async fn presence_requires_auth_and_valid_status() {
    let (app, _state) = build().await;
    let alice = token(&app, "alice", "a@s11.fr").await;

    let (s, _) = send(
        &app,
        "PUT",
        "/users/@me/presence",
        Some(json!({"status":"online"})),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "présence sans jeton refusée");

    let (s, _) = send(
        &app,
        "PUT",
        "/users/@me/presence",
        Some(json!({"status":"superactif"})),
        Some(&alice),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "statut invalide");

    let big = "x".repeat(129);
    let (s, _) = send(
        &app,
        "PUT",
        "/users/@me/presence",
        Some(json!({"status":"online","custom_status":big})),
        Some(&alice),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "statut perso trop long");
}

#[tokio::test]
async fn presences_member_only_and_invisible_hidden() {
    let (app, state) = build().await;
    let alice = token(&app, "alice", "a@s11b.fr").await;
    let alice_id = uid(&app, &alice).await;
    let bob = token(&app, "bob", "b@s11b.fr").await;
    let carol = token(&app, "carol", "c@s11b.fr").await; // non-membre
    let g = send(
        &app,
        "POST",
        "/guilds",
        Some(json!({"name":"G"})),
        Some(&alice),
    )
    .await
    .1;
    let gid = g["id"].as_str().unwrap().to_string();
    // bob rejoint.
    let inv = send(
        &app,
        "POST",
        &format!("/guilds/{gid}/invites"),
        Some(json!({})),
        Some(&alice),
    )
    .await
    .1;
    let code = inv["code"].as_str().unwrap().to_string();
    send(&app, "POST", &format!("/invites/{code}"), None, Some(&bob)).await;

    // Un non-membre ne peut pas voir les présences.
    let (s, _) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/presences"),
        None,
        Some(&carol),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "présences réservées aux membres");

    // alice en ligne puis invisible : bob (membre) ne doit pas la voir.
    state.presence.connect(alice_id);
    send(
        &app,
        "PUT",
        "/users/@me/presence",
        Some(json!({"status":"invisible"})),
        Some(&alice),
    )
    .await;
    let (_s, p) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/presences"),
        None,
        Some(&bob),
    )
    .await;
    let alice_str = alice_id.to_string();
    assert!(
        !p.as_array()
            .unwrap()
            .iter()
            .any(|x| x["user_id"] == alice_str),
        "un membre invisible ne doit apparaître dans aucune présence"
    );
}
