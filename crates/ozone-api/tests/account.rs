//! Tests fonctionnels + cyber : changement de mot de passe (ré-auth + révocation des sessions)
//! et changement d'e-mail (ré-auth + unicité).

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
    let path = std::env::temp_dir().join(format!("ozone-test-acct-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Acct".into(),
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

/// Inscrit un compte, renvoie (access_token, refresh_token).
async fn register(app: &Router, u: &str, e: &str, p: &str) -> (String, String) {
    let r = send(
        app,
        "POST",
        "/auth/register",
        Some(json!({"username":u,"email":e,"password":p})),
        None,
    )
    .await
    .1;
    (
        r["access_token"].as_str().unwrap().to_string(),
        r["refresh_token"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn change_password_revokes_sessions() {
    let app = app().await;
    let (access, refresh) = register(&app, "alice", "a@ac.fr", "Sup3r-Ozone-Pw").await;

    // Mauvais mot de passe actuel → refusé.
    let (s, _) = send(
        &app,
        "PATCH",
        "/users/@me/password",
        Some(json!({"current_password":"faux","new_password":"nouveaumdp12"})),
        Some(&access),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    // Nouveau trop court → 400.
    let (s, _) = send(
        &app,
        "PATCH",
        "/users/@me/password",
        Some(json!({"current_password":"Sup3r-Ozone-Pw","new_password":"court"})),
        Some(&access),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Changement valide.
    let (s, _) = send(
        &app,
        "PATCH",
        "/users/@me/password",
        Some(json!({"current_password":"Sup3r-Ozone-Pw","new_password":"nouveaumdp12"})),
        Some(&access),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // Les sessions sont révoquées : l'ancien refresh token ne fonctionne plus.
    let (s, _) = send(
        &app,
        "POST",
        "/auth/token/refresh",
        Some(json!({"refresh_token":refresh})),
        None,
    )
    .await;
    assert_eq!(
        s,
        StatusCode::UNAUTHORIZED,
        "refresh révoqué après changement de mot de passe"
    );

    // Connexion : nouveau mot de passe OK, ancien refusé.
    let (s, _) = send(
        &app,
        "POST",
        "/auth/login",
        Some(json!({"login":"alice","password":"nouveaumdp12"})),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = send(
        &app,
        "POST",
        "/auth/login",
        Some(json!({"login":"alice","password":"Sup3r-Ozone-Pw"})),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn change_email_flow() {
    let app = app().await;
    let (access, _r) = register(&app, "alice", "a@ac2.fr", "Sup3r-Ozone-Pw").await;
    register(&app, "bob", "bob@ac2.fr", "Sup3r-Ozone-Pw").await;

    // Sans jeton → 401.
    let (s, _) = send(
        &app,
        "PATCH",
        "/users/@me/email",
        Some(json!({"password":"Sup3r-Ozone-Pw","new_email":"x@y.fr"})),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    // Mauvais mot de passe → 401.
    let (s, _) = send(
        &app,
        "PATCH",
        "/users/@me/email",
        Some(json!({"password":"faux","new_email":"x@y.fr"})),
        Some(&access),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    // E-mail déjà pris → 409.
    let (s, _) = send(
        &app,
        "PATCH",
        "/users/@me/email",
        Some(json!({"password":"Sup3r-Ozone-Pw","new_email":"bob@ac2.fr"})),
        Some(&access),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);

    // Changement valide.
    let (s, u) = send(
        &app,
        "PATCH",
        "/users/@me/email",
        Some(json!({"password":"Sup3r-Ozone-Pw","new_email":"nouvelle@ac2.fr"})),
        Some(&access),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(u["email"], "nouvelle@ac2.fr");
    let (_s, me) = send(&app, "GET", "/users/@me", None, Some(&access)).await;
    assert_eq!(me["email"], "nouvelle@ac2.fr");
}
