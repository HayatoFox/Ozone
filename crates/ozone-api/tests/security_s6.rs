//! Vérification cybersécurité des expressions (S6a) : gardes de permission, propriété
//! (CREATE_GUILD_EXPRESSIONS = gérer les siennes), isolation inter-guildes, validation.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use ozone_api::config::Config;
use ozone_api::{bootstrap_state, build_app};
use ozone_proto::perms;
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
    let path = std::env::temp_dir().join(format!("ozone-test-secs6-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "SecS6".into(),
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

async fn send(app: &Router, r: Request<Body>) -> (StatusCode, Value) {
    let resp = app.clone().oneshot(r).await.unwrap();
    let st = resp.status();
    let by = resp.into_body().collect().await.unwrap().to_bytes();
    (st, serde_json::from_slice(&by).unwrap_or(Value::Null))
}

async fn reg(app: &Router, u: &str, e: &str) -> String {
    send(
        app,
        rq(
            "POST",
            "/auth/register",
            Some(json!({"username":u,"email":e,"password":"motdepasse"})),
            None,
        ),
    )
    .await
    .1["access_token"]
        .as_str()
        .unwrap()
        .to_string()
}
async fn uid(app: &Router, t: &str) -> String {
    send(app, rq("GET", "/users/@me", None, Some(t))).await.1["id"]
        .as_str()
        .unwrap()
        .to_string()
}
async fn make_guild(app: &Router, t: &str) -> String {
    send(
        app,
        rq("POST", "/guilds", Some(json!({"name":"G"})), Some(t)),
    )
    .await
    .1["id"]
        .as_str()
        .unwrap()
        .to_string()
}
async fn join(app: &Router, owner: &str, gid: &str, u: &str, e: &str) -> String {
    let code = send(
        app,
        rq(
            "POST",
            &format!("/guilds/{gid}/invites"),
            Some(json!({})),
            Some(owner),
        ),
    )
    .await
    .1["code"]
        .as_str()
        .unwrap()
        .to_string();
    let t = reg(app, u, e).await;
    send(app, rq("POST", &format!("/invites/{code}"), None, Some(&t))).await;
    t
}
/// Crée un rôle avec des permissions et l'attribue à `target_id`.
async fn grant(app: &Router, owner: &str, gid: &str, target_id: &str, name: &str, bits: u64) {
    let rid = send(
        app,
        rq(
            "POST",
            &format!("/guilds/{gid}/roles"),
            Some(json!({"name":name,"permissions":bits.to_string()})),
            Some(owner),
        ),
    )
    .await
    .1["id"]
        .as_str()
        .unwrap()
        .to_string();
    send(
        app,
        rq(
            "PUT",
            &format!("/guilds/{gid}/members/{target_id}/roles/{rid}"),
            None,
            Some(owner),
        ),
    )
    .await;
}

#[tokio::test]
async fn non_privileged_cannot_create_expressions() {
    let app = app().await;
    let alice = reg(&app, "alice", "a@s6.fr").await;
    let gid = make_guild(&app, &alice).await;
    let bob = join(&app, &alice, &gid, "bob", "b@s6.fr").await;
    let mallory = reg(&app, "mallory", "m@s6.fr").await;

    // Membre sans permission d'expression → refusé sur les 3 types.
    let (s, _) = send(
        &app,
        rq(
            "POST",
            &format!("/guilds/{gid}/emojis"),
            Some(json!({"name":"x_x","image_id":"i"})),
            Some(&bob),
        ),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "membre sans perm ne crée pas d'emoji"
    );
    let (s, _) = send(
        &app,
        rq(
            "POST",
            &format!("/guilds/{gid}/stickers"),
            Some(json!({"name":"coucou","asset_id":"a"})),
            Some(&bob),
        ),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "membre sans perm ne crée pas de sticker"
    );
    let (s, _) = send(
        &app,
        rq(
            "POST",
            &format!("/guilds/{gid}/soundboard"),
            Some(json!({"name":"tada","sound_id":"s"})),
            Some(&bob),
        ),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "membre sans perm ne crée pas de son"
    );

    // Non-membre → refusé (lister et créer).
    let (s, _) = send(
        &app,
        rq(
            "GET",
            &format!("/guilds/{gid}/emojis"),
            None,
            Some(&mallory),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "non-membre ne liste pas");
    let (s, _) = send(
        &app,
        rq(
            "POST",
            &format!("/guilds/{gid}/emojis"),
            Some(json!({"name":"y_y","image_id":"i"})),
            Some(&mallory),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "non-membre ne crée pas");
}

#[tokio::test]
async fn create_permission_limits_to_own_expressions() {
    let app = app().await;
    let alice = reg(&app, "alice", "a@s6b.fr").await;
    let gid = make_guild(&app, &alice).await;
    let bob = join(&app, &alice, &gid, "bob", "b@s6b.fr").await;
    let bob_id = uid(&app, &bob).await;
    let carol = join(&app, &alice, &gid, "carol", "c@s6b.fr").await;
    let carol_id = uid(&app, &carol).await;

    grant(
        &app,
        &alice,
        &gid,
        &bob_id,
        "exprB",
        perms::CREATE_GUILD_EXPRESSIONS,
    )
    .await;
    grant(
        &app,
        &alice,
        &gid,
        &carol_id,
        "exprC",
        perms::CREATE_GUILD_EXPRESSIONS,
    )
    .await;

    // Bob crée un emoji (a la permission CREATE).
    let (s, e) = send(
        &app,
        rq(
            "POST",
            &format!("/guilds/{gid}/emojis"),
            Some(json!({"name":"blob_b","image_id":"i"})),
            Some(&bob),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "bob crée son emoji");
    let eid = e["id"].as_str().unwrap().to_string();

    // Carol (CREATE mais pas MANAGE) ne peut pas supprimer celui de Bob.
    let (s, _) = send(
        &app,
        rq(
            "DELETE",
            &format!("/guilds/{gid}/emojis/{eid}"),
            None,
            Some(&carol),
        ),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "carol ne supprime pas l'emoji de bob"
    );

    // Bob peut supprimer le sien.
    let (s, _) = send(
        &app,
        rq(
            "DELETE",
            &format!("/guilds/{gid}/emojis/{eid}"),
            None,
            Some(&bob),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "bob supprime le sien");

    // Le propriétaire (MANAGE via toutes les permissions) peut supprimer celui d'autrui.
    let (_, e2) = send(
        &app,
        rq(
            "POST",
            &format!("/guilds/{gid}/emojis"),
            Some(json!({"name":"blob_b2","image_id":"i"})),
            Some(&bob),
        ),
    )
    .await;
    let eid2 = e2["id"].as_str().unwrap().to_string();
    let (s, _) = send(
        &app,
        rq(
            "DELETE",
            &format!("/guilds/{gid}/emojis/{eid2}"),
            None,
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::OK,
        "le propriétaire supprime l'emoji d'un autre"
    );
}

#[tokio::test]
async fn cross_guild_isolation_and_validation() {
    let app = app().await;
    let alice = reg(&app, "alice", "a@s6c.fr").await;
    let g1 = make_guild(&app, &alice).await;
    let g2 = make_guild(&app, &alice).await;

    // Emoji créé dans g1.
    let (_, e) = send(
        &app,
        rq(
            "POST",
            &format!("/guilds/{g1}/emojis"),
            Some(json!({"name":"only_g1","image_id":"i"})),
            Some(&alice),
        ),
    )
    .await;
    let eid = e["id"].as_str().unwrap().to_string();

    // Supprimer via le chemin de g2 → introuvable (isolation inter-guildes).
    let (s, _) = send(
        &app,
        rq(
            "DELETE",
            &format!("/guilds/{g2}/emojis/{eid}"),
            None,
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::NOT_FOUND,
        "un emoji de g1 n'est pas accessible via g2"
    );

    // Nom invalide (espace / ponctuation) → 400.
    let (s, _) = send(
        &app,
        rq(
            "POST",
            &format!("/guilds/{g1}/emojis"),
            Some(json!({"name":"nom invalide!","image_id":"i"})),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "nom d'emoji invalide refusé");

    // image_id vide → 400.
    let (s, _) = send(
        &app,
        rq(
            "POST",
            &format!("/guilds/{g1}/emojis"),
            Some(json!({"name":"vide","image_id":""})),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "image_id vide refusé");
}
