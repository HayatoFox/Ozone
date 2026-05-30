//! Test d'intégration du système de permissions : invitation, rôles, overwrites de salon.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use ozone_api::config::Config;
use ozone_api::{bootstrap_state, build_app};
use ozone_proto::perms;
use serde_json::{json, Value};
use tower::ServiceExt;

async fn open_app() -> Router {
    let unique = format!(
        "{}_{:?}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        std::thread::current().id()
    );
    let path = std::env::temp_dir().join(format!("ozone-test-perms-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Ozone Perms".into(),
        instance_description: None,
        registration_policy: "open".into(),
        instance_password: None,
        version: "0.1.0-test".into(),
    };
    build_app(bootstrap_state(&cfg).await.expect("bootstrap"))
}

async fn body_json(resp: axum::response::Response) -> Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

fn req(method: &str, uri: &str, body: Option<Value>, bearer: Option<&str>) -> Request<Body> {
    let mut b = Request::builder().method(method).uri(uri);
    if bearer.is_some() {
        b = b.header("authorization", format!("Bearer {}", bearer.unwrap()));
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

async fn register(app: &Router, username: &str, email: &str) -> String {
    let resp = app
        .clone()
        .oneshot(req(
            "POST",
            "/auth/register",
            Some(json!({"username": username, "email": email, "password": "motdepasse"})),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "register {username}");
    body_json(resp).await["access_token"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn permissions_flow() {
    let app = open_app().await;

    // Alice = propriétaire de l'instance + créatrice de la guilde
    let alice = register(&app, "alice", "alice@x.fr").await;

    let guild = body_json(
        app.clone()
            .oneshot(req(
                "POST",
                "/guilds",
                Some(json!({"name":"G"})),
                Some(&alice),
            ))
            .await
            .unwrap(),
    )
    .await;
    let gid = guild["id"].as_str().unwrap().to_string();

    // salon « général »
    let channels = body_json(
        app.clone()
            .oneshot(req(
                "GET",
                &format!("/guilds/{gid}/channels"),
                None,
                Some(&alice),
            ))
            .await
            .unwrap(),
    )
    .await;
    let cid = channels[0]["id"].as_str().unwrap().to_string();

    // Alice crée une invitation
    let invite = body_json(
        app.clone()
            .oneshot(req(
                "POST",
                &format!("/guilds/{gid}/invites"),
                Some(json!({})),
                Some(&alice),
            ))
            .await
            .unwrap(),
    )
    .await;
    let code = invite["code"].as_str().unwrap().to_string();

    // Bob s'inscrit et rejoint via l'invitation
    let bob = register(&app, "bob", "bob@x.fr").await;
    let resp = app
        .clone()
        .oneshot(req(
            "POST",
            &format!("/invites/{code}"),
            Some(json!({})),
            Some(&bob),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "join");

    // Bob voit le salon et peut écrire (permissions @everyone par défaut)
    let resp = app
        .clone()
        .oneshot(req(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({"content":"salut"})),
            Some(&bob),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "bob peut écrire");

    // Bob NE peut PAS créer de rôle (pas de MANAGE_ROLES)
    let resp = app
        .clone()
        .oneshot(req(
            "POST",
            &format!("/guilds/{gid}/roles"),
            Some(json!({"name":"r"})),
            Some(&bob),
        ))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "bob ne peut pas créer de rôle"
    );

    // Alice refuse SEND_MESSAGES à @everyone sur le salon (overwrite de rôle, target = @everyone = gid)
    let deny = perms::SEND_MESSAGES.to_string();
    let resp = app
        .clone()
        .oneshot(req(
            "PUT",
            &format!("/channels/{cid}/permissions/{gid}"),
            Some(json!({"type": 0, "deny": deny})),
            Some(&alice),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "overwrite posé");

    // Désormais Bob ne peut plus écrire…
    let resp = app
        .clone()
        .oneshot(req(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({"content":"encore"})),
            Some(&bob),
        ))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "bob bloqué par l'overwrite"
    );

    // …mais Alice (propriétaire) oui (bypass)
    let resp = app
        .clone()
        .oneshot(req(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({"content":"moi oui"})),
            Some(&alice),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "owner bypass");

    // Alice expulse Bob
    let resp = app
        .clone()
        .oneshot(req(
            "DELETE",
            &format!("/guilds/{gid}/members/{}", user_id(&app, &bob).await),
            None,
            Some(&alice),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "kick");

    // Bob n'est plus membre → ne peut plus écrire
    let resp = app
        .clone()
        .oneshot(req(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({"content":"revenu"})),
            Some(&bob),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN, "non-membre bloqué");
}

async fn user_id(app: &Router, token: &str) -> String {
    let me = body_json(
        app.clone()
            .oneshot(req("GET", "/users/@me", None, Some(token)))
            .await
            .unwrap(),
    )
    .await;
    me["id"].as_str().unwrap().to_string()
}
