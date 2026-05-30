//! Vérification cybersécurité de S3 (salons avancés) : autorisation des mutations,
//! isolation inter-guildes (parent/réordonnancement), non-contournement du slowmode.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use ozone_api::config::Config;
use ozone_api::{bootstrap_state, build_app};
use ozone_proto::perms;
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
    let path = std::env::temp_dir().join(format!("ozone-test-secs3-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Ozone SecS3".into(),
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

async fn uid(app: &Router, tok: &str) -> String {
    let (_, me) = send(app, rq("GET", "/users/@me", None, tok)).await;
    me["id"].as_str().unwrap().to_string()
}

async fn create_channel(app: &Router, tok: &str, gid: &str, body: Value) -> String {
    let (_, c) = send(
        app,
        rq("POST", &format!("/guilds/{gid}/channels"), Some(body), tok),
    )
    .await;
    c["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn member_without_manage_channels_cannot_modify() {
    let app = app().await;
    let alice = register(&app, "alice", "a@x.fr").await;
    let gid = make_guild(&app, &alice).await;
    let ch = create_channel(&app, &alice, &gid, json!({"name":"x"})).await;
    let bob = join(&app, &alice, &gid, "bob", "b@x.fr").await;

    let cases = [
        rq(
            "PATCH",
            &format!("/channels/{ch}"),
            Some(json!({"name":"y"})),
            &bob,
        ),
        rq("DELETE", &format!("/channels/{ch}"), None, &bob),
        rq(
            "POST",
            &format!("/guilds/{gid}/channels"),
            Some(json!({"name":"z"})),
            &bob,
        ),
        rq(
            "PATCH",
            &format!("/guilds/{gid}/channels"),
            Some(json!([{"id":ch,"position":1}])),
            &bob,
        ),
    ];
    for c in cases {
        let (s, _) = send(&app, c).await;
        assert_eq!(
            s,
            StatusCode::FORBIDDEN,
            "un membre sans MANAGE_CHANNELS ne doit pas modifier les salons"
        );
    }

    // non-membre : ne voit même pas le salon
    let mallory = register(&app, "mallory", "m@x.fr").await;
    let (s, _) = send(&app, rq("GET", &format!("/channels/{ch}"), None, &mallory)).await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "un non-membre ne doit pas lire le salon"
    );
}

#[tokio::test]
async fn cross_guild_parenting_and_reorder_blocked() {
    let app = app().await;
    let alice = register(&app, "alice", "a@x.fr").await;
    let g1 = make_guild(&app, &alice).await;
    let (_, g2v) = send(
        &app,
        rq("POST", "/guilds", Some(json!({"name":"G2"})), &alice),
    )
    .await;
    let g2 = g2v["id"].as_str().unwrap().to_string();

    let cat2 = create_channel(&app, &alice, &g2, json!({"name":"Cat2","type":4})).await;
    let ch2 = create_channel(&app, &alice, &g2, json!({"name":"ch2"})).await;

    // créer dans g1 un salon dont le parent est une catégorie de g2 → refusé
    let (s, _) = send(
        &app,
        rq(
            "POST",
            &format!("/guilds/{g1}/channels"),
            Some(json!({"name":"x","parent_id":cat2})),
            &alice,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "parent d'une autre guilde refusé");

    // déplacer un salon de g1 vers une catégorie de g2 → refusé
    let ch1 = create_channel(&app, &alice, &g1, json!({"name":"ch1"})).await;
    let (s, _) = send(
        &app,
        rq(
            "PATCH",
            &format!("/channels/{ch1}"),
            Some(json!({"parent_id":cat2})),
            &alice,
        ),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::NOT_FOUND,
        "déplacement vers une catégorie d'une autre guilde refusé"
    );

    // réordonner dans g1 en visant un salon de g2 → refusé
    let (s, _) = send(
        &app,
        rq(
            "PATCH",
            &format!("/guilds/{g1}/channels"),
            Some(json!([{"id":ch2,"position":1}])),
            &alice,
        ),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::NOT_FOUND,
        "réordonnancement d'un salon d'une autre guilde refusé"
    );
}

#[tokio::test]
async fn slowmode_gate_is_permission_based() {
    let app = app().await;
    let alice = register(&app, "alice", "a@x.fr").await;
    let gid = make_guild(&app, &alice).await;
    let cid = create_channel(
        &app,
        &alice,
        &gid,
        json!({"name":"lent","rate_limit_per_user":30}),
    )
    .await;

    // membre simple : bloqué au 2e message
    let carol = join(&app, &alice, &gid, "carol", "c@x.fr").await;
    send(
        &app,
        rq(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({"content":"1"})),
            &carol,
        ),
    )
    .await;
    let (s, _) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({"content":"2"})),
            &carol,
        ),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::TOO_MANY_REQUESTS,
        "le membre simple est soumis au slowmode"
    );

    // membre avec MANAGE_MESSAGES : contourne (le bypass est bien lié à la permission)
    let bob = join(&app, &alice, &gid, "bob", "b@x.fr").await;
    let bob_id = uid(&app, &bob).await;
    let (_, role) = send(
        &app,
        rq(
            "POST",
            &format!("/guilds/{gid}/roles"),
            Some(json!({"name":"mod","permissions":perms::MANAGE_MESSAGES.to_string()})),
            &alice,
        ),
    )
    .await;
    let rid = role["id"].as_str().unwrap().to_string();
    send(
        &app,
        rq(
            "PUT",
            &format!("/guilds/{gid}/members/{bob_id}/roles/{rid}"),
            None,
            &alice,
        ),
    )
    .await;

    send(
        &app,
        rq(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({"content":"a"})),
            &bob,
        ),
    )
    .await;
    let (s, _) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({"content":"b"})),
            &bob,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "MANAGE_MESSAGES contourne le slowmode");
}
