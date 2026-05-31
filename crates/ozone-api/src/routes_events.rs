//! Événements programmés d'une guilde (liste, création, modification, suppression, RSVP).
//! Cf. `docs/features/12-evenements.md`.
//!
//! `entity_type` : 1 = stage, 2 = vocal, 3 = externe.
//! `status` : 1 = programmé, 2 = actif, 3 = terminé, 4 = annulé.

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::permissions as pg;
use crate::state::{AppState, EventScope};
use crate::util::parse_i64;
use axum::extract::{Path, State};
use axum::Json;
use ozone_proto::dto::{CreateEvent, ScheduledEvent, UpdateEvent};
use ozone_proto::Snowflake;
use serde_json::{json, Value};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

// ───────────────────────── Helpers privés ─────────────────────────

/// Convertit une ligne SQLite (+ un nombre d'intéressés) en DTO [`ScheduledEvent`].
fn row_to_event(r: &SqliteRow, interested_count: i64) -> ScheduledEvent {
    ScheduledEvent {
        id: Snowflake::from_i64(r.get::<i64, _>("id")),
        guild_id: Snowflake::from_i64(r.get::<i64, _>("guild_id")),
        channel_id: r
            .get::<Option<i64>, _>("channel_id")
            .map(Snowflake::from_i64),
        creator_id: Snowflake::from_i64(r.get::<i64, _>("creator_id")),
        name: r.get("name"),
        description: r.get::<Option<String>, _>("description"),
        cover_id: r.get::<Option<String>, _>("cover_id"),
        entity_type: r.get::<i64, _>("entity_type") as u8,
        location: r.get::<Option<String>, _>("location"),
        scheduled_start: r.get::<i64, _>("scheduled_start"),
        scheduled_end: r.get::<Option<i64>, _>("scheduled_end"),
        status: r.get::<i64, _>("status") as u8,
        interested_count,
        created_at: r.get::<i64, _>("created_at") as u64,
    }
}

/// Charge un événement par `(guild_id, event_id)` ; renvoie 404 s'il est absent.
async fn fetch_event(st: &AppState, gid: i64, eid: i64) -> AppResult<SqliteRow> {
    sqlx::query(
        "SELECT id, guild_id, channel_id, creator_id, name, description, cover_id, \
         entity_type, location, scheduled_start, scheduled_end, status, created_at \
         FROM scheduled_events WHERE id = ? AND guild_id = ?",
    )
    .bind(eid)
    .bind(gid)
    .fetch_optional(&st.pool)
    .await?
    .ok_or_else(|| AppError::not_found("événement introuvable"))
}

/// Nombre d'utilisateurs intéressés par un événement.
async fn interested_count(st: &AppState, eid: i64) -> AppResult<i64> {
    let row = sqlx::query("SELECT COUNT(*) AS n FROM event_interested WHERE event_id = ?")
        .bind(eid)
        .fetch_one(&st.pool)
        .await?;
    Ok(row.get::<i64, _>("n"))
}

/// Vérifie que `channel_id` existe **et** appartient à `gid`.
async fn ensure_channel_in_guild(st: &AppState, gid: i64, channel_id: i64) -> AppResult<()> {
    let row = sqlx::query("SELECT guild_id FROM channels WHERE id = ?")
        .bind(channel_id)
        .fetch_optional(&st.pool)
        .await?;
    let ok = match row {
        Some(r) => r.get::<Option<i64>, _>("guild_id") == Some(gid),
        None => false,
    };
    if ok {
        Ok(())
    } else {
        Err(AppError::bad_request(
            "le salon doit appartenir à la guilde",
        ))
    }
}

/// Valide le nom d'un événement : longueur 1–100 après trim.
fn validate_name(name: &str) -> AppResult<String> {
    let trimmed = name.trim();
    if !(1..=100).contains(&trimmed.chars().count()) {
        return Err(AppError::bad_request(
            "le nom de l'événement doit faire entre 1 et 100 caractères",
        ));
    }
    Ok(trimmed.to_string())
}

// ───────────────────────── Handlers publics ─────────────────────────

/// `GET /guilds/:guild_id/events` — Liste les événements programmés de la guilde.
///
/// Réservé aux membres de la guilde.
pub async fn list_events(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
) -> AppResult<Json<Vec<ScheduledEvent>>> {
    let gid = parse_i64(&gid)?;
    pg::require_guild_member(&st.pool, gid, user.id.as_i64()).await?;
    let rows = sqlx::query(
        "SELECT id, guild_id, channel_id, creator_id, name, description, cover_id, \
         entity_type, location, scheduled_start, scheduled_end, status, created_at \
         FROM scheduled_events WHERE guild_id = ? ORDER BY scheduled_start ASC",
    )
    .bind(gid)
    .fetch_all(&st.pool)
    .await?;

    let mut events = Vec::with_capacity(rows.len());
    for r in &rows {
        let eid: i64 = r.get("id");
        let count = interested_count(&st, eid).await?;
        events.push(row_to_event(r, count));
    }
    Ok(Json(events))
}

/// `POST /guilds/:guild_id/events` — Crée un événement programmé.
///
/// Exige `CREATE_EVENTS` ou `MANAGE_EVENTS`.
pub async fn create_event(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
    Json(req): Json<CreateEvent>,
) -> AppResult<Json<ScheduledEvent>> {
    let gid = parse_i64(&gid)?;
    let uid = user.id.as_i64();
    pg::require_event_create(&st.pool, gid, uid).await?;

    let name = validate_name(&req.name)?;

    if !(1..=3).contains(&req.entity_type) {
        return Err(AppError::bad_request(
            "type d'événement invalide (1 = stage, 2 = vocal, 3 = externe)",
        ));
    }

    // Selon le type : salon (1/2) ou lieu externe (3).
    let (channel_id, location): (Option<i64>, Option<String>) = match req.entity_type {
        1 | 2 => {
            let cid = req.channel_id.map(|c| c.as_i64()).ok_or_else(|| {
                AppError::bad_request("un salon est requis pour ce type d'événement")
            })?;
            ensure_channel_in_guild(&st, gid, cid).await?;
            (Some(cid), None)
        }
        _ => {
            let loc = req
                .location
                .as_deref()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .ok_or_else(|| {
                    AppError::bad_request("un lieu est requis pour un événement externe")
                })?;
            if loc.chars().count() > 1000 {
                return Err(AppError::bad_request(
                    "le lieu doit faire au plus 1000 caractères",
                ));
            }
            (None, Some(loc.to_string()))
        }
    };

    if req.scheduled_start <= 0 {
        return Err(AppError::bad_request("la date de début doit être valide"));
    }
    if let Some(end) = req.scheduled_end {
        if end <= req.scheduled_start {
            return Err(AppError::bad_request(
                "la date de fin doit être postérieure au début",
            ));
        }
    }

    let id = st.ids.next();
    let now = now_ms();
    let description = req.description.as_deref();
    let cover_id = req.cover_id.as_deref();

    sqlx::query(
        "INSERT INTO scheduled_events \
         (id, guild_id, channel_id, creator_id, name, description, cover_id, entity_type, \
          location, scheduled_start, scheduled_end, status, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?)",
    )
    .bind(id.as_i64())
    .bind(gid)
    .bind(channel_id)
    .bind(uid)
    .bind(&name)
    .bind(description)
    .bind(cover_id)
    .bind(req.entity_type as i64)
    .bind(location.as_deref())
    .bind(req.scheduled_start)
    .bind(req.scheduled_end)
    .bind(now)
    .execute(&st.pool)
    .await?;

    let event = ScheduledEvent {
        id,
        guild_id: Snowflake::from_i64(gid),
        channel_id: channel_id.map(Snowflake::from_i64),
        creator_id: Snowflake::from_i64(uid),
        name,
        description: req.description,
        cover_id: req.cover_id,
        entity_type: req.entity_type,
        location,
        scheduled_start: req.scheduled_start,
        scheduled_end: req.scheduled_end,
        status: 1,
        interested_count: 0,
        created_at: now as u64,
    };
    st.publish(
        EventScope::Guild(gid),
        "GUILD_SCHEDULED_EVENT_CREATE",
        serde_json::to_value(&event).unwrap_or_default(),
    );
    Ok(Json(event))
}

/// `GET /guilds/:guild_id/events/:event_id` — Détail d'un événement.
///
/// Réservé aux membres de la guilde.
pub async fn get_event(
    State(st): State<AppState>,
    user: AuthUser,
    Path((gid, eid)): Path<(String, String)>,
) -> AppResult<Json<ScheduledEvent>> {
    let gid = parse_i64(&gid)?;
    let eid = parse_i64(&eid)?;
    pg::require_guild_member(&st.pool, gid, user.id.as_i64()).await?;
    let row = fetch_event(&st, gid, eid).await?;
    let count = interested_count(&st, eid).await?;
    Ok(Json(row_to_event(&row, count)))
}

/// `PATCH /guilds/:guild_id/events/:event_id` — Modifie un événement.
///
/// Exige `MANAGE_EVENTS`, ou `CREATE_EVENTS` si l'on en est le créateur.
pub async fn update_event(
    State(st): State<AppState>,
    user: AuthUser,
    Path((gid, eid)): Path<(String, String)>,
    Json(req): Json<UpdateEvent>,
) -> AppResult<Json<ScheduledEvent>> {
    let gid = parse_i64(&gid)?;
    let eid = parse_i64(&eid)?;
    let uid = user.id.as_i64();

    let row = fetch_event(&st, gid, eid).await?;
    let creator_id: i64 = row.get("creator_id");
    pg::require_event_manage(&st.pool, gid, uid, creator_id).await?;

    // Bornes temporelles effectives après application des champs fournis.
    let new_start = req
        .scheduled_start
        .unwrap_or_else(|| row.get::<i64, _>("scheduled_start"));
    let new_end = if req.scheduled_end.is_some() {
        req.scheduled_end
    } else {
        row.get::<Option<i64>, _>("scheduled_end")
    };

    if let Some(name) = &req.name {
        let name = validate_name(name)?;
        sqlx::query("UPDATE scheduled_events SET name = ? WHERE id = ? AND guild_id = ?")
            .bind(&name)
            .bind(eid)
            .bind(gid)
            .execute(&st.pool)
            .await?;
    }

    if let Some(description) = &req.description {
        sqlx::query("UPDATE scheduled_events SET description = ? WHERE id = ? AND guild_id = ?")
            .bind(description)
            .bind(eid)
            .bind(gid)
            .execute(&st.pool)
            .await?;
    }

    if let Some(cover_id) = &req.cover_id {
        sqlx::query("UPDATE scheduled_events SET cover_id = ? WHERE id = ? AND guild_id = ?")
            .bind(cover_id)
            .bind(eid)
            .bind(gid)
            .execute(&st.pool)
            .await?;
    }

    if let Some(channel_id) = req.channel_id {
        let cid = channel_id.as_i64();
        ensure_channel_in_guild(&st, gid, cid).await?;
        sqlx::query("UPDATE scheduled_events SET channel_id = ? WHERE id = ? AND guild_id = ?")
            .bind(cid)
            .bind(eid)
            .bind(gid)
            .execute(&st.pool)
            .await?;
    }

    if let Some(location) = &req.location {
        sqlx::query("UPDATE scheduled_events SET location = ? WHERE id = ? AND guild_id = ?")
            .bind(location)
            .bind(eid)
            .bind(gid)
            .execute(&st.pool)
            .await?;
    }

    if req.scheduled_start.is_some() || req.scheduled_end.is_some() {
        if let Some(end) = new_end {
            if end <= new_start {
                return Err(AppError::bad_request(
                    "la date de fin doit être postérieure au début",
                ));
            }
        }
        sqlx::query(
            "UPDATE scheduled_events SET scheduled_start = ?, scheduled_end = ? WHERE id = ? AND guild_id = ?",
        )
        .bind(new_start)
        .bind(new_end)
        .bind(eid)
        .bind(gid)
        .execute(&st.pool)
        .await?;
    }

    if let Some(status) = req.status {
        if !(1..=4).contains(&status) {
            return Err(AppError::bad_request(
                "statut invalide (1 = programmé, 2 = actif, 3 = terminé, 4 = annulé)",
            ));
        }
        sqlx::query("UPDATE scheduled_events SET status = ? WHERE id = ? AND guild_id = ?")
            .bind(status as i64)
            .bind(eid)
            .bind(gid)
            .execute(&st.pool)
            .await?;
    }

    let updated = fetch_event(&st, gid, eid).await?;
    let count = interested_count(&st, eid).await?;
    let event = row_to_event(&updated, count);
    st.publish(
        EventScope::Guild(gid),
        "GUILD_SCHEDULED_EVENT_UPDATE",
        serde_json::to_value(&event).unwrap_or_default(),
    );
    Ok(Json(event))
}

/// `DELETE /guilds/:guild_id/events/:event_id` — Supprime un événement.
///
/// Exige `MANAGE_EVENTS`, ou `CREATE_EVENTS` si l'on en est le créateur.
pub async fn delete_event(
    State(st): State<AppState>,
    user: AuthUser,
    Path((gid, eid)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let gid = parse_i64(&gid)?;
    let eid = parse_i64(&eid)?;
    let uid = user.id.as_i64();

    let row = fetch_event(&st, gid, eid).await?;
    let creator_id: i64 = row.get("creator_id");
    pg::require_event_manage(&st.pool, gid, uid, creator_id).await?;

    sqlx::query("DELETE FROM event_interested WHERE event_id = ?")
        .bind(eid)
        .execute(&st.pool)
        .await?;
    sqlx::query("DELETE FROM scheduled_events WHERE id = ? AND guild_id = ?")
        .bind(eid)
        .bind(gid)
        .execute(&st.pool)
        .await?;

    st.publish(
        EventScope::Guild(gid),
        "GUILD_SCHEDULED_EVENT_DELETE",
        json!({ "id": eid.to_string(), "guild_id": gid.to_string() }),
    );
    Ok(Json(json!({ "ok": true })))
}

/// `PUT /guilds/:guild_id/events/:event_id/interested` — Marquer son intérêt.
///
/// Réservé aux membres de la guilde. Idempotent.
pub async fn rsvp_event(
    State(st): State<AppState>,
    user: AuthUser,
    Path((gid, eid)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let gid = parse_i64(&gid)?;
    let eid = parse_i64(&eid)?;
    let uid = user.id.as_i64();
    pg::require_guild_member(&st.pool, gid, uid).await?;
    fetch_event(&st, gid, eid).await?;

    sqlx::query(
        "INSERT OR IGNORE INTO event_interested (event_id, user_id, created_at) VALUES (?, ?, ?)",
    )
    .bind(eid)
    .bind(uid)
    .bind(now_ms())
    .execute(&st.pool)
    .await?;

    let count = interested_count(&st, eid).await?;
    Ok(Json(json!({ "interested_count": count })))
}

/// `DELETE /guilds/:guild_id/events/:event_id/interested` — Retirer son intérêt.
///
/// Réservé aux membres de la guilde. Idempotent.
pub async fn unrsvp_event(
    State(st): State<AppState>,
    user: AuthUser,
    Path((gid, eid)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let gid = parse_i64(&gid)?;
    let eid = parse_i64(&eid)?;
    let uid = user.id.as_i64();
    pg::require_guild_member(&st.pool, gid, uid).await?;
    fetch_event(&st, gid, eid).await?;

    sqlx::query("DELETE FROM event_interested WHERE event_id = ? AND user_id = ?")
        .bind(eid)
        .bind(uid)
        .execute(&st.pool)
        .await?;

    let count = interested_count(&st, eid).await?;
    Ok(Json(json!({ "interested_count": count })))
}
