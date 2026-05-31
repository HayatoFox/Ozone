//! Vérification cybersécurité de la modération (S6b) : permissions requises (BAN_MEMBERS,
//! MODERATE_MEMBERS, VIEW_AUDIT_LOG), protection du propriétaire, hiérarchie des rôles.

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
    let path = std::env::temp_dir().join(format!("ozone-test-secs6b-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "SecS6b".into(),
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
async fn moderation_requires_permissions() {
    let app = app().await;
    let alice = reg(&app, "alice", "a@s6b.fr").await;
    let gid = make_guild(&app, &alice).await;
    let bob = join(&app, &alice, &gid, "bob", "b@s6b.fr").await;
    let carol = join(&app, &alice, &gid, "carol", "c@s6b.fr").await;
    let carol_id = uid(&app, &carol).await;

    // Bob (sans BAN_MEMBERS) ne peut pas bannir.
    let (s, _) = send(
        &app,
        rq(
            "PUT",
            &format!("/guilds/{gid}/bans/{carol_id}"),
            Some(json!({})),
            Some(&bob),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "sans BAN_MEMBERS, pas de ban");
    // Bob (sans MODERATE_MEMBERS) ne peut pas mettre en sourdine.
    let (s, _) = send(
        &app,
        rq(
            "PATCH",
            &format!("/guilds/{gid}/members/{carol_id}"),
            Some(json!({"communication_disabled_until": 99999999999i64})),
            Some(&bob),
        ),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "sans MODERATE_MEMBERS, pas de timeout"
    );
    // Bob (sans VIEW_AUDIT_LOG) ne peut pas lire le journal d'audit.
    let (s, _) = send(
        &app,
        rq(
            "GET",
            &format!("/guilds/{gid}/audit-logs"),
            None,
            Some(&bob),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "sans VIEW_AUDIT_LOG, pas d'audit");
}

#[tokio::test]
async fn cannot_ban_owner_or_higher_role() {
    let app = app().await;
    let alice = reg(&app, "alice", "a@s6c.fr").await;
    let alice_id = uid(&app, &alice).await;
    let gid = make_guild(&app, &alice).await;

    let bob = join(&app, &alice, &gid, "bob", "b@s6c.fr").await;
    let bob_id = uid(&app, &bob).await;
    grant(&app, &alice, &gid, &bob_id, "mod", perms::BAN_MEMBERS).await; // position 1

    // Bob ne peut pas bannir le propriétaire.
    let (s, _) = send(
        &app,
        rq(
            "PUT",
            &format!("/guilds/{gid}/bans/{alice_id}"),
            Some(json!({})),
            Some(&bob),
        ),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "interdit de bannir le propriétaire"
    );

    // Carol a un rôle plus haut → bob ne peut pas la bannir (hiérarchie).
    let carol = join(&app, &alice, &gid, "carol", "c@s6c.fr").await;
    let carol_id = uid(&app, &carol).await;
    grant(&app, &alice, &gid, &carol_id, "admin", perms::MANAGE_GUILD).await; // position 2 (au-dessus)
    let (s, _) = send(
        &app,
        rq(
            "PUT",
            &format!("/guilds/{gid}/bans/{carol_id}"),
            Some(json!({})),
            Some(&bob),
        ),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "interdit de bannir un rôle supérieur"
    );

    // Mais bob peut bannir un membre simple (dave, sans rôle).
    let dave = join(&app, &alice, &gid, "dave", "d@s6c.fr").await;
    let dave_id = uid(&app, &dave).await;
    let (s, _) = send(
        &app,
        rq(
            "PUT",
            &format!("/guilds/{gid}/bans/{dave_id}"),
            Some(json!({})),
            Some(&bob),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "bob peut bannir un membre sous lui");
}
