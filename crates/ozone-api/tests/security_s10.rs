//! Vérification cybersécurité S10 : profils & réglages.
//! Édition limitée à soi, e-mail jamais exposé via le profil public, réglages isolés par utilisateur.

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
    let path = std::env::temp_dir().join(format!("ozone-test-secs10-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "SecS10".into(),
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
        Some(json!({"username":u,"email":e,"password":"motdepasse"})),
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
async fn profile_edit_requires_auth() {
    let app = app().await;
    let (s, _) = send(&app, "PATCH", "/users/@me", Some(json!({"bio":"x"})), None).await;
    assert_eq!(
        s,
        StatusCode::UNAUTHORIZED,
        "édition de profil sans jeton refusée"
    );
    let (s, _) = send(&app, "GET", "/users/@me/settings", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn public_profile_never_leaks_email() {
    let app = app().await;
    let alice = token(&app, "alice", "secret-mail@pr.fr").await;
    let alice_id = uid(&app, &alice).await;
    let bob = token(&app, "bob", "b@s10.fr").await;

    let (s, p) = send(
        &app,
        "GET",
        &format!("/users/{alice_id}/profile"),
        None,
        Some(&bob),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(p.get("email").is_none(), "aucune fuite d'e-mail");
    // Sérialisation complète ne contient pas la chaîne d'e-mail.
    assert!(
        !p.to_string().contains("secret-mail"),
        "l'e-mail ne doit apparaître nulle part"
    );

    // Profil inexistant → 404.
    let (s, _) = send(&app, "GET", "/users/999999/profile", None, Some(&bob)).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn settings_are_isolated_per_user() {
    let app = app().await;
    let alice = token(&app, "alice", "a@s10b.fr").await;
    let bob = token(&app, "bob", "b@s10b.fr").await;

    send(
        &app,
        "PUT",
        "/users/@me/settings",
        Some(json!({"data":{"secret":"alice-only"}})),
        Some(&alice),
    )
    .await;

    // bob ne voit pas les réglages d'alice : il a les siens (vides).
    let (_s, got) = send(&app, "GET", "/users/@me/settings", None, Some(&bob)).await;
    assert_eq!(
        got["data"],
        json!({}),
        "réglages strictement par utilisateur"
    );

    // alice retrouve bien les siens.
    let (_s, got) = send(&app, "GET", "/users/@me/settings", None, Some(&alice)).await;
    assert_eq!(got["data"]["secret"], "alice-only");
}
