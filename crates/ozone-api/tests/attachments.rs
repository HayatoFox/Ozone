//! Tests fonctionnels + cyber : téléversement de pièces jointes, attachement à un message,
//! téléchargement gardé par permission, propriété du téléversement.

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
    let path = std::env::temp_dir().join(format!("ozone-test-att-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Att".into(),
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
        Some(json!({"username":u,"email":e,"password":"motdepasse"})),
        None,
    )
    .await
    .1["access_token"]
        .as_str()
        .unwrap()
        .to_string()
}
async fn setup(app: &Router, owner: &str) -> (String, String) {
    let g = send(
        app,
        "POST",
        "/guilds",
        Some(json!({"name":"G"})),
        Some(owner),
    )
    .await
    .1;
    let gid = g["id"].as_str().unwrap().to_string();
    let cid = send(
        app,
        "GET",
        &format!("/guilds/{gid}/channels"),
        None,
        Some(owner),
    )
    .await
    .1[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    (gid, cid)
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

fn multipart(filename: &str, ctype: &str, content: &[u8]) -> (String, Vec<u8>) {
    let boundary = "OZONETESTBOUNDARY";
    let mut body = Vec::new();
    body.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: {ctype}\r\n\r\n").as_bytes(),
    );
    body.extend_from_slice(content);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

async fn upload(
    app: &Router,
    cid: &str,
    tok: &str,
    filename: &str,
    ctype: &str,
    content: &[u8],
) -> (StatusCode, Value) {
    let (ct, body) = multipart(filename, ctype, content);
    let req = Request::builder()
        .method("POST")
        .uri(format!("/channels/{cid}/attachments"))
        .header("authorization", format!("Bearer {tok}"))
        .header("content-type", ct)
        .body(Body::from(body))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let st = resp.status();
    let by = resp.into_body().collect().await.unwrap().to_bytes();
    (st, serde_json::from_slice(&by).unwrap_or(Value::Null))
}

/// GET brut renvoyant (statut, content-type, corps).
async fn get_raw(app: &Router, uri: &str, tok: Option<&str>) -> (StatusCode, String, Vec<u8>) {
    let mut b = Request::builder().method("GET").uri(uri);
    if let Some(t) = tok {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    let resp = app
        .clone()
        .oneshot(b.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let st = resp.status();
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let by = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (st, ct, by)
}

#[tokio::test]
async fn upload_attach_and_download() {
    let app = app().await;
    let owner = token(&app, "alice", "a@at.fr").await;
    let (_gid, cid) = setup(&app, &owner).await;

    let (s, att) = upload(
        &app,
        &cid,
        &owner,
        "bonjour.txt",
        "text/plain",
        b"coucou ozone",
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(att["filename"], "bonjour.txt");
    assert_eq!(att["size"], 12);
    let aid = att["id"].as_str().unwrap().to_string();

    // Message vide MAIS avec pièce jointe → accepté ; la pièce jointe est attachée.
    let (s, msg) = send(
        &app,
        "POST",
        &format!("/channels/{cid}/messages"),
        Some(json!({"content":"","attachments":[aid]})),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(msg["attachments"].as_array().unwrap().len(), 1);
    assert_eq!(msg["attachments"][0]["filename"], "bonjour.txt");
    let url = msg["attachments"][0]["url"].as_str().unwrap().to_string();

    // L'historique reflète la pièce jointe.
    let (_s, hist) = send(
        &app,
        "GET",
        &format!("/channels/{cid}/messages"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(hist[0]["attachments"].as_array().unwrap().len(), 1);

    // Téléchargement (membre) → contenu + type.
    let (s, ct, body) = get_raw(&app, &url, Some(&owner)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(ct, "text/plain");
    assert_eq!(body, b"coucou ozone");
}

#[tokio::test]
async fn attachment_permissions_and_ownership() {
    let app = app().await;
    let owner = token(&app, "alice", "a@at2.fr").await;
    let (gid, cid) = setup(&app, &owner).await;
    let carol = token(&app, "carol", "c@at2.fr").await; // non-membre

    // Non-membre ne peut pas téléverser.
    let (s, _) = upload(&app, &cid, &carol, "x.txt", "text/plain", b"hack").await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // owner téléverse + attache + envoie.
    let (_s, att) = upload(&app, &cid, &owner, "secret.txt", "text/plain", b"donnees").await;
    let aid = att["id"].as_str().unwrap().to_string();
    let (_s, msg) = send(
        &app,
        "POST",
        &format!("/channels/{cid}/messages"),
        Some(json!({"content":"voici","attachments":[aid]})),
        Some(&owner),
    )
    .await;
    let url = msg["attachments"][0]["url"].as_str().unwrap().to_string();

    // Non-membre ne peut pas télécharger (gardé par VIEW du salon).
    let (s, _ct, _b) = get_raw(&app, &url, Some(&carol)).await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "téléchargement réservé aux membres du salon"
    );
    // Sans jeton non plus.
    let (s, _ct, _b) = get_raw(&app, &url, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // Propriété : bob (membre) téléverse ; owner ne peut PAS attacher la pièce de bob.
    let bob = token(&app, "bob", "b@at2.fr").await;
    join(&app, &owner, &gid, &bob).await;
    let (_s, batt) = upload(&app, &cid, &bob, "bob.txt", "text/plain", b"abc").await;
    let baid = batt["id"].as_str().unwrap().to_string();
    let (_s, m2) = send(
        &app,
        "POST",
        &format!("/channels/{cid}/messages"),
        Some(json!({"content":"essai","attachments":[baid]})),
        Some(&owner),
    )
    .await;
    assert_eq!(
        m2["attachments"].as_array().unwrap().len(),
        0,
        "on n'attache que ses propres pièces jointes"
    );
}
