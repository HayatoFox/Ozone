//! Tests fonctionnels : marqueurs de lecture (ack / read-states), compteur de mentions,
//! boîte de mentions, réglages de notification (niveau + mute).

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
    let path = std::env::temp_dir().join(format!("ozone-test-notif-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Notif".into(),
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
async fn uid(app: &Router, t: &str) -> String {
    send(app, "GET", "/users/@me", None, Some(t)).await.1["id"]
        .as_str()
        .unwrap()
        .to_string()
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

fn mention_count_for(read_states: &Value, channel_id: &str) -> i64 {
    read_states
        .as_array()
        .unwrap()
        .iter()
        .find(|rs| rs["channel_id"] == channel_id)
        .map(|rs| rs["mention_count"].as_i64().unwrap())
        .unwrap_or(0)
}

#[tokio::test]
async fn ack_sets_read_state() {
    let app = app().await;
    let owner = token(&app, "alice", "a@nf.fr").await;
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

    let msg = send(
        &app,
        "POST",
        &format!("/channels/{cid}/messages"),
        Some(json!({"content":"salut"})),
        Some(&owner),
    )
    .await
    .1;
    let mid = msg["id"].as_str().unwrap().to_string();

    // ack → état de lecture positionné.
    let (s, rs) = send(
        &app,
        "POST",
        &format!("/channels/{cid}/messages/{mid}/ack"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(rs["last_read_id"], mid);
    assert_eq!(rs["mention_count"], 0);

    // visible dans la synchro multi-appareils.
    let (_s, list) = send(&app, "GET", "/users/@me/read-states", None, Some(&owner)).await;
    assert_eq!(
        list.as_array()
            .unwrap()
            .iter()
            .find(|r| r["channel_id"] == cid)
            .unwrap()["last_read_id"],
        mid
    );
}

#[tokio::test]
async fn mention_increments_count_and_inbox() {
    let app = app().await;
    let owner = token(&app, "alice", "a@nf2.fr").await;
    let bob = token(&app, "bob", "b@nf2.fr").await;
    let bob_id = uid(&app, &bob).await;
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
    join(&app, &owner, &gid, &bob).await;

    // owner mentionne bob.
    let msg = send(
        &app,
        "POST",
        &format!("/channels/{cid}/messages"),
        Some(json!({"content":format!("<@{bob_id}> coucou")})),
        Some(&owner),
    )
    .await
    .1;
    let mid = msg["id"].as_str().unwrap().to_string();

    // compteur de mentions de bob = 1.
    let (_s, rs) = send(&app, "GET", "/users/@me/read-states", None, Some(&bob)).await;
    assert_eq!(mention_count_for(&rs, &cid), 1);

    // boîte de mentions de bob contient le message.
    let (s, inbox) = send(&app, "GET", "/users/@me/mentions", None, Some(&bob)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(inbox.as_array().unwrap().len(), 1);
    assert_eq!(inbox[0]["id"], mid);

    // après ack, le compteur retombe à 0.
    send(
        &app,
        "POST",
        &format!("/channels/{cid}/messages/{mid}/ack"),
        None,
        Some(&bob),
    )
    .await;
    let (_s, rs) = send(&app, "GET", "/users/@me/read-states", None, Some(&bob)).await;
    assert_eq!(mention_count_for(&rs, &cid), 0);
}

#[tokio::test]
async fn notification_settings_crud() {
    let app = app().await;
    let owner = token(&app, "alice", "a@nf3.fr").await;
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

    // Niveau serveur = @mentions seulement.
    let (s, st1) = send(
        &app,
        "PUT",
        &format!("/users/@me/notification-settings/guild/{gid}"),
        Some(json!({"level":1})),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(st1["level"], 1);
    assert!(st1["muted_until"].is_null());

    // Mute 1 h.
    let (_s, st2) = send(
        &app,
        "PUT",
        &format!("/users/@me/notification-settings/guild/{gid}"),
        Some(json!({"mute_seconds":3600})),
        Some(&owner),
    )
    .await;
    assert_eq!(st2["level"], 1, "le niveau est conservé");
    assert!(st2["muted_until"].as_i64().unwrap() > 0);

    // Réglage de salon.
    let (_s, st3) = send(
        &app,
        "PUT",
        &format!("/users/@me/notification-settings/channel/{cid}"),
        Some(json!({"level":2})),
        Some(&owner),
    )
    .await;
    assert_eq!(st3["level"], 2);

    // Démute.
    let (_s, st4) = send(
        &app,
        "PUT",
        &format!("/users/@me/notification-settings/guild/{gid}"),
        Some(json!({"mute_seconds":0})),
        Some(&owner),
    )
    .await;
    assert!(st4["muted_until"].is_null());

    // Liste = 2 réglages.
    let (_s, list) = send(
        &app,
        "GET",
        "/users/@me/notification-settings",
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(list.as_array().unwrap().len(), 2);
}
