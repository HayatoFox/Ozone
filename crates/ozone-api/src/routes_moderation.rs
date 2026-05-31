//! Modération : bannissements, mises en sourdine (timeout), journal d'audit.
//! Cf. docs/features/11-moderation-securite.md.

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::permissions as pg;
use crate::state::AppState;
use crate::util::parse_i64;
use axum::extract::{Path, State};
use axum::Json;
use ozone_proto::dto::{AuditLogEntry, Ban, CreateBan, UpdateMember, User};
use ozone_proto::{perms, Snowflake};
use serde_json::{json, Value};
use sqlx::Row;

/// Enregistre une entrée d'audit (best-effort, n'échoue jamais l'action principale).
pub async fn record_audit(
    st: &AppState,
    guild_id: i64,
    actor: i64,
    target: Option<i64>,
    action: &str,
    reason: Option<&str>,
) {
    let id = st.ids.next();
    let _ = sqlx::query(
        "INSERT INTO audit_log (id, guild_id, user_id, target_id, action_type, reason, changes, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, NULL, ?)",
    )
    .bind(id.as_i64())
    .bind(guild_id)
    .bind(actor)
    .bind(target)
    .bind(action)
    .bind(reason)
    .bind(now_ms())
    .execute(&st.pool)
    .await;
}

async fn owner_and_positions(
    st: &AppState,
    gid: i64,
    actor: i64,
    target: i64,
) -> AppResult<(i64, i32, i32)> {
    let owner = pg::guild_owner(&st.pool, gid)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?;
    let actor_pos = pg::highest_role_position(&st.pool, gid, owner, actor).await?;
    let target_pos = pg::highest_role_position(&st.pool, gid, owner, target).await?;
    Ok((owner, actor_pos, target_pos))
}

// ───────────────────────────── Bannissements ─────────────────────────────

/// `PUT /guilds/:guild_id/bans/:user_id`
pub async fn ban_member(
    State(st): State<AppState>,
    user: AuthUser,
    Path((gid, target)): Path<(String, String)>,
    Json(req): Json<CreateBan>,
) -> AppResult<Json<Value>> {
    let gid = parse_i64(&gid)?;
    let target = parse_i64(&target)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::BAN_MEMBERS).await?;
    let (owner, actor_pos, target_pos) =
        owner_and_positions(&st, gid, user.id.as_i64(), target).await?;
    if target == owner {
        return Err(AppError::forbidden("impossible de bannir le propriétaire"));
    }
    if actor_pos <= target_pos {
        return Err(AppError::forbidden(
            "ce membre est au-dessus ou égal à vous",
        ));
    }
    let exists = sqlx::query("SELECT 1 FROM users WHERE id = ?")
        .bind(target)
        .fetch_optional(&st.pool)
        .await?
        .is_some();
    if !exists {
        return Err(AppError::not_found("utilisateur introuvable"));
    }

    sqlx::query(
        "INSERT OR REPLACE INTO guild_bans (guild_id, user_id, reason, moderator_id, created_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(gid)
    .bind(target)
    .bind(req.reason.as_deref())
    .bind(user.id.as_i64())
    .bind(now_ms())
    .execute(&st.pool)
    .await?;
    sqlx::query("DELETE FROM guild_members WHERE guild_id = ? AND user_id = ?")
        .bind(gid)
        .bind(target)
        .execute(&st.pool)
        .await?;
    sqlx::query("DELETE FROM member_roles WHERE guild_id = ? AND user_id = ?")
        .bind(gid)
        .bind(target)
        .execute(&st.pool)
        .await?;

    if req.delete_message_seconds > 0 {
        let cutoff = now_ms() - req.delete_message_seconds.saturating_mul(1000);
        sqlx::query(
            "DELETE FROM reactions WHERE message_id IN \
             (SELECT m.id FROM messages m JOIN channels c ON c.id = m.channel_id \
              WHERE m.author_id = ? AND m.created_at >= ? AND c.guild_id = ?)",
        )
        .bind(target)
        .bind(cutoff)
        .bind(gid)
        .execute(&st.pool)
        .await?;
        sqlx::query(
            "DELETE FROM messages WHERE author_id = ? AND created_at >= ? \
             AND channel_id IN (SELECT id FROM channels WHERE guild_id = ?)",
        )
        .bind(target)
        .bind(cutoff)
        .bind(gid)
        .execute(&st.pool)
        .await?;
    }

    record_audit(
        &st,
        gid,
        user.id.as_i64(),
        Some(target),
        "member_ban",
        req.reason.as_deref(),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

/// `DELETE /guilds/:guild_id/bans/:user_id`
pub async fn unban_member(
    State(st): State<AppState>,
    user: AuthUser,
    Path((gid, target)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let gid = parse_i64(&gid)?;
    let target = parse_i64(&target)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::BAN_MEMBERS).await?;
    let res = sqlx::query("DELETE FROM guild_bans WHERE guild_id = ? AND user_id = ?")
        .bind(gid)
        .bind(target)
        .execute(&st.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("bannissement introuvable"));
    }
    record_audit(
        &st,
        gid,
        user.id.as_i64(),
        Some(target),
        "member_unban",
        None,
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

/// `GET /guilds/:guild_id/bans`
pub async fn list_bans(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
) -> AppResult<Json<Vec<Ban>>> {
    let gid = parse_i64(&gid)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::BAN_MEMBERS).await?;
    let rows = sqlx::query(
        "SELECT b.user_id, b.reason, b.moderator_id, u.username, u.display_name, u.avatar_id \
         FROM guild_bans b JOIN users u ON u.id = b.user_id WHERE b.guild_id = ? ORDER BY b.created_at DESC",
    )
    .bind(gid)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| Ban {
                user: User {
                    id: Snowflake::from_i64(r.get::<i64, _>("user_id")),
                    username: r.get("username"),
                    display_name: r.get("display_name"),
                    avatar_id: r.get("avatar_id"),
                    email: None,
                },
                reason: r.get("reason"),
                moderator_id: Snowflake::from_i64(r.get::<i64, _>("moderator_id")),
            })
            .collect(),
    ))
}

// ───────────────────────────── Membre (pseudo / timeout) ─────────────────────────────

/// `PATCH /guilds/:guild_id/members/:user_id`
pub async fn update_member(
    State(st): State<AppState>,
    user: AuthUser,
    Path((gid, target)): Path<(String, String)>,
    Json(req): Json<UpdateMember>,
) -> AppResult<Json<Value>> {
    let gid = parse_i64(&gid)?;
    let target = parse_i64(&target)?;
    let me = user.id.as_i64();

    let is_member = sqlx::query("SELECT 1 FROM guild_members WHERE guild_id = ? AND user_id = ?")
        .bind(gid)
        .bind(target)
        .fetch_optional(&st.pool)
        .await?
        .is_some();
    if !is_member {
        return Err(AppError::not_found("membre introuvable"));
    }

    if let Some(nick) = &req.nick {
        let needed = if target == me {
            perms::CHANGE_NICKNAME
        } else {
            perms::MANAGE_NICKNAMES
        };
        pg::require_guild_perm(&st.pool, gid, me, needed).await?;
        let trimmed = nick.trim();
        if trimmed.chars().count() > 32 {
            return Err(AppError::bad_request("pseudo trop long (max 32)"));
        }
        let value: Option<&str> = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        };
        sqlx::query("UPDATE guild_members SET nick = ? WHERE guild_id = ? AND user_id = ?")
            .bind(value)
            .bind(gid)
            .bind(target)
            .execute(&st.pool)
            .await?;
    }

    if let Some(until) = req.communication_disabled_until {
        pg::require_guild_perm(&st.pool, gid, me, perms::MODERATE_MEMBERS).await?;
        let (owner, actor_pos, target_pos) = owner_and_positions(&st, gid, me, target).await?;
        if target == owner {
            return Err(AppError::forbidden(
                "impossible de mettre en sourdine le propriétaire",
            ));
        }
        if actor_pos <= target_pos {
            return Err(AppError::forbidden(
                "ce membre est au-dessus ou égal à vous",
            ));
        }
        sqlx::query("UPDATE guild_members SET communication_disabled_until = ? WHERE guild_id = ? AND user_id = ?")
            .bind(until)
            .bind(gid)
            .bind(target)
            .execute(&st.pool)
            .await?;
        record_audit(&st, gid, me, Some(target), "member_timeout", None).await;
    }

    Ok(Json(json!({ "ok": true })))
}

// ───────────────────────────── Journal d'audit ─────────────────────────────

/// `GET /guilds/:guild_id/audit-logs`
pub async fn list_audit_logs(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
) -> AppResult<Json<Vec<AuditLogEntry>>> {
    let gid = parse_i64(&gid)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::VIEW_AUDIT_LOG).await?;
    let rows = sqlx::query(
        "SELECT id, user_id, target_id, action_type, reason, created_at \
         FROM audit_log WHERE guild_id = ? ORDER BY id DESC LIMIT 50",
    )
    .bind(gid)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| AuditLogEntry {
                id: Snowflake::from_i64(r.get::<i64, _>("id")),
                user_id: Snowflake::from_i64(r.get::<i64, _>("user_id")),
                target_id: r
                    .get::<Option<i64>, _>("target_id")
                    .map(Snowflake::from_i64),
                action_type: r.get("action_type"),
                reason: r.get("reason"),
                created_at: r.get::<i64, _>("created_at") as u64,
            })
            .collect(),
    ))
}
