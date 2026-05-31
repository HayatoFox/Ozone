//! Tests fonctionnels + cyber : annuaire de découverte (opt-in), adhésion directe,
//! confidentialité des guildes non publiques, respect des bannissements.

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
    let path = std::env::temp_dir().join(format!("ozone-test-disco-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Disco".into(),
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
fn contains_guild(list: &Value, gid: &str) -> bool {
    list.as_array().unwrap().iter().any(|g| g["id"] == gid)
}

#[tokio::test]
async fn listing_and_direct_join() {
    let app = app().await;
    let owner = token(&app, "alice", "a@disco.fr").await;
    let g = send(
        &app,
        "POST",
        "/guilds",
        Some(json!({"name":"DevHub"})),
        Some(&owner),
    )
    .await
    .1;
    let gid = g["id"].as_str().unwrap().to_string();

    // Pas encore publique → absente de l'annuaire.
    let (_s, list) = send(&app, "GET", "/discovery/guilds", None, Some(&owner)).await;
    assert!(!contains_guild(&list, &gid), "guilde non publique masquée");

    // Le propriétaire l'inscrit à la découverte + description.
    let (s, up) = send(
        &app,
        "PATCH",
        &format!("/guilds/{gid}"),
        Some(json!({"discoverable":true,"description":"Le repaire des devs"})),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(up["discoverable"], true);
    assert_eq!(up["description"], "Le repaire des devs");

    // bob (non-membre) la voit et la rejoint directement.
    let bob = token(&app, "bob", "b@disco.fr").await;
    let (_s, list) = send(&app, "GET", "/discovery/guilds?q=dev", None, Some(&bob)).await;
    let entry = list
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == gid)
        .expect("DevHub listée");
    assert_eq!(entry["member_count"], 1);
    assert_eq!(entry["description"], "Le repaire des devs");

    let (s, _) = send(
        &app,
        "POST",
        &format!("/discovery/guilds/{gid}/join"),
        None,
        Some(&bob),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    // bob est désormais membre.
    let (s, _) = send(&app, "GET", &format!("/guilds/{gid}"), None, Some(&bob)).await;
    assert_eq!(s, StatusCode::OK, "bob est membre après adhésion");
}

#[tokio::test]
async fn private_guilds_and_bans() {
    let app = app().await;
    let owner = token(&app, "alice", "a@disco2.fr").await;
    let p = send(
        &app,
        "POST",
        "/guilds",
        Some(json!({"name":"Privée"})),
        Some(&owner),
    )
    .await
    .1;
    let pid = p["id"].as_str().unwrap().to_string();

    // Guilde non publique : ni listée, ni joignable (404 — on ne révèle pas son existence).
    let bob = token(&app, "bob", "b@disco2.fr").await;
    let (_s, list) = send(&app, "GET", "/discovery/guilds", None, Some(&bob)).await;
    assert!(!contains_guild(&list, &pid));
    let (s, _) = send(
        &app,
        "POST",
        &format!("/discovery/guilds/{pid}/join"),
        None,
        Some(&bob),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::NOT_FOUND,
        "guilde non publique non joignable"
    );

    // Un non-admin ne peut pas l'inscrire à la découverte.
    let (s, _) = send(
        &app,
        "PATCH",
        &format!("/guilds/{pid}"),
        Some(json!({"discoverable":true})),
        Some(&bob),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "inscription à la découverte = MANAGE_GUILD"
    );

    // Adhésion sans jeton → 401.
    let (s, _) = send(
        &app,
        "POST",
        &format!("/discovery/guilds/{pid}/join"),
        None,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // Découverte publique mais utilisateur banni → adhésion refusée.
    send(
        &app,
        "PATCH",
        &format!("/guilds/{pid}"),
        Some(json!({"discoverable":true})),
        Some(&owner),
    )
    .await;
    let carol = token(&app, "carol", "c@disco2.fr").await;
    let carol_id = uid(&app, &carol).await;
    let (s, _) = send(
        &app,
        "PUT",
        &format!("/guilds/{pid}/bans/{carol_id}"),
        Some(json!({})),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "bannissement");
    let (s, _) = send(
        &app,
        "POST",
        &format!("/discovery/guilds/{pid}/join"),
        None,
        Some(&carol),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "un banni ne peut pas rejoindre via la découverte"
    );
}
