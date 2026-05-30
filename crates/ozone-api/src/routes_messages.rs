//! Messages : liste paginée, envoi (avec réponse), édition, suppression, réactions,
//! épingles, suppression en masse, indicateur de frappe. Cf. docs/features/04-messagerie.md.

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::permissions as pg;
use crate::state::{AppState, HubEvent};
use crate::util::parse_i64;
use axum::extract::{Path, Query, State};
use axum::Json;
use ozone_proto::dto::{BulkDelete, CreateMessage, EditMessage, Message, Reaction, User};
use ozone_proto::{perms, Snowflake};
use serde::Deserialize;
use serde_json::json;
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use std::collections::HashMap;

const MAX_PINS: i64 = 250;

const MSG_SELECT: &str =
    "SELECT m.id, m.channel_id, m.author_id, m.content, m.type AS kind, m.nonce, \
     m.created_at, m.edited_at, m.reference_id, m.pinned, u.username, u.display_name, u.avatar_id \
     FROM messages m JOIN users u ON u.id = m.author_id";

fn emit(st: &AppState, t: &str, d: serde_json::Value) {
    let _ = st.hub.send(HubEvent {
        t: t.to_string(),
        d,
    });
}

fn row_to_message_basic(r: SqliteRow) -> Message {
    Message {
        id: Snowflake::from_i64(r.get::<i64, _>("id")),
        channel_id: Snowflake::from_i64(r.get::<i64, _>("channel_id")),
        author: User {
            id: Snowflake::from_i64(r.get::<i64, _>("author_id")),
            username: r.get("username"),
            display_name: r.get("display_name"),
            avatar_id: r.get("avatar_id"),
            email: None,
        },
        content: r.get("content"),
        kind: r.get::<i64, _>("kind") as u8,
        created_at: r.get::<i64, _>("created_at") as u64,
        edited_at: r.get::<Option<i64>, _>("edited_at").map(|v| v as u64),
        pinned: r.get::<i64, _>("pinned") != 0,
        reactions: Vec::new(),
        reference_id: r
            .get::<Option<i64>, _>("reference_id")
            .map(Snowflake::from_i64),
        referenced_message: None,
        nonce: r.get("nonce"),
    }
}

/// Récupère un message **dans un salon donné** (pour inliner le message cité sans fuite inter-salons).
async fn fetch_referenced(st: &AppState, cid: i64, mid: i64) -> AppResult<Option<Message>> {
    let row = sqlx::query(&format!("{MSG_SELECT} WHERE m.id = ? AND m.channel_id = ?"))
        .bind(mid)
        .bind(cid)
        .fetch_optional(&st.pool)
        .await?;
    Ok(row.map(row_to_message_basic))
}

async fn fetch_message_in_channel(st: &AppState, cid: i64, mid: i64) -> AppResult<Message> {
    let row = sqlx::query(&format!("{MSG_SELECT} WHERE m.id = ? AND m.channel_id = ?"))
        .bind(mid)
        .bind(cid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("message introuvable"))?;
    Ok(row_to_message_basic(row))
}

/// Charge les agrégats de réactions pour un ensemble de messages.
async fn load_reactions(
    st: &AppState,
    ids: &[i64],
    user_id: i64,
) -> AppResult<HashMap<i64, Vec<Reaction>>> {
    let mut map: HashMap<i64, Vec<Reaction>> = HashMap::new();
    if ids.is_empty() {
        return Ok(map);
    }
    let list = ids
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT message_id, emoji, COUNT(*) AS count, MAX(CASE WHEN user_id = ? THEN 1 ELSE 0 END) AS me \
         FROM reactions WHERE message_id IN ({list}) GROUP BY message_id, emoji ORDER BY MIN(created_at)"
    );
    let rows = sqlx::query(&sql).bind(user_id).fetch_all(&st.pool).await?;
    for r in rows {
        map.entry(r.get::<i64, _>("message_id"))
            .or_default()
            .push(Reaction {
                emoji: r.get("emoji"),
                count: r.get::<i64, _>("count"),
                me: r.get::<i64, _>("me") != 0,
            });
    }
    Ok(map)
}

async fn hydrate(st: &AppState, mut msg: Message, user_id: i64) -> AppResult<Message> {
    let map = load_reactions(st, &[msg.id.as_i64()], user_id).await?;
    if let Some(rs) = map.get(&msg.id.as_i64()) {
        msg.reactions = rs.clone();
    }
    if let Some(ref_id) = msg.reference_id {
        msg.referenced_message = fetch_referenced(st, msg.channel_id.as_i64(), ref_id.as_i64())
            .await?
            .map(Box::new);
    }
    Ok(msg)
}

// ───────────────────────────── Liste paginée ─────────────────────────────

#[derive(Debug, Deserialize)]
pub struct MsgQuery {
    limit: Option<i64>,
    before: Option<String>,
    after: Option<String>,
    around: Option<String>,
}

/// `GET /channels/:channel_id/messages?limit=&before=&after=&around=`
pub async fn list_messages(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
    Query(q): Query<MsgQuery>,
) -> AppResult<Json<Vec<Message>>> {
    let cid = parse_i64(&cid)?;
    pg::require_channel_perm(
        &st.pool,
        cid,
        user.id.as_i64(),
        perms::VIEW_CHANNEL | perms::READ_MESSAGE_HISTORY,
    )
    .await?;
    let limit = q.limit.unwrap_or(50).clamp(1, 100);

    let rows = if let Some(before) = q.before {
        let b = parse_i64(&before)?;
        sqlx::query(&format!(
            "{MSG_SELECT} WHERE m.channel_id = ? AND m.id < ? ORDER BY m.id DESC LIMIT ?"
        ))
        .bind(cid)
        .bind(b)
        .bind(limit)
        .fetch_all(&st.pool)
        .await?
    } else if let Some(after) = q.after {
        let a = parse_i64(&after)?;
        sqlx::query(&format!(
            "{MSG_SELECT} WHERE m.channel_id = ? AND m.id > ? ORDER BY m.id ASC LIMIT ?"
        ))
        .bind(cid)
        .bind(a)
        .bind(limit)
        .fetch_all(&st.pool)
        .await?
    } else if let Some(around) = q.around {
        let a = parse_i64(&around)?;
        let half = (limit / 2).max(1);
        let mut rows = sqlx::query(&format!(
            "{MSG_SELECT} WHERE m.channel_id = ? AND m.id <= ? ORDER BY m.id DESC LIMIT ?"
        ))
        .bind(cid)
        .bind(a)
        .bind(half + 1)
        .fetch_all(&st.pool)
        .await?;
        let mut after_rows = sqlx::query(&format!(
            "{MSG_SELECT} WHERE m.channel_id = ? AND m.id > ? ORDER BY m.id ASC LIMIT ?"
        ))
        .bind(cid)
        .bind(a)
        .bind(half)
        .fetch_all(&st.pool)
        .await?;
        rows.append(&mut after_rows);
        rows
    } else {
        sqlx::query(&format!(
            "{MSG_SELECT} WHERE m.channel_id = ? ORDER BY m.id DESC LIMIT ?"
        ))
        .bind(cid)
        .bind(limit)
        .fetch_all(&st.pool)
        .await?
    };

    let mut msgs: Vec<Message> = rows.into_iter().map(row_to_message_basic).collect();
    msgs.sort_by_key(|m| m.id.as_i64());

    let ids: Vec<i64> = msgs.iter().map(|m| m.id.as_i64()).collect();
    let reactions = load_reactions(&st, &ids, user.id.as_i64()).await?;
    for m in &mut msgs {
        if let Some(rs) = reactions.get(&m.id.as_i64()) {
            m.reactions = rs.clone();
        }
        if let Some(ref_id) = m.reference_id {
            m.referenced_message = fetch_referenced(&st, m.channel_id.as_i64(), ref_id.as_i64())
                .await?
                .map(Box::new);
        }
    }
    Ok(Json(msgs))
}

// ───────────────────────────── Envoi / édition / suppression ─────────────────────────────

/// `POST /channels/:channel_id/messages`
pub async fn create_message(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
    Json(req): Json<CreateMessage>,
) -> AppResult<Json<Message>> {
    let cid = parse_i64(&cid)?;
    pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::SEND_MESSAGES).await?;
    let content = req.content.trim_end();
    if content.is_empty() || content.chars().count() > 4000 {
        return Err(AppError::bad_request(
            "contenu de message invalide (1 à 4000 caractères)",
        ));
    }

    let reference_id = match req.reply_to {
        Some(s) => {
            let rid = s.as_i64();
            let exists = sqlx::query("SELECT 1 FROM messages WHERE id = ? AND channel_id = ?")
                .bind(rid)
                .bind(cid)
                .fetch_optional(&st.pool)
                .await?;
            if exists.is_none() {
                return Err(AppError::bad_request("message de réponse introuvable"));
            }
            Some(rid)
        }
        None => None,
    };

    let id = st.ids.next();
    let now = now_ms();
    sqlx::query(
        "INSERT INTO messages (id, channel_id, author_id, content, type, nonce, reference_id, pinned, created_at, edited_at) \
         VALUES (?, ?, ?, ?, 0, ?, ?, 0, ?, NULL)",
    )
    .bind(id.as_i64())
    .bind(cid)
    .bind(user.id.as_i64())
    .bind(content)
    .bind(req.nonce.as_deref())
    .bind(reference_id)
    .bind(now)
    .execute(&st.pool)
    .await?;

    let msg = hydrate(
        &st,
        fetch_message_in_channel(&st, cid, id.as_i64()).await?,
        user.id.as_i64(),
    )
    .await?;
    emit(
        &st,
        "MESSAGE_CREATE",
        serde_json::to_value(&msg).unwrap_or_default(),
    );
    Ok(Json(msg))
}

/// `PATCH /channels/:channel_id/messages/:message_id`
pub async fn edit_message(
    State(st): State<AppState>,
    user: AuthUser,
    Path((cid, mid)): Path<(String, String)>,
    Json(req): Json<EditMessage>,
) -> AppResult<Json<Message>> {
    let cid = parse_i64(&cid)?;
    let mid = parse_i64(&mid)?;
    pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::VIEW_CHANNEL).await?;
    let existing = fetch_message_in_channel(&st, cid, mid).await?;
    if existing.author.id.as_i64() != user.id.as_i64() {
        return Err(AppError::forbidden(
            "vous n'êtes pas l'auteur de ce message",
        ));
    }
    let content = req.content.trim_end();
    if content.is_empty() || content.chars().count() > 4000 {
        return Err(AppError::bad_request(
            "contenu de message invalide (1 à 4000 caractères)",
        ));
    }
    sqlx::query("UPDATE messages SET content = ?, edited_at = ? WHERE id = ?")
        .bind(content)
        .bind(now_ms())
        .bind(mid)
        .execute(&st.pool)
        .await?;
    let msg = hydrate(
        &st,
        fetch_message_in_channel(&st, cid, mid).await?,
        user.id.as_i64(),
    )
    .await?;
    emit(
        &st,
        "MESSAGE_UPDATE",
        serde_json::to_value(&msg).unwrap_or_default(),
    );
    Ok(Json(msg))
}

/// `DELETE /channels/:channel_id/messages/:message_id`
pub async fn delete_message(
    State(st): State<AppState>,
    user: AuthUser,
    Path((cid, mid)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let cid = parse_i64(&cid)?;
    let mid = parse_i64(&mid)?;
    let (_gid, _owner, perms_acc) =
        pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::VIEW_CHANNEL).await?;
    let existing = fetch_message_in_channel(&st, cid, mid).await?;
    let is_author = existing.author.id.as_i64() == user.id.as_i64();
    if !is_author && !perms::has(perms_acc, perms::MANAGE_MESSAGES) {
        return Err(AppError::forbidden(
            "permissions insuffisantes pour supprimer ce message",
        ));
    }
    sqlx::query("DELETE FROM messages WHERE id = ?")
        .bind(mid)
        .execute(&st.pool)
        .await?;
    sqlx::query("DELETE FROM reactions WHERE message_id = ?")
        .bind(mid)
        .execute(&st.pool)
        .await?;
    emit(
        &st,
        "MESSAGE_DELETE",
        json!({ "id": mid.to_string(), "channel_id": cid.to_string() }),
    );
    Ok(Json(json!({ "ok": true })))
}

/// `POST /channels/:channel_id/messages/bulk-delete`
pub async fn bulk_delete(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
    Json(req): Json<BulkDelete>,
) -> AppResult<Json<serde_json::Value>> {
    let cid = parse_i64(&cid)?;
    pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::MANAGE_MESSAGES).await?;
    if req.messages.is_empty() || req.messages.len() > 100 {
        return Err(AppError::bad_request(
            "entre 1 et 100 messages par suppression en masse",
        ));
    }
    let mut deleted = Vec::new();
    for m in &req.messages {
        let mid = m.as_i64();
        let res = sqlx::query("DELETE FROM messages WHERE id = ? AND channel_id = ?")
            .bind(mid)
            .bind(cid)
            .execute(&st.pool)
            .await?;
        if res.rows_affected() > 0 {
            sqlx::query("DELETE FROM reactions WHERE message_id = ?")
                .bind(mid)
                .execute(&st.pool)
                .await?;
            deleted.push(mid.to_string());
        }
    }
    emit(
        &st,
        "MESSAGE_DELETE_BULK",
        json!({ "channel_id": cid.to_string(), "ids": deleted }),
    );
    Ok(Json(json!({ "deleted": deleted.len() })))
}

// ───────────────────────────── Réactions ─────────────────────────────

/// `PUT /channels/:channel_id/messages/:message_id/reactions/:emoji/@me`
pub async fn add_reaction(
    State(st): State<AppState>,
    user: AuthUser,
    Path((cid, mid, emoji)): Path<(String, String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let cid = parse_i64(&cid)?;
    let mid = parse_i64(&mid)?;
    pg::require_channel_perm(
        &st.pool,
        cid,
        user.id.as_i64(),
        perms::VIEW_CHANNEL | perms::READ_MESSAGE_HISTORY | perms::ADD_REACTIONS,
    )
    .await?;
    if emoji.is_empty() || emoji.chars().count() > 64 {
        return Err(AppError::bad_request("emoji invalide"));
    }
    fetch_message_in_channel(&st, cid, mid).await?; // existence
    sqlx::query("INSERT OR IGNORE INTO reactions (message_id, emoji, user_id, created_at) VALUES (?, ?, ?, ?)")
        .bind(mid)
        .bind(&emoji)
        .bind(user.id.as_i64())
        .bind(now_ms())
        .execute(&st.pool)
        .await?;
    emit(
        &st,
        "MESSAGE_REACTION_ADD",
        json!({ "channel_id": cid.to_string(), "message_id": mid.to_string(), "user_id": user.id.to_string(), "emoji": emoji }),
    );
    Ok(Json(json!({ "ok": true })))
}

/// `DELETE /channels/:channel_id/messages/:message_id/reactions/:emoji/@me`
pub async fn remove_reaction(
    State(st): State<AppState>,
    user: AuthUser,
    Path((cid, mid, emoji)): Path<(String, String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let cid = parse_i64(&cid)?;
    let mid = parse_i64(&mid)?;
    pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::VIEW_CHANNEL).await?;
    sqlx::query("DELETE FROM reactions WHERE message_id = ? AND emoji = ? AND user_id = ?")
        .bind(mid)
        .bind(&emoji)
        .bind(user.id.as_i64())
        .execute(&st.pool)
        .await?;
    emit(
        &st,
        "MESSAGE_REACTION_REMOVE",
        json!({ "channel_id": cid.to_string(), "message_id": mid.to_string(), "user_id": user.id.to_string(), "emoji": emoji }),
    );
    Ok(Json(json!({ "ok": true })))
}

// ───────────────────────────── Épingles ─────────────────────────────

/// `GET /channels/:channel_id/pins`
pub async fn list_pins(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
) -> AppResult<Json<Vec<Message>>> {
    let cid = parse_i64(&cid)?;
    pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::VIEW_CHANNEL).await?;
    let rows = sqlx::query(&format!(
        "{MSG_SELECT} WHERE m.channel_id = ? AND m.pinned = 1 ORDER BY m.id DESC"
    ))
    .bind(cid)
    .fetch_all(&st.pool)
    .await?;
    let mut msgs: Vec<Message> = rows.into_iter().map(row_to_message_basic).collect();
    let ids: Vec<i64> = msgs.iter().map(|m| m.id.as_i64()).collect();
    let reactions = load_reactions(&st, &ids, user.id.as_i64()).await?;
    for m in &mut msgs {
        if let Some(rs) = reactions.get(&m.id.as_i64()) {
            m.reactions = rs.clone();
        }
    }
    Ok(Json(msgs))
}

/// `PUT /channels/:channel_id/pins/:message_id`
pub async fn pin_message(
    State(st): State<AppState>,
    user: AuthUser,
    Path((cid, mid)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let cid = parse_i64(&cid)?;
    let mid = parse_i64(&mid)?;
    pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::PIN_MESSAGES).await?;
    fetch_message_in_channel(&st, cid, mid).await?;
    let count: i64 =
        sqlx::query("SELECT COUNT(*) AS c FROM messages WHERE channel_id = ? AND pinned = 1")
            .bind(cid)
            .fetch_one(&st.pool)
            .await?
            .get("c");
    if count >= MAX_PINS {
        return Err(AppError::bad_request(
            "nombre maximal de messages épinglés atteint (250)",
        ));
    }
    sqlx::query("UPDATE messages SET pinned = 1 WHERE id = ? AND channel_id = ?")
        .bind(mid)
        .bind(cid)
        .execute(&st.pool)
        .await?;
    emit(
        &st,
        "CHANNEL_PINS_UPDATE",
        json!({ "channel_id": cid.to_string() }),
    );
    Ok(Json(json!({ "ok": true })))
}

/// `DELETE /channels/:channel_id/pins/:message_id`
pub async fn unpin_message(
    State(st): State<AppState>,
    user: AuthUser,
    Path((cid, mid)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let cid = parse_i64(&cid)?;
    let mid = parse_i64(&mid)?;
    pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::PIN_MESSAGES).await?;
    sqlx::query("UPDATE messages SET pinned = 0 WHERE id = ? AND channel_id = ?")
        .bind(mid)
        .bind(cid)
        .execute(&st.pool)
        .await?;
    emit(
        &st,
        "CHANNEL_PINS_UPDATE",
        json!({ "channel_id": cid.to_string() }),
    );
    Ok(Json(json!({ "ok": true })))
}

// ───────────────────────────── Frappe ─────────────────────────────

/// `POST /channels/:channel_id/typing`
pub async fn typing(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let cid = parse_i64(&cid)?;
    pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::SEND_MESSAGES).await?;
    emit(
        &st,
        "TYPING_START",
        json!({ "channel_id": cid.to_string(), "user_id": user.id.to_string(), "timestamp": now_ms() }),
    );
    Ok(Json(json!({ "ok": true })))
}
