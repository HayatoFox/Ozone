//! Gateway WebSocket (socle Phase 1) : HELLO / IDENTIFY / HEARTBEAT / READY + diffusion d'événements.
//! Cf. docs/05-gateway-temps-reel.md.

use crate::crypto;
use crate::permissions as pg;
use crate::state::{AppState, EventScope};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use ozone_proto::gateway::{opcode, GatewayFrame};
use ozone_proto::{perms, Snowflake};
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
                                        if send_frame(&mut tx, &f).await.is_err() { break; }
                                        // Présence : 1ère connexion → en ligne, diffusé aux guildes partagées.
                                        if st.presence.connect(uid.as_i64()) {
                                            broadcast_presence(&st, uid.as_i64()).await;
                                        }
                                    }
                                    None => {
                                        let f = GatewayFrame::with_data(opcode::INVALID_SESSION, json!(false));
                                        let _ = send_frame(&mut tx, &f).await;
                                    }
                                }
                            }
                            opcode::HEARTBEAT => {
                                let f = GatewayFrame::new(opcode::HEARTBEAT_ACK);
                                if send_frame(&mut tx, &f).await.is_err() { break; }
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
                    if let Some(uid) = authed {
                        // Routage pub/sub : on ne pousse l'événement que si l'utilisateur y a droit.
                        if should_deliver(&st, uid.as_i64(), &event.scope).await {
                            seq += 1;
                            let f = GatewayFrame::dispatch(event.t, event.d, seq);
                            if send_frame(&mut tx, &f).await.is_err() { break; }
                        }
                    }
                }
            }
        }
    }

    // Déconnexion : si c'était la dernière session de l'utilisateur, il passe hors ligne.
    if let Some(uid) = authed {
        if st.presence.disconnect(uid.as_i64()) {
            broadcast_presence(&st, uid.as_i64()).await;
        }
    }
}

/// Diffuse le statut **effectif** d'un utilisateur aux guildes dont il est membre
/// (portée `Guild` ⇒ uniquement les membres concernés, via `should_deliver`).
pub async fn broadcast_presence(st: &AppState, uid: i64) {
    let (status, custom) = st.presence.effective(uid);
    let payload = json!({
        "user_id": uid.to_string(),
        "status": status,
        "custom_status": custom,
    });
    let guilds = sqlx::query("SELECT guild_id FROM guild_members WHERE user_id = ?")
        .bind(uid)
        .fetch_all(&st.pool)
        .await
        .unwrap_or_default();
    for g in guilds {
        let gid: i64 = g.get("guild_id");
        st.publish(EventScope::Guild(gid), "PRESENCE_UPDATE", payload.clone());
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

/// Décide si la session d'un utilisateur doit recevoir un événement, selon sa **portée**
/// (membre de la guilde, droit de voir le salon, destinataire du MP, ou utilisateur ciblé).
pub async fn should_deliver(st: &AppState, user_id: i64, scope: &EventScope) -> bool {
    match scope {
        EventScope::Global => true,
        EventScope::User(u) => *u == user_id,
        EventScope::Guild(g) => is_guild_member(st, *g, user_id).await,
        EventScope::Dm(c) => is_dm_recipient(st, *c, user_id).await,
        EventScope::Channel {
            guild_id,
            channel_id,
        } => {
            let owner = match pg::guild_owner(&st.pool, *guild_id).await {
                Ok(Some(o)) => o,
                _ => return false,
            };
            match pg::channel_permissions(&st.pool, *guild_id, owner, *channel_id, user_id).await {
                Ok(p) => perms::has(p, perms::VIEW_CHANNEL),
                Err(_) => false,
            }
        }
    }
}

async fn is_guild_member(st: &AppState, guild_id: i64, user_id: i64) -> bool {
    sqlx::query("SELECT 1 FROM guild_members WHERE guild_id = ? AND user_id = ?")
        .bind(guild_id)
        .bind(user_id)
        .fetch_optional(&st.pool)
        .await
        .ok()
        .flatten()
        .is_some()
}

async fn is_dm_recipient(st: &AppState, channel_id: i64, user_id: i64) -> bool {
    sqlx::query("SELECT 1 FROM dm_recipients WHERE channel_id = ? AND user_id = ?")
        .bind(channel_id)
        .bind(user_id)
        .fetch_optional(&st.pool)
        .await
        .ok()
        .flatten()
        .is_some()
}
