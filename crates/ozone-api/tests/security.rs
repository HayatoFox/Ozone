//! Tests d'intrusion (adversariaux) sur S1 (permissions/rôles/membres/invitations)
//! et S2 (messages). Chaque test tente une attaque et vérifie qu'elle est **bloquée**.
//!
//! Couverture : contrôle d'accès (IDOR), escalade de privilèges, hiérarchie de rôles,
//! isolation inter-guildes, injection SQL, falsification de JWT, déni de service.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use ozone_api::config::Config;
use ozone_api::{bootstrap_state, build_app};
use ozone_proto::perms;
use serde_json::{json, Value};
use tower::ServiceExt;

// ───────────────────────────── Helpers ─────────────────────────────

async fn app() -> Router {
    let unique = format!(
        "{}_{:?}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        std::thread::current().id()
    );
    let path = std::env::temp_dir().join(format!("ozone-test-sec-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Ozone Sec".into(),
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
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn register(app: &Router, username: &str, email: &str) -> String {
    let (s, v) = send(
        app,
        rq(
            "POST",
            "/auth/register",
            Some(json!({"username":username,"email":email,"password":"Sup3r-Ozone-Pw"})),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    v["access_token"].as_str().unwrap().to_string()
}

async fn make_guild(app: &Router, tok: &str) -> (String, String) {
    let (_, g) = send(
        app,
        rq("POST", "/guilds", Some(json!({"name":"G"})), Some(tok)),
    )
    .await;
    let gid = g["id"].as_str().unwrap().to_string();
    let (_, ch) = send(
        app,
        rq("GET", &format!("/guilds/{gid}/channels"), None, Some(tok)),
    )
    .await;
    let cid = ch[0]["id"].as_str().unwrap().to_string();
    (gid, cid)
}

async fn join(app: &Router, owner: &str, gid: &str, username: &str, email: &str) -> String {
    let (_, inv) = send(
        app,
        rq(
            "POST",
            &format!("/guilds/{gid}/invites"),
            Some(json!({})),
            Some(owner),
        ),
    )
    .await;
    let code = inv["code"].as_str().unwrap().to_string();
    let tok = register(app, username, email).await;
    let (s, _) = send(
        app,
        rq("POST", &format!("/invites/{code}"), None, Some(&tok)),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    tok
}

async fn uid(app: &Router, tok: &str) -> String {
    let (_, me) = send(app, rq("GET", "/users/@me", None, Some(tok))).await;
    me["id"].as_str().unwrap().to_string()
}

async fn create_role(app: &Router, tok: &str, gid: &str, name: &str, perms_bits: u64) -> String {
    let (s, r) = send(
        app,
        rq(
            "POST",
            &format!("/guilds/{gid}/roles"),
            Some(json!({"name":name,"permissions":perms_bits.to_string()})),
            Some(tok),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "create_role {name}");
    r["id"].as_str().unwrap().to_string()
}

async fn post_msg(app: &Router, tok: &str, cid: &str, content: &str) -> (StatusCode, Value) {
    send(
        app,
        rq(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({"content":content})),
            Some(tok),
        ),
    )
    .await
}

// ───────────────────────────── Contrôle d'accès (IDOR) ─────────────────────────────

#[tokio::test]
async fn non_member_is_denied_everything() {
    let app = app().await;
    let alice = register(&app, "alice", "a@x.fr").await;
    let (gid, cid) = make_guild(&app, &alice).await;
    let intruder = register(&app, "mallory", "m@x.fr").await; // jamais rejoint la guilde

    for r in [
        rq(
            "GET",
            &format!("/channels/{cid}/messages"),
            None,
            Some(&intruder),
        ),
        rq(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({"content":"x"})),
            Some(&intruder),
        ),
        rq(
            "GET",
            &format!("/guilds/{gid}/roles"),
            None,
            Some(&intruder),
        ),
        rq(
            "GET",
            &format!("/guilds/{gid}/members"),
            None,
            Some(&intruder),
        ),
        rq(
            "GET",
            &format!("/guilds/{gid}/channels"),
            None,
            Some(&intruder),
        ),
        rq(
            "POST",
            &format!("/guilds/{gid}/channels"),
            Some(json!({"name":"x"})),
            Some(&intruder),
        ),
    ] {
        let (s, _) = send(&app, r).await;
        assert!(
            s.is_client_error(),
            "un non-membre devrait être refusé, reçu {s}"
        );
    }
}

#[tokio::test]
async fn cannot_edit_or_delete_others_messages() {
    let app = app().await;
    let alice = register(&app, "alice", "a@x.fr").await;
    let (gid, cid) = make_guild(&app, &alice).await;
    let bob = join(&app, &alice, &gid, "bob", "b@x.fr").await;

    let (_, m) = post_msg(&app, &alice, &cid, "message d'alice").await;
    let mid = m["id"].as_str().unwrap().to_string();

    let (s, _) = send(
        &app,
        rq(
            "PATCH",
            &format!("/channels/{cid}/messages/{mid}"),
            Some(json!({"content":"piraté"})),
            Some(&bob),
        ),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "bob ne doit pas éditer le message d'alice"
    );

    let (s, _) = send(
        &app,
        rq(
            "DELETE",
            &format!("/channels/{cid}/messages/{mid}"),
            None,
            Some(&bob),
        ),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "bob ne doit pas supprimer le message d'alice"
    );
}

#[tokio::test]
async fn reply_across_channels_is_blocked() {
    let app = app().await;
    let alice = register(&app, "alice", "a@x.fr").await;
    let (gid, cid1) = make_guild(&app, &alice).await;
    let (_, ch2) = send(
        &app,
        rq(
            "POST",
            &format!("/guilds/{gid}/channels"),
            Some(json!({"name":"autre"})),
            Some(&alice),
        ),
    )
    .await;
    let cid2 = ch2["id"].as_str().unwrap().to_string();

    let (_, m) = post_msg(&app, &alice, &cid1, "dans le salon 1").await;
    let mid1 = m["id"].as_str().unwrap().to_string();

    // répondre dans cid2 à un message de cid1 → refusé
    let (s, _) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{cid2}/messages"),
            Some(json!({"content":"x","reply_to":mid1})),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "réponse inter-salons interdite");
}

// ───────────────────────────── Escalade de privilèges ─────────────────────────────

#[tokio::test]
async fn role_permissions_are_clamped_to_grantor() {
    let app = app().await;
    let alice = register(&app, "alice", "a@x.fr").await;
    let (gid, _cid) = make_guild(&app, &alice).await;
    let bob = join(&app, &alice, &gid, "bob", "b@x.fr").await;
    let bob_id = uid(&app, &bob).await;

    // alice donne à bob un rôle « mod » avec MANAGE_ROLES (mais pas ADMINISTRATOR)
    let modr = create_role(&app, &alice, &gid, "mod", perms::MANAGE_ROLES).await;
    let (s, _) = send(
        &app,
        rq(
            "PUT",
            &format!("/guilds/{gid}/members/{bob_id}/roles/{modr}"),
            None,
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // bob tente de créer un rôle ADMINISTRATOR : doit être clampé à ce qu'il possède
    let want = perms::ADMINISTRATOR | perms::MANAGE_GUILD | perms::BAN_MEMBERS;
    let (s, r) = send(
        &app,
        rq(
            "POST",
            &format!("/guilds/{gid}/roles"),
            Some(json!({"name":"evil","permissions":want.to_string()})),
            Some(&bob),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let granted: u64 = r["permissions"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        granted & perms::ADMINISTRATOR,
        0,
        "ADMINISTRATOR ne doit pas être accordé"
    );
    assert_eq!(
        granted & perms::MANAGE_GUILD,
        0,
        "MANAGE_GUILD ne doit pas être accordé"
    );
    assert_eq!(
        granted & perms::BAN_MEMBERS,
        0,
        "BAN_MEMBERS ne doit pas être accordé"
    );
}

#[tokio::test]
async fn cannot_assign_role_above_own_highest() {
    let app = app().await;
    let alice = register(&app, "alice", "a@x.fr").await;
    let (gid, _cid) = make_guild(&app, &alice).await;
    let bob = join(&app, &alice, &gid, "bob", "b@x.fr").await;
    let bob_id = uid(&app, &bob).await;

    let modr = create_role(&app, &alice, &gid, "mod", perms::MANAGE_ROLES).await; // position 1
    send(
        &app,
        rq(
            "PUT",
            &format!("/guilds/{gid}/members/{bob_id}/roles/{modr}"),
            None,
            Some(&alice),
        ),
    )
    .await;
    let high = create_role(&app, &alice, &gid, "high", perms::MANAGE_GUILD).await; // position 2 (au-dessus de mod)

    // bob (plus haut = mod) tente de s'attribuer « high » (au-dessus) → refusé
    let (s, _) = send(
        &app,
        rq(
            "PUT",
            &format!("/guilds/{gid}/members/{bob_id}/roles/{high}"),
            None,
            Some(&bob),
        ),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "impossible de s'attribuer un rôle au-dessus du sien"
    );
}

#[tokio::test]
async fn roles_are_isolated_between_guilds() {
    let app = app().await;
    let alice = register(&app, "alice", "a@x.fr").await;
    let (g1, _c1) = make_guild(&app, &alice).await;
    let bob = join(&app, &alice, &g1, "bob", "b@x.fr").await;
    let bob_id = uid(&app, &bob).await;

    // une seconde guilde, avec un rôle qui lui appartient
    let (_, g2v) = send(
        &app,
        rq("POST", "/guilds", Some(json!({"name":"G2"})), Some(&alice)),
    )
    .await;
    let g2 = g2v["id"].as_str().unwrap().to_string();
    let r2 = create_role(&app, &alice, &g2, "role-g2", perms::MANAGE_GUILD).await;

    // tenter d'attribuer un rôle de g2 à un membre de g1 → introuvable
    let (s, _) = send(
        &app,
        rq(
            "PUT",
            &format!("/guilds/{g1}/members/{bob_id}/roles/{r2}"),
            None,
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::NOT_FOUND,
        "un rôle d'une autre guilde ne doit pas être attribuable"
    );
}

#[tokio::test]
async fn cannot_kick_owner() {
    let app = app().await;
    let alice = register(&app, "alice", "a@x.fr").await;
    let (gid, _cid) = make_guild(&app, &alice).await;
    let bob = join(&app, &alice, &gid, "bob", "b@x.fr").await;
    let bob_id = uid(&app, &bob).await;
    let alice_id = uid(&app, &alice).await;

    let modr = create_role(
        &app,
        &alice,
        &gid,
        "mod",
        perms::KICK_MEMBERS | perms::MANAGE_ROLES,
    )
    .await;
    send(
        &app,
        rq(
            "PUT",
            &format!("/guilds/{gid}/members/{bob_id}/roles/{modr}"),
            None,
            Some(&alice),
        ),
    )
    .await;

    let (s, _) = send(
        &app,
        rq(
            "DELETE",
            &format!("/guilds/{gid}/members/{alice_id}"),
            None,
            Some(&bob),
        ),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "le propriétaire ne doit pas pouvoir être expulsé"
    );
}

// ───────────────────────────── Authentification ─────────────────────────────

#[tokio::test]
async fn forged_or_tampered_tokens_are_rejected() {
    let app = app().await;
    let alice = register(&app, "alice", "a@x.fr").await;

    let parts: Vec<&str> = alice.split('.').collect();
    let tampered = format!("{}.{}.{}", parts[0], parts[1], "AAAA"); // signature invalide
    let no_sig = format!("{}.{}.", parts[0], parts[1]); // signature vide
    let alg_none = "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.eyJzdWIiOiIxIiwiaWF0IjowLCJleHAiOjk5OTk5OTk5OTksImtpbmQiOiJhY2Nlc3MifQ."; // alg=none forgé

    for bad in [
        tampered.as_str(),
        no_sig.as_str(),
        alg_none,
        "n.imp.orte",
        "Bearer-less",
    ] {
        let (s, _) = send(&app, rq("GET", "/users/@me", None, Some(bad))).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "jeton invalide accepté: {bad}");
    }

    // le vrai jeton fonctionne toujours
    let (s, _) = send(&app, rq("GET", "/users/@me", None, Some(&alice))).await;
    assert_eq!(s, StatusCode::OK);
}

// ───────────────────────────── Injection ─────────────────────────────

#[tokio::test]
async fn sql_injection_is_neutralized() {
    let app = app().await;
    let alice = register(&app, "alice", "a@x.fr").await;
    let (_gid, cid) = make_guild(&app, &alice).await;

    let payload = "Robert'); DROP TABLE messages;-- ⚡";
    let (s, m) = post_msg(&app, &alice, &cid, payload).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        m["content"], payload,
        "le contenu doit être stocké littéralement"
    );
    let mid = m["id"].as_str().unwrap().to_string();

    // emoji de réaction contenant une apostrophe (paramétré, pas concaténé)
    let (s, _) = send(
        &app,
        rq(
            "PUT",
            &format!("/channels/{cid}/messages/{mid}/reactions/x'y/@me"),
            None,
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // la table existe toujours : un nouvel envoi et la lecture fonctionnent
    let (s, _) = post_msg(&app, &alice, &cid, "toujours là").await;
    assert_eq!(s, StatusCode::OK);
    let (s, list) = send(
        &app,
        rq(
            "GET",
            &format!("/channels/{cid}/messages"),
            None,
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 2);
}

// ───────────────────────────── Déni de service ─────────────────────────────

#[tokio::test]
async fn dos_limits_are_enforced() {
    let app = app().await;
    let alice = register(&app, "alice", "a@x.fr").await;
    let (_gid, cid) = make_guild(&app, &alice).await;

    // suppression en masse > 100 → refusée
    let ids: Vec<String> = (1..=101).map(|i| i.to_string()).collect();
    let (s, _) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{cid}/messages/bulk-delete"),
            Some(json!({"messages":ids})),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::BAD_REQUEST,
        "bulk-delete > 100 doit être refusé"
    );

    // message surdimensionné → refusé
    let big = "a".repeat(5000);
    let (s, _) = post_msg(&app, &alice, &cid, &big).await;
    assert_eq!(
        s,
        StatusCode::BAD_REQUEST,
        "contenu > 4000 doit être refusé"
    );
}

// ───────────────────────────── Durcissement des surcharges ─────────────────────────────

#[tokio::test]
async fn overwrites_strip_admin_and_validate_target() {
    let app = app().await;
    let alice = register(&app, "alice", "a@x.fr").await;
    let (gid, cid) = make_guild(&app, &alice).await;

    // overwrite @everyone autorisant ADMINISTRATOR + MANAGE_CHANNELS → ADMINISTRATOR retiré
    let allow = perms::ADMINISTRATOR | perms::MANAGE_CHANNELS;
    let (s, ow) = send(
        &app,
        rq(
            "PUT",
            &format!("/channels/{cid}/permissions/{gid}"),
            Some(json!({"type":0,"allow":allow.to_string()})),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let got: u64 = ow["allow"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        got & perms::ADMINISTRATOR,
        0,
        "ADMINISTRATOR ne doit pas être surchargeable"
    );
    assert_ne!(
        got & perms::MANAGE_CHANNELS,
        0,
        "MANAGE_CHANNELS doit rester"
    );

    // cible inexistante dans la guilde → refusé
    let (s, _) = send(
        &app,
        rq(
            "PUT",
            &format!("/channels/{cid}/permissions/123456"),
            Some(json!({"type":0,"allow":"0"})),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::NOT_FOUND,
        "surcharge vers une cible inexistante refusée"
    );
}
