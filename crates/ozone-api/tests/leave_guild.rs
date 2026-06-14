//! Tests : un membre quitte une guilde ; le propriétaire ne le peut pas.

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
    let path = std::env::temp_dir().join(format!("ozone-test-leave-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Leave".into(),
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
async fn member_can_leave_owner_cannot() {
    let app = app().await;
    let owner = token(&app, "alice", "a@lv.fr").await;
    let bob = token(&app, "bob", "b@lv.fr").await;
    let carol = token(&app, "carol", "c@lv.fr").await; // non-membre
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
    send(&app, "POST", &format!("/invites/{code}"), None, Some(&bob)).await;

    // bob est membre → peut quitter.
    let (s, _) = send(&app, "GET", &format!("/guilds/{gid}"), None, Some(&bob)).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = send(
        &app,
        "DELETE",
        &format!("/guilds/{gid}/members/@me"),
        None,
        Some(&bob),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "un membre peut quitter");
    let (s, _) = send(&app, "GET", &format!("/guilds/{gid}"), None, Some(&bob)).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "bob n'est plus membre");

    // Le propriétaire ne peut pas quitter.
    let (s, _) = send(
        &app,
        "DELETE",
        &format!("/guilds/{gid}/members/@me"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "le propriétaire ne quitte pas");

    // Un non-membre n'a rien à quitter.
    let (s, _) = send(
        &app,
        "DELETE",
        &format!("/guilds/{gid}/members/@me"),
        None,
        Some(&carol),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "non-membre");
}
