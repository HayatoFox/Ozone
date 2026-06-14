//! Marqueurs de lecture (`read states`) et réglages de notification (niveau + mute).
//! Cf. docs/features/13-notifications.md. La boîte de mentions vit dans `routes_messages`
//! (elle nécessite le rendu des messages et le filtrage par permission).

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::permissions as pg;
use crate::state::{AppState, EventScope};
use crate::util::parse_i64;
use axum::extract::{Path, State};
use axum::Json;
use ozone_proto::dto::{NotificationSetting, ReadState, SetNotificationSetting};
use ozone_proto::{perms, Snowflake};
use serde_json::{json, Value};
use sqlx::Row;

// ───────────────────────────── Marqueurs de lecture ─────────────────────────────

async fn read_state_of(st: &AppState, uid: i64, cid: i64) -> AppResult<ReadState> {
    let row = sqlx::query(
        "SELECT last_read_id, mention_count FROM read_states WHERE user_id = ? AND channel_id = ?",
    )
    .bind(uid)
    .bind(cid)
    .fetch_optional(&st.pool)
    .await?;
    Ok(ReadState {
        channel_id: Snowflake::from_i64(cid),
        last_read_id: Snowflake::from_i64(
            row.as_ref()
                .map(|r| r.get::<i64, _>("last_read_id"))
                .unwrap_or(0),
        ),
        mention_count: row.map(|r| r.get::<i64, _>("mention_count")).unwrap_or(0),
    })
}

/// `POST /channels/:channel_id/messages/:message_id/ack` — marque lu jusqu'à ce message.
pub async fn ack_message(
    State(st): State<AppState>,
    user: AuthUser,
    Path((cid, mid)): Path<(String, String)>,
) -> AppResult<Json<ReadState>> {
    let cid = parse_i64(&cid)?;
    let mid = parse_i64(&mid)?;
    let uid = user.id.as_i64();
    pg::require_channel_perm(&st.pool, cid, uid, perms::VIEW_CHANNEL).await?;
    sqlx::query(
        "INSERT INTO read_states (user_id, channel_id, last_read_id, mention_count) VALUES (?, ?, ?, 0) \
         ON CONFLICT(user_id, channel_id) DO UPDATE SET last_read_id = MAX(last_read_id, excluded.last_read_id), mention_count = 0",
    )
    .bind(uid)
    .bind(cid)
    .bind(mid)
    .execute(&st.pool)
    .await?;
    // Sync multi-sessions : les AUTRES sessions du même utilisateur effacent leur badge.
    st.publish(
        EventScope::User(uid),
        "MESSAGE_ACK",
        json!({ "channel_id": cid.to_string(), "last_read_id": mid.to_string() }),
    );
    Ok(Json(read_state_of(&st, uid, cid).await?))
}

/// `GET /users/@me/read-states` — synchronisation multi-appareils.
pub async fn list_read_states(
    State(st): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<ReadState>>> {
    let rows = sqlx::query(
        "SELECT channel_id, last_read_id, mention_count FROM read_states WHERE user_id = ?",
    )
    .bind(user.id.as_i64())
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| ReadState {
                channel_id: Snowflake::from_i64(r.get::<i64, _>("channel_id")),
                last_read_id: Snowflake::from_i64(r.get::<i64, _>("last_read_id")),
                mention_count: r.get::<i64, _>("mention_count"),
            })
            .collect(),
    ))
}

/// `POST /guilds/:guild_id/ack` — marque toute la guilde comme lue.
pub async fn ack_guild(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
) -> AppResult<Json<Value>> {
    let gid = parse_i64(&gid)?;
    let uid = user.id.as_i64();
    pg::require_guild_member(&st.pool, gid, uid).await?;
    let owner = pg::guild_owner(&st.pool, gid)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?;
    let channels = sqlx::query("SELECT id FROM channels WHERE guild_id = ?")
        .bind(gid)
        .fetch_all(&st.pool)
        .await?;
    for c in channels {
        let cid: i64 = c.get("id");
        let p = pg::channel_permissions(&st.pool, gid, owner, cid, uid).await?;
        if !perms::has(p, perms::VIEW_CHANNEL | perms::READ_MESSAGE_HISTORY) {
            continue;
        }
        let last: i64 =
            sqlx::query("SELECT COALESCE(MAX(id), 0) AS m FROM messages WHERE channel_id = ?")
                .bind(cid)
                .fetch_one(&st.pool)
                .await?
                .get("m");
        sqlx::query(
            "INSERT INTO read_states (user_id, channel_id, last_read_id, mention_count) VALUES (?, ?, ?, 0) \
             ON CONFLICT(user_id, channel_id) DO UPDATE SET last_read_id = MAX(last_read_id, excluded.last_read_id), mention_count = 0",
        )
        .bind(uid)
        .bind(cid)
        .bind(last)
        .execute(&st.pool)
        .await?;
    }
    // Sync multi-sessions : la guilde entière passe lue sur les autres appareils.
    st.publish(
        EventScope::User(uid),
        "GUILD_ACK",
        json!({ "guild_id": gid.to_string() }),
    );
    Ok(Json(json!({ "ok": true })))
}

// ───────────────────────────── Réglages de notification ─────────────────────────────

fn valid_level(scope_type: u8, level: u8) -> bool {
    // Salon : 0..=3 (3 = hériter du serveur). Guilde : 0..=2.
    match scope_type {
        1 => level <= 3,
        _ => level <= 2,
    }
}

async fn put_setting(
    st: &AppState,
    uid: i64,
    scope_type: u8,
    scope_id: i64,
    req: &SetNotificationSetting,
) -> AppResult<NotificationSetting> {
    let existing = sqlx::query(
        "SELECT level, muted_until FROM notification_settings WHERE user_id = ? AND scope_type = ? AND scope_id = ?",
    )
    .bind(uid)
    .bind(scope_type as i64)
    .bind(scope_id)
    .fetch_optional(&st.pool)
    .await?;
    let default_level: u8 = if scope_type == 1 { 3 } else { 0 };
    let cur_level = existing
        .as_ref()
        .map(|r| r.get::<i64, _>("level") as u8)
        .unwrap_or(default_level);
    let cur_mute = existing
        .as_ref()
        .and_then(|r| r.get::<Option<i64>, _>("muted_until"));

    let level = match req.level {
        Some(l) => {
            if !valid_level(scope_type, l) {
                return Err(AppError::bad_request("niveau de notification invalide"));
            }
            l
        }
        None => cur_level,
    };
    let muted_until = match req.mute_seconds {
        None => cur_mute,
        Some(0) => None,
        Some(n) if n > 0 => Some(now_ms() + n * 1000),
        Some(_) => Some(i64::MAX), // négatif = jusqu'à réactivation
    };
    sqlx::query(
        "INSERT INTO notification_settings (user_id, scope_type, scope_id, level, muted_until) VALUES (?, ?, ?, ?, ?) \
         ON CONFLICT(user_id, scope_type, scope_id) DO UPDATE SET level = excluded.level, muted_until = excluded.muted_until",
    )
    .bind(uid)
    .bind(scope_type as i64)
    .bind(scope_id)
    .bind(level as i64)
    .bind(muted_until)
    .execute(&st.pool)
    .await?;
    Ok(NotificationSetting {
        scope_type,
        scope_id: Snowflake::from_i64(scope_id),
        level,
        muted_until,
    })
}

/// `GET /users/@me/notification-settings`
pub async fn list_notification_settings(
    State(st): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<NotificationSetting>>> {
    let rows = sqlx::query(
        "SELECT scope_type, scope_id, level, muted_until FROM notification_settings WHERE user_id = ?",
    )
    .bind(user.id.as_i64())
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| NotificationSetting {
                scope_type: r.get::<i64, _>("scope_type") as u8,
                scope_id: Snowflake::from_i64(r.get::<i64, _>("scope_id")),
                level: r.get::<i64, _>("level") as u8,
                muted_until: r.get::<Option<i64>, _>("muted_until"),
            })
            .collect(),
    ))
}

/// `PUT /users/@me/notification-settings/guild/:guild_id`
pub async fn set_guild_notification(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
    Json(req): Json<SetNotificationSetting>,
) -> AppResult<Json<NotificationSetting>> {
    let gid = parse_i64(&gid)?;
    pg::require_guild_member(&st.pool, gid, user.id.as_i64()).await?;
    Ok(Json(
        put_setting(&st, user.id.as_i64(), 0, gid, &req).await?,
    ))
}

/// `PUT /users/@me/notification-settings/channel/:channel_id`
pub async fn set_channel_notification(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
    Json(req): Json<SetNotificationSetting>,
) -> AppResult<Json<NotificationSetting>> {
    let cid = parse_i64(&cid)?;
    pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::VIEW_CHANNEL).await?;
    Ok(Json(
        put_setting(&st, user.id.as_i64(), 1, cid, &req).await?,
    ))
}
