//! Tests fonctionnels + cyber : suppression de compte (anonymisation). Ré-auth, conservation
//! des messages (« utilisateur supprimé »), retrait des adhésions, impossibilité si l'on possède
//! une guilde, connexion impossible ensuite.

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
    let path = std::env::temp_dir().join(format!("ozone-test-acctdel-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "AcctDel".into(),
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

#[tokio::test]
async fn delete_account_anonymizes_and_keeps_messages() {
    let app = app().await;
    // bob = 1er compte (propriétaire d'instance), crée une guilde.
    let bob = token(&app, "bob", "b@ad.fr").await;
    let g = send(
        &app,
        "POST",
        "/guilds",
        Some(json!({"name":"G"})),
        Some(&bob),
    )
    .await
    .1;
    let gid = g["id"].as_str().unwrap().to_string();
    let cid = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/channels"),
        None,
        Some(&bob),
    )
    .await
    .1[0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // alice rejoint et poste un message.
    let alice = token(&app, "alice", "a@ad.fr").await;
    let inv = send(
        &app,
        "POST",
        &format!("/guilds/{gid}/invites"),
        Some(json!({})),
        Some(&bob),
    )
    .await
    .1;
    let code = inv["code"].as_str().unwrap().to_string();
    send(
        &app,
        "POST",
        &format!("/invites/{code}"),
        None,
        Some(&alice),
    )
    .await;
    send(
        &app,
        "POST",
        &format!("/channels/{cid}/messages"),
        Some(json!({"content":"salut tout le monde"})),
        Some(&alice),
    )
    .await;

    // Mauvais mot de passe → refusé.
    let (s, _) = send(
        &app,
        "DELETE",
        "/users/@me",
        Some(json!({"password":"faux"})),
        Some(&alice),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // Suppression effective.
    let (s, _) = send(
        &app,
        "DELETE",
        "/users/@me",
        Some(json!({"password":"motdepasse"})),
        Some(&alice),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // alice ne peut plus se connecter.
    let (s, _) = send(
        &app,
        "POST",
        "/auth/login",
        Some(json!({"login":"alice","password":"motdepasse"})),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // Le message subsiste, attribué à un « utilisateur supprimé », et alice n'est plus membre.
    let (_s, hist) = send(
        &app,
        "GET",
        &format!("/channels/{cid}/messages"),
        None,
        Some(&bob),
    )
    .await;
    let m = hist
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["content"] == "salut tout le monde")
        .expect("message conservé");
    assert!(
        m["author"]["username"]
            .as_str()
            .unwrap()
            .starts_with("deleted_"),
        "auteur anonymisé"
    );
    let (_s, members) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/members"),
        None,
        Some(&bob),
    )
    .await;
    assert!(!members
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x["user"]["username"] == "alice"));
}

#[tokio::test]
async fn cannot_delete_while_owning_guild() {
    let app = app().await;
    let owner = token(&app, "alice", "a@ad2.fr").await;
    send(
        &app,
        "POST",
        "/guilds",
        Some(json!({"name":"G"})),
        Some(&owner),
    )
    .await;
    let (s, _) = send(
        &app,
        "DELETE",
        "/users/@me",
        Some(json!({"password":"motdepasse"})),
        Some(&owner),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::BAD_REQUEST,
        "il faut d'abord supprimer/transférer ses guildes"
    );
}
