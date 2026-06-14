//! Tests fonctionnels : édition de profil, profil public, réglages client.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use ozone_api::config::Config;
use ozone_api::{bootstrap_state, build_app};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn app() -> Router {
    let unique = format!(
        "{}_{:?}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        std::thread::current().id()
    );
    let path = std::env::temp_dir().join(format!("ozone-test-prof-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Prof".into(),
        instance_description: None,
        registration_policy: "open".into(),
        instance_password: None,
        version: "0.1.0-test".into(),
    };
    build_app(bootstrap_state(&cfg).await.expect("bootstrap"))
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
async fn uid(app: &Router, t: &str) -> String {
    send(app, "GET", "/users/@me", None, Some(t)).await.1["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn edit_and_view_profile() {
    let app = app().await;
    let alice = token(&app, "alice", "a@pr.fr").await;
    let alice_id = uid(&app, &alice).await;

    // Édition.
    let (s, p) = send(
        &app,
        "PATCH",
        "/users/@me",
        Some(json!({"bio":"coucou","pronouns":"elle","accent_color":255,"display_name":"Alice A"})),
        Some(&alice),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(p["bio"], "coucou");
    assert_eq!(p["pronouns"], "elle");
    assert_eq!(p["accent_color"], 255);
    assert_eq!(p["display_name"], "Alice A");

    // Profil public visible par un tiers, SANS e-mail.
    let bob = token(&app, "bob", "b@pr.fr").await;
    let (s, pub_p) = send(
        &app,
        "GET",
        &format!("/users/{alice_id}/profile"),
        None,
        Some(&bob),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(pub_p["bio"], "coucou");
    assert!(
        pub_p.get("email").is_none(),
        "le profil public ne doit pas exposer l'e-mail"
    );

    // Effacer la bio (chaîne vide).
    let (_s, p) = send(
        &app,
        "PATCH",
        "/users/@me",
        Some(json!({"bio":""})),
        Some(&alice),
    )
    .await;
    assert!(p["bio"].is_null());
    // ... mais les autres champs restent.
    assert_eq!(p["pronouns"], "elle");
}

#[tokio::test]
async fn profile_validation() {
    let app = app().await;
    let alice = token(&app, "alice", "a@pr2.fr").await;
    let big = "x".repeat(191);
    let (s, _) = send(
        &app,
        "PATCH",
        "/users/@me",
        Some(json!({"bio":big})),
        Some(&alice),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "bio > 190");
    let (s, _) = send(
        &app,
        "PATCH",
        "/users/@me",
        Some(json!({"accent_color":16777216})),
        Some(&alice),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "couleur > 0xFFFFFF");
}

#[tokio::test]
async fn client_settings_roundtrip() {
    let app = app().await;
    let alice = token(&app, "alice", "a@pr3.fr").await;

    let (s, init) = send(&app, "GET", "/users/@me/settings", None, Some(&alice)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(init["data"], json!({}));

    let (s, _) = send(
        &app,
        "PUT",
        "/users/@me/settings",
        Some(json!({"data":{"theme":"dark","locale":"fr"}})),
        Some(&alice),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (_s, got) = send(&app, "GET", "/users/@me/settings", None, Some(&alice)).await;
    assert_eq!(got["data"]["theme"], "dark");

    // Doit être un objet.
    let (s, _) = send(
        &app,
        "PUT",
        "/users/@me/settings",
        Some(json!({"data":5})),
        Some(&alice),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}
