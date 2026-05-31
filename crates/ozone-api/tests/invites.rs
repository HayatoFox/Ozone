//! Tests fonctionnels + cyber : aperçu (sans rejoindre) et révocation d'invitations de guilde.

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
    let path = std::env::temp_dir().join(format!("ozone-test-inv-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Inv".into(),
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
async fn preview_does_not_join() {
    let app = app().await;
    let owner = token(&app, "alice", "a@inv.fr").await;
    let g = send(
        &app,
        "POST",
        "/guilds",
        Some(json!({"name":"Cool"})),
        Some(&owner),
    )
    .await
    .1;
    let gid = g["id"].as_str().unwrap().to_string();
    let inv = send(
        &app,
        "POST",
        &format!("/guilds/{gid}/invites"),
        Some(json!({})),
        Some(&owner),
    )
    .await
    .1;
    let code = inv["code"].as_str().unwrap().to_string();

    // bob prévisualise sans rejoindre.
    let bob = token(&app, "bob", "b@inv.fr").await;
    let (s, prev) = send(&app, "GET", &format!("/invites/{code}"), None, Some(&bob)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(prev["guild_name"], "Cool");
    assert_eq!(prev["member_count"], 1);

    // bob n'est toujours pas membre.
    let (s, _) = send(&app, "GET", &format!("/guilds/{gid}"), None, Some(&bob)).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "l'aperçu ne fait pas rejoindre");
}

#[tokio::test]
async fn revoke_authorization() {
    let app = app().await;
    let owner = token(&app, "alice", "a@inv2.fr").await;
    let g = send(
        &app,
        "POST",
        "/guilds",
        Some(json!({"name":"G"})),
        Some(&owner),
    )
    .await
    .1;
    let gid = g["id"].as_str().unwrap().to_string();
    let inv = send(
        &app,
        "POST",
        &format!("/guilds/{gid}/invites"),
        Some(json!({})),
        Some(&owner),
    )
    .await
    .1;
    let code = inv["code"].as_str().unwrap().to_string();

    // bob rejoint puis crée SA propre invitation (CREATE_INSTANT_INVITE par défaut).
    let bob = token(&app, "bob", "b@inv2.fr").await;
    send(&app, "POST", &format!("/invites/{code}"), None, Some(&bob)).await;
    let inv2 = send(
        &app,
        "POST",
        &format!("/guilds/{gid}/invites"),
        Some(json!({})),
        Some(&bob),
    )
    .await
    .1;
    let code2 = inv2["code"].as_str().unwrap().to_string();

    // bob ne peut pas révoquer l'invitation d'autrui (pas MANAGE_GUILD).
    let (s, _) = send(
        &app,
        "DELETE",
        &format!("/invites/{code}"),
        None,
        Some(&bob),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "révocation d'autrui sans MANAGE_GUILD"
    );

    // …mais peut révoquer la sienne.
    let (s, _) = send(
        &app,
        "DELETE",
        &format!("/invites/{code2}"),
        None,
        Some(&bob),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "le créateur révoque la sienne");

    // Le propriétaire révoque n'importe laquelle ; ensuite elle est inutilisable.
    let (s, _) = send(
        &app,
        "DELETE",
        &format!("/invites/{code}"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = send(&app, "GET", &format!("/invites/{code}"), None, Some(&owner)).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "invitation révoquée");
    let carol = token(&app, "carol", "c@inv2.fr").await;
    let (s, _) = send(
        &app,
        "POST",
        &format!("/invites/{code}"),
        None,
        Some(&carol),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::NOT_FOUND,
        "impossible de rejoindre via une invitation révoquée"
    );
}
