//! Messages : liste paginée, envoi (avec réponse), édition, suppression, réactions,
//! épingles, suppression en masse, indicateur de frappe. Cf. docs/features/04-messagerie.md.

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::permissions as pg;
use crate::state::{AppState, EventScope, HubEvent};
use crate::util::parse_i64;
use axum::extract::{Path, Query, State};
use axum::Json;
use ozone_proto::dto::{
    BulkDelete, CreateMessage, EditMessage, Message, Reaction, SearchResponse, User,
};
use ozone_proto::{perms, Snowflake};
use serde::Deserialize;
use serde_json::json;
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use std::collections::HashMap;

const MAX_PINS: i64 = 250;

const MSG_SELECT: &str =
    "SELECT m.id, m.channel_id, m.author_id, m.content, m.type AS kind, m.nonce, \
     m.created_at, m.edited_at, m.reference_id, m.pinned, m.webhook_id, \
     m.author_name AS wh_name, m.author_avatar AS wh_avatar, \
     u.username, u.display_name, u.avatar_id \
     FROM messages m JOIN users u ON u.id = m.author_id";

async fn emit(st: &AppState, channel_id: i64, t: &str, d: serde_json::Value) {
    // Portée pub/sub = ce salon (MP ou salon de guilde), qui existe au moment de l'émission.
    let scope = match crate::permissions::channel_guild(&st.pool, channel_id).await {
        Ok(Some(Some(gid))) => EventScope::Channel {
            guild_id: gid,
            channel_id,
        },
        _ => EventScope::Dm(channel_id),
    };
    let _ = st.hub.send(HubEvent {
        t: t.to_string(),
        d,
        scope,
    });
}

fn row_to_message_basic(r: SqliteRow) -> Message {
    let webhook_id = r
        .get::<Option<i64>, _>("webhook_id")
        .map(Snowflake::from_i64);
    // Pour un message de webhook, le nom/avatar de remplacement priment sur ceux de l'auteur réel.
    let wh_name: Option<String> = r.get("wh_name");
    let wh_avatar: Option<String> = r.get("wh_avatar");
    let (display_name, avatar_id) = if webhook_id.is_some() {
        (
            wh_name.or_else(|| r.get("display_name")),
            wh_avatar.or_else(|| r.get("avatar_id")),
        )
    } else {
        (r.get("display_name"), r.get("avatar_id"))
    };
    Message {
        id: Snowflake::from_i64(r.get::<i64, _>("id")),
        channel_id: Snowflake::from_i64(r.get::<i64, _>("channel_id")),
        author: User {
            id: Snowflake::from_i64(r.get::<i64, _>("author_id")),
            username: r.get("username"),
            display_name,
            avatar_id,
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
        webhook_id,
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
    let (gid, _owner, perms_acc) =
        pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::SEND_MESSAGES).await?;
    let content = req.content.trim_end();
    if content.is_empty() || content.chars().count() > 4000 {
        return Err(AppError::bad_request(
            "contenu de message invalide (1 à 4000 caractères)",
        ));
    }
    // Timeout : un membre en sourdine ne peut pas écrire dans un salon de guilde.
    if gid != 0 {
        let until: Option<i64> = sqlx::query(
            "SELECT communication_disabled_until FROM guild_members WHERE guild_id = ? AND user_id = ?",
        )
        .bind(gid)
        .bind(user.id.as_i64())
        .fetch_optional(&st.pool)
        .await?
        .and_then(|r| r.get("communication_disabled_until"));
        if let Some(until) = until {
            if until > now_ms() {
                return Err(AppError::forbidden(
                    "vous êtes temporairement en sourdine (timeout)",
                ));
            }
        }
    }
    // Slowmode (sauf MANAGE_MESSAGES / MANAGE_CHANNELS / BYPASS_SLOWMODE).
    if !(perms::has(perms_acc, perms::MANAGE_MESSAGES)
        || perms::has(perms_acc, perms::MANAGE_CHANNELS)
        || perms::has(perms_acc, perms::BYPASS_SLOWMODE))
    {
        let rate: i64 = sqlx::query("SELECT rate_limit_per_user FROM channels WHERE id = ?")
            .bind(cid)
            .fetch_one(&st.pool)
            .await?
            .get("rate_limit_per_user");
        if rate > 0 {
            let last: Option<i64> = sqlx::query(
                "SELECT created_at FROM messages WHERE channel_id = ? AND author_id = ? ORDER BY id DESC LIMIT 1",
            )
            .bind(cid)
            .bind(user.id.as_i64())
            .fetch_optional(&st.pool)
            .await?
            .map(|r| r.get::<i64, _>("created_at"));
            if let Some(last) = last {
                if now_ms() - last < rate * 1000 {
                    return Err(AppError::too_many("slowmode actif, réessayez plus tard"));
                }
            }
        }
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
        cid,
        "MESSAGE_CREATE",
        serde_json::to_value(&msg).unwrap_or_default(),
    )
    .await;
    Ok(Json(msg))
}

/// Insère un message **émis par un webhook** et diffuse `MESSAGE_CREATE`.
/// `author_id` = créateur du webhook (pour la jointure `users`) ; le nom/avatar de
/// remplacement priment à l'affichage. Renvoie le message hydraté.
pub async fn insert_webhook_message(
    st: &AppState,
    channel_id: i64,
    webhook_id: i64,
    author_id: i64,
    name_override: Option<&str>,
    avatar_override: Option<&str>,
    content: &str,
) -> AppResult<Message> {
    let id = st.ids.next();
    let now = now_ms();
    sqlx::query(
        "INSERT INTO messages (id, channel_id, author_id, content, type, nonce, reference_id, pinned, webhook_id, author_name, author_avatar, created_at, edited_at) \
         VALUES (?, ?, ?, ?, 0, NULL, NULL, 0, ?, ?, ?, ?, NULL)",
    )
    .bind(id.as_i64())
    .bind(channel_id)
    .bind(author_id)
    .bind(content)
    .bind(webhook_id)
    .bind(name_override)
    .bind(avatar_override)
    .bind(now)
    .execute(&st.pool)
    .await?;
    let msg = hydrate(
        st,
        fetch_message_in_channel(st, channel_id, id.as_i64()).await?,
        author_id,
    )
    .await?;
    emit(
        st,
        channel_id,
        "MESSAGE_CREATE",
        serde_json::to_value(&msg).unwrap_or_default(),
    )
    .await;
    Ok(msg)
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
        cid,
        "MESSAGE_UPDATE",
        serde_json::to_value(&msg).unwrap_or_default(),
    )
    .await;
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
        cid,
        "MESSAGE_DELETE",
        json!({ "id": mid.to_string(), "channel_id": cid.to_string() }),
    )
    .await;
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
        cid,
        "MESSAGE_DELETE_BULK",
        json!({ "channel_id": cid.to_string(), "ids": deleted }),
    )
    .await;
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
        cid,
        "MESSAGE_REACTION_ADD",
        json!({ "channel_id": cid.to_string(), "message_id": mid.to_string(), "user_id": user.id.to_string(), "emoji": emoji }),
    )
    .await;
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
        cid,
        "MESSAGE_REACTION_REMOVE",
        json!({ "channel_id": cid.to_string(), "message_id": mid.to_string(), "user_id": user.id.to_string(), "emoji": emoji }),
    )
    .await;
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
        cid,
        "CHANNEL_PINS_UPDATE",
        json!({ "channel_id": cid.to_string() }),
    )
    .await;
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
        cid,
        "CHANNEL_PINS_UPDATE",
        json!({ "channel_id": cid.to_string() }),
    )
    .await;
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
        cid,
        "TYPING_START",
        json!({ "channel_id": cid.to_string(), "user_id": user.id.to_string(), "timestamp": now_ms() }),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

// ───────────────────────────── Recherche (FTS5) ─────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// Texte recherché (plein-texte). Vide = pas de filtre textuel.
    q: Option<String>,
    /// Restreindre à un salon (recherche de guilde).
    channel_id: Option<String>,
    author_id: Option<String>,
    /// `link` = messages contenant un lien (autres valeurs : non encore indexées → 0 résultat).
    has: Option<String>,
    pinned: Option<bool>,
    before: Option<String>,
    after: Option<String>,
    /// Tri : `recent` (défaut) | `old` | `relevance` (avec `q`).
    sort: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

/// Transforme le texte utilisateur en requête FTS5 sûre (chaque terme est une phrase entre
/// guillemets ⇒ aucun opérateur FTS injectable, aucune erreur de syntaxe). `None` si vide.
fn fts_query(q: &str) -> Option<String> {
    let terms: Vec<String> = q
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
}

/// Identifiants des salons d'une guilde que l'utilisateur peut **lire** (VIEW + historique).
async fn viewable_channel_ids(
    st: &AppState,
    guild_id: i64,
    owner_id: i64,
    user_id: i64,
) -> AppResult<Vec<i64>> {
    let rows = sqlx::query("SELECT id FROM channels WHERE guild_id = ?")
        .bind(guild_id)
        .fetch_all(&st.pool)
        .await?;
    let mut ids = Vec::new();
    for r in rows {
        let cid = r.get::<i64, _>("id");
        let p = pg::channel_permissions(&st.pool, guild_id, owner_id, cid, user_id).await?;
        if perms::has(p, perms::VIEW_CHANNEL | perms::READ_MESSAGE_HISTORY) {
            ids.push(cid);
        }
    }
    Ok(ids)
}

/// Exécute la recherche sur un ensemble de salons **déjà autorisés**.
async fn run_search(
    st: &AppState,
    channel_ids: &[i64],
    q: &SearchQuery,
    user_id: i64,
) -> AppResult<SearchResponse> {
    if channel_ids.is_empty() {
        return Ok(SearchResponse {
            total: 0,
            messages: Vec::new(),
        });
    }
    // `has` non pris en charge (hors `link`) : aucune pièce jointe indexée pour l'instant.
    let has_link = match q.has.as_deref() {
        None => None,
        Some("link") => Some(true),
        Some(_) => {
            return Ok(SearchResponse {
                total: 0,
                messages: Vec::new(),
            })
        }
    };
    let fts = q.q.as_deref().and_then(fts_query);
    let id_list = channel_ids
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",");

    // Filtres entiers : validés en i64 (donc sûrs à interpoler). Le texte FTS est **lié**.
    let mut filters = format!(" WHERE m.channel_id IN ({id_list})");
    if fts.is_some() {
        filters.push_str(" AND messages_fts MATCH ?");
    }
    if let Some(a) = &q.author_id {
        filters.push_str(&format!(" AND m.author_id = {}", parse_i64(a)?));
    }
    if let Some(b) = &q.before {
        filters.push_str(&format!(" AND m.id < {}", parse_i64(b)?));
    }
    if let Some(a) = &q.after {
        filters.push_str(&format!(" AND m.id > {}", parse_i64(a)?));
    }
    match q.pinned {
        Some(true) => filters.push_str(" AND m.pinned = 1"),
        Some(false) => filters.push_str(" AND m.pinned = 0"),
        None => {}
    }
    if has_link.is_some() {
        filters.push_str(" AND m.content LIKE '%http%'");
    }
    let join_fts = if fts.is_some() {
        " JOIN messages_fts ON messages_fts.rowid = m.id"
    } else {
        ""
    };

    // Total (mêmes filtres, sans tri ni pagination).
    let count_sql = format!("SELECT COUNT(*) AS c FROM messages m{join_fts}{filters}");
    let mut cq = sqlx::query(&count_sql);
    if let Some(f) = &fts {
        cq = cq.bind(f);
    }
    let total: i64 = cq.fetch_one(&st.pool).await?.get("c");

    let order = match q.sort.as_deref() {
        Some("old") => " ORDER BY m.id ASC",
        Some("relevance") if fts.is_some() => " ORDER BY messages_fts.rank",
        _ => " ORDER BY m.id DESC",
    };
    let limit = q.limit.unwrap_or(25).clamp(1, 50);
    let offset = q.offset.unwrap_or(0).max(0);
    let sql = format!("{MSG_SELECT}{join_fts}{filters}{order} LIMIT {limit} OFFSET {offset}");
    let mut query = sqlx::query(&sql);
    if let Some(f) = &fts {
        query = query.bind(f);
    }
    let rows = query.fetch_all(&st.pool).await?;
    let mut msgs: Vec<Message> = rows.into_iter().map(row_to_message_basic).collect();
    let ids: Vec<i64> = msgs.iter().map(|m| m.id.as_i64()).collect();
    let reactions = load_reactions(st, &ids, user_id).await?;
    for m in &mut msgs {
        if let Some(rs) = reactions.get(&m.id.as_i64()) {
            m.reactions = rs.clone();
        }
    }
    Ok(SearchResponse {
        total,
        messages: msgs,
    })
}

/// `GET /guilds/:guild_id/messages/search` — recherche sur toute la guilde, filtrée par permissions.
pub async fn search_guild(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
    Query(q): Query<SearchQuery>,
) -> AppResult<Json<SearchResponse>> {
    let gid = parse_i64(&gid)?;
    pg::require_guild_member(&st.pool, gid, user.id.as_i64()).await?;
    let owner = pg::guild_owner(&st.pool, gid)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?;
    let mut channels = viewable_channel_ids(&st, gid, owner, user.id.as_i64()).await?;
    // Restriction facultative à un salon (qui doit être dans l'ensemble autorisé).
    if let Some(c) = &q.channel_id {
        let target = parse_i64(c)?;
        channels.retain(|&id| id == target);
    }
    Ok(Json(
        run_search(&st, &channels, &q, user.id.as_i64()).await?,
    ))
}

/// `GET /channels/:channel_id/messages/search` — recherche dans un seul salon (guilde ou MP).
pub async fn search_channel(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
    Query(q): Query<SearchQuery>,
) -> AppResult<Json<SearchResponse>> {
    let cid = parse_i64(&cid)?;
    // Doit pouvoir lire le salon (gère salons de guilde et MP, avec surcharges).
    pg::require_channel_perm(
        &st.pool,
        cid,
        user.id.as_i64(),
        perms::VIEW_CHANNEL | perms::READ_MESSAGE_HISTORY,
    )
    .await?;
    Ok(Json(run_search(&st, &[cid], &q, user.id.as_i64()).await?))
}
