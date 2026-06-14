//! Tests fonctionnels de la présence : statut effectif, statut personnalisé, invisible,
//! comptage multi-connexions. La connexion/déconnexion Gateway est simulée via le registre
//! (exactement ce que fait `handle_socket`).

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
    let path = std::env::temp_dir().join(format!("ozone-test-pres-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Pres".into(),
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
        Some(json!({"username":u,"email":e,"password":"Sup3r-Ozone-Pw"})),
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
async fn presence_lifecycle_and_status() {
    let (app, state) = build().await;
    let alice = token(&app, "alice", "a@ps.fr").await;
    let alice_id = uid(&app, &alice).await;
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

    // Hors ligne par défaut → pas de présence.
    let (_s, p) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/presences"),
        None,
        Some(&alice),
    )
    .await;
    assert_eq!(p.as_array().unwrap().len(), 0);

    // Connexion Gateway simulée → en ligne.
    assert!(
        state.presence.connect(alice_id),
        "1ère connexion → en ligne"
    );
    let (_s, p) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/presences"),
        None,
        Some(&alice),
    )
    .await;
    assert_eq!(p.as_array().unwrap().len(), 1);
    assert_eq!(p[0]["status"], "online");

    // Statut dnd + statut perso.
    let (s, sp) = send(
        &app,
        "PUT",
        "/users/@me/presence",
        Some(json!({"status":"dnd","custom_status":"au taf"})),
        Some(&alice),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(sp["status"], "dnd");
    let (_s, p) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/presences"),
        None,
        Some(&alice),
    )
    .await;
    assert_eq!(p[0]["status"], "dnd");
    assert_eq!(p[0]["custom_status"], "au taf");

    // Invisible → apparaît hors ligne (absent de la liste).
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
        Some(&alice),
    )
    .await;
    assert_eq!(p.as_array().unwrap().len(), 0, "invisible = masqué");

    // Déconnexion → hors ligne.
    send(
        &app,
        "PUT",
        "/users/@me/presence",
        Some(json!({"status":"online"})),
        Some(&alice),
    )
    .await;
    assert!(
        state.presence.disconnect(alice_id),
        "dernière déconnexion → hors ligne"
    );
    let (_s, p) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/presences"),
        None,
        Some(&alice),
    )
    .await;
    assert_eq!(p.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn multi_connection_counting() {
    let (_app, state) = build().await;
    let uid = 42_i64;
    assert!(state.presence.connect(uid), "1ère → online");
    assert!(
        !state.presence.connect(uid),
        "2ème connexion ne (re)passe pas online"
    );
    assert!(!state.presence.disconnect(uid), "il reste 1 connexion");
    assert_eq!(state.presence.effective(uid).0, "online");
    assert!(state.presence.disconnect(uid), "dernière → offline");
    assert_eq!(state.presence.effective(uid).0, "offline");
}
