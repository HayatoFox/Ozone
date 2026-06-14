//! Rôles, attribution de rôles aux membres, surcharges de permission de salon.
//! Cf. docs/features/10-roles-permissions.md.

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::permissions as pg;
use crate::state::{AppState, EventScope};
use crate::util::parse_i64;
use axum::extract::{Path, State};
use axum::Json;
use ozone_proto::dto::{
    CreateRole, PermissionOverwrite, ReorderRoles, Role, SetOverwrite, UpdateRole,
};
use ozone_proto::{perms, Snowflake};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

fn row_to_role(r: SqliteRow) -> Role {
    Role {
        id: Snowflake::from_i64(r.get::<i64, _>("id")),
        guild_id: Snowflake::from_i64(r.get::<i64, _>("guild_id")),
        name: r.get("name"),
        color: r.get::<i64, _>("color") as u32,
        secondary_color: r
            .try_get::<Option<i64>, _>("secondary_color")
            .ok()
            .flatten()
            .map(|v| v as u32),
        color_style: r
            .try_get::<String, _>("color_style")
            .unwrap_or_else(|_| "solid".into()),
        hoist: r.get::<i64, _>("hoist") != 0,
        position: r.get::<i64, _>("position") as i32,
        permissions: (r.get::<i64, _>("permissions") as u64).to_string(),
        mentionable: r.get::<i64, _>("mentionable") != 0,
        managed: r.get::<i64, _>("managed") != 0,
    }
}

/// Restreint le style de couleur à une liste blanche (anti-injection / valeurs arbitraires).
fn sanitize_color_style(s: Option<&str>) -> &'static str {
    match s {
        Some("gradient") => "gradient",
        Some("neon") => "neon",
        Some("wave") => "wave",
        _ => "solid",
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
    let secondary_color = req.secondary_color.map(|c| c as i64);
    let color_style = sanitize_color_style(req.color_style.as_deref());
    let hoist = req.hoist.unwrap_or(false) as i64;
    let mentionable = req.mentionable.unwrap_or(false) as i64;
    // Anti-escalade : on ne peut accorder que des permissions qu'on possède.
    let permissions = (req.permissions.as_deref().map(perms::parse).unwrap_or(0) & actor) as i64;

    sqlx::query(
        "INSERT INTO roles (id, guild_id, name, color, secondary_color, color_style, hoist, position, permissions, mentionable, managed, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?)",
    )
    .bind(id.as_i64())
    .bind(gid)
    .bind(&name)
    .bind(color)
    .bind(secondary_color)
    .bind(color_style)
    .bind(hoist)
    .bind(position)
    .bind(permissions)
    .bind(mentionable)
    .bind(now_ms())
    .execute(&st.pool)
    .await?;

    let role = Role {
        id,
        guild_id: Snowflake::from_i64(gid),
        name,
        color: color as u32,
        secondary_color: secondary_color.map(|c| c as u32),
        color_style: color_style.to_string(),
        hoist: hoist != 0,
        position: position as i32,
        permissions: (permissions as u64).to_string(),
        mentionable: mentionable != 0,
        managed: false,
    };
    crate::routes_moderation::audit_named(&st, gid, user.id.as_i64(), "role_create", &role.name)
        .await;
    st.publish(
        EventScope::Guild(gid),
        "GUILD_ROLE_CREATE",
        serde_json::to_value(&role).unwrap_or_default(),
    );
    Ok(Json(role))
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
    // `secondary_color` absent ⇒ inchangé ; présent (même nul) ⇒ appliqué tel quel.
    let secondary_color: Option<i64> = if req.secondary_color.is_some() {
        req.secondary_color.map(|c| c as i64)
    } else {
        role.try_get::<Option<i64>, _>("secondary_color").ok().flatten()
    };
    let color_style: &str = if req.color_style.is_some() {
        sanitize_color_style(req.color_style.as_deref())
    } else {
        // Conserve l'existant (validé à l'écriture).
        match role.try_get::<String, _>("color_style").ok().as_deref() {
            Some("gradient") => "gradient",
            Some("neon") => "neon",
            Some("wave") => "wave",
            _ => "solid",
        }
    };
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

    sqlx::query("UPDATE roles SET name = ?, color = ?, secondary_color = ?, color_style = ?, hoist = ?, permissions = ?, mentionable = ? WHERE id = ? AND guild_id = ?")
        .bind(&name)
        .bind(color)
        .bind(secondary_color)
        .bind(color_style)
        .bind(hoist)
        .bind(permissions)
        .bind(mentionable)
        .bind(rid)
        .bind(gid)
        .execute(&st.pool)
        .await?;

    let role = row_to_role(fetch_role(&st, gid, rid).await?);
    st.publish(
        EventScope::Guild(gid),
        "GUILD_ROLE_UPDATE",
        serde_json::to_value(&role).unwrap_or_default(),
    );
    Ok(Json(role))
}

/// `PATCH /guilds/:guild_id/roles`
/// Réordonne les rôles. `ids` = liste **complète** des rôles (hors `@everyone`),
/// du plus haut (index 0) au plus bas. Recalcule les positions (n..1 ; `@everyone` = 0).
pub async fn reorder_roles(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
    Json(req): Json<ReorderRoles>,
) -> AppResult<Json<Vec<Role>>> {
    let gid = parse_i64(&gid)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::MANAGE_ROLES).await?;
    let owner = pg::guild_owner(&st.pool, gid)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?;
    let is_owner = owner == user.id.as_i64();
    let actor_pos = pg::highest_role_position(&st.pool, gid, owner, user.id.as_i64()).await?;

    // État courant : tous les rôles de la guilde (positions actuelles).
    let rows = sqlx::query("SELECT id, position FROM roles WHERE guild_id = ?")
        .bind(gid)
        .fetch_all(&st.pool)
        .await?;
    let mut cur_pos: std::collections::HashMap<i64, i32> = std::collections::HashMap::new();
    for r in &rows {
        cur_pos.insert(r.get::<i64, _>("id"), r.get::<i64, _>("position") as i32);
    }
    // Ensemble des rôles hors @everyone (le rôle dont l'id == gid).
    let mut existing: Vec<i64> = cur_pos.keys().copied().filter(|&id| id != gid).collect();
    existing.sort_unstable();

    // Parse + validation : `ids` doit être une permutation exacte des rôles hors @everyone.
    let mut ids: Vec<i64> = Vec::with_capacity(req.ids.len());
    for s in &req.ids {
        ids.push(parse_i64(s)?);
    }
    if ids.contains(&gid) {
        return Err(AppError::bad_request("le rôle @everyone ne peut pas être réordonné"));
    }
    let mut sorted_ids = ids.clone();
    sorted_ids.sort_unstable();
    sorted_ids.dedup();
    if sorted_ids.len() != ids.len() || sorted_ids != existing {
        return Err(AppError::bad_request(
            "la liste doit contenir exactement tous les rôles (hors @everyone), sans doublon",
        ));
    }

    // Nouvelles positions : index 0 (haut) ⇒ position la plus élevée.
    let n = ids.len() as i32;
    let new_pos: Vec<(i64, i32)> = ids.iter().enumerate().map(|(i, &id)| (id, n - i as i32)).collect();

    // Hiérarchie (non-propriétaire) : interdit de placer un rôle ≥ à soi, ou de déplacer
    // un rôle déjà ≥ à soi. Le propriétaire (actor_pos = i32::MAX) n'est pas contraint.
    if !is_owner {
        for &(id, np) in &new_pos {
            let op = *cur_pos.get(&id).unwrap_or(&0);
            if np >= actor_pos {
                return Err(AppError::forbidden(
                    "impossible de placer un rôle au-dessus ou au niveau du vôtre",
                ));
            }
            if op >= actor_pos && np != op {
                return Err(AppError::forbidden(
                    "impossible de déplacer un rôle au-dessus ou au niveau du vôtre",
                ));
            }
        }
    }

    // Application transactionnelle.
    let mut tx = st.pool.begin().await?;
    for &(id, np) in &new_pos {
        sqlx::query("UPDATE roles SET position = ? WHERE id = ? AND guild_id = ?")
            .bind(np as i64)
            .bind(id)
            .bind(gid)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;

    let rows = sqlx::query("SELECT * FROM roles WHERE guild_id = ? ORDER BY position DESC, id")
        .bind(gid)
        .fetch_all(&st.pool)
        .await?;
    let roles: Vec<Role> = rows.into_iter().map(row_to_role).collect();
    // Diffuse chaque rôle mis à jour (les clients rafraîchissent l'ordre).
    for role in &roles {
        st.publish(
            EventScope::Guild(gid),
            "GUILD_ROLE_UPDATE",
            serde_json::to_value(role).unwrap_or_default(),
        );
    }
    Ok(Json(roles))
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
    let role_name: String = role.get("name");
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
    crate::routes_moderation::audit_named(&st, gid, user.id.as_i64(), "role_delete", &role_name)
        .await;
    st.publish(
        EventScope::Guild(gid),
        "GUILD_ROLE_DELETE",
        serde_json::json!({ "role_id": rid.to_string(), "guild_id": gid.to_string() }),
    );
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
    st.publish(
        EventScope::Guild(gid),
        "GUILD_MEMBER_UPDATE",
        serde_json::json!({ "guild_id": gid.to_string(), "user_id": target.to_string(), "role_id": rid.to_string(), "added": true }),
    );
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
    // @everyone (id == guild_id) ne se retire pas (garde de cohérence, symétrique à l'ajout).
    if rid == gid {
        return Err(AppError::bad_request("@everyone ne peut pas être retiré"));
    }
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
    st.publish(
        EventScope::Guild(gid),
        "GUILD_MEMBER_UPDATE",
        serde_json::json!({ "guild_id": gid.to_string(), "user_id": target.to_string(), "role_id": rid.to_string(), "added": false }),
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `GET /channels/:channel_id/permissions` — surcharges de permission du salon.
/// Réservé à qui peut gérer les permissions (la config de salon n'est pas publique).
pub async fn list_overwrites(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
) -> AppResult<Json<Vec<PermissionOverwrite>>> {
    let cid = parse_i64(&cid)?;
    pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::MANAGE_ROLES).await?;
    let rows = sqlx::query(
        "SELECT target_id, target_type, allow, deny FROM channel_overwrites WHERE channel_id = ?",
    )
    .bind(cid)
    .fetch_all(&st.pool)
    .await?;
    let out = rows
        .into_iter()
        .map(|r| PermissionOverwrite {
            id: Snowflake::from_i64(r.get::<i64, _>("target_id")),
            kind: r.get::<i64, _>("target_type") as u8,
            allow: (r.get::<i64, _>("allow") as u64).to_string(),
            deny: (r.get::<i64, _>("deny") as u64).to_string(),
        })
        .collect();
    Ok(Json(out))
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
    st.publish(
        EventScope::Guild(gid),
        "CHANNEL_UPDATE",
        serde_json::json!({ "id": cid.to_string(), "guild_id": gid.to_string() }),
    );
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
    let (gid, _owner, _p) =
        pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::MANAGE_ROLES).await?;
    sqlx::query("DELETE FROM channel_overwrites WHERE channel_id = ? AND target_id = ?")
        .bind(cid)
        .bind(tid)
        .execute(&st.pool)
        .await?;
    st.publish(
        EventScope::Guild(gid),
        "CHANNEL_UPDATE",
        serde_json::json!({ "id": cid.to_string(), "guild_id": gid.to_string() }),
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}
