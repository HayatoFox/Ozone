//! Membres (liste, expulsion) et invitations de guilde (création, liste, jonction).

use crate::crypto;
use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::permissions as pg;
use crate::state::{AppState, EventScope};
use crate::util::parse_i64;
use axum::extract::{Path, State};
use axum::Json;
use ozone_proto::dto::{CreateInvite, Guild, Invite, InvitePreview, Member, User};
use ozone_proto::{perms, Snowflake};
use sqlx::Row;

// ───────────────────────────── Membres ─────────────────────────────

/// `GET /guilds/:guild_id/members`
pub async fn list_members(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
) -> AppResult<Json<Vec<Member>>> {
    let gid = parse_i64(&gid)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::VIEW_CHANNEL).await?;
    let rows = sqlx::query(
        "SELECT gm.user_id, gm.nick, gm.joined_at, u.username, u.display_name, u.avatar_id \
         FROM guild_members gm JOIN users u ON u.id = gm.user_id WHERE gm.guild_id = ? ORDER BY gm.joined_at",
    )
    .bind(gid)
    .fetch_all(&st.pool)
    .await?;

    let mut members = Vec::with_capacity(rows.len());
    for r in rows {
        let uid: i64 = r.get("user_id");
        let role_ids = pg::member_role_ids(&st.pool, gid, uid).await?;
        members.push(Member {
            user: User {
                id: Snowflake::from_i64(uid),
                username: r.get("username"),
                display_name: r.get("display_name"),
                avatar_id: r.get("avatar_id"),
                email: None,
            },
            nick: r.get("nick"),
            roles: role_ids.into_iter().map(Snowflake::from_i64).collect(),
            joined_at: r.get::<i64, _>("joined_at") as u64,
        });
    }
    Ok(Json(members))
}

/// `DELETE /guilds/:guild_id/members/:user_id` (expulsion)
pub async fn kick_member(
    State(st): State<AppState>,
    user: AuthUser,
    Path((gid, target)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let gid = parse_i64(&gid)?;
    let target = parse_i64(&target)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::KICK_MEMBERS).await?;
    let owner = pg::guild_owner(&st.pool, gid)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?;
    if target == owner {
        return Err(AppError::forbidden("impossible d'expulser le propriétaire"));
    }
    let actor_pos = pg::highest_role_position(&st.pool, gid, owner, user.id.as_i64()).await?;
    let target_pos = pg::highest_role_position(&st.pool, gid, owner, target).await?;
    if actor_pos <= target_pos {
        return Err(AppError::forbidden(
            "ce membre est au-dessus ou égal à vous",
        ));
    }
    let res = sqlx::query("DELETE FROM guild_members WHERE guild_id = ? AND user_id = ?")
        .bind(gid)
        .bind(target)
        .execute(&st.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("membre introuvable"));
    }
    sqlx::query("DELETE FROM member_roles WHERE guild_id = ? AND user_id = ?")
        .bind(gid)
        .bind(target)
        .execute(&st.pool)
        .await?;
    crate::routes_moderation::record_audit(
        &st,
        gid,
        user.id.as_i64(),
        Some(target),
        "member_kick",
        None,
    )
    .await;
    st.publish(
        EventScope::Guild(gid),
        "GUILD_MEMBER_REMOVE",
        serde_json::json!({ "guild_id": gid.to_string(), "user_id": target.to_string() }),
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ───────────────────────────── Invitations ─────────────────────────────

fn gen_code() -> String {
    crypto::random_token()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect()
}

fn row_to_invite(r: sqlx::sqlite::SqliteRow) -> Invite {
    Invite {
        code: r.get("code"),
        guild_id: Snowflake::from_i64(r.get::<i64, _>("guild_id")),
        channel_id: r
            .get::<Option<i64>, _>("channel_id")
            .map(Snowflake::from_i64),
        inviter_id: Snowflake::from_i64(r.get::<i64, _>("inviter_id")),
        uses: r.get::<i64, _>("uses") as i32,
        max_uses: r.get::<i64, _>("max_uses") as i32,
        max_age: r.get::<i64, _>("max_age"),
        created_at: r.get::<i64, _>("created_at") as u64,
        expires_at: r.get::<Option<i64>, _>("expires_at").map(|v| v as u64),
    }
}

/// `POST /guilds/:guild_id/invites`
pub async fn create_invite(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
    Json(req): Json<CreateInvite>,
) -> AppResult<Json<Invite>> {
    let gid = parse_i64(&gid)?;
    pg::require_guild_perm(
        &st.pool,
        gid,
        user.id.as_i64(),
        perms::CREATE_INSTANT_INVITE,
    )
    .await?;
    let now = now_ms();
    let expires_at = if req.max_age > 0 {
        Some(now + req.max_age * 1000)
    } else {
        None
    };
    let code = gen_code();
    sqlx::query(
        "INSERT INTO invites (code, guild_id, channel_id, inviter_id, uses, max_uses, max_age, created_at, expires_at) \
         VALUES (?, ?, NULL, ?, 0, ?, ?, ?, ?)",
    )
    .bind(&code)
    .bind(gid)
    .bind(user.id.as_i64())
    .bind(req.max_uses as i64)
    .bind(req.max_age)
    .bind(now)
    .bind(expires_at)
    .execute(&st.pool)
    .await?;
    Ok(Json(Invite {
        code,
        guild_id: Snowflake::from_i64(gid),
        channel_id: None,
        inviter_id: user.id,
        uses: 0,
        max_uses: req.max_uses,
        max_age: req.max_age,
        created_at: now as u64,
        expires_at: expires_at.map(|v| v as u64),
    }))
}

/// `GET /guilds/:guild_id/invites`
pub async fn list_invites(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
) -> AppResult<Json<Vec<Invite>>> {
    let gid = parse_i64(&gid)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::MANAGE_GUILD).await?;
    let rows = sqlx::query("SELECT * FROM invites WHERE guild_id = ? ORDER BY created_at DESC")
        .bind(gid)
        .fetch_all(&st.pool)
        .await?;
    Ok(Json(rows.into_iter().map(row_to_invite).collect()))
}

/// `POST /invites/:code` — rejoindre une guilde via une invitation.
pub async fn join_invite(
    State(st): State<AppState>,
    user: AuthUser,
    Path(code): Path<String>,
) -> AppResult<Json<Guild>> {
    let row = sqlx::query("SELECT * FROM invites WHERE code = ?")
        .bind(&code)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("invitation invalide"))?;
    let inv = row_to_invite(row);
    let now = now_ms();
    if let Some(exp) = inv.expires_at {
        if (exp as i64) < now {
            return Err(AppError::not_found("invitation expirée"));
        }
    }
    if inv.max_uses > 0 && inv.uses >= inv.max_uses {
        return Err(AppError::forbidden("invitation épuisée"));
    }
    let gid = inv.guild_id.as_i64();

    let banned = sqlx::query("SELECT 1 FROM guild_bans WHERE guild_id = ? AND user_id = ?")
        .bind(gid)
        .bind(user.id.as_i64())
        .fetch_optional(&st.pool)
        .await?
        .is_some();
    if banned {
        return Err(AppError::forbidden("vous êtes banni de cette guilde"));
    }

    sqlx::query("INSERT OR IGNORE INTO guild_members (guild_id, user_id, nick, joined_at) VALUES (?, ?, NULL, ?)")
        .bind(gid)
        .bind(user.id.as_i64())
        .bind(now)
        .execute(&st.pool)
        .await?;
    sqlx::query("UPDATE invites SET uses = uses + 1 WHERE code = ?")
        .bind(&code)
        .execute(&st.pool)
        .await?;
    st.publish(
        EventScope::Guild(gid),
        "GUILD_MEMBER_ADD",
        serde_json::json!({ "guild_id": gid.to_string(), "user_id": user.id.to_string() }),
    );

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

/// `GET /invites/:code` — aperçu d'une invitation **sans** rejoindre la guilde.
pub async fn preview_invite(
    State(st): State<AppState>,
    _user: AuthUser,
    Path(code): Path<String>,
) -> AppResult<Json<InvitePreview>> {
    let row = sqlx::query("SELECT guild_id, inviter_id, expires_at FROM invites WHERE code = ?")
        .bind(&code)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("invitation invalide"))?;
    let expires_at: Option<i64> = row.get("expires_at");
    if let Some(exp) = expires_at {
        if exp < now_ms() {
            return Err(AppError::not_found("invitation expirée"));
        }
    }
    let gid: i64 = row.get("guild_id");
    let inviter: i64 = row.get("inviter_id");
    let g = sqlx::query("SELECT name, icon_id FROM guilds WHERE id = ?")
        .bind(gid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?;
    let member_count: i64 =
        sqlx::query("SELECT COUNT(*) AS c FROM guild_members WHERE guild_id = ?")
            .bind(gid)
            .fetch_one(&st.pool)
            .await?
            .get("c");
    Ok(Json(InvitePreview {
        code,
        guild_id: Snowflake::from_i64(gid),
        guild_name: g.get("name"),
        guild_icon: g.get("icon_id"),
        inviter_id: Snowflake::from_i64(inviter),
        member_count,
        expires_at: expires_at.map(|v| v as u64),
    }))
}

/// `DELETE /invites/:code` — révoque une invitation (son créateur ou `MANAGE_GUILD`).
pub async fn revoke_invite(
    State(st): State<AppState>,
    user: AuthUser,
    Path(code): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let row = sqlx::query("SELECT guild_id, inviter_id FROM invites WHERE code = ?")
        .bind(&code)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("invitation invalide"))?;
    let gid: i64 = row.get("guild_id");
    let inviter: i64 = row.get("inviter_id");
    // Le créateur peut révoquer la sienne ; sinon il faut MANAGE_GUILD.
    if inviter != user.id.as_i64() {
        pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::MANAGE_GUILD).await?;
    }
    sqlx::query("DELETE FROM invites WHERE code = ?")
        .bind(&code)
        .execute(&st.pool)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
