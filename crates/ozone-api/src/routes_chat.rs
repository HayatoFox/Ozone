//! Routes guildes & salons (création, lecture, mise à jour, suppression, réordonnancement,
//! catégories, slowmode/NSFW). Applique les permissions. Cf. docs/features/03-salons.md.

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::permissions as pg;
use crate::state::{AppState, EventScope, HubEvent};
use crate::util::parse_i64;
use axum::extract::{Path, State};
use axum::Json;
use ozone_proto::dto::{
    Channel, ChannelPosition, CreateChannel, CreateGuild, Guild, UpdateChannel, UpdateGuild,
};
use ozone_proto::{perms, Snowflake};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

const CHANNEL_SELECT: &str =
    "SELECT id, guild_id, type AS kind, name, topic, position, parent_id, nsfw, rate_limit_per_user FROM channels";
const ALLOWED_KINDS: [u8; 7] = [0, 2, 4, 5, 13, 15, 16];
const MAX_SLOWMODE: i32 = 21_600; // 6 h

fn emit(st: &AppState, scope: EventScope, t: &str, d: serde_json::Value) {
    let _ = st.hub.send(HubEvent {
        t: t.to_string(),
        d,
        scope,
    });
}

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
    let guild = Guild {
        id,
        name: name.to_string(),
        owner_id: user.id,
        icon_id: None,
    };
    // Notifie le créateur (ses sessions) de la nouvelle guilde.
    st.publish(
        EventScope::User(user.id.as_i64()),
        "GUILD_CREATE",
        serde_json::to_value(&guild).unwrap_or_default(),
    );
    Ok(Json(guild))
}

/// `GET /guilds/:guild_id` — détail d'une guilde (membres uniquement).
pub async fn get_guild(
    State(st): State<AppState>,
    user: AuthUser,
    Path(guild_id): Path<String>,
) -> AppResult<Json<Guild>> {
    let gid = parse_i64(&guild_id)?;
    pg::require_guild_member(&st.pool, gid, user.id.as_i64()).await?;
    let g = sqlx::query("SELECT id, name, owner_id, icon_id FROM guilds WHERE id = ?")
        .bind(gid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?;
    Ok(Json(Guild {
        id: Snowflake::from_i64(g.get::<i64, _>("id")),
        name: g.get("name"),
        owner_id: Snowflake::from_i64(g.get::<i64, _>("owner_id")),
        icon_id: g.get("icon_id"),
    }))
}

/// `PATCH /guilds/:guild_id` — renommer / changer l'icône (`MANAGE_GUILD`).
pub async fn update_guild(
    State(st): State<AppState>,
    user: AuthUser,
    Path(guild_id): Path<String>,
    Json(req): Json<UpdateGuild>,
) -> AppResult<Json<Guild>> {
    let gid = parse_i64(&guild_id)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::MANAGE_GUILD).await?;
    let g = sqlx::query("SELECT id, name, owner_id, icon_id FROM guilds WHERE id = ?")
        .bind(gid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?;

    let name = match req.name {
        Some(n) => {
            let t = n.trim().to_string();
            if t.is_empty() || t.chars().count() > 100 {
                return Err(AppError::bad_request(
                    "nom de guilde invalide (1 à 100 caractères)",
                ));
            }
            t
        }
        None => g.get("name"),
    };
    let icon_id = match req.icon_id {
        Some(s) if s.trim().is_empty() => None,
        Some(s) => Some(s),
        None => g.get("icon_id"),
    };
    sqlx::query("UPDATE guilds SET name = ?, icon_id = ? WHERE id = ?")
        .bind(&name)
        .bind(icon_id.as_deref())
        .bind(gid)
        .execute(&st.pool)
        .await?;
    let guild = Guild {
        id: Snowflake::from_i64(gid),
        name,
        owner_id: Snowflake::from_i64(g.get::<i64, _>("owner_id")),
        icon_id,
    };
    st.publish(
        EventScope::Guild(gid),
        "GUILD_UPDATE",
        serde_json::to_value(&guild).unwrap_or_default(),
    );
    Ok(Json(guild))
}

/// `DELETE /guilds/:guild_id` — supprime la guilde et toutes ses données (propriétaire uniquement).
pub async fn delete_guild(
    State(st): State<AppState>,
    user: AuthUser,
    Path(guild_id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let gid = parse_i64(&guild_id)?;
    let owner = pg::guild_owner(&st.pool, gid)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?;
    if owner != user.id.as_i64() {
        return Err(AppError::forbidden(
            "seul le propriétaire peut supprimer la guilde",
        ));
    }
    // Membres avant suppression (pour notifier ensuite via portée individuelle).
    let members: Vec<i64> = sqlx::query("SELECT user_id FROM guild_members WHERE guild_id = ?")
        .bind(gid)
        .fetch_all(&st.pool)
        .await?
        .into_iter()
        .map(|r| r.get::<i64, _>("user_id"))
        .collect();

    // Cascade atomique. `IN (SELECT id FROM channels WHERE guild_id = ?)` cible les salons de la guilde.
    let ch = "(SELECT id FROM channels WHERE guild_id = ?)";
    let mut tx = st.pool.begin().await?;
    // Messages & dérivés (les déclencheurs FTS se synchronisent à la suppression des messages).
    for sql in [
        format!("DELETE FROM reactions WHERE message_id IN (SELECT id FROM messages WHERE channel_id IN {ch})"),
        format!("DELETE FROM mentions WHERE channel_id IN {ch}"),
        format!("DELETE FROM read_states WHERE channel_id IN {ch}"),
        format!("DELETE FROM messages WHERE channel_id IN {ch}"),
        format!("DELETE FROM channel_overwrites WHERE channel_id IN {ch}"),
        format!("DELETE FROM notification_settings WHERE (scope_type = 1 AND scope_id IN {ch}) OR (scope_type = 0 AND scope_id = ?)"),
        "DELETE FROM webhooks WHERE guild_id = ?".to_string(),
        "DELETE FROM event_interested WHERE event_id IN (SELECT id FROM scheduled_events WHERE guild_id = ?)".to_string(),
        "DELETE FROM scheduled_events WHERE guild_id = ?".to_string(),
        "DELETE FROM channels WHERE guild_id = ?".to_string(),
        "DELETE FROM emojis WHERE guild_id = ?".to_string(),
        "DELETE FROM stickers WHERE guild_id = ?".to_string(),
        "DELETE FROM soundboard_sounds WHERE guild_id = ?".to_string(),
        "DELETE FROM member_roles WHERE guild_id = ?".to_string(),
        "DELETE FROM roles WHERE guild_id = ?".to_string(),
        "DELETE FROM invites WHERE guild_id = ?".to_string(),
        "DELETE FROM guild_bans WHERE guild_id = ?".to_string(),
        "DELETE FROM audit_log WHERE guild_id = ?".to_string(),
        "DELETE FROM guild_members WHERE guild_id = ?".to_string(),
        "DELETE FROM guilds WHERE id = ?".to_string(),
    ] {
        // Chaque requête de cette cascade ne référence `gid` qu'une seule fois, sauf la ligne
        // `notification_settings` qui le lie deux fois (salons puis guilde).
        let binds = sql.matches('?').count();
        let mut q = sqlx::query(&sql);
        for _ in 0..binds {
            q = q.bind(gid);
        }
        q.execute(&mut *tx).await?;
    }
    tx.commit().await?;

    // Notifie chaque ancien membre (la guilde n'existe plus → portée individuelle).
    for uid in members {
        st.publish(
            EventScope::User(uid),
            "GUILD_DELETE",
            serde_json::json!({ "id": gid.to_string() }),
        );
    }
    Ok(Json(serde_json::json!({ "ok": true })))
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
    Ok(Json(
        rows.into_iter()
            .map(|r| Guild {
                id: Snowflake::from_i64(r.get::<i64, _>("id")),
                name: r.get("name"),
                owner_id: Snowflake::from_i64(r.get::<i64, _>("owner_id")),
                icon_id: r.get("icon_id"),
            })
            .collect(),
    ))
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
    if !ALLOWED_KINDS.contains(&req.kind) {
        return Err(AppError::bad_request("type de salon non supporté"));
    }
    validate_topic(&req.topic)?;
    let parent_id = match req.parent_id {
        Some(p) => {
            if req.kind == 4 {
                return Err(AppError::bad_request(
                    "une catégorie ne peut pas avoir de parent",
                ));
            }
            ensure_category(&st, gid, p.as_i64()).await?;
            Some(p.as_i64())
        }
        None => None,
    };
    let nsfw = req.nsfw.unwrap_or(false) as i64;
    let rate = req.rate_limit_per_user.unwrap_or(0).clamp(0, MAX_SLOWMODE) as i64;

    let maxpos: i64 =
        sqlx::query("SELECT COALESCE(MAX(position), 0) AS m FROM channels WHERE guild_id = ?")
            .bind(gid)
            .fetch_one(&st.pool)
            .await?
            .get("m");
    let position = maxpos + 1;
    let id = st.ids.next();
    sqlx::query(
        "INSERT INTO channels (id, guild_id, type, name, topic, position, parent_id, nsfw, rate_limit_per_user, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.as_i64())
    .bind(gid)
    .bind(req.kind as i64)
    .bind(name)
    .bind(req.topic.as_deref())
    .bind(position)
    .bind(parent_id)
    .bind(nsfw)
    .bind(rate)
    .bind(now_ms())
    .execute(&st.pool)
    .await?;

    let ch = fetch_channel(&st, id.as_i64()).await?;
    emit(
        &st,
        EventScope::Channel {
            guild_id: gid,
            channel_id: id.as_i64(),
        },
        "CHANNEL_CREATE",
        serde_json::to_value(&ch).unwrap_or_default(),
    );
    Ok(Json(ch))
}

/// `GET /guilds/:guild_id/channels` — uniquement les salons visibles.
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
    let rows = sqlx::query(&format!(
        "{CHANNEL_SELECT} WHERE guild_id = ? ORDER BY position, id"
    ))
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

/// `GET /channels/:channel_id`
pub async fn get_channel(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
) -> AppResult<Json<Channel>> {
    let cid = parse_i64(&cid)?;
    pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::VIEW_CHANNEL).await?;
    Ok(Json(fetch_channel(&st, cid).await?))
}

/// `PATCH /channels/:channel_id`
pub async fn update_channel(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
    Json(req): Json<UpdateChannel>,
) -> AppResult<Json<Channel>> {
    let cid = parse_i64(&cid)?;
    let (gid, _owner, _p) =
        pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::MANAGE_CHANNELS).await?;
    let cur = fetch_channel(&st, cid).await?;

    let name = match req.name {
        Some(n) => {
            let n = n.trim().to_string();
            if n.is_empty() || n.chars().count() > 100 {
                return Err(AppError::bad_request("nom de salon invalide"));
            }
            n
        }
        None => cur.name.clone(),
    };
    if req.topic.is_some() {
        validate_topic(&req.topic)?;
    }
    let topic = req.topic.or(cur.topic.clone());
    let nsfw = req.nsfw.unwrap_or(cur.nsfw) as i64;
    let rate = req
        .rate_limit_per_user
        .unwrap_or(cur.rate_limit_per_user)
        .clamp(0, MAX_SLOWMODE) as i64;
    let position = req.position.unwrap_or(cur.position) as i64;
    let parent_id = match req.parent_id {
        Some(p) => {
            if cur.kind == 4 {
                return Err(AppError::bad_request(
                    "une catégorie ne peut pas avoir de parent",
                ));
            }
            ensure_category(&st, gid, p.as_i64()).await?;
            Some(p.as_i64())
        }
        None => cur.parent_id.map(|s| s.as_i64()),
    };

    sqlx::query(
        "UPDATE channels SET name = ?, topic = ?, nsfw = ?, rate_limit_per_user = ?, position = ?, parent_id = ? WHERE id = ?",
    )
    .bind(&name)
    .bind(topic.as_deref())
    .bind(nsfw)
    .bind(rate)
    .bind(position)
    .bind(parent_id)
    .bind(cid)
    .execute(&st.pool)
    .await?;

    let ch = fetch_channel(&st, cid).await?;
    emit(
        &st,
        EventScope::Channel {
            guild_id: gid,
            channel_id: cid,
        },
        "CHANNEL_UPDATE",
        serde_json::to_value(&ch).unwrap_or_default(),
    );
    Ok(Json(ch))
}

/// `DELETE /channels/:channel_id`
pub async fn delete_channel(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let cid = parse_i64(&cid)?;
    let (gid, _owner, _p) =
        pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::MANAGE_CHANNELS).await?;
    // Les salons enfants d'une catégorie supprimée sont détachés (parent → NULL).
    sqlx::query("UPDATE channels SET parent_id = NULL WHERE parent_id = ?")
        .bind(cid)
        .execute(&st.pool)
        .await?;
    sqlx::query(
        "DELETE FROM reactions WHERE message_id IN (SELECT id FROM messages WHERE channel_id = ?)",
    )
    .bind(cid)
    .execute(&st.pool)
    .await?;
    sqlx::query("DELETE FROM messages WHERE channel_id = ?")
        .bind(cid)
        .execute(&st.pool)
        .await?;
    sqlx::query("DELETE FROM channel_overwrites WHERE channel_id = ?")
        .bind(cid)
        .execute(&st.pool)
        .await?;
    sqlx::query("DELETE FROM channels WHERE id = ?")
        .bind(cid)
        .execute(&st.pool)
        .await?;
    emit(
        &st,
        EventScope::Guild(gid),
        "CHANNEL_DELETE",
        serde_json::json!({ "id": cid.to_string(), "guild_id": gid.to_string() }),
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `PATCH /guilds/:guild_id/channels` — réordonnancement / déplacement entre catégories.
pub async fn reorder_channels(
    State(st): State<AppState>,
    user: AuthUser,
    Path(guild_id): Path<String>,
    Json(items): Json<Vec<ChannelPosition>>,
) -> AppResult<Json<serde_json::Value>> {
    let gid = parse_i64(&guild_id)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::MANAGE_CHANNELS).await?;
    if items.len() > 500 {
        return Err(AppError::bad_request("trop d'éléments"));
    }
    for it in &items {
        let cid = it.id.as_i64();
        let in_guild = sqlx::query("SELECT 1 FROM channels WHERE id = ? AND guild_id = ?")
            .bind(cid)
            .bind(gid)
            .fetch_optional(&st.pool)
            .await?
            .is_some();
        if !in_guild {
            return Err(AppError::not_found("salon hors de cette guilde"));
        }
        match it.parent_id {
            Some(p) => {
                ensure_category(&st, gid, p.as_i64()).await?;
                sqlx::query(
                    "UPDATE channels SET position = ?, parent_id = ? WHERE id = ? AND guild_id = ?",
                )
                .bind(it.position as i64)
                .bind(p.as_i64())
                .bind(cid)
                .bind(gid)
                .execute(&st.pool)
                .await?;
            }
            None => {
                sqlx::query("UPDATE channels SET position = ? WHERE id = ? AND guild_id = ?")
                    .bind(it.position as i64)
                    .bind(cid)
                    .bind(gid)
                    .execute(&st.pool)
                    .await?;
            }
        }
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ───────────────────────────── Helpers ─────────────────────────────

fn validate_topic(topic: &Option<String>) -> AppResult<()> {
    if let Some(t) = topic {
        if t.chars().count() > 1024 {
            return Err(AppError::bad_request("sujet trop long (max 1024)"));
        }
    }
    Ok(())
}

async fn ensure_category(st: &AppState, gid: i64, parent_id: i64) -> AppResult<()> {
    let row = sqlx::query("SELECT type FROM channels WHERE id = ? AND guild_id = ?")
        .bind(parent_id)
        .bind(gid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("catégorie parente introuvable"))?;
    if row.get::<i64, _>("type") != 4 {
        return Err(AppError::bad_request("le parent n'est pas une catégorie"));
    }
    Ok(())
}

async fn fetch_channel(st: &AppState, cid: i64) -> AppResult<Channel> {
    let row = sqlx::query(&format!("{CHANNEL_SELECT} WHERE id = ?"))
        .bind(cid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("salon introuvable"))?;
    Ok(row_to_channel(row))
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
        nsfw: r.get::<i64, _>("nsfw") != 0,
        rate_limit_per_user: r.get::<i64, _>("rate_limit_per_user") as i32,
    }
}
