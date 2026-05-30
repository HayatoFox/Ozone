//! Vérification cybersécurité du routage Gateway (S5) : un événement ne fuite jamais
//! vers un utilisateur non autorisé (non-membre, non-visiteur d'un salon privé, non-destinataire).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use ozone_api::config::Config;
use ozone_api::state::{AppState, EventScope};
use ozone_api::{bootstrap_state, build_app, gateway};
use serde_json::{json, Value};
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
    let path = std::env::temp_dir().join(format!("ozone-test-secs5-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "SecS5".into(),
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

fn idi(s: &str) -> i64 {
    s.parse::<u64>().unwrap() as i64
}

#[tokio::test]
async fn routing_denies_unauthorized_users() {
    let (app, state) = build().await;

    let alice = reg(&app, "alice", "a@gws.fr").await;
    let alice_id = idi(&uid(&app, &alice).await);
    let (_, g) = send(
        &app,
        "POST",
        "/guilds",
        Some(json!({"name":"G"})),
        Some(&alice),
    )
    .await;
    let gid = idi(g["id"].as_str().unwrap());
    let (_, chans) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/channels"),
        None,
        Some(&alice),
    )
    .await;
    let cid = idi(chans[0]["id"].as_str().unwrap());

    let (_, inv) = send(
        &app,
        "POST",
        &format!("/guilds/{gid}/invites"),
        Some(json!({})),
        Some(&alice),
    )
    .await;
    let code = inv["code"].as_str().unwrap().to_string();
    let bob = reg(&app, "bob", "b@gws.fr").await;
    let bob_id = idi(&uid(&app, &bob).await);
    send(&app, "POST", &format!("/invites/{code}"), None, Some(&bob)).await;

    let mallory = reg(&app, "mallory", "m@gws.fr").await;
    let mallory_id = idi(&uid(&app, &mallory).await);

    let (_, dm) = send(
        &app,
        "POST",
        "/users/@me/channels",
        Some(json!({"recipients":[bob_id.to_string()]})),
        Some(&alice),
    )
    .await;
    let dm_id = idi(dm["id"].as_str().unwrap());

    // Salon privé : VIEW_CHANNEL refusé à @everyone (rôle id == gid).
    let (_, pc) = send(
        &app,
        "POST",
        &format!("/guilds/{gid}/channels"),
        Some(json!({"name":"prive"})),
        Some(&alice),
    )
    .await;
    let pcid = idi(pc["id"].as_str().unwrap());
    let deny = ozone_proto::perms::VIEW_CHANNEL.to_string();
    let (s, _) = send(
        &app,
        "PUT",
        &format!("/channels/{pcid}/permissions/{gid}"),
        Some(json!({"type":0,"deny":deny})),
        Some(&alice),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "pose de l'overwrite");

    // Un non-membre n'est concerné par aucune portée de la guilde / du MP.
    assert!(
        !gateway::should_deliver(&state, mallory_id, &EventScope::Guild(gid)).await,
        "non-membre ≠ guilde"
    );
    assert!(
        !gateway::should_deliver(
            &state,
            mallory_id,
            &EventScope::Channel {
                guild_id: gid,
                channel_id: cid
            }
        )
        .await,
        "non-membre ≠ salon"
    );
    assert!(
        !gateway::should_deliver(&state, mallory_id, &EventScope::Dm(dm_id)).await,
        "non-destinataire ≠ MP"
    );

    // Salon privé : un membre sans VIEW ne reçoit pas ; le propriétaire oui.
    assert!(
        !gateway::should_deliver(
            &state,
            bob_id,
            &EventScope::Channel {
                guild_id: gid,
                channel_id: pcid
            }
        )
        .await,
        "membre sans VIEW ne reçoit pas"
    );
    assert!(
        gateway::should_deliver(
            &state,
            alice_id,
            &EventScope::Channel {
                guild_id: gid,
                channel_id: pcid
            }
        )
        .await,
        "le propriétaire reçoit"
    );

    // Portée utilisateur : seule la cible.
    assert!(
        !gateway::should_deliver(&state, bob_id, &EventScope::User(alice_id)).await,
        "portée user ciblée"
    );
}
