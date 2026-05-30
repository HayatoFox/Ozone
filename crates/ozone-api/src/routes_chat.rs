//! Routes guildes / salons / messages. Applique les permissions (cf. permissions.rs).
//! Diffuse `MESSAGE_CREATE` via la Gateway.

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::permissions as pg;
use crate::state::{AppState, HubEvent};
use crate::util::parse_i64;
use axum::extract::{Path, State};
use axum::Json;
use ozone_proto::dto::{Channel, CreateChannel, CreateGuild, CreateMessage, Guild, Message, User};
use ozone_proto::{perms, Snowflake};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

// ───────────────────────────── Guildes ─────────────────────────────

/// `POST /guilds` — crée une guilde (+ rôle @everyone + salon « général »).
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
    // Rôle @everyone (id = guild_id), permissions par défaut.
    sqlx::query(
        "INSERT INTO roles (id, guild_id, name, color, hoist, position, permissions, mentionable, managed, created_at) \
         VALUES (?, ?, '@everyone', 0, 0, 0, ?, 0, 1, ?)",
    )
    .bind(id.as_i64())
    .bind(id.as_i64())
    .bind(perms::DEFAULT_EVERYONE as i64)
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
    let gid = parse_i64(&guild_id)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::MANAGE_CHANNELS).await?;
    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(AppError::bad_request("nom de salon invalide"));
    }
    let id = st.ids.next();
    sqlx::query(
        "INSERT INTO channels (id, guild_id, type, name, topic, position, parent_id, created_at) VALUES (?, ?, ?, ?, ?, 0, NULL, ?)",
    )
    .bind(id.as_i64())
    .bind(gid)
    .bind(req.kind as i64)
    .bind(name)
    .bind(req.topic.as_deref())
    .bind(now_ms())
    .execute(&st.pool)
    .await?;
    Ok(Json(Channel {
        id,
        guild_id: Some(Snowflake::from_i64(gid)),
        kind: req.kind,
        name: name.to_string(),
        topic: req.topic,
        position: 0,
        parent_id: None,
    }))
}

/// `GET /guilds/:guild_id/channels` — uniquement les salons visibles par l'utilisateur.
pub async fn list_channels(
    State(st): State<AppState>,
    user: AuthUser,
    Path(guild_id): Path<String>,
) -> AppResult<Json<Vec<Channel>>> {
    let gid = parse_i64(&guild_id)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::VIEW_CHANNEL).await?;
    let owner = pg::guild_owner(&st.pool, gid)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?;
    let rows = sqlx::query(
        "SELECT id, guild_id, type AS kind, name, topic, position, parent_id FROM channels WHERE guild_id = ? ORDER BY position, id",
    )
    .bind(gid)
    .fetch_all(&st.pool)
    .await?;
    let mut out = Vec::new();
    for r in rows {
        let ch = row_to_channel(r);
        let p =
            pg::channel_permissions(&st.pool, gid, owner, ch.id.as_i64(), user.id.as_i64()).await?;
        if perms::has(p, perms::VIEW_CHANNEL) {
            out.push(ch);
        }
    }
    Ok(Json(out))
}

// ───────────────────────────── Messages ─────────────────────────────

/// `GET /channels/:channel_id/messages`
pub async fn list_messages(
    State(st): State<AppState>,
    user: AuthUser,
    Path(channel_id): Path<String>,
) -> AppResult<Json<Vec<Message>>> {
    let cid = parse_i64(&channel_id)?;
    pg::require_channel_perm(
        &st.pool,
        cid,
        user.id.as_i64(),
        perms::VIEW_CHANNEL | perms::READ_MESSAGE_HISTORY,
    )
    .await?;
    let rows = sqlx::query(
        "SELECT m.id, m.channel_id, m.author_id, m.content, m.type AS kind, m.nonce, m.created_at, m.edited_at, \
                u.username, u.display_name, u.avatar_id \
         FROM messages m JOIN users u ON u.id = m.author_id \
         WHERE m.channel_id = ? ORDER BY m.id DESC LIMIT 50",
    )
    .bind(cid)
    .fetch_all(&st.pool)
    .await?;
    let mut messages: Vec<Message> = rows.into_iter().map(row_to_message).collect();
    messages.reverse();
    Ok(Json(messages))
}

/// `POST /channels/:channel_id/messages`
pub async fn create_message(
    State(st): State<AppState>,
    user: AuthUser,
    Path(channel_id): Path<String>,
    Json(req): Json<CreateMessage>,
) -> AppResult<Json<Message>> {
    let cid = parse_i64(&channel_id)?;
    pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::SEND_MESSAGES).await?;
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
    .bind(cid)
    .bind(user.id.as_i64())
    .bind(content)
    .bind(req.nonce.as_deref())
    .bind(now)
    .execute(&st.pool)
    .await?;

    let author = fetch_user(&st, user.id).await?;
    let msg = Message {
        id,
        channel_id: Snowflake::from_i64(cid),
        author,
        content: content.to_string(),
        kind: 0,
        created_at: now as u64,
        edited_at: None,
        nonce: req.nonce.clone(),
    };
    let _ = st.hub.send(HubEvent {
        t: "MESSAGE_CREATE".into(),
        d: serde_json::to_value(&msg).unwrap_or_default(),
    });
    Ok(Json(msg))
}

// ───────────────────────────── Helpers ─────────────────────────────

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
