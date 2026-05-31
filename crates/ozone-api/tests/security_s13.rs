//! Vérification cybersécurité S13 : gestion de guilde.
//! Lecture réservée aux membres, édition à `MANAGE_GUILD`, **suppression au seul propriétaire**.

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
    let path = std::env::temp_dir().join(format!("ozone-test-secs13-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "SecS13".into(),
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
async fn guild_management_authorization() {
    let app = app().await;
    let owner = token(&app, "alice", "a@s13.fr").await;
    let bob = token(&app, "bob", "b@s13.fr").await;
    let carol = token(&app, "carol", "c@s13.fr").await; // non-membre
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

    // Non-membre : pas même la lecture.
    let (s, _) = send(&app, "GET", &format!("/guilds/{gid}"), None, Some(&carol)).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "lecture réservée aux membres");

    // Membre simple : peut lire, mais ni éditer ni supprimer.
    let (s, _) = send(&app, "GET", &format!("/guilds/{gid}"), None, Some(&bob)).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = send(
        &app,
        "PATCH",
        &format!("/guilds/{gid}"),
        Some(json!({"name":"x"})),
        Some(&bob),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "édition réservée à MANAGE_GUILD");
    let (s, _) = send(&app, "DELETE", &format!("/guilds/{gid}"), None, Some(&bob)).await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "suppression réservée au propriétaire"
    );

    // Validation du nom.
    let (s, _) = send(
        &app,
        "PATCH",
        &format!("/guilds/{gid}"),
        Some(json!({"name":"  "})),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn manage_guild_can_rename_but_not_delete() {
    let app = app().await;
    let owner = token(&app, "alice", "a@s13b.fr").await;
    let bob = token(&app, "bob", "b@s13b.fr").await;
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

    // Rôle avec MANAGE_GUILD (bit 1<<5 = 32), attribué à bob.
    let role = send(
        &app,
        "POST",
        &format!("/guilds/{gid}/roles"),
        Some(json!({"name":"mods","permissions":"32"})),
        Some(&owner),
    )
    .await
    .1;
    let rid = role["id"].as_str().unwrap().to_string();
    let (s, _) = send(
        &app,
        "PUT",
        &format!("/guilds/{gid}/members/{bob_id}/roles/{rid}"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // Avec MANAGE_GUILD, bob peut renommer…
    let (s, _) = send(
        &app,
        "PATCH",
        &format!("/guilds/{gid}"),
        Some(json!({"name":"Par un mod"})),
        Some(&bob),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "MANAGE_GUILD autorise le renommage");
    // …mais PAS supprimer (réservé au propriétaire).
    let (s, _) = send(&app, "DELETE", &format!("/guilds/{gid}"), None, Some(&bob)).await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "la suppression reste réservée au propriétaire"
    );

    // Le propriétaire, lui, peut supprimer.
    let (s, _) = send(
        &app,
        "DELETE",
        &format!("/guilds/{gid}"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
}
