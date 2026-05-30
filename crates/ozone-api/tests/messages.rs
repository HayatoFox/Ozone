//! Test d'intégration des messages avancés : réponses, édition, réactions, épingles,
//! suppression (unitaire et en masse), pagination, frappe.

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
    let path = std::env::temp_dir().join(format!("ozone-test-msg-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Ozone Msg".into(),
        instance_description: None,
        registration_policy: "open".into(),
        instance_password: None,
        version: "0.1.0-test".into(),
    };
    build_app(bootstrap_state(&cfg).await.expect("bootstrap"))
}

async fn json_body(resp: axum::response::Response) -> Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

fn rq(method: &str, uri: &str, body: Option<Value>, token: &str) -> Request<Body> {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"));
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
    (status, json_body(resp).await)
}

fn find<'a>(arr: &'a Value, content: &str) -> Option<&'a Value> {
    arr.as_array()?.iter().find(|m| m["content"] == content)
}

#[tokio::test]
async fn messages_flow() {
    let app = app().await;

    // setup : Alice + guilde + salon général
    let (_, reg) = send(&app, {
        Request::builder()
            .method("POST")
            .uri("/auth/register")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"username":"alice","email":"a@x.fr","password":"motdepasse"}).to_string(),
            ))
            .unwrap()
    })
    .await;
    let tok = reg["access_token"].as_str().unwrap().to_string();
    let (_, guild) = send(&app, rq("POST", "/guilds", Some(json!({"name":"G"})), &tok)).await;
    let gid = guild["id"].as_str().unwrap().to_string();
    let (_, chans) = send(
        &app,
        rq("GET", &format!("/guilds/{gid}/channels"), None, &tok),
    )
    .await;
    let cid = chans[0]["id"].as_str().unwrap().to_string();

    // envoi de deux messages
    let (s, m1) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({"content":"un"})),
            &tok,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let m1_id = m1["id"].as_str().unwrap().to_string();
    let (_, m2) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({"content":"deux"})),
            &tok,
        ),
    )
    .await;
    let m2_id = m2["id"].as_str().unwrap().to_string();

    // réponse à m1
    let (s, m3) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{cid}/messages"),
            Some(json!({"content":"réponse","reply_to": m1_id})),
            &tok,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(m3["reference_id"], m1_id);
    assert_eq!(m3["referenced_message"]["content"], "un");
    let m3_id = m3["id"].as_str().unwrap().to_string();

    // édition de m2
    let (s, edited) = send(
        &app,
        rq(
            "PATCH",
            &format!("/channels/{cid}/messages/{m2_id}"),
            Some(json!({"content":"deux modifié"})),
            &tok,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(edited["content"], "deux modifié");
    assert!(!edited["edited_at"].is_null());

    // réaction sur m1
    let (s, _) = send(
        &app,
        rq(
            "PUT",
            &format!("/channels/{cid}/messages/{m1_id}/reactions/tada/@me"),
            None,
            &tok,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (_, list) = send(
        &app,
        rq("GET", &format!("/channels/{cid}/messages"), None, &tok),
    )
    .await;
    let m1v = find(&list, "un").unwrap();
    assert_eq!(m1v["reactions"][0]["emoji"], "tada");
    assert_eq!(m1v["reactions"][0]["count"], 1);
    assert_eq!(m1v["reactions"][0]["me"], true);

    // retrait de la réaction
    send(
        &app,
        rq(
            "DELETE",
            &format!("/channels/{cid}/messages/{m1_id}/reactions/tada/@me"),
            None,
            &tok,
        ),
    )
    .await;
    let (_, list) = send(
        &app,
        rq("GET", &format!("/channels/{cid}/messages"), None, &tok),
    )
    .await;
    assert_eq!(
        find(&list, "un").unwrap()["reactions"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    // épingle
    let (s, _) = send(
        &app,
        rq("PUT", &format!("/channels/{cid}/pins/{m1_id}"), None, &tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (_, pins) = send(
        &app,
        rq("GET", &format!("/channels/{cid}/pins"), None, &tok),
    )
    .await;
    assert_eq!(pins.as_array().unwrap().len(), 1);
    assert_eq!(pins[0]["content"], "un");
    send(
        &app,
        rq(
            "DELETE",
            &format!("/channels/{cid}/pins/{m1_id}"),
            None,
            &tok,
        ),
    )
    .await;
    let (_, pins) = send(
        &app,
        rq("GET", &format!("/channels/{cid}/pins"), None, &tok),
    )
    .await;
    assert_eq!(pins.as_array().unwrap().len(), 0);

    // frappe
    let (s, _) = send(
        &app,
        rq("POST", &format!("/channels/{cid}/typing"), None, &tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // pagination : limit=2 → les 2 derniers (m2, m3)
    let (_, page) = send(
        &app,
        rq(
            "GET",
            &format!("/channels/{cid}/messages?limit=2"),
            None,
            &tok,
        ),
    )
    .await;
    assert_eq!(page.as_array().unwrap().len(), 2);

    // before=m3 → messages avant m3 (m1, m2)
    let (_, before) = send(
        &app,
        rq(
            "GET",
            &format!("/channels/{cid}/messages?before={m3_id}"),
            None,
            &tok,
        ),
    )
    .await;
    assert!(find(&before, "un").is_some());
    assert!(find(&before, "réponse").is_none());

    // suppression en masse de m1 et m3
    let (s, res) = send(
        &app,
        rq(
            "POST",
            &format!("/channels/{cid}/messages/bulk-delete"),
            Some(json!({"messages":[m1_id, m3_id]})),
            &tok,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(res["deleted"], 2);

    // suppression unitaire de m2
    let (s, _) = send(
        &app,
        rq(
            "DELETE",
            &format!("/channels/{cid}/messages/{m2_id}"),
            None,
            &tok,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // plus aucun message
    let (_, list) = send(
        &app,
        rq("GET", &format!("/channels/{cid}/messages"), None, &tok),
    )
    .await;
    assert_eq!(list.as_array().unwrap().len(), 0);
}
