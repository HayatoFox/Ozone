//! Routes guildes / salons / messages (socle Phase 1). Diffuse `MESSAGE_CREATE` via la Gateway.

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::state::{AppState, HubEvent};
use axum::extract::{Path, State};
use axum::Json;
use ozone_proto::dto::{Channel, CreateChannel, CreateGuild, CreateMessage, Guild, Message, User};
use ozone_proto::Snowflake;
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

// ───────────────────────────── Guildes ─────────────────────────────

/// `POST /guilds` — crée une guilde (+ un salon « général » par défaut).
pub async fn create_guild(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateGuild>,
) -> AppResult<Json<Guild>> {
    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(AppError::bad_request(
            "nom de guilde invalide (1 à 100 caractères)",
        ));
    }
    let id = st.ids.next();
    let now = now_ms();
    sqlx::query(
        "INSERT INTO guilds (id, name, owner_id, icon_id, created_at) VALUES (?, ?, ?, NULL, ?)",
    )
    .bind(id.as_i64())
    .bind(name)
    .bind(user.id.as_i64())
    .bind(now)
    .execute(&st.pool)
    .await?;
    sqlx::query(
        "INSERT INTO guild_members (guild_id, user_id, nick, joined_at) VALUES (?, ?, NULL, ?)",
    )
    .bind(id.as_i64())
    .bind(user.id.as_i64())
    .bind(now)
    .execute(&st.pool)
    .await?;
    let chan = st.ids.next();
    sqlx::query(
        "INSERT INTO channels (id, guild_id, type, name, topic, position, parent_id, created_at) VALUES (?, ?, 0, 'général', NULL, 0, NULL, ?)",
    )
    .bind(chan.as_i64())
    .bind(id.as_i64())
    .bind(now)
    .execute(&st.pool)
    .await?;
    Ok(Json(Guild {
        id,
        name: name.to_string(),
        owner_id: user.id,
        icon_id: None,
    }))
}

/// `GET /guilds` — guildes dont l'utilisateur est membre.
pub async fn list_guilds(
    State(st): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<Guild>>> {
    let rows = sqlx::query(
        "SELECT g.id, g.name, g.owner_id, g.icon_id FROM guilds g \
         JOIN guild_members m ON m.guild_id = g.id WHERE m.user_id = ? ORDER BY g.id",
    )
    .bind(user.id.as_i64())
    .fetch_all(&st.pool)
    .await?;
    let guilds = rows
        .into_iter()
        .map(|r| Guild {
            id: Snowflake::from_i64(r.get::<i64, _>("id")),
            name: r.get("name"),
            owner_id: Snowflake::from_i64(r.get::<i64, _>("owner_id")),
            icon_id: r.get("icon_id"),
        })
        .collect();
    Ok(Json(guilds))
}

// ───────────────────────────── Salons ─────────────────────────────

/// `POST /guilds/:guild_id/channels`
pub async fn create_channel(
    State(st): State<AppState>,
    user: AuthUser,
    Path(guild_id): Path<String>,
    Json(req): Json<CreateChannel>,
) -> AppResult<Json<Channel>> {
    let gid = parse_id(&guild_id)?;
    ensure_member(&st, gid, user.id).await?;
    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(AppError::bad_request("nom de salon invalide"));
    }
    let id = st.ids.next();
    sqlx::query(
        "INSERT INTO channels (id, guild_id, type, name, topic, position, parent_id, created_at) VALUES (?, ?, ?, ?, ?, 0, NULL, ?)",
    )
    .bind(id.as_i64())
    .bind(gid.as_i64())
    .bind(req.kind as i64)
    .bind(name)
    .bind(req.topic.as_deref())
    .bind(now_ms())
    .execute(&st.pool)
    .await?;
    Ok(Json(Channel {
        id,
        guild_id: Some(gid),
        kind: req.kind,
        name: name.to_string(),
        topic: req.topic,
        position: 0,
        parent_id: None,
    }))
}

/// `GET /guilds/:guild_id/channels`
pub async fn list_channels(
    State(st): State<AppState>,
    user: AuthUser,
    Path(guild_id): Path<String>,
) -> AppResult<Json<Vec<Channel>>> {
    let gid = parse_id(&guild_id)?;
    ensure_member(&st, gid, user.id).await?;
    let rows = sqlx::query(
        "SELECT id, guild_id, type AS kind, name, topic, position, parent_id FROM channels WHERE guild_id = ? ORDER BY position, id",
    )
    .bind(gid.as_i64())
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(rows.into_iter().map(row_to_channel).collect()))
}

// ───────────────────────────── Messages ─────────────────────────────

/// `GET /channels/:channel_id/messages`
pub async fn list_messages(
    State(st): State<AppState>,
    user: AuthUser,
    Path(channel_id): Path<String>,
) -> AppResult<Json<Vec<Message>>> {
    let cid = parse_id(&channel_id)?;
    ensure_channel_access(&st, cid, user.id).await?;
    let rows = sqlx::query(
        "SELECT m.id, m.channel_id, m.author_id, m.content, m.type AS kind, m.nonce, m.created_at, m.edited_at, \
                u.username, u.display_name, u.avatar_id \
         FROM messages m JOIN users u ON u.id = m.author_id \
         WHERE m.channel_id = ? ORDER BY m.id DESC LIMIT 50",
    )
    .bind(cid.as_i64())
    .fetch_all(&st.pool)
    .await?;
    let mut messages: Vec<Message> = rows.into_iter().map(row_to_message).collect();
    messages.reverse(); // ordre chronologique
    Ok(Json(messages))
}

/// `POST /channels/:channel_id/messages`
pub async fn create_message(
    State(st): State<AppState>,
    user: AuthUser,
    Path(channel_id): Path<String>,
    Json(req): Json<CreateMessage>,
) -> AppResult<Json<Message>> {
    let cid = parse_id(&channel_id)?;
    ensure_channel_access(&st, cid, user.id).await?;
    let content = req.content.trim_end();
    if content.is_empty() || content.chars().count() > 4000 {
        return Err(AppError::bad_request(
            "contenu de message invalide (1 à 4000 caractères)",
        ));
    }
    let id = st.ids.next();
    let now = now_ms();
    sqlx::query(
        "INSERT INTO messages (id, channel_id, author_id, content, type, nonce, created_at, edited_at) VALUES (?, ?, ?, ?, 0, ?, ?, NULL)",
    )
    .bind(id.as_i64())
    .bind(cid.as_i64())
    .bind(user.id.as_i64())
    .bind(content)
    .bind(req.nonce.as_deref())
    .bind(now)
    .execute(&st.pool)
    .await?;

    let author = fetch_user(&st, user.id).await?;
    let msg = Message {
        id,
        channel_id: cid,
        author,
        content: content.to_string(),
        kind: 0,
        created_at: now as u64,
        edited_at: None,
        nonce: req.nonce.clone(),
    };
    // Fan-out temps réel vers les sessions Gateway.
    let _ = st.hub.send(HubEvent {
        t: "MESSAGE_CREATE".into(),
        d: serde_json::to_value(&msg).unwrap_or_default(),
    });
    Ok(Json(msg))
}

// ───────────────────────────── Helpers ─────────────────────────────

fn parse_id(s: &str) -> AppResult<Snowflake> {
    s.parse::<u64>()
        .map(Snowflake::new)
        .map_err(|_| AppError::bad_request("identifiant invalide"))
}

async fn fetch_user(st: &AppState, id: Snowflake) -> AppResult<User> {
    let row = sqlx::query("SELECT username, display_name, avatar_id FROM users WHERE id = ?")
        .bind(id.as_i64())
        .fetch_one(&st.pool)
        .await?;
    Ok(User {
        id,
        username: row.get("username"),
        display_name: row.get("display_name"),
        avatar_id: row.get("avatar_id"),
        email: None,
    })
}

async fn ensure_member(st: &AppState, gid: Snowflake, uid: Snowflake) -> AppResult<()> {
    let ok = sqlx::query("SELECT 1 FROM guild_members WHERE guild_id = ? AND user_id = ?")
        .bind(gid.as_i64())
        .bind(uid.as_i64())
        .fetch_optional(&st.pool)
        .await?;
    if ok.is_none() {
        return Err(AppError::forbidden(
            "vous n'êtes pas membre de cette guilde",
        ));
    }
    Ok(())
}

async fn ensure_channel_access(st: &AppState, cid: Snowflake, uid: Snowflake) -> AppResult<()> {
    let row = sqlx::query("SELECT guild_id FROM channels WHERE id = ?")
        .bind(cid.as_i64())
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("salon introuvable"))?;
    match row.get::<Option<i64>, _>("guild_id") {
        Some(g) => ensure_member(st, Snowflake::from_i64(g), uid).await,
        None => Ok(()), // MP : accès géré ailleurs (hors socle Phase 1)
    }
}

fn row_to_channel(r: SqliteRow) -> Channel {
    Channel {
        id: Snowflake::from_i64(r.get::<i64, _>("id")),
        guild_id: r.get::<Option<i64>, _>("guild_id").map(Snowflake::from_i64),
        kind: r.get::<i64, _>("kind") as u8,
        name: r.get("name"),
        topic: r.get("topic"),
        position: r.get::<i64, _>("position") as i32,
        parent_id: r
            .get::<Option<i64>, _>("parent_id")
            .map(Snowflake::from_i64),
    }
}

fn row_to_message(r: SqliteRow) -> Message {
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
        nonce: r.get("nonce"),
    }
}
