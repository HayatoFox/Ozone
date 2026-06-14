//! Tests fonctionnels de la signalisation vocale : rejoindre, indicateurs, quitter,
//! états vocaux, modération (mute serveur, déconnexion), régions.

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
    let path = std::env::temp_dir().join(format!("ozone-test-voice-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Voice".into(),
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
async fn join_update_leave_voice() {
    let app = app().await;
    let owner = token(&app, "alice", "a@vo.fr").await;
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
    let general = send(
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

    // Rejoindre (mute auto) → état + infos de connexion.
    let (s, r) = send(
        &app,
        "PATCH",
        &format!("/guilds/{gid}/voice-states/@me"),
        Some(json!({"channel_id":vc,"self_mute":true})),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(r["voice_state"]["channel_id"], vc);
    assert_eq!(r["voice_state"]["self_mute"], true);
    assert!(
        !r["connection"]["token"].as_str().unwrap().is_empty(),
        "jeton vocal fourni"
    );
    assert!(!r["connection"]["session_id"].as_str().unwrap().is_empty());

    // Présent dans la liste.
    let (_s, list) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/voice-states"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(list.as_array().unwrap().len(), 1);

    // Mise à jour d'indicateur sans changer de salon.
    let (s, vs) = send(
        &app,
        "PATCH",
        &format!("/guilds/{gid}/voice-states/@me"),
        Some(json!({"self_deaf":true})),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(vs["self_deaf"], true);
    assert_eq!(vs["channel_id"], vc, "toujours dans le salon vocal");

    // Rejoindre un salon TEXTE → refusé.
    let (s, _) = send(
        &app,
        "PATCH",
        &format!("/guilds/{gid}/voice-states/@me"),
        Some(json!({"channel_id":general})),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "le salon doit être vocal");

    // Quitter.
    let (s, _) = send(
        &app,
        "DELETE",
        &format!("/guilds/{gid}/voice-states/@me"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (_s, list) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/voice-states"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(list.as_array().unwrap().len(), 0);

    // Régions.
    let (s, regions) = send(&app, "GET", "/voice/regions", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::OK);
    assert!(regions
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["id"] == "auto" && r["optimal"] == true));
}

#[tokio::test]
async fn server_mute_and_disconnect() {
    let app = app().await;
    let owner = token(&app, "alice", "a@vo2.fr").await;
    let bob = token(&app, "bob", "b@vo2.fr").await;
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
    let vc = voice_channel(&app, &gid, &owner).await;
    join_guild(&app, &owner, &gid, &bob).await;
    send(
        &app,
        "PATCH",
        &format!("/guilds/{gid}/voice-states/@me"),
        Some(json!({"channel_id":vc})),
        Some(&bob),
    )
    .await;

    // owner (toutes permissions) mute serveur bob.
    let (s, vs) = send(
        &app,
        "PATCH",
        &format!("/guilds/{gid}/voice-states/{bob_id}"),
        Some(json!({"mute":true})),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(vs["mute"], true);

    // owner déconnecte bob du vocal.
    let (s, _) = send(
        &app,
        "PATCH",
        &format!("/guilds/{gid}/voice-states/{bob_id}"),
        Some(json!({"disconnect":true})),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (_s, list) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/voice-states"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(list.as_array().unwrap().len(), 0, "bob déconnecté du vocal");
}
