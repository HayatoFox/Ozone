//! Tests fonctionnels des webhooks entrants : création, exécution (avec/sans `wait`),
//! régénération du jeton, mise à jour, suppression.

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
    let path = std::env::temp_dir().join(format!("ozone-test-wh-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "WH".into(),
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

async fn setup_guild(app: &Router, tok: &str) -> (String, String) {
    let g = send(app, "POST", "/guilds", Some(json!({"name":"G"})), Some(tok))
        .await
        .1;
    let gid = g["id"].as_str().unwrap().to_string();
    let chans = send(
        app,
        "GET",
        &format!("/guilds/{gid}/channels"),
        None,
        Some(tok),
    )
    .await
    .1;
    let cid = chans[0]["id"].as_str().unwrap().to_string();
    (gid, cid)
}

#[tokio::test]
async fn webhook_full_lifecycle() {
    let app = app().await;
    let owner = token(&app, "alice", "a@wh.fr").await;
    let (_gid, cid) = setup_guild(&app, &owner).await;

    // Création (le propriétaire a MANAGE_WEBHOOKS).
    let (s, wh) = send(
        &app,
        "POST",
        &format!("/channels/{cid}/webhooks"),
        Some(json!({"name":"Robot"})),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let wid = wh["id"].as_str().unwrap().to_string();
    let tok = wh["token"].as_str().unwrap().to_string();
    assert!(!tok.is_empty(), "le jeton est renvoyé à la création");

    // Liste : présente, jeton masqué.
    let (s, list) = send(
        &app,
        "GET",
        &format!("/channels/{cid}/webhooks"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert!(list[0]["token"].is_null(), "le jeton n'est jamais listé");

    // Exécution avec wait=true → renvoie le message créé, avec le nom de remplacement.
    let (s, msg) = send(
        &app,
        "POST",
        &format!("/webhooks/{wid}/{tok}?wait=true"),
        Some(json!({"content":"ping","username":"CustomBot"})),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(msg["content"], "ping");
    assert_eq!(msg["author"]["display_name"], "CustomBot");
    assert_eq!(msg["webhook_id"], wid);

    // Le message apparaît bien dans l'historique du salon (jointure auteur intacte).
    let (_s, hist) = send(
        &app,
        "GET",
        &format!("/channels/{cid}/messages"),
        None,
        Some(&owner),
    )
    .await;
    assert!(hist
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["content"] == "ping"));

    // Exécution sans wait → simple accusé.
    let (s, ack) = send(
        &app,
        "POST",
        &format!("/webhooks/{wid}/{tok}"),
        Some(json!({"content":"pong"})),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(ack["ok"], true);

    // Régénération du jeton : l'ancien ne fonctionne plus, le nouveau oui.
    let (s, re) = send(
        &app,
        "POST",
        &format!("/webhooks/{wid}"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let newtok = re["token"].as_str().unwrap().to_string();
    assert_ne!(newtok, tok);
    let (s, _) = send(
        &app,
        "POST",
        &format!("/webhooks/{wid}/{tok}"),
        Some(json!({"content":"x"})),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "ancien jeton invalidé");
    let (s, _) = send(
        &app,
        "POST",
        &format!("/webhooks/{wid}/{newtok}"),
        Some(json!({"content":"x"})),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "nouveau jeton valide");

    // Mise à jour du nom.
    let (s, up) = send(
        &app,
        "PATCH",
        &format!("/webhooks/{wid}"),
        Some(json!({"name":"Robot2"})),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(up["name"], "Robot2");

    // Suppression : exécution ensuite refusée.
    let (s, _) = send(
        &app,
        "DELETE",
        &format!("/webhooks/{wid}"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = send(
        &app,
        "POST",
        &format!("/webhooks/{wid}/{newtok}"),
        Some(json!({"content":"x"})),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "webhook supprimé");
}

#[tokio::test]
async fn webhook_validates_content() {
    let app = app().await;
    let owner = token(&app, "alice", "a@wh2.fr").await;
    let (_gid, cid) = setup_guild(&app, &owner).await;
    let (_s, wh) = send(
        &app,
        "POST",
        &format!("/channels/{cid}/webhooks"),
        Some(json!({"name":"R"})),
        Some(&owner),
    )
    .await;
    let wid = wh["id"].as_str().unwrap().to_string();
    let tok = wh["token"].as_str().unwrap().to_string();

    // Contenu vide → 400.
    let (s, _) = send(
        &app,
        "POST",
        &format!("/webhooks/{wid}/{tok}"),
        Some(json!({"content":"   "})),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // Contenu trop long → 400.
    let big = "a".repeat(4001);
    let (s, _) = send(
        &app,
        "POST",
        &format!("/webhooks/{wid}/{tok}"),
        Some(json!({"content":big})),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}
