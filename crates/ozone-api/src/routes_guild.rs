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

/// Filtres de la liste des membres : pagination par curseur (`after` = dernier `user_id`),
/// `limit`, et recherche `query` (préfixe sur pseudo/nom d'utilisateur).
#[derive(Debug, serde::Deserialize)]
pub struct MemberQuery {
    pub after: Option<String>,
    pub limit: Option<i64>,
    pub query: Option<String>,
}

// Construit un Member depuis une ligne portant un GROUP_CONCAT des role_id (« r1,r2 »).
fn row_to_member(r: &sqlx::sqlite::SqliteRow, can_see_method: bool) -> Member {
    let roles: Vec<Snowflake> = r
        .get::<Option<String>, _>("role_ids")
        .map(|s| {
            s.split(',')
                .filter_map(|p| p.parse::<i64>().ok().map(Snowflake::from_i64))
                .collect()
        })
        .unwrap_or_default();
    Member {
        user: User {
            id: Snowflake::from_i64(r.get::<i64, _>("user_id")),
            username: r.get("username"),
            display_name: r.get("display_name"),
            avatar_id: r.get("avatar_id"),
            email: None,
        },
        nick: r.get("nick"),
        roles,
        joined_at: r.get::<i64, _>("joined_at") as u64,
        joined_via: if can_see_method { r.get("invite_code") } else { None },
    }
}

/// `GET /guilds/:guild_id/members?after=&limit=&query=`
///
/// Les rôles sont agrégés en **une seule requête** (`GROUP_CONCAT`) — plus de N+1.
/// Sans paramètre : renvoie tous les membres (jusqu'au plafond) triés par date d'arrivée,
/// comme avant. Avec `after`/`limit` : pagination par curseur (`user_id` croissant).
pub async fn list_members(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
    axum::extract::Query(q): axum::extract::Query<MemberQuery>,
) -> AppResult<Json<Vec<Member>>> {
    let gid = parse_i64(&gid)?;
    let viewer_perms =
        pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::VIEW_CHANNEL).await?;
    // La méthode d'adhésion (code d'invitation) n'est révélée qu'aux gestionnaires du serveur
    // (sinon `list_members`, utilisé aussi par la liste latérale, fuiterait des codes à tous).
    let can_see_method = perms::has(viewer_perms, perms::MANAGE_GUILD);
    let limit = q.limit.unwrap_or(1000).clamp(1, 1000);
    let after = q.after.as_deref().and_then(|s| s.parse::<i64>().ok());
    // Recherche : motif LIKE en minuscules (préfixe + sous-chaîne).
    let like = q
        .query
        .as_deref()
        .map(|s| format!("%{}%", s.trim().to_lowercase()))
        .filter(|s| s.len() > 2);

    let base = "SELECT gm.user_id, gm.nick, gm.joined_at, gm.invite_code, u.username, u.display_name, u.avatar_id, \
        (SELECT GROUP_CONCAT(mr.role_id) FROM member_roles mr WHERE mr.guild_id = gm.guild_id AND mr.user_id = gm.user_id) AS role_ids \
        FROM guild_members gm JOIN users u ON u.id = gm.user_id \
        WHERE gm.guild_id = ?1 \
          AND (?2 IS NULL OR gm.user_id > ?2) \
          AND (?3 IS NULL OR LOWER(u.username) LIKE ?3 OR LOWER(COALESCE(gm.nick, u.display_name, u.username)) LIKE ?3)";
    // Tri : par curseur (user_id) si pagination/ recherche, sinon par date d'arrivée (compat).
    let order = if after.is_some() || like.is_some() {
        " ORDER BY gm.user_id LIMIT ?4"
    } else {
        " ORDER BY gm.joined_at LIMIT ?4"
    };
    let rows = sqlx::query(&format!("{base}{order}"))
        .bind(gid)
        .bind(after)
        .bind(like.as_deref())
        .bind(limit)
        .fetch_all(&st.pool)
        .await?;
    Ok(Json(
        rows.iter().map(|r| row_to_member(r, can_see_method)).collect(),
    ))
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

/// `DELETE /guilds/:guild_id/members/@me` — quitter une guilde soi-même.
pub async fn leave_guild(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let gid = parse_i64(&gid)?;
    let me = user.id.as_i64();
    let owner = pg::guild_owner(&st.pool, gid)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?;
    if owner == me {
        return Err(AppError::forbidden(
            "le propriétaire ne peut pas quitter sa guilde (supprimez-la d'abord)",
        ));
    }
    let res = sqlx::query("DELETE FROM guild_members WHERE guild_id = ? AND user_id = ?")
        .bind(gid)
        .bind(me)
        .execute(&st.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found(
            "vous n'êtes pas membre de cette guilde",
        ));
    }
    sqlx::query("DELETE FROM member_roles WHERE guild_id = ? AND user_id = ?")
        .bind(gid)
        .bind(me)
        .execute(&st.pool)
        .await?;
    st.publish(
        EventScope::Guild(gid),
        "GUILD_MEMBER_REMOVE",
        serde_json::json!({ "guild_id": gid.to_string(), "user_id": me.to_string() }),
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `POST /guilds/:guild_id/transfer` — transfère la propriété de la guilde
/// (réservé au propriétaire actuel ; la cible doit être membre).
pub async fn transfer_guild(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
    Json(req): Json<ozone_proto::dto::TransferGuild>,
) -> AppResult<Json<serde_json::Value>> {
    let gid = parse_i64(&gid)?;
    let me = user.id.as_i64();
    let owner = pg::guild_owner(&st.pool, gid)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?;
    if owner != me {
        return Err(AppError::forbidden(
            "seul le propriétaire peut transférer la guilde",
        ));
    }
    let target = req.new_owner_id.as_i64();
    if target == me {
        return Err(AppError::bad_request("vous êtes déjà le propriétaire"));
    }
    let is_member = sqlx::query("SELECT 1 FROM guild_members WHERE guild_id = ? AND user_id = ?")
        .bind(gid)
        .bind(target)
        .fetch_optional(&st.pool)
        .await?
        .is_some();
    if !is_member {
        return Err(AppError::bad_request(
            "le nouveau propriétaire doit être membre de la guilde",
        ));
    }
    sqlx::query("UPDATE guilds SET owner_id = ? WHERE id = ?")
        .bind(target)
        .bind(gid)
        .execute(&st.pool)
        .await?;
    crate::routes_moderation::record_audit(
        &st,
        gid,
        me,
        Some(target),
        "guild_owner_transfer",
        None,
    )
    .await;
    // Diffuse la guilde complète : le client met à jour owner_id (couronne, gardes UI).
    if let Some(guild) = crate::routes_chat::fetch_guild_full(&st, gid).await? {
        st.publish(
            EventScope::Guild(gid),
            "GUILD_UPDATE",
            serde_json::to_value(&guild).unwrap_or_default(),
        );
    }
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
    st.rate
        .check(crate::ratelimit::INVITE, &user.id.to_string())
        .map_err(AppError::rate_limited)?;
    pg::require_guild_perm(
        &st.pool,
        gid,
        user.id.as_i64(),
        perms::CREATE_INSTANT_INVITE,
    )
    .await?;
    let now = now_ms();
    // `max_age` est contrôlé par le client : on le borne (≤ 30 jours, comme Discord) et on calcule
    // l'expiration en arithmétique vérifiée pour éviter tout débordement (panic en debug / valeur
    // aberrante en release). ≤ 0 = invitation permanente.
    const MAX_INVITE_AGE_SECS: i64 = 30 * 24 * 3600;
    if req.max_age > MAX_INVITE_AGE_SECS {
        return Err(AppError::bad_request(
            "durée de validité trop longue (max 30 jours)",
        ));
    }
    let expires_at = if req.max_age > 0 {
        Some(now + req.max_age * 1000) // borné ≤ 30 j ⇒ aucun débordement possible
    } else {
        None
    };
    // Code personnalisé optionnel ([a-z0-9-]{2,32}) — sinon code aléatoire. Doit être MANAGE_GUILD
    // pour un code custom (réserver un code lisible est plus sensible qu'un code jetable).
    let code = match req.code.as_ref().map(|c| c.trim()).filter(|c| !c.is_empty()) {
        Some(custom) => {
            pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::MANAGE_GUILD).await?;
            let c = custom.to_lowercase();
            if c.len() < 2
                || c.len() > 32
                || !c.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
            {
                return Err(AppError::bad_request(
                    "code invalide (2 à 32 caractères : a-z, 0-9, -)",
                ));
            }
            let taken = sqlx::query("SELECT 1 FROM invites WHERE code = ?")
                .bind(&c)
                .fetch_optional(&st.pool)
                .await?
                .is_some();
            if taken {
                return Err(AppError::bad_request("ce code est déjà utilisé"));
            }
            c
        }
        None => gen_code(),
    };
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
    crate::routes_moderation::audit_named(&st, gid, user.id.as_i64(), "invite_create", &code).await;
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
    let now = now_ms();
    // Le code peut être une invitation classique OU un code vanity de guilde (permanent).
    let invite_row = sqlx::query("SELECT * FROM invites WHERE code = ?")
        .bind(&code)
        .fetch_optional(&st.pool)
        .await?;
    let (gid, is_vanity) = match invite_row {
        Some(row) => {
            let inv = row_to_invite(row);
            if let Some(exp) = inv.expires_at {
                if (exp as i64) < now {
                    return Err(AppError::not_found("invitation expirée"));
                }
            }
            if inv.max_uses > 0 && inv.uses >= inv.max_uses {
                return Err(AppError::forbidden("invitation épuisée"));
            }
            (inv.guild_id.as_i64(), false)
        }
        None => {
            // Repli vanity : code permanent défini sur la guilde.
            let vid = sqlx::query("SELECT id FROM guilds WHERE vanity_code = ?")
                .bind(code.to_lowercase())
                .fetch_optional(&st.pool)
                .await?
                .map(|r| r.get::<i64, _>("id"))
                .ok_or_else(|| AppError::not_found("invitation invalide"))?;
            (vid, true)
        }
    };

    let banned = sqlx::query("SELECT 1 FROM guild_bans WHERE guild_id = ? AND user_id = ?")
        .bind(gid)
        .bind(user.id.as_i64())
        .fetch_optional(&st.pool)
        .await?
        .is_some();
    if banned {
        return Err(AppError::forbidden("vous êtes banni de cette guilde"));
    }

    let inserted = sqlx::query(
        "INSERT OR IGNORE INTO guild_members (guild_id, user_id, nick, joined_at, invite_code) \
         VALUES (?, ?, NULL, ?, ?)",
    )
    .bind(gid)
    .bind(user.id.as_i64())
    .bind(now)
    .bind(&code)
    .execute(&st.pool)
    .await?
    .rows_affected()
        > 0;
    // Rôle @everyone (id == guild_id) attribué dès l'arrivée.
    sqlx::query("INSERT OR IGNORE INTO member_roles (guild_id, user_id, role_id) VALUES (?, ?, ?)")
        .bind(gid)
        .bind(user.id.as_i64())
        .bind(gid)
        .execute(&st.pool)
        .await?;
    // Adhésion réellement NOUVELLE : consomme l'invitation (sauf vanity, permanent), annonce.
    if inserted {
        if !is_vanity {
            sqlx::query("UPDATE invites SET uses = uses + 1 WHERE code = ?")
                .bind(&code)
                .execute(&st.pool)
                .await?;
        }
        st.publish(
            EventScope::Guild(gid),
            "GUILD_MEMBER_ADD",
            serde_json::json!({ "guild_id": gid.to_string(), "user_id": user.id.to_string() }),
        );
        announce_member_join(&st, gid, user.id.as_i64()).await;
    }

    let guild = crate::routes_chat::fetch_guild_full(&st, gid)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?;
    Ok(Json(guild))
}

/// Message système d'arrivée (type 7) dans le salon système de la guilde, s'il est configuré
/// et existe encore. Best-effort : ne bloque jamais l'adhésion.
pub async fn announce_member_join(st: &AppState, gid: i64, uid: i64) {
    let sys: Option<i64> = sqlx::query(
        "SELECT g.system_channel_id AS c FROM guilds g \
         JOIN channels ch ON ch.id = g.system_channel_id WHERE g.id = ?",
    )
    .bind(gid)
    .fetch_optional(&st.pool)
    .await
    .ok()
    .flatten()
    .and_then(|r| r.get("c"));
    if let Some(cid) = sys {
        crate::routes_messages::insert_system_message(st, cid, uid, 7).await;
    }
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
    let g = sqlx::query("SELECT name, icon_id, private_profile FROM guilds WHERE id = ?")
        .bind(gid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?;
    // R13 — profil privé : l'aperçu est LIMITÉ pour les non-membres (nom seul, pas d'icône
    // ni d'effectif). Le nom reste nécessaire pour décider d'accepter l'invitation.
    let private = g.get::<i64, _>("private_profile") != 0;
    let is_member = sqlx::query("SELECT 1 FROM guild_members WHERE guild_id = ? AND user_id = ?")
        .bind(gid)
        .bind(_user.id.as_i64())
        .fetch_optional(&st.pool)
        .await?
        .is_some();
    let limited = private && !is_member;
    let member_count: i64 = if limited {
        0
    } else {
        sqlx::query("SELECT COUNT(*) AS c FROM guild_members WHERE guild_id = ?")
            .bind(gid)
            .fetch_one(&st.pool)
            .await?
            .get("c")
    };
    Ok(Json(InvitePreview {
        code,
        guild_id: Snowflake::from_i64(gid),
        guild_name: g.get("name"),
        guild_icon: if limited { None } else { g.get("icon_id") },
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
    crate::routes_moderation::audit_named(&st, gid, user.id.as_i64(), "invite_delete", &code).await;
    Ok(Json(serde_json::json!({ "ok": true })))
}
