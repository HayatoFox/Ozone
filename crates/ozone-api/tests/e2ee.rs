//! Tests d'intégration : chiffrement de bout en bout des MP (clés publiques + blob `cipher`).
//! Le serveur ne doit JAMAIS voir le texte clair : il stocke/restitue un blob opaque et le `content`
//! reste vide. Le chiffrement n'est autorisé qu'en MP (pas de guilde).

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
    let path = std::env::temp_dir().join(format!("ozone-test-e2ee-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Ozone E2EE Tests".into(),
        instance_description: None,
        registration_policy: "open".into(),
        instance_password: None,
        version: "0.1.0-test".into(),
    };
    build_app(bootstrap_state(&cfg).await.expect("bootstrap"))
}

fn rq(method: &str, uri: &str, body: Option<Value>, token: Option<&str>) -> Request<Body> {
    let mut b = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    let body = match body {
        Some(v) => {
            b = b.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    b.body(body).unwrap()
}

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

async fn register(app: &Router, username: &str, email: &str) -> String {
    let (status, body) = send(
        app,
        rq(
            "POST",
            "/auth/register",
            Some(json!({"username": username, "email": email, "password": "Sup3r-Ozone-Pw"})),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "register {username} a échoué");
    body["access_token"].as_str().unwrap().to_string()
}

async fn uid(app: &Router, token: &str) -> String {
    let (status, body) = send(app, rq("GET", "/users/@me", None, Some(token))).await;
    assert_eq!(status, StatusCode::OK);
    body["id"].as_str().unwrap().to_string()
}

async fn open_dm(app: &Router, token: &str, other_id: &str) -> String {
    let (status, ch) = send(
        app,
        rq(
            "POST",
            "/users/@me/channels",
            Some(json!({ "recipients": [other_id] })),
            Some(token),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "ouverture du MP a échoué");
    ch["id"].as_str().unwrap().to_string()
}

/// 1. Publication et lecture d'une clé publique ; validation des bornes.
#[tokio::test]
async fn publish_and_fetch_public_key() {
    let app = app().await;
    let alice = register(&app, "alice", "alice@e2ee.fr").await;
    let bob = register(&app, "bob", "bob@e2ee.fr").await;
    let bob_id = uid(&app, &bob).await;

    // Avant publication : la clé de Bob est absente (null), pas une 404.
    let (st, body) = send(&app, rq("GET", &format!("/users/{bob_id}/keys"), None, Some(&alice))).await;
    assert_eq!(st, StatusCode::OK);
    assert!(body["public_key"].is_null(), "clé absente attendue (null)");

    // Bob publie sa clé.
    let (st, body) = send(
        &app,
        rq("PUT", "/users/@me/keys", Some(json!({"public_key":"BBBpubkeyBob=="})), Some(&bob)),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body["public_key"].as_str(), Some("BBBpubkeyBob=="));

    // Alice relit la clé de Bob.
    let (st, body) = send(&app, rq("GET", &format!("/users/{bob_id}/keys"), None, Some(&alice))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body["public_key"].as_str(), Some("BBBpubkeyBob=="));

    // Clé vide rejetée.
    let (st, _) = send(
        &app,
        rq("PUT", "/users/@me/keys", Some(json!({"public_key":"   "})), Some(&bob)),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "clé vide doit être rejetée");

    // Clé surdimensionnée rejetée (> 1024).
    let huge = "A".repeat(2000);
    let (st, _) = send(
        &app,
        rq("PUT", "/users/@me/keys", Some(json!({ "public_key": huge })), Some(&bob)),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "clé trop longue doit être rejetée");

    // La lecture des clés exige une authentification.
    let (st, _) = send(&app, rq("GET", &format!("/users/{bob_id}/keys"), None, None)).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
}

/// 2. Aller-retour d'un message MP chiffré : le `content` reste vide, le `cipher` est restitué tel quel.
#[tokio::test]
async fn dm_cipher_roundtrip() {
    let app = app().await;
    let alice = register(&app, "alice", "alice@rt.fr").await;
    let bob = register(&app, "bob", "bob@rt.fr").await;
    let bob_id = uid(&app, &bob).await;
    let cid = open_dm(&app, &alice, &bob_id).await;

    let blob = "aXZ=|Y2lwaGVydGV4dA=="; // « iv|ciphertext » factice mais bien formé
    let (st, msg) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{cid}/messages"),
            // content non vide ET cipher : le serveur doit ignorer le clair et ne stocker que le blob.
            Some(json!({ "content": "ne doit pas fuir", "cipher": blob })),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "envoi du MP chiffré a échoué");
    assert_eq!(msg["cipher"].as_str(), Some(blob), "le cipher doit être restitué");
    assert_eq!(
        msg["content"].as_str().unwrap_or(""),
        "",
        "le content en clair ne doit JAMAIS être stocké/retourné pour un message chiffré"
    );

    // Bob relit : il reçoit le même blob, et toujours pas de clair.
    let (st, list) = send(
        &app,
        rq("GET", &format!("/channels/{cid}/messages"), None, Some(&bob)),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let m = &list.as_array().unwrap()[0];
    assert_eq!(m["cipher"].as_str(), Some(blob));
    assert_eq!(m["content"].as_str().unwrap_or(""), "");
}

/// 3. Le chiffrement est refusé hors MP (salon de guilde).
#[tokio::test]
async fn cipher_rejected_in_guild_channel() {
    let app = app().await;
    let alice = register(&app, "alice", "alice@g.fr").await;

    let (_, guild) = send(
        &app,
        rq("POST", "/guilds", Some(json!({"name":"G"})), Some(&alice)),
    )
    .await;
    let gid = guild["id"].as_str().unwrap().to_string();
    let (_, channels) = send(
        &app,
        rq("GET", &format!("/guilds/{gid}/channels"), None, Some(&alice)),
    )
    .await;
    let cid = channels[0]["id"].as_str().unwrap().to_string();

    let (st, _) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({ "content": "", "cipher": "aXY=|Yg==" })),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::BAD_REQUEST,
        "le chiffrement E2EE ne doit être autorisé qu'en MP"
    );
}

/// 4. Édition d'un MP chiffré : le nouveau blob remplace l'ancien, le clair reste vide.
#[tokio::test]
async fn edit_cipher_roundtrip() {
    let app = app().await;
    let alice = register(&app, "alice", "alice@ed.fr").await;
    let bob = register(&app, "bob", "bob@ed.fr").await;
    let bob_id = uid(&app, &bob).await;
    let cid = open_dm(&app, &alice, &bob_id).await;

    let (st, msg) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({ "content": "", "cipher": "aXY=|YQ==" })),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let mid = msg["id"].as_str().unwrap().to_string();

    let new_blob = "ZWZn=|Ymxhcg==";
    let (st, edited) = send(
        &app,
        rq(
            "PATCH",
            &format!("/channels/{cid}/messages/{mid}"),
            Some(json!({ "content": "encore en clair, à ignorer", "cipher": new_blob })),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "édition chiffrée a échoué");
    assert_eq!(edited["cipher"].as_str(), Some(new_blob), "le cipher doit être mis à jour");
    assert_eq!(edited["content"].as_str().unwrap_or(""), "", "pas de clair après édition chiffrée");
    assert!(edited["edited_at"].as_u64().is_some(), "edited_at doit être posé");
}
