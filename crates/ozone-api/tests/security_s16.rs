//! Vérification cybersécurité S16 : signalisation vocale.
//! Rejoindre exige adhésion + CONNECT ; modération vocale exige les permissions ; propriétaire protégé.

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
    let path = std::env::temp_dir().join(format!("ozone-test-secs16-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "SecS16".into(),
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
async fn join_guild(app: &Router, owner: &str, gid: &str, member: &str) {
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
async fn voice_channel(app: &Router, gid: &str, owner: &str) -> String {
    send(
        app,
        "POST",
        &format!("/guilds/{gid}/channels"),
        Some(json!({"name":"Vocal","type":2})),
        Some(owner),
    )
    .await
    .1["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn join_requires_membership_and_connect() {
    let app = app().await;
    let owner = token(&app, "alice", "a@s16.fr").await;
    let carol = token(&app, "carol", "c@s16.fr").await; // non-membre
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
    let vc = voice_channel(&app, &gid, &owner).await;

    // Non-membre : ni rejoindre, ni lister.
    let (s, _) = send(
        &app,
        "PATCH",
        &format!("/guilds/{gid}/voice-states/@me"),
        Some(json!({"channel_id":vc})),
        Some(&carol),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "non-membre ne rejoint pas");
    let (s, _) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/voice-states"),
        None,
        Some(&carol),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "non-membre ne liste pas");

    // Membre mais CONNECT refusé sur le salon → rejoindre interdit.
    let bob = token(&app, "bob", "b@s16.fr").await;
    join_guild(&app, &owner, &gid, &bob).await;
    // Refuse CONNECT (1<<20 = 1048576) à @everyone sur le salon vocal.
    send(
        &app,
        "PUT",
        &format!("/channels/{vc}/permissions/{gid}"),
        Some(json!({"type":0,"deny":"1048576"})),
        Some(&owner),
    )
    .await;
    let (s, _) = send(
        &app,
        "PATCH",
        &format!("/guilds/{gid}/voice-states/@me"),
        Some(json!({"channel_id":vc})),
        Some(&bob),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "CONNECT requis pour rejoindre");
}

#[tokio::test]
async fn moderation_requires_perms_and_protects_owner() {
    let app = app().await;
    let owner = token(&app, "alice", "a@s16b.fr").await;
    let owner_id = uid(&app, &owner).await;
    let bob = token(&app, "bob", "b@s16b.fr").await;
    let bob_id = uid(&app, &bob).await;
    let dave = token(&app, "dave", "d@s16b.fr").await;
    let dave_id = uid(&app, &dave).await;
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
    let vc = voice_channel(&app, &gid, &owner).await;
    join_guild(&app, &owner, &gid, &bob).await;
    join_guild(&app, &owner, &gid, &dave).await;

    // dave et owner sont en vocal.
    send(
        &app,
        "PATCH",
        &format!("/guilds/{gid}/voice-states/@me"),
        Some(json!({"channel_id":vc})),
        Some(&dave),
    )
    .await;
    send(
        &app,
        "PATCH",
        &format!("/guilds/{gid}/voice-states/@me"),
        Some(json!({"channel_id":vc})),
        Some(&owner),
    )
    .await;

    // bob (membre simple) ne peut pas mute dave.
    let (s, _) = send(
        &app,
        "PATCH",
        &format!("/guilds/{gid}/voice-states/{dave_id}"),
        Some(json!({"mute":true})),
        Some(&bob),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "mute serveur exige MUTE_MEMBERS");

    // On accorde MUTE_MEMBERS à bob (bit 1<<22 = 4194304).
    let role = send(
        &app,
        "POST",
        &format!("/guilds/{gid}/roles"),
        Some(json!({"name":"vmod","permissions":"4194304"})),
        Some(&owner),
    )
    .await
    .1;
    let rid = role["id"].as_str().unwrap().to_string();
    send(
        &app,
        "PUT",
        &format!("/guilds/{gid}/members/{bob_id}/roles/{rid}"),
        None,
        Some(&owner),
    )
    .await;

    // Désormais bob peut mute dave…
    let (s, _) = send(
        &app,
        "PATCH",
        &format!("/guilds/{gid}/voice-states/{dave_id}"),
        Some(json!({"mute":true})),
        Some(&bob),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "avec MUTE_MEMBERS, mute autorisé");
    // …mais PAS mute le propriétaire.
    let (s, _) = send(
        &app,
        "PATCH",
        &format!("/guilds/{gid}/voice-states/{owner_id}"),
        Some(json!({"mute":true})),
        Some(&bob),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "le propriétaire est protégé en vocal"
    );
}
