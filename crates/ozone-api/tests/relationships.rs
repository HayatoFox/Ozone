//! Tests d'intégration des relations : demandes d'amitié, blocage, notes.

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
    let path = std::env::temp_dir().join(format!("ozone-test-rel-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Ozone Rel".into(),
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

async fn send(app: Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

async fn register(app: &Router, username: &str, email: &str) -> String {
    let (status, body) = send(
        app.clone(),
        rq(
            "POST",
            "/auth/register",
            Some(json!({"username": username, "email": email, "password": "Sup3r-Ozone-Pw"})),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "register {username}");
    body["access_token"].as_str().unwrap().to_string()
}

async fn uid(app: &Router, token: &str) -> String {
    let (status, body) = send(app.clone(), rq("GET", "/users/@me", None, Some(token))).await;
    assert_eq!(status, StatusCode::OK, "GET /users/@me");
    body["id"].as_str().unwrap().to_string()
}

fn find_by_user<'a>(arr: &'a Value, username: &str) -> Option<&'a Value> {
    arr.as_array()?
        .iter()
        .find(|entry| entry["user"]["username"].as_str() == Some(username))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests fonctionnels
// ─────────────────────────────────────────────────────────────────────────────

/// Flux d'amitié complet : demande → incoming/outgoing → acceptation → friend.
#[tokio::test]
async fn friendship_flow() {
    let app = app().await;
    let alice_tok = register(&app, "alice", "alice@rel.test").await;
    let bob_tok = register(&app, "bob", "bob@rel.test").await;
    let alice_id = uid(&app, &alice_tok).await;

    // Alice envoie une demande d'ami à Bob
    let (status, _) = send(
        app.clone(),
        rq(
            "POST",
            "/users/@me/relationships",
            Some(json!({"username": "bob"})),
            Some(&alice_tok),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "alice envoie la demande d'ami");

    // Bob voit alice en "incoming"
    let (status, bob_rels) = send(
        app.clone(),
        rq("GET", "/users/@me/relationships", None, Some(&bob_tok)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "GET relations de bob");
    let entry =
        find_by_user(&bob_rels, "alice").expect("alice doit apparaître dans les relations de bob");
    assert_eq!(
        entry["type"].as_str(),
        Some("incoming"),
        "bob doit voir alice en 'incoming'"
    );

    // Alice voit bob en "outgoing"
    let (status, alice_rels) = send(
        app.clone(),
        rq("GET", "/users/@me/relationships", None, Some(&alice_tok)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "GET relations d'alice");
    let entry =
        find_by_user(&alice_rels, "bob").expect("bob doit apparaître dans les relations d'alice");
    assert_eq!(
        entry["type"].as_str(),
        Some("outgoing"),
        "alice doit voir bob en 'outgoing'"
    );

    // Bob accepte la demande via PUT /:aliceId
    let (status, _) = send(
        app.clone(),
        rq(
            "PUT",
            &format!("/users/@me/relationships/{alice_id}"),
            None,
            Some(&bob_tok),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "bob accepte la demande d'alice");

    // Alice voit bob en "friend"
    let (status, alice_rels) = send(
        app.clone(),
        rq("GET", "/users/@me/relationships", None, Some(&alice_tok)),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "GET relations d'alice après acceptation"
    );
    let entry = find_by_user(&alice_rels, "bob")
        .expect("bob doit toujours apparaître dans les relations d'alice");
    assert_eq!(
        entry["type"].as_str(),
        Some("friend"),
        "alice doit voir bob en 'friend'"
    );

    // Bob voit alice en "friend"
    let (status, bob_rels) = send(
        app.clone(),
        rq("GET", "/users/@me/relationships", None, Some(&bob_tok)),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "GET relations de bob après acceptation"
    );
    let entry = find_by_user(&bob_rels, "alice")
        .expect("alice doit toujours apparaître dans les relations de bob");
    assert_eq!(
        entry["type"].as_str(),
        Some("friend"),
        "bob doit voir alice en 'friend'"
    );
}

/// Acceptation via POST (quand la cible a déjà une demande incoming).
#[tokio::test]
async fn friendship_accept_via_post() {
    let app = app().await;
    let alice_tok = register(&app, "alice", "alice@rel2.test").await;
    let bob_tok = register(&app, "bob", "bob@rel2.test").await;

    // Alice envoie une demande
    let (status, _) = send(
        app.clone(),
        rq(
            "POST",
            "/users/@me/relationships",
            Some(json!({"username": "bob"})),
            Some(&alice_tok),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "alice envoie la demande");

    // Bob accepte en POST {username:"alice"}
    let (status, _) = send(
        app.clone(),
        rq(
            "POST",
            "/users/@me/relationships",
            Some(json!({"username": "alice"})),
            Some(&bob_tok),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "bob accepte via POST");

    // Les deux voient "friend"
    let (_, alice_rels) = send(
        app.clone(),
        rq("GET", "/users/@me/relationships", None, Some(&alice_tok)),
    )
    .await;
    let entry =
        find_by_user(&alice_rels, "bob").expect("bob doit apparaître dans les relations d'alice");
    assert_eq!(
        entry["type"].as_str(),
        Some("friend"),
        "alice doit voir bob en 'friend' (acceptation via POST)"
    );

    let (_, bob_rels) = send(
        app.clone(),
        rq("GET", "/users/@me/relationships", None, Some(&bob_tok)),
    )
    .await;
    let entry =
        find_by_user(&bob_rels, "alice").expect("alice doit apparaître dans les relations de bob");
    assert_eq!(
        entry["type"].as_str(),
        Some("friend"),
        "bob doit voir alice en 'friend' (acceptation via POST)"
    );
}

/// Notes : écriture puis lecture.
#[tokio::test]
async fn notes_write_and_read() {
    let app = app().await;
    let alice_tok = register(&app, "alice", "alice@notes.test").await;
    let bob_tok = register(&app, "bob", "bob@notes.test").await;
    let bob_id = uid(&app, &bob_tok).await;

    // Alice écrit une note sur Bob
    let (status, body) = send(
        app.clone(),
        rq(
            "PUT",
            &format!("/users/@me/notes/{bob_id}"),
            Some(json!({"note": "pote"})),
            Some(&alice_tok),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "PUT note sur bob : {body}");

    // Alice lit la note
    let (status, body) = send(
        app.clone(),
        rq(
            "GET",
            &format!("/users/@me/notes/{bob_id}"),
            None,
            Some(&alice_tok),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "GET note sur bob");
    assert_eq!(
        body["note"].as_str(),
        Some("pote"),
        "la note doit valoir 'pote'"
    );
}

/// Blocage : alice bloque bob ; alice voit "blocked", bob ne voit plus alice.
#[tokio::test]
async fn block_relationship() {
    let app = app().await;
    let alice_tok = register(&app, "alice", "alice@block.test").await;
    let bob_tok = register(&app, "bob", "bob@block.test").await;
    let bob_id = uid(&app, &bob_tok).await;

    // D'abord une demande d'ami pour qu'il y ait une relation
    let _ = send(
        app.clone(),
        rq(
            "POST",
            "/users/@me/relationships",
            Some(json!({"username": "bob"})),
            Some(&alice_tok),
        ),
    )
    .await;

    // Alice bloque Bob
    let (status, body) = send(
        app.clone(),
        rq(
            "POST",
            "/users/@me/relationships",
            Some(json!({"username": "bob", "block": true})),
            Some(&alice_tok),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "alice bloque bob : {body}");

    // Alice voit bob en "blocked"
    let (status, alice_rels) = send(
        app.clone(),
        rq("GET", "/users/@me/relationships", None, Some(&alice_tok)),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "GET relations d'alice après blocage"
    );
    let entry = find_by_user(&alice_rels, "bob")
        .expect("bob doit apparaître dans les relations d'alice après blocage");
    assert_eq!(
        entry["type"].as_str(),
        Some("blocked"),
        "alice doit voir bob en 'blocked'"
    );

    // Bob ne voit plus alice dans ses relations
    let (status, bob_rels) = send(
        app.clone(),
        rq("GET", "/users/@me/relationships", None, Some(&bob_tok)),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "GET relations de bob après avoir été bloqué"
    );
    let entry = find_by_user(&bob_rels, "alice");
    assert!(
        entry.is_none(),
        "bob ne doit plus voir alice dans ses relations après avoir été bloqué"
    );

    // Suppression du blocage
    let (status, _) = send(
        app.clone(),
        rq(
            "DELETE",
            &format!("/users/@me/relationships/{bob_id}"),
            None,
            Some(&alice_tok),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "alice supprime le blocage");

    // Alice ne voit plus bob
    let (status, alice_rels) = send(
        app.clone(),
        rq("GET", "/users/@me/relationships", None, Some(&alice_tok)),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "GET relations d'alice après suppression du blocage"
    );
    let entry = find_by_user(&alice_rels, "bob");
    assert!(
        entry.is_none(),
        "alice ne doit plus voir bob après suppression du blocage"
    );
}

/// Suppression d'une demande en cours retire la relation dans les deux sens.
#[tokio::test]
async fn cancel_friend_request() {
    let app = app().await;
    let alice_tok = register(&app, "alice", "alice@cancel.test").await;
    let bob_tok = register(&app, "bob", "bob@cancel.test").await;
    let bob_id = uid(&app, &bob_tok).await;

    // Alice envoie la demande
    let (status, _) = send(
        app.clone(),
        rq(
            "POST",
            "/users/@me/relationships",
            Some(json!({"username": "bob"})),
            Some(&alice_tok),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "alice envoie la demande");

    // Alice annule
    let (status, _) = send(
        app.clone(),
        rq(
            "DELETE",
            &format!("/users/@me/relationships/{bob_id}"),
            None,
            Some(&alice_tok),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "alice annule la demande");

    // Alice et Bob ont une liste vide
    let (_, alice_rels) = send(
        app.clone(),
        rq("GET", "/users/@me/relationships", None, Some(&alice_tok)),
    )
    .await;
    assert!(
        find_by_user(&alice_rels, "bob").is_none(),
        "alice ne doit plus voir bob après annulation"
    );

    let (_, bob_rels) = send(
        app.clone(),
        rq("GET", "/users/@me/relationships", None, Some(&bob_tok)),
    )
    .await;
    assert!(
        find_by_user(&bob_rels, "alice").is_none(),
        "bob ne doit plus voir alice après annulation de la demande"
    );
}
