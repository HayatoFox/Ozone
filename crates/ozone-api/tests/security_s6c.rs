//! Vérification cybersécurité de l'admin d'instance (S6c) : accès réservé aux admins,
//! gestion des rôles réservée au propriétaire, propriétaire protégé.

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
    let path = std::env::temp_dir().join(format!("ozone-test-secs6c-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "SecS6c".into(),
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
async fn admin_endpoints_require_admin() {
    let app = app().await;
    let _alice = token(&app, "alice", "a@s6c.fr").await; // propriétaire
    let bob = token(&app, "bob", "b@s6c.fr").await; // simple utilisateur

    let cases = [
        ("GET", "/instance/admin/config", None),
        ("GET", "/instance/admin/invites", None),
        ("POST", "/instance/admin/invites", Some(json!({}))),
        ("GET", "/instance/admin/users", None),
    ];
    for (m, uri, body) in cases {
        let (s, _) = send(&app, m, uri, body, Some(&bob)).await;
        assert_eq!(
            s,
            StatusCode::FORBIDDEN,
            "un non-admin ne doit pas accéder à {uri}"
        );
    }
}

#[tokio::test]
async fn only_owner_manages_roles_and_owner_is_protected() {
    let app = app().await;
    let alice = token(&app, "alice", "a@s6cb.fr").await; // propriétaire
    let alice_id = uid(&app, &alice).await;
    let bob = token(&app, "bob", "b@s6cb.fr").await;
    let bob_id = uid(&app, &bob).await;
    let _carol = token(&app, "carol", "c@s6cb.fr").await;
    let carol_id = uid(&app, &_carol).await;

    // Le propriétaire promeut bob admin.
    let (s, _) = send(
        &app,
        "PUT",
        &format!("/instance/admin/users/{bob_id}/role"),
        Some(json!({"role":"admin"})),
        Some(&alice),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "le propriétaire promeut");

    // Bob (admin, pas propriétaire) ne peut pas gérer les rôles.
    let (s, _) = send(
        &app,
        "PUT",
        &format!("/instance/admin/users/{carol_id}/role"),
        Some(json!({"role":"admin"})),
        Some(&bob),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "gestion des rôles réservée au propriétaire"
    );

    // Bob (admin) ne peut pas suspendre le propriétaire.
    let (s, _) = send(
        &app,
        "PATCH",
        &format!("/instance/admin/users/{alice_id}"),
        Some(json!({"suspended":true})),
        Some(&bob),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "le propriétaire ne peut pas être suspendu"
    );

    // Le propriétaire ne peut pas changer son propre rôle.
    let (s, _) = send(
        &app,
        "PUT",
        &format!("/instance/admin/users/{alice_id}/role"),
        Some(json!({"role":"admin"})),
        Some(&alice),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "le rôle du propriétaire est immuable"
    );

    // On ne peut pas promouvoir « owner ».
    let (s, _) = send(
        &app,
        "PUT",
        &format!("/instance/admin/users/{bob_id}/role"),
        Some(json!({"role":"owner"})),
        Some(&alice),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "rôle owner non attribuable");
}

/// Régression : un compte suspendu ne doit pas pouvoir contourner la suspension
/// en renouvelant ses jetons via son refresh token.
#[tokio::test]
async fn suspension_revokes_token_renewal() {
    let app = app().await;
    let alice = token(&app, "alice", "a@s6cc.fr").await; // propriétaire
    let (_, b) = send(
        &app,
        "POST",
        "/auth/register",
        Some(json!({"username":"bob","email":"b@s6cc.fr","password":"Sup3r-Ozone-Pw"})),
        None,
    )
    .await;
    let bob_refresh = b["refresh_token"].as_str().unwrap().to_string();
    let bob_id = uid(&app, b["access_token"].as_str().unwrap()).await;

    // Le propriétaire suspend bob → ses sessions doivent être révoquées.
    let (s, _) = send(
        &app,
        "PATCH",
        &format!("/instance/admin/users/{bob_id}"),
        Some(json!({"suspended":true})),
        Some(&alice),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "suspension");

    // Le refresh token de bob ne peut plus produire de nouveaux jetons d'accès.
    let (s, _) = send(
        &app,
        "POST",
        "/auth/token/refresh",
        Some(json!({ "refresh_token": bob_refresh })),
        None,
    )
    .await;
    assert!(
        s == StatusCode::UNAUTHORIZED || s == StatusCode::FORBIDDEN,
        "un compte suspendu ne peut pas renouveler ses jetons (reçu {s})"
    );
}
