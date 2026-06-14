//! Tests de sécurité / adversarial pour les relations (S4).

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
    let path = std::env::temp_dir().join(format!("ozone-test-secs4-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Ozone SecS4".into(),
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
// Tests adversarial
// ─────────────────────────────────────────────────────────────────────────────

/// Demande d'ami à soi-même → 400.
#[tokio::test]
async fn self_friend_request_rejected() {
    let app = app().await;
    let alice_tok = register(&app, "alice", "alice@secs4a.test").await;

    let (status, body) = send(
        app.clone(),
        rq(
            "POST",
            "/users/@me/relationships",
            Some(json!({"username": "alice"})),
            Some(&alice_tok),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "une demande d'ami à soi-même doit retourner 400 : {body}"
    );
}

/// Demande d'ami à quelqu'un qui vous a bloqué → 403.
#[tokio::test]
async fn friend_request_to_blocker_rejected() {
    let app = app().await;
    let alice_tok = register(&app, "alice", "alice@secs4b.test").await;
    let bob_tok = register(&app, "bob", "bob@secs4b.test").await;

    // Bob bloque Alice
    let (status, body) = send(
        app.clone(),
        rq(
            "POST",
            "/users/@me/relationships",
            Some(json!({"username": "alice", "block": true})),
            Some(&bob_tok),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "bob bloque alice : {body}");

    // Alice essaie d'envoyer une demande à Bob
    let (status, body) = send(
        app.clone(),
        rq(
            "POST",
            "/users/@me/relationships",
            Some(json!({"username": "bob"})),
            Some(&alice_tok),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "demande d'ami vers quelqu'un qui vous a bloqué doit retourner 403 : {body}"
    );
}

/// Demande d'ami vers un pseudo inconnu → 404.
#[tokio::test]
async fn friend_request_unknown_user() {
    let app = app().await;
    let alice_tok = register(&app, "alice", "alice@secs4c.test").await;

    let (status, body) = send(
        app.clone(),
        rq(
            "POST",
            "/users/@me/relationships",
            Some(json!({"username": "utilisateur_inexistant_xyz"})),
            Some(&alice_tok),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "demande d'ami vers un pseudo inexistant doit retourner 404 : {body}"
    );
}

/// Note trop longue (> 256 caractères) → 400.
#[tokio::test]
async fn note_too_long_rejected() {
    let app = app().await;
    let alice_tok = register(&app, "alice", "alice@secs4d.test").await;
    let bob_tok = register(&app, "bob", "bob@secs4d.test").await;
    let bob_id = uid(&app, &bob_tok).await;

    let long_note = "a".repeat(300);

    let (status, body) = send(
        app.clone(),
        rq(
            "PUT",
            &format!("/users/@me/notes/{bob_id}"),
            Some(json!({"note": long_note})),
            Some(&alice_tok),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "une note de 300 caractères doit retourner 400 : {body}"
    );
}

/// Isolation : carol ne voit QUE ses propres relations (liste vide) même si alice et bob sont amis.
#[tokio::test]
async fn relationships_isolation() {
    let app = app().await;
    let alice_tok = register(&app, "alice", "alice@secs4e.test").await;
    let bob_tok = register(&app, "bob", "bob@secs4e.test").await;
    let carol_tok = register(&app, "carol", "carol@secs4e.test").await;
    let alice_id = uid(&app, &alice_tok).await;

    // Alice envoie une demande à Bob
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
    assert_eq!(status, StatusCode::OK, "alice envoie la demande à bob");

    // Bob accepte
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
    assert_eq!(status, StatusCode::OK, "bob accepte");

    // Carol liste ses relations → doit être vide
    let (status, carol_rels) = send(
        app.clone(),
        rq("GET", "/users/@me/relationships", None, Some(&carol_tok)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "GET relations de carol");
    let arr = carol_rels
        .as_array()
        .expect("la réponse de relations doit être un tableau JSON");
    assert!(
        arr.is_empty(),
        "carol ne doit voir aucune relation (isolation) ; reçu : {carol_rels}"
    );

    // Vérification supplémentaire : carol ne voit ni alice ni bob
    assert!(
        find_by_user(&carol_rels, "alice").is_none(),
        "carol ne doit pas voir alice dans ses relations"
    );
    assert!(
        find_by_user(&carol_rels, "bob").is_none(),
        "carol ne doit pas voir bob dans ses relations"
    );
}

/// Durcissement : auto-relation par identifiant ou cible inexistante → refusé.
#[tokio::test]
async fn cannot_self_relate_or_target_ghost_via_id() {
    let app = app().await;
    let alice_tok = register(&app, "alice", "alice@secs4f.test").await;
    let alice_id = uid(&app, &alice_tok).await;

    // s'ajouter soi-même par identifiant → 400
    let (s, _) = send(
        app.clone(),
        rq(
            "PUT",
            &format!("/users/@me/relationships/{alice_id}"),
            None,
            Some(&alice_tok),
        ),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::BAD_REQUEST,
        "auto-relation par identifiant refusée"
    );

    // cibler un utilisateur inexistant par identifiant → 404
    let (s, _) = send(
        app.clone(),
        rq(
            "PUT",
            "/users/@me/relationships/999999999",
            None,
            Some(&alice_tok),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "cible inexistante refusée");

    // aucune ligne parasite n'a été créée
    let (_, rels) = send(
        app.clone(),
        rq("GET", "/users/@me/relationships", None, Some(&alice_tok)),
    )
    .await;
    assert!(
        rels.as_array().unwrap().is_empty(),
        "aucune relation parasite ne doit subsister"
    );
}
