//! Annuaire de découverte : liste publique des guildes ayant **opté** pour la découverte
//! (`discoverable`) et adhésion directe. Cf. docs/features/19-decouverte-onboarding.md.

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::state::{AppState, EventScope};
use crate::util::parse_i64;
use axum::extract::{Path, Query, State};
use axum::Json;
use ozone_proto::dto::{DiscoveryGuild, Guild};
use ozone_proto::Snowflake;
use serde::Deserialize;
use sqlx::Row;

#[derive(Debug, Deserialize)]
pub struct DiscoveryQuery {
    q: Option<String>,
    limit: Option<i64>,
}

/// `GET /discovery/guilds?q=&limit=` — guildes publiques (opt-in), triées par taille.
pub async fn list_discovery(
    State(st): State<AppState>,
    _user: AuthUser,
    Query(query): Query<DiscoveryQuery>,
) -> AppResult<Json<Vec<DiscoveryGuild>>> {
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let like = format!("%{}%", query.q.as_deref().unwrap_or("").trim());
    let rows = sqlx::query(
        "SELECT g.id, g.name, g.icon_id, g.description, \
                (SELECT COUNT(*) FROM guild_members m WHERE m.guild_id = g.id) AS member_count \
         FROM guilds g WHERE g.discoverable = 1 AND g.name LIKE ? \
         ORDER BY member_count DESC, g.id LIMIT ?",
    )
    .bind(&like)
    .bind(limit)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| DiscoveryGuild {
                id: Snowflake::from_i64(r.get::<i64, _>("id")),
                name: r.get("name"),
                icon_id: r.get("icon_id"),
                description: r.get("description"),
                member_count: r.get::<i64, _>("member_count"),
            })
            .collect(),
    ))
}

/// `POST /discovery/guilds/:guild_id/join` — rejoindre directement une guilde publique.
pub async fn join_discovery(
    State(st): State<AppState>,
    user: AuthUser,
    Path(guild_id): Path<String>,
) -> AppResult<Json<Guild>> {
    let gid = parse_i64(&guild_id)?;
    let me = user.id.as_i64();
    let row = sqlx::query(
        "SELECT id, name, owner_id, icon_id, description, discoverable FROM guilds WHERE id = ?",
    )
    .bind(gid)
    .fetch_optional(&st.pool)
    .await?
    .ok_or_else(|| AppError::not_found("guilde introuvable"))?;
    if row.get::<i64, _>("discoverable") == 0 {
        // On ne révèle pas l'existence d'une guilde non publique.
        return Err(AppError::not_found("guilde introuvable"));
    }
    // Respecte les bannissements.
    let banned = sqlx::query("SELECT 1 FROM guild_bans WHERE guild_id = ? AND user_id = ?")
        .bind(gid)
        .bind(me)
        .fetch_optional(&st.pool)
        .await?
        .is_some();
    if banned {
        return Err(AppError::forbidden("vous êtes banni de cette guilde"));
    }

    let res = sqlx::query(
        "INSERT OR IGNORE INTO guild_members (guild_id, user_id, nick, joined_at) VALUES (?, ?, NULL, ?)",
    )
    .bind(gid)
    .bind(me)
    .bind(now_ms())
    .execute(&st.pool)
    .await?;
    // Rôle @everyone (id == guild_id) attribué dès l'arrivée.
    sqlx::query("INSERT OR IGNORE INTO member_roles (guild_id, user_id, role_id) VALUES (?, ?, ?)")
        .bind(gid)
        .bind(me)
        .bind(gid)
        .execute(&st.pool)
        .await?;
    if res.rows_affected() > 0 {
        st.publish(
            EventScope::Guild(gid),
            "GUILD_MEMBER_ADD",
            serde_json::json!({ "guild_id": gid.to_string(), "user_id": me.to_string() }),
        );
        crate::routes_guild::announce_member_join(&st, gid, me).await;
    }
    let guild = crate::routes_chat::fetch_guild_full(&st, gid)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?;
    Ok(Json(guild))
}
