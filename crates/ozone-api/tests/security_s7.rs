//! Vérification cybersécurité S7 : webhooks (autorisation + jeton), recherche
//! (filtrage par permissions, confidentialité inter-salons), événements (autorisation, isolation).

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
    let path = std::env::temp_dir().join(format!("ozone-test-secs7-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "SecS7".into(),
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
async fn uid(app: &Router, t: &str) -> String {
    send(app, "GET", "/users/@me", None, Some(t)).await.1["id"]
        .as_str()
        .unwrap()
        .to_string()
}
async fn first_channel(app: &Router, gid: &str, tok: &str) -> String {
    let chans = send(
        app,
        "GET",
        &format!("/guilds/{gid}/channels"),
        None,
        Some(tok),
    )
    .await
    .1;
    chans[0]["id"].as_str().unwrap().to_string()
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

// ───────────────────────────── Webhooks ─────────────────────────────

#[tokio::test]
async fn webhook_authorization() {
    let app = app().await;
    let owner = token(&app, "alice", "a@s7.fr").await;
    let bob = token(&app, "bob", "b@s7.fr").await;
    let bob_id = uid(&app, &bob).await;
    let g = send(
        &app,
        "POST",
        "/guilds",
        Some(json!({"name":"G"})),
        Some(&owner),
    )
    .await
    .1;
    let gid = g["id"].as_str().unwrap().to_string();
    let cid = first_channel(&app, &gid, &owner).await;
    join(&app, &owner, &gid, &bob).await;

    // Un membre sans MANAGE_WEBHOOKS ne peut ni créer ni lister.
    let (s, _) = send(
        &app,
        "POST",
        &format!("/channels/{cid}/webhooks"),
        Some(json!({"name":"X"})),
        Some(&bob),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = send(
        &app,
        "GET",
        &format!("/channels/{cid}/webhooks"),
        None,
        Some(&bob),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/webhooks"),
        None,
        Some(&bob),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Le propriétaire crée un webhook.
    let (_s, wh) = send(
        &app,
        "POST",
        &format!("/channels/{cid}/webhooks"),
        Some(json!({"name":"R"})),
        Some(&owner),
    )
    .await;
    let wid = wh["id"].as_str().unwrap().to_string();

    // bob ne peut ni modifier, ni supprimer, ni régénérer.
    for (m, body) in [
        ("PATCH", Some(json!({"name":"z"}))),
        ("DELETE", None),
        ("POST", None),
    ] {
        let (s, _) = send(&app, m, &format!("/webhooks/{wid}"), body, Some(&bob)).await;
        assert_eq!(
            s,
            StatusCode::FORBIDDEN,
            "{m} /webhooks/:id réservé à MANAGE_WEBHOOKS"
        );
    }

    // Exécution : mauvais jeton et identifiant inconnu → 401 uniforme.
    let (s, _) = send(
        &app,
        "POST",
        &format!("/webhooks/{wid}/MAUVAISJETON"),
        Some(json!({"content":"x"})),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "jeton invalide");
    let (s, _) = send(
        &app,
        "POST",
        "/webhooks/999999/quelconque",
        Some(json!({"content":"x"})),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "webhook inconnu");

    // Pas de webhook en message privé.
    let dm = send(
        &app,
        "POST",
        "/users/@me/channels",
        Some(json!({"recipients":[bob_id]})),
        Some(&owner),
    )
    .await
    .1;
    let dmid = dm["id"].as_str().unwrap().to_string();
    let (s, _) = send(
        &app,
        "POST",
        &format!("/channels/{dmid}/webhooks"),
        Some(json!({"name":"X"})),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "webhooks indisponibles en MP");
}

// ───────────────────────────── Recherche ─────────────────────────────

#[tokio::test]
async fn search_respects_permissions() {
    let app = app().await;
    let owner = token(&app, "alice", "a@s7s.fr").await;
    let bob = token(&app, "bob", "b@s7s.fr").await;
    let carol = token(&app, "carol", "c@s7s.fr").await; // jamais membre
    let g = send(
        &app,
        "POST",
        "/guilds",
        Some(json!({"name":"G"})),
        Some(&owner),
    )
    .await
    .1;
    let gid = g["id"].as_str().unwrap().to_string();
    let general = first_channel(&app, &gid, &owner).await;
    join(&app, &owner, &gid, &bob).await;

    // Salon privé : @everyone se voit refuser VIEW_CHANNEL (bit 1<<10 = 1024).
    let sec = send(
        &app,
        "POST",
        &format!("/guilds/{gid}/channels"),
        Some(json!({"name":"secret","type":0})),
        Some(&owner),
    )
    .await
    .1;
    let secret = sec["id"].as_str().unwrap().to_string();
    let (s, _) = send(
        &app,
        "PUT",
        &format!("/channels/{secret}/permissions/{gid}"),
        Some(json!({"type":0,"deny":"1024"})),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // Messages : un public, un secret (terme unique « topsecret »).
    send(
        &app,
        "POST",
        &format!("/channels/{general}/messages"),
        Some(json!({"content":"motpublic visible"})),
        Some(&owner),
    )
    .await;
    send(
        &app,
        "POST",
        &format!("/channels/{secret}/messages"),
        Some(json!({"content":"topsecret caché"})),
        Some(&owner),
    )
    .await;

    // Le propriétaire trouve le message secret ; un membre ordinaire (bob) ne le trouve PAS.
    let (_s, r) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/messages/search?q=topsecret"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(r["total"], 1, "le propriétaire voit le salon secret");
    let (_s, r) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/messages/search?q=topsecret"),
        None,
        Some(&bob),
    )
    .await;
    assert_eq!(
        r["total"], 0,
        "fuite inter-salons : bob ne doit rien trouver dans le salon secret"
    );
    // bob trouve quand même le message public.
    let (_s, r) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/messages/search?q=motpublic"),
        None,
        Some(&bob),
    )
    .await;
    assert_eq!(r["total"], 1);

    // Un non-membre ne peut pas chercher dans la guilde.
    let (s, _) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/messages/search?q=motpublic"),
        None,
        Some(&carol),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "recherche réservée aux membres");

    // bob ne peut pas faire de recherche par salon sur un salon qu'il ne voit pas.
    let (s, _) = send(
        &app,
        "GET",
        &format!("/channels/{secret}/messages/search?q=topsecret"),
        None,
        Some(&bob),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "recherche par salon refusée sans VIEW_CHANNEL"
    );
}

// ───────────────────────────── Événements ─────────────────────────────

#[tokio::test]
async fn event_authorization_and_isolation() {
    let app = app().await;
    let owner = token(&app, "alice", "a@s7e.fr").await;
    let bob = token(&app, "bob", "b@s7e.fr").await;
    let carol = token(&app, "carol", "c@s7e.fr").await; // non-membre
    let g = send(
        &app,
        "POST",
        "/guilds",
        Some(json!({"name":"A"})),
        Some(&owner),
    )
    .await
    .1;
    let gid = g["id"].as_str().unwrap().to_string();
    join(&app, &owner, &gid, &bob).await;
    let g2 = send(
        &app,
        "POST",
        "/guilds",
        Some(json!({"name":"B"})),
        Some(&owner),
    )
    .await
    .1;
    let gid2 = g2["id"].as_str().unwrap().to_string();

    let evt = json!({"name":"Conf","entity_type":3,"location":"https://x.example","scheduled_start":4102444800000i64});

    // Un membre sans CREATE_EVENTS ne peut pas créer.
    let (s, _) = send(
        &app,
        "POST",
        &format!("/guilds/{gid}/events"),
        Some(evt.clone()),
        Some(&bob),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "création réservée à CREATE_EVENTS"
    );

    // Un non-membre ne peut pas lister.
    let (s, _) = send(
        &app,
        "GET",
        &format!("/guilds/{gid}/events"),
        None,
        Some(&carol),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Le propriétaire crée l'événement.
    let (s, ev) = send(
        &app,
        "POST",
        &format!("/guilds/{gid}/events"),
        Some(evt.clone()),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let eid = ev["id"].as_str().unwrap().to_string();

    // bob (ni créateur ni MANAGE_EVENTS) ne peut ni modifier ni supprimer.
    let (s, _) = send(
        &app,
        "PATCH",
        &format!("/guilds/{gid}/events/{eid}"),
        Some(json!({"name":"z"})),
        Some(&bob),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "gestion réservée au créateur / MANAGE_EVENTS"
    );
    let (s, _) = send(
        &app,
        "DELETE",
        &format!("/guilds/{gid}/events/{eid}"),
        None,
        Some(&bob),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Isolation inter-guildes : l'événement de A est invisible / non gérable via B.
    let (s, _) = send(
        &app,
        "GET",
        &format!("/guilds/{gid2}/events/{eid}"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::NOT_FOUND,
        "un événement d'une autre guilde n'est pas accessible"
    );
    let (s, _) = send(
        &app,
        "PATCH",
        &format!("/guilds/{gid2}/events/{eid}"),
        Some(json!({"name":"z"})),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}
