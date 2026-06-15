//! Tests d'intégration : chiffrement de bout en bout des MP (clés publiques + blob `cipher`).
//! Le serveur ne doit JAMAIS voir le texte clair : il stocke/restitue un blob opaque et le `content`
//! reste vide. Le chiffrement n'est autorisé qu'en MP (pas de guilde).

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
    let path = std::env::temp_dir().join(format!("ozone-test-e2ee-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Ozone E2EE Tests".into(),
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
    assert_eq!(status, StatusCode::OK);
    body["id"].as_str().unwrap().to_string()
}

async fn open_dm(app: &Router, token: &str, other_id: &str) -> String {
    let (status, ch) = send(
        app,
        rq(
            "POST",
            "/users/@me/channels",
            Some(json!({ "recipients": [other_id] })),
            Some(token),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "ouverture du MP a échoué");
    ch["id"].as_str().unwrap().to_string()
}

/// 1. Publication et lecture d'une clé publique ; validation des bornes.
#[tokio::test]
async fn publish_and_fetch_public_key() {
    let app = app().await;
    let alice = register(&app, "alice", "alice@e2ee.fr").await;
    let bob = register(&app, "bob", "bob@e2ee.fr").await;
    let bob_id = uid(&app, &bob).await;

    // Avant publication : la clé de Bob est absente (null), pas une 404.
    let (st, body) = send(&app, rq("GET", &format!("/users/{bob_id}/keys"), None, Some(&alice))).await;
    assert_eq!(st, StatusCode::OK);
    assert!(body["public_key"].is_null(), "clé absente attendue (null)");

    // Bob publie sa clé.
    let (st, body) = send(
        &app,
        rq("PUT", "/users/@me/keys", Some(json!({"public_key":"BBBpubkeyBob=="})), Some(&bob)),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body["public_key"].as_str(), Some("BBBpubkeyBob=="));

    // Alice relit la clé de Bob.
    let (st, body) = send(&app, rq("GET", &format!("/users/{bob_id}/keys"), None, Some(&alice))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body["public_key"].as_str(), Some("BBBpubkeyBob=="));

    // Clé vide rejetée.
    let (st, _) = send(
        &app,
        rq("PUT", "/users/@me/keys", Some(json!({"public_key":"   "})), Some(&bob)),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "clé vide doit être rejetée");

    // Clé surdimensionnée rejetée (> 1024).
    let huge = "A".repeat(2000);
    let (st, _) = send(
        &app,
        rq("PUT", "/users/@me/keys", Some(json!({ "public_key": huge })), Some(&bob)),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "clé trop longue doit être rejetée");

    // La lecture des clés exige une authentification.
    let (st, _) = send(&app, rq("GET", &format!("/users/{bob_id}/keys"), None, None)).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
}

/// 2. Aller-retour d'un message MP chiffré : le `content` reste vide, le `cipher` est restitué tel quel.
#[tokio::test]
async fn dm_cipher_roundtrip() {
    let app = app().await;
    let alice = register(&app, "alice", "alice@rt.fr").await;
    let bob = register(&app, "bob", "bob@rt.fr").await;
    let bob_id = uid(&app, &bob).await;
    let cid = open_dm(&app, &alice, &bob_id).await;

    let blob = "aXZ=|Y2lwaGVydGV4dA=="; // « iv|ciphertext » factice mais bien formé
    let (st, msg) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{cid}/messages"),
            // content non vide ET cipher : le serveur doit ignorer le clair et ne stocker que le blob.
            Some(json!({ "content": "ne doit pas fuir", "cipher": blob })),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "envoi du MP chiffré a échoué");
    assert_eq!(msg["cipher"].as_str(), Some(blob), "le cipher doit être restitué");
    assert_eq!(
        msg["content"].as_str().unwrap_or(""),
        "",
        "le content en clair ne doit JAMAIS être stocké/retourné pour un message chiffré"
    );

    // Bob relit : il reçoit le même blob, et toujours pas de clair.
    let (st, list) = send(
        &app,
        rq("GET", &format!("/channels/{cid}/messages"), None, Some(&bob)),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let m = &list.as_array().unwrap()[0];
    assert_eq!(m["cipher"].as_str(), Some(blob));
    assert_eq!(m["content"].as_str().unwrap_or(""), "");
}

/// 3. Le chiffrement est refusé hors MP (salon de guilde).
#[tokio::test]
async fn cipher_rejected_in_guild_channel() {
    let app = app().await;
    let alice = register(&app, "alice", "alice@g.fr").await;

    let (_, guild) = send(
        &app,
        rq("POST", "/guilds", Some(json!({"name":"G"})), Some(&alice)),
    )
    .await;
    let gid = guild["id"].as_str().unwrap().to_string();
    let (_, channels) = send(
        &app,
        rq("GET", &format!("/guilds/{gid}/channels"), None, Some(&alice)),
    )
    .await;
    let cid = channels[0]["id"].as_str().unwrap().to_string();

    let (st, _) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({ "content": "", "cipher": "aXY=|Yg==" })),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::BAD_REQUEST,
        "le chiffrement E2EE ne doit être autorisé qu'en MP"
    );
}

/// 4. Édition d'un MP chiffré : le nouveau blob remplace l'ancien, le clair reste vide.
#[tokio::test]
async fn edit_cipher_roundtrip() {
    let app = app().await;
    let alice = register(&app, "alice", "alice@ed.fr").await;
    let bob = register(&app, "bob", "bob@ed.fr").await;
    let bob_id = uid(&app, &bob).await;
    let cid = open_dm(&app, &alice, &bob_id).await;

    let (st, msg) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({ "content": "", "cipher": "aXY=|YQ==" })),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let mid = msg["id"].as_str().unwrap().to_string();

    let new_blob = "ZWZn=|Ymxhcg==";
    let (st, edited) = send(
        &app,
        rq(
            "PATCH",
            &format!("/channels/{cid}/messages/{mid}"),
            Some(json!({ "content": "encore en clair, à ignorer", "cipher": new_blob })),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "édition chiffrée a échoué");
    assert_eq!(edited["cipher"].as_str(), Some(new_blob), "le cipher doit être mis à jour");
    assert_eq!(edited["content"].as_str().unwrap_or(""), "", "pas de clair après édition chiffrée");
    assert!(edited["edited_at"].as_u64().is_some(), "edited_at doit être posé");
}

// ─────────────────── Persistance multi-appareils (escrow + auth ZK, v2) ───────────────────
// Le serveur traite l'identifiant de façon opaque : `password`/`auth_secret` = l'authSecret dérivé
// côté client (ici une chaîne factice), et il stocke/restitue un escrow opaque qu'il ne peut pas lire.

async fn register_v2(
    app: &Router,
    username: &str,
    email: &str,
    auth_secret: &str,
    public_key: &str,
    priv_wrapped: &str,
) -> String {
    let (st, body) = send(
        app,
        rq(
            "POST",
            "/auth/register",
            Some(json!({
                "username": username, "email": email, "password": auth_secret,
                "public_key": public_key, "priv_wrapped": priv_wrapped,
                "kdf_salt": format!("SALT_{username}")
            })),
            None,
        ),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "register v2 {username} a échoué");
    body["access_token"].as_str().unwrap().to_string()
}

async fn login(app: &Router, login: &str, secret: &str) -> StatusCode {
    send(
        app,
        rq("POST", "/auth/login", Some(json!({ "login": login, "password": secret })), None),
    )
    .await
    .0
}

/// 5. Inscription v2 : l'escrow est déposé et restituable ; le login se fait avec l'authSecret.
#[tokio::test]
async fn v2_register_escrow_and_login() {
    let app = app().await;
    let tok = register_v2(&app, "alice", "alice@v2.fr", "AUTH_alice", "PUB_alice", "WRAP_alice").await;

    let (st, enc) = send(&app, rq("GET", "/users/@me/encryption", None, Some(&tok))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(enc["public_key"].as_str(), Some("PUB_alice"));
    assert_eq!(enc["priv_wrapped"].as_str(), Some("WRAP_alice"), "escrow restitué pour récupération");
    assert_eq!(enc["pw_scheme"].as_i64(), Some(2));

    // Login avec l'authSecret (= ce que dérive n'importe quel appareil) → OK ; secret erroné → 401.
    assert_eq!(login(&app, "alice", "AUTH_alice").await, StatusCode::OK);
    assert_eq!(login(&app, "alice", "MAUVAIS").await, StatusCode::UNAUTHORIZED);
}

/// 6. Compte legacy (v1) → migration vers v2 : preuve du mdp brut une fois, puis bascule sur authSecret.
#[tokio::test]
async fn legacy_upgrade_to_v2() {
    let app = app().await;
    let tok = register(&app, "bob", "bob@v2.fr").await; // legacy, mdp brut "Sup3r-Ozone-Pw"

    let (_, enc) = send(&app, rq("GET", "/users/@me/encryption", None, Some(&tok))).await;
    assert_eq!(enc["pw_scheme"].as_i64(), Some(1), "compte legacy = v1");
    assert!(enc["priv_wrapped"].is_null(), "pas d'escrow avant migration");

    let (st, _) = send(
        &app,
        rq(
            "POST",
            "/users/@me/encryption/upgrade",
            Some(json!({
                "current_password": "Sup3r-Ozone-Pw", "auth_secret": "AUTH_bob",
                "public_key": "PUB_bob", "priv_wrapped": "WRAP_bob", "kdf_salt": "SALT_bob"
            })),
            Some(&tok),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "migration v1→v2 a échoué");

    // Désormais : login avec authSecret OK, mdp brut REFUSÉ.
    assert_eq!(login(&app, "bob", "AUTH_bob").await, StatusCode::OK);
    assert_eq!(login(&app, "bob", "Sup3r-Ozone-Pw").await, StatusCode::UNAUTHORIZED);

    let (_, enc) = send(&app, rq("GET", "/users/@me/encryption", None, Some(&tok))).await;
    assert_eq!(enc["pw_scheme"].as_i64(), Some(2));
    assert_eq!(enc["priv_wrapped"].as_str(), Some("WRAP_bob"));

    // Re-migration interdite (déjà en v2).
    let (st, _) = send(
        &app,
        rq(
            "POST",
            "/users/@me/encryption/upgrade",
            Some(json!({"current_password":"AUTH_bob","auth_secret":"X","public_key":"Y","priv_wrapped":"Z","kdf_salt":"S"})),
            Some(&tok),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

/// 8. Prelogin : restitue le sel KDF d'un compte v2 ; anti-énumération pour un compte inexistant.
#[tokio::test]
async fn prelogin_returns_salt_and_hides_unknown() {
    let app = app().await;
    register_v2(&app, "dave", "dave@v2.fr", "AUTH_dave", "PUB_dave", "WRAP_dave").await;
    // (register_v2 n'envoie pas de kdf_salt ⇒ NULL ⇒ prelogin renvoie un sel factice déterministe.)

    // Compte existant trouvé par e-mail OU pseudo, schéma v2.
    let (st, p1) = send(&app, rq("POST", "/auth/prelogin", Some(json!({ "login": "dave" })), None)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(p1["pw_scheme"].as_i64(), Some(2));
    assert!(!p1["kdf_salt"].as_str().unwrap().is_empty());
    let (_, p2) = send(&app, rq("POST", "/auth/prelogin", Some(json!({ "login": "dave@v2.fr" })), None)).await;
    assert_eq!(p2["kdf_salt"], p1["kdf_salt"], "même sel par e-mail ou pseudo");

    // Compte inexistant : réponse de même forme (sel factice déterministe), pas d'erreur d'énumération.
    let (st, u1) = send(&app, rq("POST", "/auth/prelogin", Some(json!({ "login": "inconnu" })), None)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(u1["kdf_salt"].as_str().unwrap().len(), 64, "sel factice = SHA-256 hex (64 car.)");
    let (_, u2) = send(&app, rq("POST", "/auth/prelogin", Some(json!({ "login": "inconnu" })), None)).await;
    assert_eq!(u1["kdf_salt"], u2["kdf_salt"], "déterministe pour le même identifiant");
}

/// 7. Changement de mot de passe (v2) : ré-emballe l'escrow avec la nouvelle KEK.
#[tokio::test]
async fn change_password_rewraps_escrow() {
    let app = app().await;
    let tok = register_v2(&app, "carol", "carol@v2.fr", "AUTH1", "PUB_carol", "WRAP1").await;

    let (st, _) = send(
        &app,
        rq(
            "PATCH",
            "/users/@me/password",
            Some(json!({ "current_password": "AUTH1", "new_password": "AUTH2", "priv_wrapped": "WRAP2" })),
            Some(&tok),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "changement de mot de passe v2 a échoué");

    // Sessions révoquées → on se reconnecte avec le nouvel authSecret ; l'ancien est refusé.
    assert_eq!(login(&app, "carol", "AUTH2").await, StatusCode::OK);
    assert_eq!(login(&app, "carol", "AUTH1").await, StatusCode::UNAUTHORIZED);

    // L'escrow a été ré-emballé.
    let tok2 = register_v2_relogin(&app, "carol", "AUTH2").await;
    let (_, enc) = send(&app, rq("GET", "/users/@me/encryption", None, Some(&tok2))).await;
    assert_eq!(enc["priv_wrapped"].as_str(), Some("WRAP2"), "escrow ré-emballé après changement de mdp");
}

/// Reconnexion utilitaire renvoyant le token d'accès.
async fn register_v2_relogin(app: &Router, login: &str, secret: &str) -> String {
    let (st, body) = send(
        app,
        rq("POST", "/auth/login", Some(json!({ "login": login, "password": secret })), None),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    body["access_token"].as_str().unwrap().to_string()
}
