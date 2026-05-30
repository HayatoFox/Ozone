//! Gateway WebSocket (socle Phase 1) : HELLO / IDENTIFY / HEARTBEAT / READY + diffusion d'événements.
//! Cf. docs/05-gateway-temps-reel.md.

use crate::crypto;
use crate::state::AppState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use ozone_proto::gateway::{opcode, GatewayFrame};
use ozone_proto::Snowflake;
use serde_json::json;
use sqlx::Row;

pub async fn ws_handler(ws: WebSocketUpgrade, State(st): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, st))
}

async fn handle_socket(socket: WebSocket, st: AppState) {
    let (mut tx, mut rx_ws) = socket.split();
    let mut hub_rx = st.hub.subscribe();
    let mut seq: u64 = 0;
    let mut authed: Option<Snowflake> = None;

    let hello = GatewayFrame::with_data(opcode::HELLO, json!({ "heartbeat_interval": 41250 }));
    if send_frame(&mut tx, &hello).await.is_err() {
        return;
    }

    loop {
        tokio::select! {
            incoming = rx_ws.next() => {
                match incoming {
                    Some(Ok(Message::Text(txt))) => {
                        let Ok(frame) = serde_json::from_str::<GatewayFrame>(&txt) else { continue };
                        match frame.op {
                            opcode::IDENTIFY => {
                                let token = frame.d.as_ref()
                                    .and_then(|d| d.get("token"))
                                    .and_then(|t| t.as_str())
                                    .unwrap_or_default();
                                match crypto::jwt_verify(&st.jwt_secret, token, "access")
                                    .and_then(|c| c.sub.parse::<u64>().ok())
                                {
                                    Some(uid) => {
                                        let uid = Snowflake::new(uid);
                                        authed = Some(uid);
                                        seq += 1;
                                        let ready = build_ready(&st, uid).await;
                                        let f = GatewayFrame::dispatch("READY", ready, seq);
                                        if send_frame(&mut tx, &f).await.is_err() { return; }
                                    }
                                    None => {
                                        let f = GatewayFrame::with_data(opcode::INVALID_SESSION, json!(false));
                                        let _ = send_frame(&mut tx, &f).await;
                                    }
                                }
                            }
                            opcode::HEARTBEAT => {
                                let f = GatewayFrame::new(opcode::HEARTBEAT_ACK);
                                if send_frame(&mut tx, &f).await.is_err() { return; }
                            }
                            _ => {}
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
            ev = hub_rx.recv() => {
                if let Ok(event) = ev {
                    if authed.is_some() {
                        seq += 1;
                        let f = GatewayFrame::dispatch(event.t, event.d, seq);
                        if send_frame(&mut tx, &f).await.is_err() { break; }
                    }
                }
            }
        }
    }
}

async fn send_frame<S>(tx: &mut S, frame: &GatewayFrame) -> Result<(), ()>
where
    S: futures_util::Sink<Message> + Unpin,
{
    let txt = serde_json::to_string(frame).map_err(|_| ())?;
    tx.send(Message::Text(txt)).await.map_err(|_| ())
}

async fn build_ready(st: &AppState, uid: Snowflake) -> serde_json::Value {
    let user_row = sqlx::query("SELECT username, display_name, avatar_id FROM users WHERE id = ?")
        .bind(uid.as_i64())
        .fetch_optional(&st.pool)
        .await
        .ok()
        .flatten();
    let (username, display_name, avatar_id) = match user_row {
        Some(r) => (
            r.get::<String, _>("username"),
            r.get::<Option<String>, _>("display_name"),
            r.get::<Option<String>, _>("avatar_id"),
        ),
        None => (String::new(), None, None),
    };

    let guilds = sqlx::query(
        "SELECT g.id, g.name FROM guilds g JOIN guild_members m ON m.guild_id = g.id WHERE m.user_id = ?",
    )
    .bind(uid.as_i64())
    .fetch_all(&st.pool)
    .await
    .unwrap_or_default();
    let guild_list: Vec<serde_json::Value> = guilds
        .into_iter()
        .map(|r| {
            json!({
                "id": Snowflake::from_i64(r.get::<i64, _>("id")).to_string(),
                "name": r.get::<String, _>("name"),
            })
        })
        .collect();

    json!({
        "session_id": st.ids.next().to_string(),
        "user": {
            "id": uid.to_string(),
            "username": username,
            "display_name": display_name,
            "avatar_id": avatar_id,
        },
        "instance": { "id": st.instance.instance_id.to_string(), "name": st.instance.name },
        "guilds": guild_list,
    })
}
