//! Tests d'intégration : emojis, stickers et sons de soundboard.

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
    let path = std::env::temp_dir().join(format!("ozone-test-expr-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Ozone Expr".into(),
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
            Some(json!({"username": username, "email": email, "password": "motdepasse"})),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "register {username}");
    body["access_token"].as_str().unwrap().to_string()
}

// ---------------------------------------------------------------------------
// Helper : trouve un élément dans un tableau JSON par valeur de champ `name`
// ---------------------------------------------------------------------------
fn find_by_name<'a>(arr: &'a Value, name: &str) -> Option<&'a Value> {
    arr.as_array()
        .and_then(|a| a.iter().find(|v| v["name"].as_str() == Some(name)))
}

// ---------------------------------------------------------------------------
// Test 1 : CRUD complet des emojis
// ---------------------------------------------------------------------------
#[tokio::test]
async fn emoji_crud() {
    let app = app().await;

    // Alice s'inscrit — premier utilisateur = propriétaire de l'instance
    let alice = register(&app, "alice", "alice@x.fr").await;

    // Création d'une guilde
    let (status, guild) = send(
        app.clone(),
        rq("POST", "/guilds", Some(json!({"name": "G"})), Some(&alice)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "création de la guilde");
    let gid = guild["id"]
        .as_str()
        .expect("id de guilde absent")
        .to_string();

    // --- CREATE ---
    let (status, emoji) = send(
        app.clone(),
        rq(
            "POST",
            &format!("/guilds/{gid}/emojis"),
            Some(json!({"name": "blob_cat", "animated": false, "image_id": "img1"})),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "création de l'emoji blob_cat");
    let eid = emoji["id"].as_str().expect("id d'emoji absent").to_string();
    assert_eq!(emoji["name"].as_str(), Some("blob_cat"), "nom de l'emoji");
    assert_eq!(
        emoji["guild_id"].as_str(),
        Some(gid.as_str()),
        "guild_id de l'emoji"
    );
    assert_eq!(
        emoji["animated"].as_bool(),
        Some(false),
        "animated == false"
    );
    assert_eq!(
        emoji["image_id"].as_str(),
        Some("img1"),
        "image_id de l'emoji"
    );
    assert!(emoji["created_by"].as_str().is_some(), "created_by présent");
    assert!(emoji.get("available").is_some(), "champ available présent");

    // --- LIST (présence) ---
    let (status, list) = send(
        app.clone(),
        rq("GET", &format!("/guilds/{gid}/emojis"), None, Some(&alice)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "liste des emojis");
    assert!(
        find_by_name(&list, "blob_cat").is_some(),
        "blob_cat doit être dans la liste après création"
    );

    // --- PATCH ---
    let (status, patched) = send(
        app.clone(),
        rq(
            "PATCH",
            &format!("/guilds/{gid}/emojis/{eid}"),
            Some(json!({"name": "blob_dance"})),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "renommage de l'emoji");
    assert_eq!(
        patched["name"].as_str(),
        Some("blob_dance"),
        "le nom doit être mis à jour en blob_dance"
    );

    // Le nouveau nom est visible dans la liste
    let (_, list) = send(
        app.clone(),
        rq("GET", &format!("/guilds/{gid}/emojis"), None, Some(&alice)),
    )
    .await;
    assert!(
        find_by_name(&list, "blob_dance").is_some(),
        "blob_dance doit apparaître dans la liste après renommage"
    );
    assert!(
        find_by_name(&list, "blob_cat").is_none(),
        "blob_cat ne doit plus exister après renommage"
    );

    // --- DELETE ---
    let (status, del) = send(
        app.clone(),
        rq(
            "DELETE",
            &format!("/guilds/{gid}/emojis/{eid}"),
            None,
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "suppression de l'emoji");
    assert_eq!(
        del["ok"].as_bool(),
        Some(true),
        "la réponse de suppression doit contenir ok:true"
    );

    // L'emoji n'est plus dans la liste
    let (_, list) = send(
        app.clone(),
        rq("GET", &format!("/guilds/{gid}/emojis"), None, Some(&alice)),
    )
    .await;
    assert!(
        find_by_name(&list, "blob_dance").is_none(),
        "blob_dance ne doit plus apparaître dans la liste après suppression"
    );
}

// ---------------------------------------------------------------------------
// Test 2 : CRUD complet des stickers
// ---------------------------------------------------------------------------
#[tokio::test]
async fn sticker_crud() {
    let app = app().await;

    let alice = register(&app, "alice", "alice@x.fr").await;

    let (status, guild) = send(
        app.clone(),
        rq("POST", "/guilds", Some(json!({"name": "G"})), Some(&alice)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "création de la guilde");
    let gid = guild["id"]
        .as_str()
        .expect("id de guilde absent")
        .to_string();

    // --- CREATE ---
    let (status, sticker) = send(
        app.clone(),
        rq(
            "POST",
            &format!("/guilds/{gid}/stickers"),
            Some(json!({
                "name": "coucou",
                "description": "salut",
                "asset_id": "ast1",
                "format_type": 1
            })),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "création du sticker coucou");
    let sid = sticker["id"]
        .as_str()
        .expect("id de sticker absent")
        .to_string();
    assert_eq!(sticker["name"].as_str(), Some("coucou"), "nom du sticker");
    assert_eq!(
        sticker["description"].as_str(),
        Some("salut"),
        "description du sticker"
    );
    assert_eq!(
        sticker["asset_id"].as_str(),
        Some("ast1"),
        "asset_id du sticker"
    );
    assert_eq!(
        sticker["format_type"].as_u64(),
        Some(1),
        "format_type du sticker"
    );

    // --- LIST (présence) ---
    let (status, list) = send(
        app.clone(),
        rq(
            "GET",
            &format!("/guilds/{gid}/stickers"),
            None,
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "liste des stickers");
    assert!(
        find_by_name(&list, "coucou").is_some(),
        "coucou doit être dans la liste après création"
    );

    // --- PATCH description ---
    let (status, patched) = send(
        app.clone(),
        rq(
            "PATCH",
            &format!("/guilds/{gid}/stickers/{sid}"),
            Some(json!({"description": "bonjour tout le monde"})),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "mise à jour de la description du sticker"
    );
    assert_eq!(
        patched["description"].as_str(),
        Some("bonjour tout le monde"),
        "la description doit être mise à jour"
    );

    // La description mise à jour est visible dans la liste
    let (_, list) = send(
        app.clone(),
        rq(
            "GET",
            &format!("/guilds/{gid}/stickers"),
            None,
            Some(&alice),
        ),
    )
    .await;
    let entry =
        find_by_name(&list, "coucou").expect("coucou doit toujours être présent après PATCH");
    assert_eq!(
        entry["description"].as_str(),
        Some("bonjour tout le monde"),
        "description mise à jour visible dans la liste"
    );

    // --- DELETE ---
    let (status, del) = send(
        app.clone(),
        rq(
            "DELETE",
            &format!("/guilds/{gid}/stickers/{sid}"),
            None,
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "suppression du sticker");
    assert_eq!(
        del["ok"].as_bool(),
        Some(true),
        "la réponse de suppression doit contenir ok:true"
    );

    // Le sticker n'est plus dans la liste
    let (_, list) = send(
        app.clone(),
        rq(
            "GET",
            &format!("/guilds/{gid}/stickers"),
            None,
            Some(&alice),
        ),
    )
    .await;
    assert!(
        find_by_name(&list, "coucou").is_none(),
        "coucou ne doit plus apparaître dans la liste après suppression"
    );
}

// ---------------------------------------------------------------------------
// Test 3 : CRUD complet des sons de soundboard
// ---------------------------------------------------------------------------
#[tokio::test]
async fn soundboard_crud() {
    let app = app().await;

    let alice = register(&app, "alice", "alice@x.fr").await;

    let (status, guild) = send(
        app.clone(),
        rq("POST", "/guilds", Some(json!({"name": "G"})), Some(&alice)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "création de la guilde");
    let gid = guild["id"]
        .as_str()
        .expect("id de guilde absent")
        .to_string();

    // --- CREATE ---
    let (status, sound) = send(
        app.clone(),
        rq(
            "POST",
            &format!("/guilds/{gid}/soundboard"),
            Some(json!({
                "name": "tada",
                "sound_id": "snd1",
                "volume": 0.5
            })),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "création du son tada");
    let sound_id_val = sound["id"].as_str().expect("id de son absent").to_string();
    assert_eq!(sound["name"].as_str(), Some("tada"), "nom du son");
    assert_eq!(sound["sound_id"].as_str(), Some("snd1"), "sound_id du son");
    // volume retourné doit être 0.5 (comparaison en f64 avec tolérance)
    let volume = sound["volume"]
        .as_f64()
        .expect("champ volume absent ou non numérique");
    assert!(
        (volume - 0.5_f64).abs() < 1e-6,
        "volume doit être 0.5, obtenu : {volume}"
    );

    // --- LIST (présence + volume) ---
    let (status, list) = send(
        app.clone(),
        rq(
            "GET",
            &format!("/guilds/{gid}/soundboard"),
            None,
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "liste du soundboard");
    let entry = find_by_name(&list, "tada").expect("tada doit être dans la liste après création");
    let listed_vol = entry["volume"]
        .as_f64()
        .expect("volume absent dans la liste");
    assert!(
        (listed_vol - 0.5_f64).abs() < 1e-6,
        "volume dans la liste doit être 0.5, obtenu : {listed_vol}"
    );

    // --- PATCH volume ---
    let (status, patched) = send(
        app.clone(),
        rq(
            "PATCH",
            &format!("/guilds/{gid}/soundboard/{sound_id_val}"),
            Some(json!({"volume": 0.8})),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "mise à jour du volume");
    let new_vol = patched["volume"]
        .as_f64()
        .expect("volume absent dans la réponse PATCH");
    assert!(
        (new_vol - 0.8_f64).abs() < 1e-6,
        "volume doit être 0.8 après PATCH, obtenu : {new_vol}"
    );

    // Le nouveau volume est visible dans la liste
    let (_, list) = send(
        app.clone(),
        rq(
            "GET",
            &format!("/guilds/{gid}/soundboard"),
            None,
            Some(&alice),
        ),
    )
    .await;
    let entry = find_by_name(&list, "tada").expect("tada doit toujours être présent après PATCH");
    let listed_vol = entry["volume"]
        .as_f64()
        .expect("volume absent dans la liste après PATCH");
    assert!(
        (listed_vol - 0.8_f64).abs() < 1e-6,
        "volume dans la liste doit être 0.8 après PATCH, obtenu : {listed_vol}"
    );

    // --- DELETE ---
    let (status, del) = send(
        app.clone(),
        rq(
            "DELETE",
            &format!("/guilds/{gid}/soundboard/{sound_id_val}"),
            None,
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "suppression du son");
    assert_eq!(
        del["ok"].as_bool(),
        Some(true),
        "la réponse de suppression doit contenir ok:true"
    );

    // Le son n'est plus dans la liste
    let (_, list) = send(
        app.clone(),
        rq(
            "GET",
            &format!("/guilds/{gid}/soundboard"),
            None,
            Some(&alice),
        ),
    )
    .await;
    assert!(
        find_by_name(&list, "tada").is_none(),
        "tada ne doit plus apparaître dans la liste après suppression"
    );
}
