//! Tests fonctionnels du tableau de bord d'instance : inscription sur invitation,
//! suspension de comptes, promotion de rôle d'instance.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use ozone_api::config::Config;
use ozone_api::{bootstrap_state, build_app};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn app_with(policy: &str) -> Router {
    let unique = format!(
        "{}_{:?}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        std::thread::current().id()
    );
    let path = std::env::temp_dir().join(format!("ozone-test-ia-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "IA".into(),
        instance_description: None,
        registration_policy: policy.into(),
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
async fn invite_only_registration() {
    let app = app_with("invite").await;
    // 1er compte = propriétaire, contourne la politique.
    let (s, owner) = send(
        &app,
        "POST",
        "/auth/register",
        Some(json!({"username":"alice","email":"a@ia.fr","password":"Sup3r-Ozone-Pw"})),
        None,
    )
    .await;
    assert_eq!(
        s,
        StatusCode::OK,
        "le 1er compte (propriétaire) contourne la politique"
    );
    let atok = owner["access_token"].as_str().unwrap().to_string();

    let (s, cfg) = send(&app, "GET", "/instance/admin/config", None, Some(&atok)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(cfg["registration_policy"], "invite");

    // 2e sans code → refusé.
    let (s, _) = send(
        &app,
        "POST",
        "/auth/register",
        Some(json!({"username":"bob","email":"b@ia.fr","password":"Sup3r-Ozone-Pw"})),
        None,
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "inscription sans invitation refusée"
    );

    // Le propriétaire crée une invitation d'instance.
    let (s, inv) = send(
        &app,
        "POST",
        "/instance/admin/invites",
        Some(json!({"max_uses":1})),
        Some(&atok),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let code = inv["code"].as_str().unwrap().to_string();

    // 2e avec code → accepté.
    let (s, _) = send(
        &app,
        "POST",
        "/auth/register",
        Some(
            json!({"username":"bob","email":"b@ia.fr","password":"Sup3r-Ozone-Pw","invite_code":code}),
        ),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "inscription avec invitation acceptée");

    // Mauvais code → refusé.
    let (s, _) = send(&app, "POST", "/auth/register", Some(json!({"username":"carol","email":"c@ia.fr","password":"Sup3r-Ozone-Pw","invite_code":"ZZZZZZZZ"})), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "mauvaise invitation refusée");
}

#[tokio::test]
async fn suspension_blocks_login() {
    let app = app_with("open").await;
    let atok = token(&app, "alice", "a@ia2.fr").await;
    let btok = token(&app, "bob", "b@ia2.fr").await;
    let bob_id = uid(&app, &btok).await;

    let (s, _) = send(
        &app,
        "POST",
        "/auth/login",
        Some(json!({"login":"bob","password":"Sup3r-Ozone-Pw"})),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "bob se connecte avant suspension");

    let (s, _) = send(
        &app,
        "PATCH",
        &format!("/instance/admin/users/{bob_id}"),
        Some(json!({"suspended":true})),
        Some(&atok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "suspension");
    let (s, _) = send(
        &app,
        "POST",
        "/auth/login",
        Some(json!({"login":"bob","password":"Sup3r-Ozone-Pw"})),
        None,
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "compte suspendu : connexion refusée"
    );

    let (s, _) = send(
        &app,
        "PATCH",
        &format!("/instance/admin/users/{bob_id}"),
        Some(json!({"suspended":false})),
        Some(&atok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "réactivation");
    let (s, _) = send(
        &app,
        "POST",
        "/auth/login",
        Some(json!({"login":"bob","password":"Sup3r-Ozone-Pw"})),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "connexion de nouveau possible");
}

#[tokio::test]
async fn role_promotion_grants_admin() {
    let app = app_with("open").await;
    let atok = token(&app, "alice", "a@ia3.fr").await;
    let btok = token(&app, "bob", "b@ia3.fr").await;
    let bob_id = uid(&app, &btok).await;

    // Bob (simple) ne peut pas voir la config admin.
    let (s, _) = send(&app, "GET", "/instance/admin/config", None, Some(&btok)).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "non-admin refusé");

    // Le propriétaire promeut bob admin.
    let (s, _) = send(
        &app,
        "PUT",
        &format!("/instance/admin/users/{bob_id}/role"),
        Some(json!({"role":"admin"})),
        Some(&atok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "promotion admin");

    // Bob (désormais admin) accède à la config.
    let (s, _) = send(&app, "GET", "/instance/admin/config", None, Some(&btok)).await;
    assert_eq!(s, StatusCode::OK, "admin autorisé");
}
