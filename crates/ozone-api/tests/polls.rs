//! Tests fonctionnels + cyber : sondages (création, vote mono/multi, résultats, validation,
//! permissions de salon).

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
    let path = std::env::temp_dir().join(format!("ozone-test-polls-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Polls".into(),
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
async fn setup(app: &Router, owner: &str) -> (String, String) {
    let g = send(
        app,
        "POST",
        "/guilds",
        Some(json!({"name":"G"})),
        Some(owner),
    )
    .await
    .1;
    let gid = g["id"].as_str().unwrap().to_string();
    let cid = send(
        app,
        "GET",
        &format!("/guilds/{gid}/channels"),
        None,
        Some(owner),
    )
    .await
    .1[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    (gid, cid)
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
fn count(poll: &Value, answer_id: i64) -> i64 {
    poll["answers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["answer_id"] == answer_id)
        .unwrap()["vote_count"]
        .as_i64()
        .unwrap()
}

#[tokio::test]
async fn create_vote_results() {
    let app = app().await;
    let owner = token(&app, "alice", "a@pl.fr").await;
    let (gid, cid) = setup(&app, &owner).await;
    let bob = token(&app, "bob", "b@pl.fr").await;
    join(&app, &owner, &gid, &bob).await;

    let (s, poll) = send(
        &app,
        "POST",
        &format!("/channels/{cid}/polls"),
        Some(json!({"question":"Pizza ce soir ?","answers":["Oui","Non"]})),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(poll["question"], "Pizza ce soir ?");
    assert_eq!(poll["answers"].as_array().unwrap().len(), 2);
    assert_eq!(poll["finished"], false);
    let mid = poll["message_id"].as_str().unwrap().to_string();

    // Le message porteur apparaît dans l'historique.
    let (_s, hist) = send(
        &app,
        "GET",
        &format!("/channels/{cid}/messages"),
        None,
        Some(&owner),
    )
    .await;
    assert!(hist
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["content"] == "Pizza ce soir ?"));

    // bob vote « Oui » (réponse 1).
    let (s, r) = send(
        &app,
        "PUT",
        &format!("/channels/{cid}/polls/{mid}/votes"),
        Some(json!({"answer_ids":[1]})),
        Some(&bob),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(count(&r, 1), 1);
    assert_eq!(r["answers"][0]["me_voted"], true);

    // owner vote « Non » (réponse 2).
    send(
        &app,
        "PUT",
        &format!("/channels/{cid}/polls/{mid}/votes"),
        Some(json!({"answer_ids":[2]})),
        Some(&owner),
    )
    .await;
    let (_s, r) = send(
        &app,
        "GET",
        &format!("/channels/{cid}/polls/{mid}"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(count(&r, 1), 1);
    assert_eq!(count(&r, 2), 1);

    // bob change d'avis → « Non ».
    let (_s, r) = send(
        &app,
        "PUT",
        &format!("/channels/{cid}/polls/{mid}/votes"),
        Some(json!({"answer_ids":[2]})),
        Some(&bob),
    )
    .await;
    assert_eq!(count(&r, 1), 0);
    assert_eq!(count(&r, 2), 2);

    // Mono-réponse : voter pour deux → refusé.
    let (s, _) = send(
        &app,
        "PUT",
        &format!("/channels/{cid}/polls/{mid}/votes"),
        Some(json!({"answer_ids":[1,2]})),
        Some(&bob),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn multiselect_and_validation() {
    let app = app().await;
    let owner = token(&app, "alice", "a@pl2.fr").await;
    let (_gid, cid) = setup(&app, &owner).await;

    let (_s, poll) = send(
        &app,
        "POST",
        &format!("/channels/{cid}/polls"),
        Some(json!({"question":"Langages ?","answers":["Rust","Go","Zig"],"multiselect":true})),
        Some(&owner),
    )
    .await;
    let mid = poll["message_id"].as_str().unwrap().to_string();
    let (s, r) = send(
        &app,
        "PUT",
        &format!("/channels/{cid}/polls/{mid}/votes"),
        Some(json!({"answer_ids":[1,3]})),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(count(&r, 1), 1);
    assert_eq!(count(&r, 3), 1);
    assert_eq!(count(&r, 2), 0);

    // Réponse inexistante → 400.
    let (s, _) = send(
        &app,
        "PUT",
        &format!("/channels/{cid}/polls/{mid}/votes"),
        Some(json!({"answer_ids":[99]})),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Validation de création.
    let (s, _) = send(
        &app,
        "POST",
        &format!("/channels/{cid}/polls"),
        Some(json!({"question":"  ","answers":["a"]})),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "question vide");
    let (s, _) = send(
        &app,
        "POST",
        &format!("/channels/{cid}/polls"),
        Some(json!({"question":"q","answers":[]})),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "aucune réponse");
}

#[tokio::test]
async fn poll_requires_channel_access() {
    let app = app().await;
    let owner = token(&app, "alice", "a@pl3.fr").await;
    let (_gid, cid) = setup(&app, &owner).await;
    let carol = token(&app, "carol", "c@pl3.fr").await; // non-membre

    // Non-membre : ni créer, ni lire, ni voter.
    let (s, _) = send(
        &app,
        "POST",
        &format!("/channels/{cid}/polls"),
        Some(json!({"question":"q","answers":["a","b"]})),
        Some(&carol),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    let (_s, poll) = send(
        &app,
        "POST",
        &format!("/channels/{cid}/polls"),
        Some(json!({"question":"q","answers":["a","b"]})),
        Some(&owner),
    )
    .await;
    let mid = poll["message_id"].as_str().unwrap().to_string();
    let (s, _) = send(
        &app,
        "GET",
        &format!("/channels/{cid}/polls/{mid}"),
        None,
        Some(&carol),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = send(
        &app,
        "PUT",
        &format!("/channels/{cid}/polls/{mid}/votes"),
        Some(json!({"answer_ids":[1]})),
        Some(&carol),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}
