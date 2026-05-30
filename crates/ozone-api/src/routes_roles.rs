//! Rôles, attribution de rôles aux membres, surcharges de permission de salon.
//! Cf. docs/features/10-roles-permissions.md.

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::permissions as pg;
use crate::state::AppState;
use crate::util::parse_i64;
use axum::extract::{Path, State};
use axum::Json;
use ozone_proto::dto::{CreateRole, PermissionOverwrite, Role, SetOverwrite, UpdateRole};
use ozone_proto::{perms, Snowflake};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

fn row_to_role(r: SqliteRow) -> Role {
    Role {
        id: Snowflake::from_i64(r.get::<i64, _>("id")),
        guild_id: Snowflake::from_i64(r.get::<i64, _>("guild_id")),
        name: r.get("name"),
        color: r.get::<i64, _>("color") as u32,
        hoist: r.get::<i64, _>("hoist") != 0,
        position: r.get::<i64, _>("position") as i32,
        permissions: (r.get::<i64, _>("permissions") as u64).to_string(),
        mentionable: r.get::<i64, _>("mentionable") != 0,
        managed: r.get::<i64, _>("managed") != 0,
    }
}

async fn fetch_role(st: &AppState, gid: i64, rid: i64) -> AppResult<SqliteRow> {
    sqlx::query("SELECT * FROM roles WHERE id = ? AND guild_id = ?")
        .bind(rid)
        .bind(gid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("rôle introuvable"))
}

/// `GET /guilds/:guild_id/roles`
pub async fn list_roles(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
) -> AppResult<Json<Vec<Role>>> {
    let gid = parse_i64(&gid)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::VIEW_CHANNEL).await?;
    let rows = sqlx::query("SELECT * FROM roles WHERE guild_id = ? ORDER BY position DESC, id")
        .bind(gid)
        .fetch_all(&st.pool)
        .await?;
    Ok(Json(rows.into_iter().map(row_to_role).collect()))
}

/// `POST /guilds/:guild_id/roles`
pub async fn create_role(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
    Json(req): Json<CreateRole>,
) -> AppResult<Json<Role>> {
    let gid = parse_i64(&gid)?;
    let actor =
        pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::MANAGE_ROLES).await?;

    let maxpos: i64 =
        sqlx::query("SELECT COALESCE(MAX(position), 0) AS m FROM roles WHERE guild_id = ?")
            .bind(gid)
            .fetch_one(&st.pool)
            .await?
            .get("m");
    let position = maxpos + 1;
    let id = st.ids.next();
    let name = req.name.unwrap_or_else(|| "nouveau rôle".into());
    let color = req.color.unwrap_or(0) as i64;
    let hoist = req.hoist.unwrap_or(false) as i64;
    let mentionable = req.mentionable.unwrap_or(false) as i64;
    // Anti-escalade : on ne peut accorder que des permissions qu'on possède.
    let permissions = (req.permissions.as_deref().map(perms::parse).unwrap_or(0) & actor) as i64;

    sqlx::query(
        "INSERT INTO roles (id, guild_id, name, color, hoist, position, permissions, mentionable, managed, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, ?)",
    )
    .bind(id.as_i64())
    .bind(gid)
    .bind(&name)
    .bind(color)
    .bind(hoist)
    .bind(position)
    .bind(permissions)
    .bind(mentionable)
    .bind(now_ms())
    .execute(&st.pool)
    .await?;

    Ok(Json(Role {
        id,
        guild_id: Snowflake::from_i64(gid),
        name,
        color: color as u32,
        hoist: hoist != 0,
        position: position as i32,
        permissions: (permissions as u64).to_string(),
        mentionable: mentionable != 0,
        managed: false,
    }))
}

/// `PATCH /guilds/:guild_id/roles/:role_id`
pub async fn update_role(
    State(st): State<AppState>,
    user: AuthUser,
    Path((gid, rid)): Path<(String, String)>,
    Json(req): Json<UpdateRole>,
) -> AppResult<Json<Role>> {
    let gid = parse_i64(&gid)?;
    let rid = parse_i64(&rid)?;
    let actor =
        pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::MANAGE_ROLES).await?;
    let owner = pg::guild_owner(&st.pool, gid)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?;
    let role = fetch_role(&st, gid, rid).await?;
    let role_pos = role.get::<i64, _>("position") as i32;
    let actor_pos = pg::highest_role_position(&st.pool, gid, owner, user.id.as_i64()).await?;
    if actor_pos <= role_pos {
        return Err(AppError::forbidden(
            "ce rôle est au-dessus ou égal au vôtre",
        ));
    }

    let name: String = req.name.unwrap_or_else(|| role.get("name"));
    let color = req
        .color
        .map(|c| c as i64)
        .unwrap_or_else(|| role.get("color"));
    let hoist = req
        .hoist
        .map(|b| b as i64)
        .unwrap_or_else(|| role.get("hoist"));
    let mentionable = req
        .mentionable
        .map(|b| b as i64)
        .unwrap_or_else(|| role.get("mentionable"));
    let cur_perms = role.get::<i64, _>("permissions") as u64;
    let permissions = (req
        .permissions
        .as_deref()
        .map(perms::parse)
        .map(|p| p & actor)
        .unwrap_or(cur_perms)) as i64;

    sqlx::query("UPDATE roles SET name = ?, color = ?, hoist = ?, permissions = ?, mentionable = ? WHERE id = ? AND guild_id = ?")
        .bind(&name)
        .bind(color)
        .bind(hoist)
        .bind(permissions)
        .bind(mentionable)
        .bind(rid)
        .bind(gid)
        .execute(&st.pool)
        .await?;

    Ok(Json(row_to_role(fetch_role(&st, gid, rid).await?)))
}

/// `DELETE /guilds/:guild_id/roles/:role_id`
pub async fn delete_role(
    State(st): State<AppState>,
    user: AuthUser,
    Path((gid, rid)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let gid = parse_i64(&gid)?;
    let rid = parse_i64(&rid)?;
    if rid == gid {
        return Err(AppError::bad_request(
            "impossible de supprimer le rôle @everyone",
        ));
    }
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::MANAGE_ROLES).await?;
    let owner = pg::guild_owner(&st.pool, gid)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?;
    let role = fetch_role(&st, gid, rid).await?;
    let role_pos = role.get::<i64, _>("position") as i32;
    let actor_pos = pg::highest_role_position(&st.pool, gid, owner, user.id.as_i64()).await?;
    if actor_pos <= role_pos {
        return Err(AppError::forbidden(
            "ce rôle est au-dessus ou égal au vôtre",
        ));
    }
    sqlx::query("DELETE FROM roles WHERE id = ? AND guild_id = ?")
        .bind(rid)
        .bind(gid)
        .execute(&st.pool)
        .await?;
    sqlx::query("DELETE FROM member_roles WHERE guild_id = ? AND role_id = ?")
        .bind(gid)
        .bind(rid)
        .execute(&st.pool)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `PUT /guilds/:guild_id/members/:user_id/roles/:role_id`
pub async fn add_member_role(
    State(st): State<AppState>,
    user: AuthUser,
    Path((gid, target, rid)): Path<(String, String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let gid = parse_i64(&gid)?;
    let target = parse_i64(&target)?;
    let rid = parse_i64(&rid)?;
    if rid == gid {
        return Err(AppError::bad_request("le rôle @everyone est implicite"));
    }
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::MANAGE_ROLES).await?;
    let owner = pg::guild_owner(&st.pool, gid)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?;
    let role = fetch_role(&st, gid, rid).await?;
    let role_pos = role.get::<i64, _>("position") as i32;
    let actor_pos = pg::highest_role_position(&st.pool, gid, owner, user.id.as_i64()).await?;
    if actor_pos <= role_pos {
        return Err(AppError::forbidden(
            "ce rôle est au-dessus ou égal au vôtre",
        ));
    }
    let is_member = sqlx::query("SELECT 1 FROM guild_members WHERE guild_id = ? AND user_id = ?")
        .bind(gid)
        .bind(target)
        .fetch_optional(&st.pool)
        .await?
        .is_some();
    if !is_member {
        return Err(AppError::not_found("membre introuvable"));
    }
    sqlx::query("INSERT OR IGNORE INTO member_roles (guild_id, user_id, role_id) VALUES (?, ?, ?)")
        .bind(gid)
        .bind(target)
        .bind(rid)
        .execute(&st.pool)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `DELETE /guilds/:guild_id/members/:user_id/roles/:role_id`
pub async fn remove_member_role(
    State(st): State<AppState>,
    user: AuthUser,
    Path((gid, target, rid)): Path<(String, String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let gid = parse_i64(&gid)?;
    let target = parse_i64(&target)?;
    let rid = parse_i64(&rid)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::MANAGE_ROLES).await?;
    let owner = pg::guild_owner(&st.pool, gid)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?;
    let role = fetch_role(&st, gid, rid).await?;
    let role_pos = role.get::<i64, _>("position") as i32;
    let actor_pos = pg::highest_role_position(&st.pool, gid, owner, user.id.as_i64()).await?;
    if actor_pos <= role_pos {
        return Err(AppError::forbidden(
            "ce rôle est au-dessus ou égal au vôtre",
        ));
    }
    sqlx::query("DELETE FROM member_roles WHERE guild_id = ? AND user_id = ? AND role_id = ?")
        .bind(gid)
        .bind(target)
        .bind(rid)
        .execute(&st.pool)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `PUT /channels/:channel_id/permissions/:overwrite_id`
pub async fn set_overwrite(
    State(st): State<AppState>,
    user: AuthUser,
    Path((cid, tid)): Path<(String, String)>,
    Json(req): Json<SetOverwrite>,
) -> AppResult<Json<PermissionOverwrite>> {
    let cid = parse_i64(&cid)?;
    let tid = parse_i64(&tid)?;
    let (gid, _owner, actor) =
        pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::MANAGE_ROLES).await?;
    let kind = req.kind;
    // La cible doit appartenir à la guilde du salon (rôle ou membre).
    let valid_target = match kind {
        0 => sqlx::query("SELECT 1 FROM roles WHERE id = ? AND guild_id = ?")
            .bind(tid)
            .bind(gid)
            .fetch_optional(&st.pool)
            .await?
            .is_some(),
        1 => sqlx::query("SELECT 1 FROM guild_members WHERE guild_id = ? AND user_id = ?")
            .bind(gid)
            .bind(tid)
            .fetch_optional(&st.pool)
            .await?
            .is_some(),
        _ => {
            return Err(AppError::bad_request(
                "type de surcharge invalide (0 = rôle, 1 = membre)",
            ))
        }
    };
    if !valid_target {
        return Err(AppError::not_found(
            "cible de surcharge introuvable dans la guilde",
        ));
    }
    // On n'autorise/refuse que des permissions qu'on possède, et ADMINISTRATOR n'est pas surchargeable.
    let mask = actor & !perms::ADMINISTRATOR;
    let allow = req.allow.as_deref().map(perms::parse).unwrap_or(0) & mask;
    let deny = req.deny.as_deref().map(perms::parse).unwrap_or(0) & mask;
    sqlx::query(
        "INSERT INTO channel_overwrites (channel_id, target_id, target_type, allow, deny) VALUES (?, ?, ?, ?, ?) \
         ON CONFLICT(channel_id, target_id) DO UPDATE SET target_type = excluded.target_type, allow = excluded.allow, deny = excluded.deny",
    )
    .bind(cid)
    .bind(tid)
    .bind(kind as i64)
    .bind(allow as i64)
    .bind(deny as i64)
    .execute(&st.pool)
    .await?;
    Ok(Json(PermissionOverwrite {
        id: Snowflake::from_i64(tid),
        kind,
        allow: allow.to_string(),
        deny: deny.to_string(),
    }))
}

/// `DELETE /channels/:channel_id/permissions/:overwrite_id`
pub async fn delete_overwrite(
    State(st): State<AppState>,
    user: AuthUser,
    Path((cid, tid)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let cid = parse_i64(&cid)?;
    let tid = parse_i64(&tid)?;
    pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::MANAGE_ROLES).await?;
    sqlx::query("DELETE FROM channel_overwrites WHERE channel_id = ? AND target_id = ?")
        .bind(cid)
        .bind(tid)
        .execute(&st.pool)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
