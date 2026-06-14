//! Vérification cybersécurité S8 : marqueurs de lecture et notifications.
//! Confidentialité : pas d'ack hors de portée, pas de mention fantôme, réglages gardés,
//! boîte de mentions filtrée dynamiquement par permission.

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
    let path = std::env::temp_dir().join(format!("ozone-test-secs8-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "SecS8".into(),
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
async fn general(app: &Router, gid: &str, tok: &str) -> String {
    send(
        app,
        "GET",
        &format!("/guilds/{gid}/channels"),
        None,
        Some(tok),
    )
    .await
    .1[0]["id"]
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

#[tokio::test]
async fn ack_requires_channel_view() {
    let app = app().await;
    let owner = token(&app, "alice", "a@s8.fr").await;
    let carol = token(&app, "carol", "c@s8.fr").await; // non-membre
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
    let cid = general(&app, &gid, &owner).await;

    let (s, _) = send(
        &app,
        "POST",
        &format!("/channels/{cid}/messages/1/ack"),
        None,
        Some(&carol),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "ack interdit sur un salon non visible"
    );
}

#[tokio::test]
async fn mention_to_unauthorized_user_is_ignored() {
    let app = app().await;
    let owner = token(&app, "alice", "a@s8b.fr").await;
    let carol = token(&app, "carol", "c@s8b.fr").await; // jamais membre
    let carol_id = uid(&app, &carol).await;
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
    let cid = general(&app, &gid, &owner).await;

    // owner mentionne un utilisateur qui ne peut pas voir le salon.
    send(
        &app,
        "POST",
        &format!("/channels/{cid}/messages"),
        Some(json!({"content":format!("<@{carol_id}> coucou")})),
        Some(&owner),
    )
    .await;

    // Aucune mention fantôme : ni compteur, ni boîte de réception.
    let (_s, rs) = send(&app, "GET", "/users/@me/read-states", None, Some(&carol)).await;
    assert_eq!(
        rs.as_array().unwrap().len(),
        0,
        "pas d'état de lecture créé"
    );
    let (_s, inbox) = send(&app, "GET", "/users/@me/mentions", None, Some(&carol)).await;
    assert_eq!(inbox.as_array().unwrap().len(), 0, "pas de mention fantôme");
}

#[tokio::test]
async fn notification_settings_authorization() {
    let app = app().await;
    let owner = token(&app, "alice", "a@s8c.fr").await;
    let carol = token(&app, "carol", "c@s8c.fr").await; // non-membre
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
    let cid = general(&app, &gid, &owner).await;

    let (s, _) = send(
        &app,
        "PUT",
        &format!("/users/@me/notification-settings/guild/{gid}"),
        Some(json!({"level":1})),
        Some(&carol),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "régler les notifs d'une guilde dont on n'est pas membre"
    );
    let (s, _) = send(
        &app,
        "PUT",
        &format!("/users/@me/notification-settings/channel/{cid}"),
        Some(json!({"level":1})),
        Some(&carol),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "régler les notifs d'un salon non visible"
    );
}

#[tokio::test]
async fn inbox_drops_channels_after_access_loss() {
    let app = app().await;
    let owner = token(&app, "alice", "a@s8d.fr").await;
    let bob = token(&app, "bob", "b@s8d.fr").await;
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
    join(&app, &owner, &gid, &bob).await;

    // Salon où bob peut voir (par défaut @everyone a VIEW).
    let temp = send(
        &app,
        "POST",
        &format!("/guilds/{gid}/channels"),
        Some(json!({"name":"temp","type":0})),
        Some(&owner),
    )
    .await
    .1;
    let temp_id = temp["id"].as_str().unwrap().to_string();

    // Mention de bob : enregistrée tant qu'il peut voir.
    send(
        &app,
        "POST",
        &format!("/channels/{temp_id}/messages"),
        Some(json!({"content":format!("<@{bob_id}> ping")})),
        Some(&owner),
    )
    .await;
    let (_s, inbox) = send(&app, "GET", "/users/@me/mentions", None, Some(&bob)).await;
    assert_eq!(
        inbox.as_array().unwrap().len(),
        1,
        "mention visible tant que l'accès existe"
    );

    // On retire VIEW à @everyone (bit 1<<10 = 1024) → la boîte filtre dynamiquement.
    send(
        &app,
        "PUT",
        &format!("/channels/{temp_id}/permissions/{gid}"),
        Some(json!({"type":0,"deny":"1024"})),
        Some(&owner),
    )
    .await;
    let (_s, inbox) = send(&app, "GET", "/users/@me/mentions", None, Some(&bob)).await;
    assert_eq!(
        inbox.as_array().unwrap().len(),
        0,
        "mention masquée après perte d'accès"
    );
}
