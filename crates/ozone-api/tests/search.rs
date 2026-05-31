//! Tests fonctionnels de la recherche de messages (FTS5) : texte, filtres `author`,
//! `has=link`, `pinned`, et recherche par salon.

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
    let path = std::env::temp_dir().join(format!("ozone-test-search-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Search".into(),
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
async fn uid(app: &Router, t: &str) -> String {
    send(app, "GET", "/users/@me", None, Some(t)).await.1["id"]
        .as_str()
        .unwrap()
        .to_string()
}
async fn post_msg(app: &Router, cid: &str, content: &str, tok: &str) -> String {
    send(
        app,
        "POST",
        &format!("/channels/{cid}/messages"),
        Some(json!({"content":content})),
        Some(tok),
    )
    .await
    .1["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn search_text_and_filters() {
    let app = app().await;
    let owner = token(&app, "alice", "a@se.fr").await;
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
    let chans = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/channels"),
        None,
        Some(&owner),
    )
    .await
    .1;
    let cid = chans[0]["id"].as_str().unwrap().to_string();

    post_msg(&app, &cid, "alpha banana split", &owner).await;
    let apple_id = post_msg(&app, &cid, "beta apple pie", &owner).await;
    post_msg(&app, &cid, "gamma cherry http://link.example/x", &owner).await;

    // Texte
    let (s, r) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/messages/search?q=banana"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "corps={r}");
    assert_eq!(r["total"], 1);
    assert_eq!(r["messages"][0]["content"], "alpha banana split");

    let (_s, r) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/messages/search?q=apple"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(r["total"], 1);

    // has=link
    let (_s, r) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/messages/search?has=link"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(r["total"], 1);
    assert!(r["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains("http"));

    // pinned=true
    send(
        &app,
        "PUT",
        &format!("/channels/{cid}/pins/{apple_id}"),
        None,
        Some(&owner),
    )
    .await;
    let (_s, r) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/messages/search?pinned=true"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(r["total"], 1);
    assert_eq!(r["messages"][0]["id"], apple_id);

    // Filtre auteur : bob rejoint et poste.
    let bob = token(&app, "bob", "b@se.fr").await;
    let bob_id = uid(&app, &bob).await;
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
    post_msg(&app, &cid, "delta banana bread", &bob).await;

    let (_s, r) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/messages/search?q=banana&author_id={bob_id}"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(r["total"], 1, "filtre auteur");
    assert_eq!(r["messages"][0]["content"], "delta banana bread");

    // Recherche par salon.
    let (s, r) = send(
        &app,
        "GET",
        &format!("/channels/{cid}/messages/search?q=banana"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(r["total"], 2, "deux messages contiennent banana");
}
