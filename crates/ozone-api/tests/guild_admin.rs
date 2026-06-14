//! Tests fonctionnels : récupération, renommage et suppression (cascade) d'une guilde.

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
    let path = std::env::temp_dir().join(format!("ozone-test-gadm-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "GAdm".into(),
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

#[tokio::test]
async fn get_update_delete_guild() {
    let app = app().await;
    let owner = token(&app, "alice", "a@ga.fr").await;
    let g = send(
        &app,
        "POST",
        "/guilds",
        Some(json!({"name":"Ma Guilde"})),
        Some(&owner),
    )
    .await
    .1;
    let gid = g["id"].as_str().unwrap().to_string();

    // GET
    let (s, got) = send(&app, "GET", &format!("/guilds/{gid}"), None, Some(&owner)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(got["name"], "Ma Guilde");

    // PATCH (renommer + icône)
    let (s, up) = send(
        &app,
        "PATCH",
        &format!("/guilds/{gid}"),
        Some(json!({"name":"Renommée","icon_id":"ic1"})),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(up["name"], "Renommée");
    assert_eq!(up["icon_id"], "ic1");

    // DELETE
    let (s, _) = send(
        &app,
        "DELETE",
        &format!("/guilds/{gid}"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    // La guilde n'existe plus.
    let (s, _) = send(&app, "GET", &format!("/guilds/{gid}"), None, Some(&owner)).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (_s, list) = send(&app, "GET", "/guilds", None, Some(&owner)).await;
    assert_eq!(list.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn delete_cascades_content() {
    let app = app().await;
    let owner = token(&app, "alice", "a@ga2.fr").await;
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
    let cid = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/channels"),
        None,
        Some(&owner),
    )
    .await
    .1[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    // Du contenu : message, rôle, invitation.
    send(
        &app,
        "POST",
        &format!("/channels/{cid}/messages"),
        Some(json!({"content":"hello"})),
        Some(&owner),
    )
    .await;
    send(
        &app,
        "POST",
        &format!("/guilds/{gid}/roles"),
        Some(json!({"name":"VIP"})),
        Some(&owner),
    )
    .await;
    send(
        &app,
        "POST",
        &format!("/guilds/{gid}/invites"),
        Some(json!({})),
        Some(&owner),
    )
    .await;

    // Suppression de la guilde.
    let (s, _) = send(
        &app,
        "DELETE",
        &format!("/guilds/{gid}"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // Les salons et messages ne sont plus accessibles (guilde et salon supprimés).
    let (s, _) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/channels"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "guilde supprimée");
    let (s, _) = send(
        &app,
        "GET",
        &format!("/channels/{cid}/messages"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "salon supprimé en cascade");
}
