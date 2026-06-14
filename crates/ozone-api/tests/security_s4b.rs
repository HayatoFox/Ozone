//! Tests d'intégration de sécurité (adversarial) — messages privés & groupes (S4b).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use ozone_api::config::Config;
use ozone_api::{bootstrap_state, build_app};
use serde_json::{json, Value};
use tower::ServiceExt;

// ── Harnais ──────────────────────────────────────────────────────────────────

async fn app() -> Router {
    let unique = format!(
        "{}_{:?}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        std::thread::current().id()
    );
    let path = std::env::temp_dir().join(format!("ozone-test-secs4b-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Ozone SecS4b".into(),
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
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
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
    assert_eq!(status, StatusCode::OK, "register {username}");
    body["access_token"].as_str().unwrap().to_string()
}

async fn uid(app: &Router, token: &str) -> String {
    let (_, body) = send(app, rq("GET", "/users/@me", None, Some(token))).await;
    body["id"].as_str().unwrap().to_string()
}

// ── Tests adversariaux ────────────────────────────────────────────────────────

/// Invariant 1 — Un tiers non destinataire ne peut ni lire ni envoyer dans un MP.
#[tokio::test]
async fn non_recipient_cannot_read_or_send() {
    let app = app().await;

    let alice = register(&app, "alice", "alice@s4b.fr").await;
    let bob = register(&app, "bob", "bob@s4b.fr").await;
    let mallory = register(&app, "mallory", "mallory@s4b.fr").await;

    let bob_id = uid(&app, &bob).await;

    // Alice ouvre un MP avec Bob.
    let (status, dm) = send(
        &app,
        rq(
            "POST",
            "/users/@me/channels",
            Some(json!({"recipients": [bob_id]})),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "alice doit pouvoir ouvrir un MP avec bob"
    );
    let dm_id = dm["id"].as_str().unwrap().to_string();

    // Mallory tente de lire les messages du MP → doit être refusé.
    let (status, _) = send(
        &app,
        rq(
            "GET",
            &format!("/channels/{dm_id}/messages"),
            None,
            Some(&mallory),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "mallory (non destinataire) ne doit pas pouvoir lire les messages du MP"
    );

    // Mallory tente d'envoyer un message dans le MP → doit être refusé.
    let (status, _) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{dm_id}/messages"),
            Some(json!({"content": "intrusion"})),
            Some(&mallory),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "mallory (non destinataire) ne doit pas pouvoir envoyer un message dans le MP"
    );
}

/// Invariant 2 — Un tiers hors groupe ne peut pas ajouter quelqu'un au groupe.
#[tokio::test]
async fn non_recipient_cannot_add_to_group() {
    let app = app().await;

    let alice = register(&app, "alice", "alice@s4b2.fr").await;
    let bob = register(&app, "bob", "bob@s4b2.fr").await;
    let carol = register(&app, "carol", "carol@s4b2.fr").await;
    let mallory = register(&app, "mallory", "mallory@s4b2.fr").await;

    let bob_id = uid(&app, &bob).await;
    let carol_id = uid(&app, &carol).await;
    let mallory_id = uid(&app, &mallory).await;

    // Alice crée un groupe avec Bob.
    let (status, group) = send(
        &app,
        rq(
            "POST",
            "/users/@me/channels",
            Some(json!({"recipients": [bob_id, carol_id]})),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "alice doit pouvoir créer un groupe avec bob et carol"
    );
    let group_id = group["id"].as_str().unwrap().to_string();

    // Mallory (hors groupe) tente d'ajouter quelqu'un au groupe → doit être refusé.
    let (status, _) = send(
        &app,
        rq(
            "PUT",
            &format!("/channels/{group_id}/recipients/{mallory_id}"),
            None,
            Some(&mallory),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "mallory (hors groupe) ne doit pas pouvoir s'ajouter lui-même au groupe"
    );
}

/// Invariant 3 — Un membre non propriétaire ne peut pas retirer un autre membre,
///               mais peut se retirer lui-même.
#[tokio::test]
async fn non_owner_cannot_remove_other_member() {
    let app = app().await;

    let alice = register(&app, "alice", "alice@s4b3.fr").await;
    let bob = register(&app, "bob", "bob@s4b3.fr").await;
    let carol = register(&app, "carol", "carol@s4b3.fr").await;

    let bob_id = uid(&app, &bob).await;
    let carol_id = uid(&app, &carol).await;

    // Alice crée un groupe avec Bob et Carol.
    let (status, group) = send(
        &app,
        rq(
            "POST",
            "/users/@me/channels",
            Some(json!({"recipients": [bob_id, carol_id]})),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "alice doit pouvoir créer un groupe avec bob et carol"
    );
    let group_id = group["id"].as_str().unwrap().to_string();

    // Bob (membre, non propriétaire) tente de retirer Carol → doit être refusé.
    let (status, _) = send(
        &app,
        rq(
            "DELETE",
            &format!("/channels/{group_id}/recipients/{carol_id}"),
            None,
            Some(&bob),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "bob (non propriétaire) ne doit pas pouvoir retirer carol du groupe"
    );

    // Bob peut se retirer lui-même → doit être accepté.
    let (status, _) = send(
        &app,
        rq(
            "DELETE",
            &format!("/channels/{group_id}/recipients/{bob_id}"),
            None,
            Some(&bob),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "bob doit pouvoir quitter le groupe lui-même"
    );
}

/// Invariant 4 — Un utilisateur bloqué ne peut pas envoyer de message dans le MP.
#[tokio::test]
async fn blocked_user_cannot_send_in_dm() {
    let app = app().await;

    let alice = register(&app, "alice", "alice@s4b4.fr").await;
    let bob = register(&app, "bob", "bob@s4b4.fr").await;

    let bob_id = uid(&app, &bob).await;

    // Alice ouvre (ou s'assure d'avoir) le MP avec Bob avant le blocage.
    let (status, dm) = send(
        &app,
        rq(
            "POST",
            "/users/@me/channels",
            Some(json!({"recipients": [bob_id]})),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "l'ouverture du MP doit réussir avant le blocage"
    );
    let dm_id = dm["id"].as_str().unwrap().to_string();

    // Bob bloque Alice.
    let (status, _) = send(
        &app,
        rq(
            "POST",
            "/users/@me/relationships",
            Some(json!({"username": "alice", "block": true})),
            Some(&bob),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "bob doit pouvoir bloquer alice");

    // Alice ré-ouvre/obtient le MP → le canal doit toujours exister (200).
    let (status, dm2) = send(
        &app,
        rq(
            "POST",
            "/users/@me/channels",
            Some(json!({"recipients": [bob_id]})),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "le MP doit toujours exister après le blocage"
    );
    let dm_id2 = dm2["id"].as_str().unwrap().to_string();
    assert_eq!(
        dm_id, dm_id2,
        "l'id du MP ne doit pas changer après le blocage"
    );

    // Alice tente d'envoyer un message dans le MP → doit être refusé (blocage actif).
    let (status, _) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{dm_id}/messages"),
            Some(json!({"content": "tu me réponds ?"})),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "alice (bloquée par bob) ne doit pas pouvoir envoyer un message dans le MP"
    );
}

/// Invariant 5 — L'isolation de la liste des DM : un tiers ne voit pas les MP des autres.
#[tokio::test]
async fn dm_listing_isolation() {
    let app = app().await;

    let alice = register(&app, "alice", "alice@s4b5.fr").await;
    let bob = register(&app, "bob", "bob@s4b5.fr").await;
    let mallory = register(&app, "mallory", "mallory@s4b5.fr").await;

    let bob_id = uid(&app, &bob).await;

    // Alice ouvre un MP avec Bob.
    let (status, _) = send(
        &app,
        rq(
            "POST",
            "/users/@me/channels",
            Some(json!({"recipients": [bob_id]})),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "alice doit pouvoir ouvrir un MP avec bob"
    );

    // Mallory récupère SA liste de canaux → doit être vide.
    let (status, channels) =
        send(&app, rq("GET", "/users/@me/channels", None, Some(&mallory))).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "la récupération de la liste de canaux de mallory doit réussir"
    );
    let list = channels
        .as_array()
        .expect("la réponse doit être un tableau JSON");
    assert!(
        list.is_empty(),
        "mallory ne doit voir aucun canal dans sa liste (isolement) — trouvé : {list:?}"
    );
}

/// Durcissement (revue mainteneur) — IDOR sur la lecture du canal et immuabilité d'un MP 1:1.
#[tokio::test]
async fn dm_channel_idor_and_one_to_one_is_immutable() {
    let app = app().await;

    let alice = register(&app, "alice", "alice@s4b6.fr").await;
    let bob = register(&app, "bob", "bob@s4b6.fr").await;
    let mallory = register(&app, "mallory", "mallory@s4b6.fr").await;
    let carol = register(&app, "carol", "carol@s4b6.fr").await;

    let bob_id = uid(&app, &bob).await;
    let carol_id = uid(&app, &carol).await;

    let (status, dm) = send(
        &app,
        rq(
            "POST",
            "/users/@me/channels",
            Some(json!({"recipients": [bob_id]})),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let dm_id = dm["id"].as_str().unwrap().to_string();

    // Un tiers ne peut pas lire le canal de MP (IDOR sur GET /channels/:id).
    let (status, _) = send(
        &app,
        rq("GET", &format!("/channels/{dm_id}"), None, Some(&mallory)),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "un non-destinataire ne doit pas lire le canal de MP"
    );

    // On ne peut pas ajouter un membre à un MP 1:1 (réservé aux groupes).
    let (status, _) = send(
        &app,
        rq(
            "PUT",
            &format!("/channels/{dm_id}/recipients/{carol_id}"),
            None,
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "ajout de membre interdit sur un MP 1:1"
    );
}
