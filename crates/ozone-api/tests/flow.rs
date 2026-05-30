//! Test d'intégration du parcours complet : instance → inscription → connexion →
//! guilde → salon → message. Utilise `tower::ServiceExt::oneshot` (pas de socket réseau).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use ozone_api::config::Config;
use ozone_api::{bootstrap_state, build_app};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn test_state() -> ozone_api::state::AppState {
    let unique = format!(
        "{}_{:?}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        std::thread::current().id()
    );
    let path = std::env::temp_dir().join(format!("ozone-test-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Ozone Test".into(),
        instance_description: Some("instance de test".into()),
        registration_policy: "open".into(),
        instance_password: None,
        version: "0.1.0-test".into(),
    };
    bootstrap_state(&cfg).await.expect("bootstrap")
}

async fn body_json(resp: axum::response::Response) -> Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

#[tokio::test]
async fn full_flow() {
    let state = test_state().await;
    let app = build_app(state);

    // GET /instance — métadonnées publiques
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/instance")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let info = body_json(resp).await;
    assert_eq!(info["name"], "Ozone Test");
    assert_eq!(info["registration_policy"], "open");
    assert_eq!(info["access_gate"]["required"], false);

    // POST /auth/register — premier compte = propriétaire
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"username":"Alice","email":"alice@example.com","password":"motdepasse"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let tokens = body_json(resp).await;
    let access = tokens["access_token"].as_str().unwrap().to_string();
    assert!(!access.is_empty());

    // GET /users/@me
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/users/@me")
                .header("authorization", format!("Bearer {access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let me = body_json(resp).await;
    assert_eq!(me["username"], "alice");
    assert_eq!(me["email"], "alice@example.com");

    // POST /guilds
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/guilds")
                .header("authorization", format!("Bearer {access}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({"name":"Ma Guilde"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let guild = body_json(resp).await;
    let gid = guild["id"].as_str().unwrap().to_string();

    // GET /guilds/:id/channels — doit contenir « général »
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/guilds/{gid}/channels"))
                .header("authorization", format!("Bearer {access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let channels = body_json(resp).await;
    let cid = channels[0]["id"].as_str().unwrap().to_string();
    assert_eq!(channels[0]["name"], "général");

    // POST /channels/:id/messages
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/channels/{cid}/messages"))
                .header("authorization", format!("Bearer {access}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"content":"Bonjour Ozone !","nonce":"n1"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let msg = body_json(resp).await;
    assert_eq!(msg["content"], "Bonjour Ozone !");
    assert_eq!(msg["author"]["username"], "alice");

    // GET /channels/:id/messages
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/channels/{cid}/messages"))
                .header("authorization", format!("Bearer {access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let msgs = body_json(resp).await;
    assert_eq!(msgs.as_array().unwrap().len(), 1);
    assert_eq!(msgs[0]["content"], "Bonjour Ozone !");
}

#[tokio::test]
async fn unauthorized_without_token() {
    let state = test_state().await;
    let app = build_app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/guilds")
                .header("content-type", "application/json")
                .body(Body::from(json!({"name":"X"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ─────────── Instance protégée par mot de passe (gate) ───────────

async fn gated_state(password: &str) -> ozone_api::state::AppState {
    let unique = format!(
        "{}_{:?}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        std::thread::current().id()
    );
    let path = std::env::temp_dir().join(format!("ozone-test-gated-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Ozone Gated".into(),
        instance_description: None,
        registration_policy: "open".into(),
        instance_password: Some(password.into()),
        version: "0.1.0-test".into(),
    };
    bootstrap_state(&cfg).await.expect("bootstrap")
}

fn post_req(uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn gated_instance_requires_password() {
    let app = build_app(gated_state("secret123").await);

    // /instance annonce que le gate est requis
    let info = body_json(
        app.clone()
            .oneshot(
                Request::builder()
                    .uri("/instance")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(info["access_gate"]["required"], true);

    // inscription sans jeton de gate → refusée
    let resp = app
        .clone()
        .oneshot(post_req(
            "/auth/register",
            json!({"username":"eve","email":"eve@x.fr","password":"motdepasse"}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // gate avec mauvais mot de passe → refusé
    let resp = app
        .clone()
        .oneshot(post_req("/instance/gate", json!({"password":"mauvais"})))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // gate avec bon mot de passe → jeton de gate
    let resp = app
        .clone()
        .oneshot(post_req("/instance/gate", json!({"password":"secret123"})))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let gate = body_json(resp).await;
    let gt = gate["gate_token"].as_str().unwrap().to_string();

    // inscription avec jeton de gate → acceptée (et 1er compte = propriétaire)
    let resp = app
        .clone()
        .oneshot(post_req(
            "/auth/register",
            json!({"username":"Bob","email":"bob@x.fr","password":"motdepasse","gate_token": gt}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
