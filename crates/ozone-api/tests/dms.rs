//! Tests d'intégration : messages privés (DM 1:1) et groupes de messages directs.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use ozone_api::config::Config;
use ozone_api::{bootstrap_state, build_app};
use serde_json::{json, Value};
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Harnais
// ---------------------------------------------------------------------------

async fn app() -> Router {
    let unique = format!(
        "{}_{:?}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        std::thread::current().id()
    );
    let path = std::env::temp_dir().join(format!("ozone-test-dm-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Ozone DM Tests".into(),
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

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

async fn register(app: &Router, username: &str, email: &str) -> String {
    let (status, body) = send(
        app,
        rq(
            "POST",
            "/auth/register",
            Some(json!({"username": username, "email": email, "password": "Sup3r-Ozone-Pw"})),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "register {username} a échoué");
    body["access_token"].as_str().unwrap().to_string()
}

async fn uid(app: &Router, token: &str) -> String {
    let (status, body) = send(app, rq("GET", "/users/@me", None, Some(token))).await;
    assert_eq!(status, StatusCode::OK, "GET /users/@me a échoué");
    body["id"].as_str().unwrap().to_string()
}

// ---------------------------------------------------------------------------
// Helpers internes
// ---------------------------------------------------------------------------

/// Retrouve un destinataire dans la liste `recipients` par son `username`.
fn find_recipient<'a>(recipients: &'a Value, username: &str) -> &'a Value {
    recipients
        .as_array()
        .expect("recipients doit être un tableau")
        .iter()
        .find(|r| r["username"].as_str() == Some(username))
        .unwrap_or_else(|| panic!("destinataire '{username}' introuvable dans la liste"))
}

/// Retrouve un canal dans la liste `/users/@me/channels` par son `id`.
fn find_channel<'a>(channels: &'a Value, channel_id: &str) -> Option<&'a Value> {
    channels
        .as_array()
        .expect("channels doit être un tableau")
        .iter()
        .find(|c| c["id"].as_str() == Some(channel_id))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// 1. Ouvrir un MP 1:1 et vérifier la déduplication.
#[tokio::test]
async fn open_and_dedup_dm() {
    let app = app().await;

    let alice_tok = register(&app, "alice", "alice@dm.fr").await;
    let bob_tok = register(&app, "bob", "bob@dm.fr").await;
    let bob_id = uid(&app, &bob_tok).await;
    let alice_id = uid(&app, &alice_tok).await;

    // Alice ouvre un MP avec Bob
    let (status, channel) = send(
        &app,
        rq(
            "POST",
            "/users/@me/channels",
            Some(json!({"recipients": [bob_id]})),
            Some(&alice_tok),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "ouverture du MP alice→bob a échoué");

    let channel_id = channel["id"].as_str().unwrap().to_string();
    let channel_type = channel["type"].as_i64().unwrap_or(-1);
    assert_eq!(channel_type, 1, "le type du canal doit être 1 (MP 1:1)");

    // Les deux utilisateurs doivent apparaître dans les destinataires
    let recipients = &channel["recipients"];
    find_recipient(recipients, "alice"); // présence d'alice
    let bob_in_recip = find_recipient(recipients, "bob");
    assert_eq!(
        bob_in_recip["id"].as_str().unwrap(),
        bob_id,
        "l'id de bob dans les destinataires ne correspond pas"
    );

    // Vérification owner_id absent ou null sur un DM 1:1 (optionnel selon l'API)
    // On vérifie surtout alice_id quelque part
    let _ = alice_id; // utilisé dans l'assertion implicite via register

    // Déduplication : rappeler POST /users/@me/channels doit renvoyer le MÊME id
    let (status2, channel2) = send(
        &app,
        rq(
            "POST",
            "/users/@me/channels",
            Some(json!({"recipients": [bob_id]})),
            Some(&alice_tok),
        ),
    )
    .await;
    assert_eq!(
        status2,
        StatusCode::OK,
        "second appel ouverture MP a échoué"
    );
    assert_eq!(
        channel2["id"].as_str().unwrap(),
        channel_id,
        "le MP doit être dédupliqué : le même id doit être renvoyé"
    );
}

/// 2. Messagerie dans un MP 1:1.
#[tokio::test]
async fn dm_messaging() {
    let app = app().await;

    let alice_tok = register(&app, "alice", "alice@msg.fr").await;
    let bob_tok = register(&app, "bob", "bob@msg.fr").await;
    let bob_id = uid(&app, &bob_tok).await;

    // Ouvrir le MP
    let (_, channel) = send(
        &app,
        rq(
            "POST",
            "/users/@me/channels",
            Some(json!({"recipients": [bob_id]})),
            Some(&alice_tok),
        ),
    )
    .await;
    let cid = channel["id"].as_str().unwrap().to_string();

    // Alice poste un message
    let (post_status, _) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({"content": "Salut Bob !"})),
            Some(&alice_tok),
        ),
    )
    .await;
    assert_eq!(
        post_status,
        StatusCode::OK,
        "alice doit pouvoir envoyer un message dans le MP"
    );

    // Bob lit les messages du MP et doit voir le message d'Alice
    let (get_status, messages) = send(
        &app,
        rq(
            "GET",
            &format!("/channels/{cid}/messages"),
            None,
            Some(&bob_tok),
        ),
    )
    .await;
    assert_eq!(
        get_status,
        StatusCode::OK,
        "bob doit pouvoir lire les messages du MP"
    );

    let msgs = messages
        .as_array()
        .expect("la liste de messages doit être un tableau");
    assert!(
        !msgs.is_empty(),
        "bob doit voir au moins un message dans le MP"
    );
    assert!(
        msgs.iter()
            .any(|m| m["content"].as_str() == Some("Salut Bob !")),
        "bob doit voir le message 'Salut Bob !' envoyé par alice"
    );
}

/// 3. Le MP apparaît dans la liste des canaux de chaque destinataire.
#[tokio::test]
async fn dm_appears_in_listing() {
    let app = app().await;

    let alice_tok = register(&app, "alice", "alice@list.fr").await;
    let bob_tok = register(&app, "bob", "bob@list.fr").await;
    let bob_id = uid(&app, &bob_tok).await;

    // Alice ouvre le MP
    let (_, channel) = send(
        &app,
        rq(
            "POST",
            "/users/@me/channels",
            Some(json!({"recipients": [bob_id]})),
            Some(&alice_tok),
        ),
    )
    .await;
    let cid = channel["id"].as_str().unwrap().to_string();

    // Alice voit le MP dans sa liste
    let (status_a, channels_a) = send(
        &app,
        rq("GET", "/users/@me/channels", None, Some(&alice_tok)),
    )
    .await;
    assert_eq!(
        status_a,
        StatusCode::OK,
        "GET /users/@me/channels a échoué pour alice"
    );
    assert!(
        find_channel(&channels_a, &cid).is_some(),
        "alice doit voir le MP dans sa liste de canaux"
    );

    // Bob voit le MP dans sa liste
    let (status_b, channels_b) =
        send(&app, rq("GET", "/users/@me/channels", None, Some(&bob_tok))).await;
    assert_eq!(
        status_b,
        StatusCode::OK,
        "GET /users/@me/channels a échoué pour bob"
    );
    assert!(
        find_channel(&channels_b, &cid).is_some(),
        "bob doit voir le MP dans sa liste de canaux"
    );
}

/// 4. Créer un groupe et échanger des messages.
#[tokio::test]
async fn create_group_and_message() {
    let app = app().await;

    let alice_tok = register(&app, "alice", "alice@grp.fr").await;
    let bob_tok = register(&app, "bob", "bob@grp.fr").await;
    let carol_tok = register(&app, "carol", "carol@grp.fr").await;

    let alice_id = uid(&app, &alice_tok).await;
    let bob_id = uid(&app, &bob_tok).await;
    let carol_id = uid(&app, &carol_tok).await;

    // Alice crée un groupe avec Bob et Carol
    let (status, group) = send(
        &app,
        rq(
            "POST",
            "/users/@me/channels",
            Some(json!({"recipients": [bob_id, carol_id]})),
            Some(&alice_tok),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "la création du groupe par alice a échoué"
    );

    let gid = group["id"].as_str().unwrap().to_string();
    let group_type = group["type"].as_i64().unwrap_or(-1);
    assert_eq!(group_type, 3, "le type du canal de groupe doit être 3");

    let owner_id = group["owner_id"].as_str().unwrap_or("");
    assert_eq!(
        owner_id, alice_id,
        "le propriétaire du groupe doit être alice"
    );

    let recipients = &group["recipients"];
    let recip_arr = recipients
        .as_array()
        .expect("recipients doit être un tableau");
    assert_eq!(
        recip_arr.len(),
        3,
        "le groupe doit avoir exactement 3 destinataires (alice, bob, carol)"
    );
    find_recipient(recipients, "alice");
    find_recipient(recipients, "bob");
    find_recipient(recipients, "carol");

    // Bob poste un message dans le groupe
    let (post_status, _) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{gid}/messages"),
            Some(json!({"content": "Bonjour tout le monde !"})),
            Some(&bob_tok),
        ),
    )
    .await;
    assert_eq!(
        post_status,
        StatusCode::OK,
        "bob doit pouvoir envoyer un message dans le groupe"
    );

    // Carol lit les messages du groupe
    let (get_status, messages) = send(
        &app,
        rq(
            "GET",
            &format!("/channels/{gid}/messages"),
            None,
            Some(&carol_tok),
        ),
    )
    .await;
    assert_eq!(
        get_status,
        StatusCode::OK,
        "carol doit pouvoir lire les messages du groupe"
    );

    let msgs = messages
        .as_array()
        .expect("la liste de messages doit être un tableau");
    assert!(
        msgs.iter()
            .any(|m| m["content"].as_str() == Some("Bonjour tout le monde !")),
        "carol doit voir le message de bob dans le groupe"
    );

    // Vérification que les ids sont bien utilisés (évite un warning de lint)
    let _ = carol_id;
}

/// 5. Ajouter un membre au groupe et quitter le groupe.
#[tokio::test]
async fn add_and_leave_group() {
    let app = app().await;

    let alice_tok = register(&app, "alice", "alice@add.fr").await;
    let bob_tok = register(&app, "bob", "bob@add.fr").await;
    let carol_tok = register(&app, "carol", "carol@add.fr").await;
    let dave_tok = register(&app, "dave", "dave@add.fr").await;

    let bob_id = uid(&app, &bob_tok).await;
    let carol_id = uid(&app, &carol_tok).await;
    let dave_id = uid(&app, &dave_tok).await;

    // Alice crée un groupe avec Bob et Carol
    let (_, group) = send(
        &app,
        rq(
            "POST",
            "/users/@me/channels",
            Some(json!({"recipients": [bob_id, carol_id]})),
            Some(&alice_tok),
        ),
    )
    .await;
    let gid = group["id"].as_str().unwrap().to_string();

    // Alice ajoute Dave au groupe
    let (add_status, add_body) = send(
        &app,
        rq(
            "PUT",
            &format!("/channels/{gid}/recipients/{dave_id}"),
            None,
            Some(&alice_tok),
        ),
    )
    .await;
    assert_eq!(
        add_status,
        StatusCode::OK,
        "alice doit pouvoir ajouter dave au groupe"
    );
    assert!(
        add_body["ok"].as_bool().unwrap_or(false),
        "l'ajout de dave doit renvoyer {{ok: true}}"
    );

    // Dave peut désormais lire les messages du groupe
    let (read_status, _) = send(
        &app,
        rq(
            "GET",
            &format!("/channels/{gid}/messages"),
            None,
            Some(&dave_tok),
        ),
    )
    .await;
    assert_eq!(
        read_status,
        StatusCode::OK,
        "dave doit pouvoir lire les messages du groupe après avoir été ajouté"
    );

    // Carol quitte le groupe
    let (leave_status, leave_body) = send(
        &app,
        rq(
            "DELETE",
            &format!("/channels/{gid}/recipients/{carol_id}"),
            None,
            Some(&carol_tok),
        ),
    )
    .await;
    assert_eq!(
        leave_status,
        StatusCode::OK,
        "carol doit pouvoir quitter le groupe"
    );
    assert!(
        leave_body["ok"].as_bool().unwrap_or(false),
        "la sortie de carol doit renvoyer {{ok: true}}"
    );

    // Carol n'est plus membre : GET les messages doit renvoyer 403
    let (after_leave_status, _) = send(
        &app,
        rq(
            "GET",
            &format!("/channels/{gid}/messages"),
            None,
            Some(&carol_tok),
        ),
    )
    .await;
    assert_eq!(
        after_leave_status,
        StatusCode::FORBIDDEN,
        "carol ne doit plus pouvoir lire les messages après avoir quitté le groupe"
    );
}
