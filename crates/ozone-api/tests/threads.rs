//! Tests fonctionnels + cyber : fils (threads). Création, messages, liste, et surtout
//! **héritage des permissions du salon parent** (un fil sous un salon privé reste privé).

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
    let path = std::env::temp_dir().join(format!("ozone-test-threads-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Threads".into(),
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
async fn setup(app: &Router, owner: &str) -> (String, String) {
    let g = send(
        app,
        "POST",
        "/guilds",
        Some(json!({"name":"G"})),
        Some(owner),
    )
    .await
    .1;
    let gid = g["id"].as_str().unwrap().to_string();
    let cid = send(
        app,
        "GET",
        &format!("/guilds/{gid}/channels"),
        None,
        Some(owner),
    )
    .await
    .1[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    (gid, cid)
}
async fn join(app: &Router, owner: &str, gid: &str, member: &str) {
    let inv = send(
        app,
        "POST",
        &format!("/guilds/{gid}/invites"),
        Some(json!({})),
        Some(owner),
    )
    .await
    .1;
    let code = inv["code"].as_str().unwrap().to_string();
    send(app, "POST", &format!("/invites/{code}"), None, Some(member)).await;
}

#[tokio::test]
async fn create_post_list_thread() {
    let app = app().await;
    let owner = token(&app, "alice", "a@th.fr").await;
    let (gid, cid) = setup(&app, &owner).await;

    let (s, th) = send(
        &app,
        "POST",
        &format!("/channels/{cid}/threads"),
        Some(json!({"name":"Discussion"})),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(th["type"], 11);
    assert_eq!(th["parent_id"], cid);
    let tid = th["id"].as_str().unwrap().to_string();

    // Poster dans le fil (un fil est un salon).
    let (s, _) = send(
        &app,
        "POST",
        &format!("/channels/{tid}/messages"),
        Some(json!({"content":"coucou le fil"})),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (_s, msgs) = send(
        &app,
        "GET",
        &format!("/channels/{tid}/messages"),
        None,
        Some(&owner),
    )
    .await;
    assert!(msgs
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["content"] == "coucou le fil"));

    // Le fil est listé.
    let (_s, list) = send(
        &app,
        "GET",
        &format!("/channels/{cid}/threads"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], tid);

    // Pas de fil sous un salon vocal.
    let vc = send(
        &app,
        "POST",
        &format!("/guilds/{gid}/channels"),
        Some(json!({"name":"Vocal","type":2})),
        Some(&owner),
    )
    .await
    .1["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (s, _) = send(
        &app,
        "POST",
        &format!("/channels/{vc}/threads"),
        Some(json!({"name":"x"})),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn thread_inherits_parent_permissions() {
    let app = app().await;
    let owner = token(&app, "alice", "a@th2.fr").await;
    let (gid, general) = setup(&app, &owner).await;
    let bob = token(&app, "bob", "b@th2.fr").await;
    join(&app, &owner, &gid, &bob).await;

    // Fil public sous #général : bob (membre) peut poster.
    let pub_thread = send(
        &app,
        "POST",
        &format!("/channels/{general}/threads"),
        Some(json!({"name":"public"})),
        Some(&owner),
    )
    .await
    .1["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (s, _) = send(
        &app,
        "POST",
        &format!("/channels/{pub_thread}/messages"),
        Some(json!({"content":"salut"})),
        Some(&bob),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "fil public accessible aux membres");

    // Salon privé : on retire VIEW à @everyone (bit 1<<10 = 1024).
    let secret = send(
        &app,
        "POST",
        &format!("/guilds/{gid}/channels"),
        Some(json!({"name":"secret","type":0})),
        Some(&owner),
    )
    .await
    .1["id"]
        .as_str()
        .unwrap()
        .to_string();
    send(
        &app,
        "PUT",
        &format!("/channels/{secret}/permissions/{gid}"),
        Some(json!({"type":0,"deny":"1024"})),
        Some(&owner),
    )
    .await;

    // Fil sous le salon privé.
    let secret_thread = send(
        &app,
        "POST",
        &format!("/channels/{secret}/threads"),
        Some(json!({"name":"privé"})),
        Some(&owner),
    )
    .await
    .1["id"]
        .as_str()
        .unwrap()
        .to_string();

    // bob ne voit PAS le salon privé → ni son fil (héritage des permissions).
    let (s, _) = send(
        &app,
        "GET",
        &format!("/channels/{secret_thread}/messages"),
        None,
        Some(&bob),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "le fil hérite de la confidentialité du parent"
    );
    let (s, _) = send(
        &app,
        "POST",
        &format!("/channels/{secret_thread}/messages"),
        Some(json!({"content":"intrus"})),
        Some(&bob),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "pas d'écriture dans un fil privé hérité"
    );
    let (s, _) = send(
        &app,
        "GET",
        &format!("/channels/{secret}/threads"),
        None,
        Some(&bob),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "lister les fils d'un salon non visible"
    );

    // Le propriétaire, lui, accède au fil privé.
    let (s, _) = send(
        &app,
        "GET",
        &format!("/channels/{secret_thread}/messages"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
}
