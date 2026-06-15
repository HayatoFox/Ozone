//! Gateway WebSocket (socle Phase 1) : HELLO / IDENTIFY / HEARTBEAT / READY + diffusion d'événements.
//! Cf. docs/05-gateway-temps-reel.md.

use crate::crypto;
use crate::gateway_session::{create_session, resume_session, SessionConn};
use crate::permissions as pg;
use crate::state::{AppState, EventScope};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use ozone_proto::gateway::{opcode, GatewayFrame};
use ozone_proto::{perms, Snowflake};
use serde_json::json;
use sqlx::Row;
use tokio::sync::mpsc;

pub async fn ws_handler(ws: WebSocketUpgrade, State(st): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, st))
}

async fn handle_socket(socket: WebSocket, st: AppState) {
    let (mut tx, mut rx_ws) = socket.split();

    let hello = GatewayFrame::with_data(opcode::HELLO, json!({ "heartbeat_interval": 41250 }));
    if send_frame(&mut tx, &hello).await.is_err() {
        return;
    }

    // Attente du handshake : IDENTIFY (nouvelle session) ou RESUME (reprise). On boucle pour
    // tolérer un heartbeat ou un essai refusé avant l'établissement de la session.
    loop {
        let txt = match rx_ws.next().await {
            Some(Ok(Message::Text(t))) => t,
            Some(Ok(Message::Close(_))) | None => return,
            Some(Ok(_)) => continue, // ping/pong/binaire avant auth
            Some(Err(_)) => return,
        };
        let Ok(frame) = serde_json::from_str::<GatewayFrame>(&txt) else {
            continue;
        };

        match frame.op {
            opcode::IDENTIFY => {
                let Some(uid) = verify_token(&st, &frame) else {
                    let _ = send_frame(&mut tx, &invalid_session()).await;
                    continue;
                };
                // Nouvelle session résumable (l'acteur gère présence + tampon de rejeu).
                let (session_id, conn) = create_session(&st, uid.as_i64());
                let Some((mut sink_rx, current_seq)) = conn.attach(0).await else {
                    conn.close();
                    return;
                };
                let ready = build_ready(&st, uid, &session_id).await;
                let f = GatewayFrame::dispatch("READY", ready, current_seq);
                if send_frame(&mut tx, &f).await.is_err() {
                    conn.close();
                    return;
                }
                pump(&mut tx, &mut rx_ws, &conn, &mut sink_rx, &st, uid.as_i64()).await;
                return;
            }
            opcode::RESUME => {
                let Some(uid) = verify_token(&st, &frame) else {
                    let _ = send_frame(&mut tx, &invalid_session()).await;
                    continue;
                };
                let d = frame.d.clone().unwrap_or_default();
                let session_id = d
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let after_seq = d.get("seq").and_then(|v| v.as_u64()).unwrap_or(0);

                // Session inconnue/expirée, ou appartenant à un autre utilisateur ⇒ refus.
                let Some(conn) = resume_session(&st, &session_id, uid.as_i64()) else {
                    let _ = send_frame(&mut tx, &invalid_session()).await;
                    continue;
                };
                match conn.attach(after_seq).await {
                    Some((mut sink_rx, current_seq)) => {
                        let f = GatewayFrame::dispatch(
                            "RESUMED",
                            json!({ "session_id": session_id }),
                            current_seq,
                        );
                        if send_frame(&mut tx, &f).await.is_err() {
                            conn.close();
                            return;
                        }
                        pump(&mut tx, &mut rx_ws, &conn, &mut sink_rx, &st, uid.as_i64()).await;
                        return;
                    }
                    // Tampon dépassé (trop d'événements manqués) ⇒ le client doit re-IDENTIFY.
                    None => {
                        let _ = send_frame(&mut tx, &invalid_session()).await;
                        continue;
                    }
                }
            }
            opcode::HEARTBEAT => {
                if send_frame(&mut tx, &GatewayFrame::new(opcode::HEARTBEAT_ACK))
                    .await
                    .is_err()
                {
                    return;
                }
            }
            _ => {}
        }
    }
}

fn invalid_session() -> GatewayFrame {
    GatewayFrame::with_data(opcode::INVALID_SESSION, json!(false))
}

/// Extrait et vérifie le jeton d'accès d'une trame IDENTIFY/RESUME ⇒ l'identifiant utilisateur.
fn verify_token(st: &AppState, frame: &GatewayFrame) -> Option<Snowflake> {
    let token = frame
        .d
        .as_ref()
        .and_then(|d| d.get("token"))
        .and_then(|t| t.as_str())
        .unwrap_or_default();
    crypto::jwt_verify(&st.jwt_secret, token, "access")
        .and_then(|c| c.sub.parse::<u64>().ok())
        .map(Snowflake::new)
}

/// Boucle « socket attaché » : pousse les événements de l'acteur vers le WS et répond aux
/// heartbeats. À la sortie : `detach()` (coupure réseau, session résumable) ou `close()` (Close).
async fn pump(
    tx: &mut SplitSink<WebSocket, Message>,
    rx_ws: &mut SplitStream<WebSocket>,
    conn: &SessionConn,
    sink_rx: &mut mpsc::UnboundedReceiver<GatewayFrame>,
    st: &AppState,
    uid: i64,
) {
    loop {
        tokio::select! {
            out = sink_rx.recv() => match out {
                Some(frame) => {
                    if send_frame(tx, &frame).await.is_err() {
                        conn.detach();
                        return;
                    }
                }
                None => return, // l'acteur s'est arrêté (grâce expirée)
            },
            inc = rx_ws.next() => match inc {
                Some(Ok(Message::Text(txt))) => {
                    if let Ok(f) = serde_json::from_str::<GatewayFrame>(&txt) {
                        if f.op == opcode::HEARTBEAT {
                            if send_frame(tx, &GatewayFrame::new(opcode::HEARTBEAT_ACK))
                                .await
                                .is_err()
                            {
                                conn.detach();
                                return;
                            }
                        } else if f.op == opcode::VOICE_SPEAKING {
                            // Le client signale parle/se-tait. On relaie aux autres membres de SON
                            // salon vocal (salon lu côté serveur depuis voice_states → non spoofable).
                            let speaking = f
                                .d
                                .as_ref()
                                .and_then(|d| d.get("speaking"))
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            broadcast_speaking(st, uid, speaking).await;
                        }
                    }
                }
                Some(Ok(Message::Close(_))) => {
                    conn.close();
                    return;
                }
                None | Some(Err(_)) => {
                    conn.detach();
                    return;
                }
                _ => {}
            },
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

/// Diffuse l'indicateur « parle / se tait » d'un utilisateur aux autres membres de SON salon
/// vocal. Le salon est lu côté serveur depuis `voice_states` (l'uid vient de la session
/// authentifiée → non spoofable). Portée `Channel` ⇒ seuls les membres du salon le reçoivent.
async fn broadcast_speaking(st: &AppState, uid: i64, speaking: bool) {
    let row = sqlx::query("SELECT guild_id, channel_id FROM voice_states WHERE user_id = ?")
        .bind(uid)
        .fetch_optional(&st.pool)
        .await
        .ok()
        .flatten();
    let Some(row) = row else { return }; // pas connecté au vocal → rien à diffuser
    let gid: i64 = row.get("guild_id");
    let cid: Option<i64> = row.get("channel_id");
    let Some(cid) = cid else { return };
    st.publish(
        EventScope::Channel {
            guild_id: gid,
            channel_id: cid,
        },
        "VOICE_SPEAKING",
        json!({ "user_id": uid.to_string(), "speaking": speaking }),
    );
}

/// Diffuse le **profil public** (pseudo/avatar) mis à jour d'un utilisateur EN DIRECT à toutes
/// les vues qui l'affichent : ses propres sessions, ses **amis**, ses **co-destinataires de MP**
/// et toutes les **guildes partagées** (listes de membres, auteurs de messages…).
pub async fn broadcast_user_update(st: &AppState, uid: i64) {
    let Some(row) =
        sqlx::query("SELECT username, display_name, avatar_id, name_style FROM users WHERE id = ?")
            .bind(uid)
            .fetch_optional(&st.pool)
            .await
            .ok()
            .flatten()
    else {
        return;
    };
    let payload = json!({
        "id": uid.to_string(),
        "username": row.get::<String, _>("username"),
        "display_name": row.get::<Option<String>, _>("display_name"),
        "avatar_id": row.get::<Option<String>, _>("avatar_id"),
        // Propage aussi le style de pseudo (peut être null → réinitialise chez les pairs).
        "name_style": crate::util::parse_name_style(row.get("name_style")),
    });

    // Destinataires « par utilisateur » (dédupliqués) : soi + amis (deux sens) + co-MP.
    let mut users: std::collections::HashSet<i64> = std::collections::HashSet::new();
    users.insert(uid);
    if let Ok(rows) = sqlx::query(
        "SELECT target_id AS other FROM relationships WHERE user_id = ? AND type = 'friend' \
         UNION SELECT user_id AS other FROM relationships WHERE target_id = ? AND type = 'friend'",
    )
    .bind(uid)
    .bind(uid)
    .fetch_all(&st.pool)
    .await
    {
        for r in rows {
            users.insert(r.get::<i64, _>("other"));
        }
    }
    if let Ok(rows) = sqlx::query(
        "SELECT DISTINCT user_id FROM dm_recipients \
         WHERE channel_id IN (SELECT channel_id FROM dm_recipients WHERE user_id = ?) AND user_id != ?",
    )
    .bind(uid)
    .bind(uid)
    .fetch_all(&st.pool)
    .await
    {
        for r in rows {
            users.insert(r.get::<i64, _>("user_id"));
        }
    }
    for u in users {
        st.publish(EventScope::User(u), "USER_UPDATE", payload.clone());
    }

    // Guildes partagées (couvre les listes de membres + auteurs de messages).
    if let Ok(guilds) = sqlx::query("SELECT guild_id FROM guild_members WHERE user_id = ?")
        .bind(uid)
        .fetch_all(&st.pool)
        .await
    {
        for g in guilds {
            st.publish(EventScope::Guild(g.get::<i64, _>("guild_id")), "USER_UPDATE", payload.clone());
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

async fn build_ready(st: &AppState, uid: Snowflake, session_id: &str) -> serde_json::Value {
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
        "session_id": session_id,
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
