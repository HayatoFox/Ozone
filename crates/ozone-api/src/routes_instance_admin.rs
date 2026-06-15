//! Tableau de bord d'instance (self-hoster) : config, invitations d'instance,
//! comptes (rôles d'instance, suspension). Cf. docs/features/00-instances.md §6-7.

use crate::crypto;
use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::permissions as pg;
use crate::state::AppState;
use crate::util::parse_i64;
use axum::extract::{Path, State};
use axum::Json;
use ozone_proto::dto::{
    CreateInstanceInvite, InstanceInvite, InstanceUserView, SetInstanceRole, SetSuspended, User,
};
use ozone_proto::Snowflake;
use serde_json::{json, Value};
use sqlx::Row;

fn gen_code() -> String {
    crypto::random_token()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(10)
        .collect()
}

/// `GET /instance/admin/config`
pub async fn get_config(State(st): State<AppState>, user: AuthUser) -> AppResult<Json<Value>> {
    pg::require_instance_admin(&st.pool, user.id.as_i64()).await?;
    let inst = &st.instance;
    Ok(Json(json!({
        "instance_id": inst.instance_id.to_string(),
        "name": inst.name,
        "description": inst.description,
        "version": inst.version,
        "registration_policy": inst.registration_policy,
        "gate_enabled": inst.gate_enabled,
    })))
}

// ───────────────────────────── Invitations d'instance ─────────────────────────────

/// `POST /instance/admin/invites`
pub async fn create_invite(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateInstanceInvite>,
) -> AppResult<Json<InstanceInvite>> {
    pg::require_instance_admin(&st.pool, user.id.as_i64()).await?;
    let now = now_ms();
    // Borne la durée (≤ 90 jours) + arithmétique sûre : `max_age` est contrôlé par l'appelant.
    const MAX_INSTANCE_INVITE_AGE_SECS: i64 = 90 * 24 * 3600;
    if req.max_age > MAX_INSTANCE_INVITE_AGE_SECS {
        return Err(AppError::bad_request(
            "durée de validité trop longue (max 90 jours)",
        ));
    }
    let expires_at = if req.max_age > 0 {
        Some(now + req.max_age * 1000)
    } else {
        None
    };
    let code = gen_code();
    sqlx::query(
        "INSERT INTO instance_invites (code, created_by, max_uses, uses, expires_at, created_at) VALUES (?, ?, ?, 0, ?, ?)",
    )
    .bind(&code)
    .bind(user.id.as_i64())
    .bind(req.max_uses as i64)
    .bind(expires_at)
    .bind(now)
    .execute(&st.pool)
    .await?;
    Ok(Json(InstanceInvite {
        code,
        created_by: user.id,
        uses: 0,
        max_uses: req.max_uses,
        expires_at: expires_at.map(|v| v as u64),
        created_at: now as u64,
    }))
}

/// `GET /instance/admin/invites`
pub async fn list_invites(
    State(st): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<InstanceInvite>>> {
    pg::require_instance_admin(&st.pool, user.id.as_i64()).await?;
    let rows = sqlx::query("SELECT * FROM instance_invites ORDER BY created_at DESC")
        .fetch_all(&st.pool)
        .await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| InstanceInvite {
                code: r.get("code"),
                created_by: Snowflake::from_i64(r.get::<i64, _>("created_by")),
                uses: r.get::<i64, _>("uses") as i32,
                max_uses: r.get::<i64, _>("max_uses") as i32,
                expires_at: r.get::<Option<i64>, _>("expires_at").map(|v| v as u64),
                created_at: r.get::<i64, _>("created_at") as u64,
            })
            .collect(),
    ))
}

/// `DELETE /instance/admin/invites/:code`
pub async fn revoke_invite(
    State(st): State<AppState>,
    user: AuthUser,
    Path(code): Path<String>,
) -> AppResult<Json<Value>> {
    pg::require_instance_admin(&st.pool, user.id.as_i64()).await?;
    let res = sqlx::query("DELETE FROM instance_invites WHERE code = ?")
        .bind(&code)
        .execute(&st.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("invitation d'instance introuvable"));
    }
    Ok(Json(json!({ "ok": true })))
}

// ───────────────────────────── Comptes ─────────────────────────────

/// `GET /instance/admin/users`
pub async fn list_users(
    State(st): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<InstanceUserView>>> {
    pg::require_instance_admin(&st.pool, user.id.as_i64()).await?;
    let rows = sqlx::query(
        "SELECT u.id, u.username, u.display_name, u.avatar_id, u.suspended, \
                COALESCE(ir.role, 'user') AS role \
         FROM users u LEFT JOIN instance_roles ir ON ir.user_id = u.id ORDER BY u.id LIMIT 200",
    )
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| InstanceUserView {
                user: User {
                    id: Snowflake::from_i64(r.get::<i64, _>("id")),
                    username: r.get("username"),
                    display_name: r.get("display_name"),
                    avatar_id: r.get("avatar_id"),
                    email: None,
                    name_style: None, // vue admin (table) : nom brut
                },
                role: r.get("role"),
                suspended: r.get::<i64, _>("suspended") != 0,
            })
            .collect(),
    ))
}

/// `PUT /instance/admin/users/:user_id/role` — promotion/rétrogradation (propriétaire uniquement).
pub async fn set_role(
    State(st): State<AppState>,
    user: AuthUser,
    Path(target): Path<String>,
    Json(req): Json<SetInstanceRole>,
) -> AppResult<Json<Value>> {
    pg::require_instance_owner(&st.pool, user.id.as_i64()).await?;
    let target = parse_i64(&target)?;
    if !matches!(req.role.as_str(), "admin" | "moderator" | "user") {
        return Err(AppError::bad_request(
            "rôle invalide (admin | moderator | user)",
        ));
    }
    // Le propriétaire ne peut pas être rétrogradé via cette route.
    if pg::instance_role(&st.pool, target).await? == "owner" {
        return Err(AppError::forbidden(
            "le rôle du propriétaire ne peut pas être modifié",
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
    sqlx::query("INSERT OR REPLACE INTO instance_roles (user_id, role) VALUES (?, ?)")
        .bind(target)
        .bind(&req.role)
        .execute(&st.pool)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

/// `PATCH /instance/admin/users/:user_id` — suspendre / réactiver un compte.
pub async fn set_suspended(
    State(st): State<AppState>,
    user: AuthUser,
    Path(target): Path<String>,
    Json(req): Json<SetSuspended>,
) -> AppResult<Json<Value>> {
    pg::require_instance_admin(&st.pool, user.id.as_i64()).await?;
    let target = parse_i64(&target)?;
    if pg::instance_role(&st.pool, target).await? == "owner" {
        return Err(AppError::forbidden(
            "le propriétaire de l'instance ne peut pas être suspendu",
        ));
    }
    let res = sqlx::query("UPDATE users SET suspended = ? WHERE id = ?")
        .bind(req.suspended as i64)
        .bind(target)
        .execute(&st.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("utilisateur introuvable"));
    }
    // Suspension : révocation immédiate de toutes les sessions (refresh tokens) du compte.
    if req.suspended {
        sqlx::query("DELETE FROM sessions WHERE user_id = ?")
            .bind(target)
            .execute(&st.pool)
            .await?;
    }
    Ok(Json(json!({ "ok": true })))
}
