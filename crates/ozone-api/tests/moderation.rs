//! Tests fonctionnels de modération : bannissements, timeout, journal d'audit.

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
    let path = std::env::temp_dir().join(format!("ozone-test-mod-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Mod".into(),
        instance_description: None,
        registration_policy: "open".into(),
        instance_password: None,
        version: "0.1.0-test".into(),
    };
    build_app(bootstrap_state(&cfg).await.expect("bootstrap"))
}

fn rq(method: &str, uri: &str, body: Option<Value>, token: Option<&str>) -> Request<Body> {
    let mut b = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        b = b.header("authorization", format!("Bearer {t}"));
    }
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
    let st = resp.status();
    let by = resp.into_body().collect().await.unwrap().to_bytes();
    (st, serde_json::from_slice(&by).unwrap_or(Value::Null))
}

async fn reg(app: &Router, u: &str, e: &str) -> String {
    send(
        app,
        rq(
            "POST",
            "/auth/register",
            Some(json!({"username":u,"email":e,"password":"Sup3r-Ozone-Pw"})),
            None,
        ),
    )
    .await
    .1["access_token"]
        .as_str()
        .unwrap()
        .to_string()
}
async fn uid(app: &Router, t: &str) -> String {
    send(app, rq("GET", "/users/@me", None, Some(t))).await.1["id"]
        .as_str()
        .unwrap()
        .to_string()
}
async fn make_guild(app: &Router, t: &str) -> String {
    send(
        app,
        rq("POST", "/guilds", Some(json!({"name":"G"})), Some(t)),
    )
    .await
    .1["id"]
        .as_str()
        .unwrap()
        .to_string()
}
async fn general(app: &Router, gid: &str, t: &str) -> String {
    send(
        app,
        rq("GET", &format!("/guilds/{gid}/channels"), None, Some(t)),
    )
    .await
    .1[0]["id"]
        .as_str()
        .unwrap()
        .to_string()
}
async fn invite_code(app: &Router, gid: &str, owner: &str) -> String {
    send(
        app,
        rq(
            "POST",
            &format!("/guilds/{gid}/invites"),
            Some(json!({})),
            Some(owner),
        ),
    )
    .await
    .1["code"]
        .as_str()
        .unwrap()
        .to_string()
}
async fn join(app: &Router, owner: &str, gid: &str, u: &str, e: &str) -> String {
    let code = invite_code(app, gid, owner).await;
    let t = reg(app, u, e).await;
    send(app, rq("POST", &format!("/invites/{code}"), None, Some(&t))).await;
    t
}
fn future_ms(secs: i64) -> i64 {
    (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64)
        + secs * 1000
}

#[tokio::test]
async fn ban_unban_and_audit() {
    let app = app().await;
    let alice = reg(&app, "alice", "a@mod.fr").await;
    let gid = make_guild(&app, &alice).await;
    let cid = general(&app, &gid, &alice).await;
    let bob = join(&app, &alice, &gid, "bob", "b@mod.fr").await;
    let bob_id = uid(&app, &bob).await;

    // Bannissement
    let (s, _) = send(
        &app,
        rq(
            "PUT",
            &format!("/guilds/{gid}/bans/{bob_id}"),
            Some(json!({"reason":"spam"})),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "ban");
    // Banni → plus membre → ne peut plus écrire
    let (s, _) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({"content":"x"})),
            Some(&bob),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "banni ne peut écrire");
    // Liste des bans
    let bans = send(
        &app,
        rq("GET", &format!("/guilds/{gid}/bans"), None, Some(&alice)),
    )
    .await
    .1;
    assert!(
        bans.as_array()
            .unwrap()
            .iter()
            .any(|b| b["user"]["username"] == "bob"),
        "bob dans la liste des bannis"
    );
    // Banni ne peut pas rejoindre
    let code = invite_code(&app, &gid, &alice).await;
    let (s, _) = send(
        &app,
        rq("POST", &format!("/invites/{code}"), None, Some(&bob)),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "banni ne peut rejoindre");
    // Déban
    let (s, _) = send(
        &app,
        rq(
            "DELETE",
            &format!("/guilds/{gid}/bans/{bob_id}"),
            None,
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "unban");
    let (s, _) = send(
        &app,
        rq("POST", &format!("/invites/{code}"), None, Some(&bob)),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "rejoint après déban");

    // Journal d'audit
    let audit = send(
        &app,
        rq(
            "GET",
            &format!("/guilds/{gid}/audit-logs"),
            None,
            Some(&alice),
        ),
    )
    .await
    .1;
    let actions: Vec<&str> = audit
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a["action_type"].as_str())
        .collect();
    assert!(actions.contains(&"member_ban"), "audit ban");
    assert!(actions.contains(&"member_unban"), "audit unban");
}

#[tokio::test]
async fn timeout_blocks_then_lifts() {
    let app = app().await;
    let alice = reg(&app, "alice", "a@mod2.fr").await;
    let gid = make_guild(&app, &alice).await;
    let cid = general(&app, &gid, &alice).await;
    let bob = join(&app, &alice, &gid, "bob", "b@mod2.fr").await;
    let bob_id = uid(&app, &bob).await;

    // Avant : bob peut écrire
    let (s, _) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({"content":"avant"})),
            Some(&bob),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    // Timeout d'une heure
    let (s, _) = send(
        &app,
        rq(
            "PATCH",
            &format!("/guilds/{gid}/members/{bob_id}"),
            Some(json!({"communication_disabled_until": future_ms(3600)})),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "timeout posé");
    // Pendant : bloqué
    let (s, _) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({"content":"pendant"})),
            Some(&bob),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "timeout bloque l'envoi");
    // Levée (timestamp passé)
    let (s, _) = send(
        &app,
        rq(
            "PATCH",
            &format!("/guilds/{gid}/members/{bob_id}"),
            Some(json!({"communication_disabled_until": 1})),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "levée du timeout");
    // Après : de nouveau autorisé
    let (s, _) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({"content":"après"})),
            Some(&bob),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "après levée");
}
