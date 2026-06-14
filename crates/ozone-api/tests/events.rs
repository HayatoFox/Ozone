//! Tests d'intégration : événements programmés (cycle de vie externe, RSVP, validation).

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
    let path = std::env::temp_dir().join(format!("ozone-test-events-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Events".into(),
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
        Some(json!({"username":u,"email":e,"password":"Sup3r-Ozone-Pw"})),
        None,
    )
    .await
    .1["access_token"]
        .as_str()
        .unwrap()
        .to_string()
}

// Instant fixe et lointain dans le futur (2100-01-01) pour le déterminisme.
const FAR_FUTURE: i64 = 4_102_444_800_000;

/// Crée une guilde et renvoie son `id` (l'inscrit en devient propriétaire).
async fn make_guild(app: &Router, owner: &str) -> String {
    let (st, guild) = send(
        app,
        "POST",
        "/guilds",
        Some(json!({"name": "G"})),
        Some(owner),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "création de la guilde");
    guild["id"]
        .as_str()
        .expect("id de guilde absent")
        .to_string()
}

/// Cherche un élément d'un tableau JSON par son champ `id`.
fn find_by_id<'a>(arr: &'a Value, id: &str) -> Option<&'a Value> {
    arr.as_array()
        .and_then(|a| a.iter().find(|v| v["id"].as_str() == Some(id)))
}

// ---------------------------------------------------------------------------
// Test 1 : cycle de vie complet d'un événement externe + RSVP à deux
// ---------------------------------------------------------------------------
#[tokio::test]
async fn external_event_lifecycle() {
    let app = app().await;
    let alice = token(&app, "alice", "alice@x.fr").await;
    let gid = make_guild(&app, &alice).await;

    // --- CREATE (externe) ---
    let (st, ev) = send(
        &app,
        "POST",
        &format!("/guilds/{gid}/events"),
        Some(json!({
            "name": "Soirée jeux",
            "entity_type": 3,
            "location": "https://meet.example/x",
            "scheduled_start": FAR_FUTURE
        })),
        Some(&alice),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "création de l'événement externe");
    let eid = ev["id"]
        .as_str()
        .expect("id d'événement absent")
        .to_string();
    assert_eq!(ev["entity_type"].as_u64(), Some(3), "entity_type == 3");
    assert_eq!(
        ev["location"].as_str(),
        Some("https://meet.example/x"),
        "lieu de l'événement"
    );
    assert!(
        ev["channel_id"].is_null(),
        "channel_id doit être null pour un externe"
    );
    assert_eq!(
        ev["status"].as_u64(),
        Some(1),
        "statut initial == programmé (1)"
    );
    assert_eq!(
        ev["interested_count"].as_i64(),
        Some(0),
        "0 intéressé à la création"
    );

    // --- LIST (présence + compteur) ---
    let (st, list) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/events"),
        None,
        Some(&alice),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "liste des événements");
    let listed = find_by_id(&list, &eid).expect("l'événement doit figurer dans la liste");
    assert_eq!(
        listed["interested_count"].as_i64(),
        Some(0),
        "compteur d'intéressés == 0 dans la liste"
    );

    // --- GET (détail) ---
    let (st, single) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/events/{eid}"),
        None,
        Some(&alice),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "détail de l'événement");
    assert_eq!(single["id"].as_str(), Some(eid.as_str()), "id du détail");

    // --- RSVP (Alice) → 1 ---
    let (st, r) = send(
        &app,
        "PUT",
        &format!("/guilds/{gid}/events/{eid}/interested"),
        None,
        Some(&alice),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "RSVP d'Alice");
    assert_eq!(
        r["interested_count"].as_i64(),
        Some(1),
        "1 intéressé après RSVP d'Alice"
    );

    // --- 2e utilisateur : inscription, invitation + jonction ---
    let bob = token(&app, "bob", "bob@x.fr").await;
    let (st, inv) = send(
        &app,
        "POST",
        &format!("/guilds/{gid}/invites"),
        Some(json!({})),
        Some(&alice),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "création de l'invitation");
    let code = inv["code"]
        .as_str()
        .expect("code d'invitation absent")
        .to_string();
    let (st, _) = send(&app, "POST", &format!("/invites/{code}"), None, Some(&bob)).await;
    assert_eq!(st, StatusCode::OK, "Bob rejoint la guilde");

    // --- RSVP (Bob) → 2 ---
    let (st, r) = send(
        &app,
        "PUT",
        &format!("/guilds/{gid}/events/{eid}/interested"),
        None,
        Some(&bob),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "RSVP de Bob");
    assert_eq!(
        r["interested_count"].as_i64(),
        Some(2),
        "2 intéressés après RSVP de Bob"
    );

    // --- unRSVP (Bob) → 1 ---
    let (st, r) = send(
        &app,
        "DELETE",
        &format!("/guilds/{gid}/events/{eid}/interested"),
        None,
        Some(&bob),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "retrait d'intérêt de Bob");
    assert_eq!(
        r["interested_count"].as_i64(),
        Some(1),
        "retour à 1 intéressé après retrait de Bob"
    );

    // --- PATCH (nom + statut actif) par Alice ---
    let (st, patched) = send(
        &app,
        "PATCH",
        &format!("/guilds/{gid}/events/{eid}"),
        Some(json!({"name": "Soirée jeux (en cours)", "status": 2})),
        Some(&alice),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "mise à jour de l'événement");
    assert_eq!(
        patched["name"].as_str(),
        Some("Soirée jeux (en cours)"),
        "le nom doit être mis à jour"
    );
    assert_eq!(
        patched["status"].as_u64(),
        Some(2),
        "le statut doit passer à actif (2)"
    );

    // --- DELETE ---
    let (st, del) = send(
        &app,
        "DELETE",
        &format!("/guilds/{gid}/events/{eid}"),
        None,
        Some(&alice),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "suppression de l'événement");
    assert_eq!(
        del["ok"].as_bool(),
        Some(true),
        "la réponse doit contenir ok:true"
    );

    // --- GET après suppression → 404 ---
    let (st, _) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/events/{eid}"),
        None,
        Some(&alice),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::NOT_FOUND,
        "l'événement supprimé doit renvoyer 404"
    );
}

// ---------------------------------------------------------------------------
// Test 2 : validation du salon (doit être dans la guilde) + cas d'erreur
// ---------------------------------------------------------------------------
#[tokio::test]
async fn channel_event_must_be_in_guild() {
    let app = app().await;
    let alice = token(&app, "alice", "alice@x.fr").await;
    let gid = make_guild(&app, &alice).await;

    // Salon "général" par défaut de la guilde.
    let (st, channels) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/channels"),
        None,
        Some(&alice),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "liste des salons");
    let chan_id = channels
        .as_array()
        .and_then(|a| a.first())
        .and_then(|c| c["id"].as_str())
        .expect("salon général absent")
        .to_string();

    // --- Vocal (2) référençant un salon de la guilde → 200 ---
    let (st, ev) = send(
        &app,
        "POST",
        &format!("/guilds/{gid}/events"),
        Some(json!({
            "name": "Vocal du soir",
            "entity_type": 2,
            "channel_id": chan_id,
            "scheduled_start": FAR_FUTURE
        })),
        Some(&alice),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::OK,
        "événement vocal sur un salon de la guilde accepté"
    );
    assert_eq!(
        ev["channel_id"].as_str(),
        Some(chan_id.as_str()),
        "channel_id de l'événement vocal"
    );
    assert!(
        ev["location"].is_null(),
        "location doit être null pour un événement de salon"
    );

    // --- Vocal (2) référençant un salon inexistant → 400 ---
    let (st, _) = send(
        &app,
        "POST",
        &format!("/guilds/{gid}/events"),
        Some(json!({
            "name": "Vocal fantôme",
            "entity_type": 2,
            "channel_id": "999999",
            "scheduled_start": FAR_FUTURE
        })),
        Some(&alice),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::BAD_REQUEST,
        "salon hors guilde doit être rejeté (400)"
    );

    // --- Externe (3) sans lieu → 400 ---
    let (st, _) = send(
        &app,
        "POST",
        &format!("/guilds/{gid}/events"),
        Some(json!({
            "name": "Externe sans lieu",
            "entity_type": 3,
            "scheduled_start": FAR_FUTURE
        })),
        Some(&alice),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::BAD_REQUEST,
        "événement externe sans lieu doit être rejeté (400)"
    );

    // --- Type d'entité invalide (9) → 400 ---
    let (st, _) = send(
        &app,
        "POST",
        &format!("/guilds/{gid}/events"),
        Some(json!({
            "name": "Type invalide",
            "entity_type": 9,
            "location": "https://meet.example/y",
            "scheduled_start": FAR_FUTURE
        })),
        Some(&alice),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::BAD_REQUEST,
        "type d'entité invalide doit être rejeté (400)"
    );
}
