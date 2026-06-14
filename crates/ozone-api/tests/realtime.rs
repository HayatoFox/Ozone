//! Émission temps réel (S9) : les mutations de guilde publient les bons événements Gateway,
//! avec une **portée** correcte — et donc filtrés aux seuls membres par `should_deliver`.

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
    let path = std::env::temp_dir().join(format!("ozone-test-rt-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "RT".into(),
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
async fn guild_mutations_emit_scoped_events() {
    let (app, state) = build().await;
    let alice = reg(&app, "alice", "a@rt.fr").await;
    let alice_id = uid(&app, &alice).await;
    let carol = reg(&app, "carol", "c@rt.fr").await; // jamais membre
    let carol_id = uid(&app, &carol).await;
    let g = send(
        &app,
        "POST",
        "/guilds",
        Some(json!({"name":"G"})),
        Some(&alice),
    )
    .await
    .1;
    let gid = g["id"].as_str().unwrap().parse::<u64>().unwrap() as i64;

    // On s'abonne AVANT les mutations à observer.
    let mut rx = state.hub.subscribe();

    // bob rejoint → GUILD_MEMBER_ADD.
    let bob = reg(&app, "bob", "b@rt.fr").await;
    let inv = send(
        &app,
        "POST",
        &format!("/guilds/{gid}/invites"),
        Some(json!({})),
        Some(&alice),
    )
    .await
    .1;
    let code = inv["code"].as_str().unwrap().to_string();
    send(&app, "POST", &format!("/invites/{code}"), None, Some(&bob)).await;
    let bob_id = uid(&app, &bob).await;

    // Création de rôle, de salon, puis bannissement de bob.
    send(
        &app,
        "POST",
        &format!("/guilds/{gid}/roles"),
        Some(json!({"name":"VIP"})),
        Some(&alice),
    )
    .await;
    send(
        &app,
        "POST",
        &format!("/guilds/{gid}/channels"),
        Some(json!({"name":"annexe","type":0})),
        Some(&alice),
    )
    .await;
    send(
        &app,
        "PUT",
        &format!("/guilds/{gid}/bans/{bob_id}"),
        Some(json!({})),
        Some(&alice),
    )
    .await;

    let events = drain(&mut rx);
    let types: Vec<&str> = events.iter().map(|e| e.t.as_str()).collect();
    for expected in [
        "GUILD_MEMBER_ADD",
        "GUILD_ROLE_CREATE",
        "CHANNEL_CREATE",
        "GUILD_BAN_ADD",
        "GUILD_MEMBER_REMOVE",
    ] {
        assert!(
            types.contains(&expected),
            "événement {expected} attendu, reçus : {types:?}"
        );
    }

    // Portée correcte : le rôle est diffusé à l'échelle de la guilde, le salon à l'échelle salon.
    let role_ev = events.iter().find(|e| e.t == "GUILD_ROLE_CREATE").unwrap();
    assert!(matches!(role_ev.scope, EventScope::Guild(g) if g == gid));
    let chan_ev = events.iter().find(|e| e.t == "CHANNEL_CREATE").unwrap();
    assert!(matches!(chan_ev.scope, EventScope::Channel { guild_id, .. } if guild_id == gid));

    // Confidentialité : un événement de guilde n'est routé qu'aux membres.
    assert!(gateway::should_deliver(&state, alice_id, &EventScope::Guild(gid)).await);
    assert!(
        !gateway::should_deliver(&state, carol_id, &EventScope::Guild(gid)).await,
        "un non-membre ne reçoit pas les événements de la guilde"
    );
}
