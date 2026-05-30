//! Test d'intégration des salons avancés : catégories, parent, get/update/delete,
//! réordonnancement, NSFW/topic, slowmode.

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
    let path = std::env::temp_dir().join(format!("ozone-test-chan-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Ozone Chan".into(),
        instance_description: None,
        registration_policy: "open".into(),
        instance_password: None,
        version: "0.1.0-test".into(),
    };
    build_app(bootstrap_state(&cfg).await.expect("bootstrap"))
}

fn rq(method: &str, uri: &str, body: Option<Value>, token: &str) -> Request<Body> {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"));
    let body = match body {
        Some(v) => {
            b = b.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    b.body(body).unwrap()
}

async fn send(app: &Router, r: Request<Body>) -> (StatusCode, Value) {
    let resp = app.clone().oneshot(r).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn register(app: &Router, u: &str, e: &str) -> String {
    let (_, v) = send(
        app,
        rq(
            "POST",
            "/auth/register",
            Some(json!({"username":u,"email":e,"password":"motdepasse"})),
            "",
        ),
    )
    .await;
    v["access_token"].as_str().unwrap().to_string()
}

async fn make_guild(app: &Router, tok: &str) -> String {
    let (_, g) = send(app, rq("POST", "/guilds", Some(json!({"name":"G"})), tok)).await;
    g["id"].as_str().unwrap().to_string()
}

async fn join(app: &Router, owner: &str, gid: &str, u: &str, e: &str) -> String {
    let (_, inv) = send(
        app,
        rq(
            "POST",
            &format!("/guilds/{gid}/invites"),
            Some(json!({})),
            owner,
        ),
    )
    .await;
    let code = inv["code"].as_str().unwrap().to_string();
    let tok = register(app, u, e).await;
    send(app, rq("POST", &format!("/invites/{code}"), None, &tok)).await;
    tok
}

#[tokio::test]
async fn channel_management() {
    let app = app().await;
    let alice = register(&app, "alice", "a@x.fr").await;
    let gid = make_guild(&app, &alice).await;

    // catégorie
    let (s, cat) = send(
        &app,
        rq(
            "POST",
            &format!("/guilds/{gid}/channels"),
            Some(json!({"name":"Cat","type":4})),
            &alice,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(cat["type"], 4);
    let cat_id = cat["id"].as_str().unwrap().to_string();

    // salon sous la catégorie, avec topic/nsfw/slowmode
    let (s, ch) = send(
        &app,
        rq("POST", &format!("/guilds/{gid}/channels"), Some(json!({"name":"salon","type":0,"parent_id":cat_id,"topic":"sujet","nsfw":true,"rate_limit_per_user":5})), &alice),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(ch["parent_id"], cat_id);
    assert_eq!(ch["nsfw"], true);
    assert_eq!(ch["rate_limit_per_user"], 5);
    assert_eq!(ch["topic"], "sujet");
    let ch_id = ch["id"].as_str().unwrap().to_string();

    // get
    let (s, got) = send(&app, rq("GET", &format!("/channels/{ch_id}"), None, &alice)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(got["name"], "salon");

    // update
    let (s, upd) = send(
        &app,
        rq(
            "PATCH",
            &format!("/channels/{ch_id}"),
            Some(json!({"topic":"nouveau","nsfw":false,"rate_limit_per_user":0})),
            &alice,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(upd["topic"], "nouveau");
    assert_eq!(upd["nsfw"], false);
    assert_eq!(upd["rate_limit_per_user"], 0);

    // réordonnancement
    let (s, _) = send(
        &app,
        rq(
            "PATCH",
            &format!("/guilds/{gid}/channels"),
            Some(json!([{"id":ch_id,"position":7}])),
            &alice,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (_, got2) = send(&app, rq("GET", &format!("/channels/{ch_id}"), None, &alice)).await;
    assert_eq!(got2["position"], 7);

    // suppression de catégorie → l'enfant est détaché (parent NULL), pas supprimé
    let (_, child) = send(
        &app,
        rq(
            "POST",
            &format!("/guilds/{gid}/channels"),
            Some(json!({"name":"enfant","type":0,"parent_id":cat_id})),
            &alice,
        ),
    )
    .await;
    let child_id = child["id"].as_str().unwrap().to_string();
    let (s, _) = send(
        &app,
        rq("DELETE", &format!("/channels/{cat_id}"), None, &alice),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, orphan) = send(
        &app,
        rq("GET", &format!("/channels/{child_id}"), None, &alice),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(orphan["parent_id"].is_null(), "l'enfant doit être détaché");

    // suppression du salon
    let (s, _) = send(
        &app,
        rq("DELETE", &format!("/channels/{ch_id}"), None, &alice),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = send(&app, rq("GET", &format!("/channels/{ch_id}"), None, &alice)).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn slowmode_blocks_rapid_members_but_not_owner() {
    let app = app().await;
    let alice = register(&app, "alice", "a@x.fr").await;
    let gid = make_guild(&app, &alice).await;
    let (_, ch) = send(
        &app,
        rq(
            "POST",
            &format!("/guilds/{gid}/channels"),
            Some(json!({"name":"lent","type":0,"rate_limit_per_user":10})),
            &alice,
        ),
    )
    .await;
    let cid = ch["id"].as_str().unwrap().to_string();

    let bob = join(&app, &alice, &gid, "bob", "b@x.fr").await;

    let (s1, _) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({"content":"1"})),
            &bob,
        ),
    )
    .await;
    assert_eq!(s1, StatusCode::OK);
    let (s2, _) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({"content":"2"})),
            &bob,
        ),
    )
    .await;
    assert_eq!(
        s2,
        StatusCode::TOO_MANY_REQUESTS,
        "le slowmode doit bloquer le membre"
    );

    // le propriétaire n'est pas soumis au slowmode
    let (a1, _) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({"content":"x"})),
            &alice,
        ),
    )
    .await;
    let (a2, _) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({"content":"y"})),
            &alice,
        ),
    )
    .await;
    assert_eq!(a1, StatusCode::OK);
    assert_eq!(a2, StatusCode::OK, "le propriétaire contourne le slowmode");
}
