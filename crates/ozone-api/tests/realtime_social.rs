//! Émission temps réel des relations & MP (S12) : portée `User`/`Dm` correcte
//! → délivrée uniquement aux intéressés (vérifié via `should_deliver`).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use ozone_api::config::Config;
use ozone_api::state::{AppState, EventScope, HubEvent};
use ozone_api::{bootstrap_state, build_app, gateway};
use serde_json::{json, Value};
use tokio::sync::broadcast;
use tower::ServiceExt;

async fn build() -> (Router, AppState) {
    let unique = format!(
        "{}_{:?}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        std::thread::current().id()
    );
    let path = std::env::temp_dir().join(format!("ozone-test-rts-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "RTS".into(),
        instance_description: None,
        registration_policy: "open".into(),
        instance_password: None,
        version: "0.1.0-test".into(),
    };
    let state = bootstrap_state(&cfg).await.expect("bootstrap");
    (build_app(state.clone()), state)
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

async fn reg(app: &Router, u: &str, e: &str) -> String {
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
async fn uid(app: &Router, t: &str) -> i64 {
    send(app, "GET", "/users/@me", None, Some(t)).await.1["id"]
        .as_str()
        .unwrap()
        .parse::<u64>()
        .unwrap() as i64
}
fn drain(rx: &mut broadcast::Receiver<HubEvent>) -> Vec<HubEvent> {
    let mut out = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        out.push(ev);
    }
    out
}

#[tokio::test]
async fn friend_request_and_dm_are_scoped() {
    let (app, state) = build().await;
    let alice = reg(&app, "alice", "a@rts.fr").await;
    let alice_id = uid(&app, &alice).await;
    let bob = reg(&app, "bob", "b@rts.fr").await;
    let bob_id = uid(&app, &bob).await;
    let carol = reg(&app, "carol", "c@rts.fr").await;
    let carol_id = uid(&app, &carol).await;

    let mut rx = state.hub.subscribe();

    // Demande d'ami alice → bob.
    let (s, _) = send(
        &app,
        "POST",
        "/users/@me/relationships",
        Some(json!({"username":"bob"})),
        Some(&alice),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    let events = drain(&mut rx);
    // bob doit recevoir une demande entrante, en portée User(bob).
    let to_bob = events.iter().find(|e| {
        e.t == "RELATIONSHIP_ADD" && matches!(e.scope, EventScope::User(u) if u == bob_id)
    });
    assert!(to_bob.is_some(), "RELATIONSHIP_ADD vers bob attendu");
    assert_eq!(to_bob.unwrap().d["type"], "incoming");

    // Confidentialité : seul bob reçoit l'événement User(bob), pas carol.
    assert!(gateway::should_deliver(&state, bob_id, &EventScope::User(bob_id)).await);
    assert!(!gateway::should_deliver(&state, carol_id, &EventScope::User(bob_id)).await);
    assert!(!gateway::should_deliver(&state, alice_id, &EventScope::User(bob_id)).await);

    // Ouverture d'un MP alice ↔ bob.
    let (_s, dm) = send(
        &app,
        "POST",
        "/users/@me/channels",
        Some(json!({"recipients":[bob_id.to_string()]})),
        Some(&alice),
    )
    .await;
    let dm_id = dm["id"].as_str().unwrap().parse::<u64>().unwrap() as i64;

    let events = drain(&mut rx);
    let dm_ev = events
        .iter()
        .find(|e| e.t == "CHANNEL_CREATE" && matches!(e.scope, EventScope::Dm(c) if c == dm_id));
    assert!(dm_ev.is_some(), "CHANNEL_CREATE (MP) en portée Dm attendu");

    // Confidentialité : destinataires seulement.
    assert!(gateway::should_deliver(&state, alice_id, &EventScope::Dm(dm_id)).await);
    assert!(gateway::should_deliver(&state, bob_id, &EventScope::Dm(dm_id)).await);
    assert!(!gateway::should_deliver(&state, carol_id, &EventScope::Dm(dm_id)).await);
}
