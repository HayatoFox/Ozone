//! Messages privés (1:1) et groupes de MP. Les salons réutilisent `channels`
//! (guild_id NULL, type 1 = MP, 3 = groupe). Cf. docs/features/07-messages-prives.md.
//!
//! La messagerie elle-même (envoi, réactions, épingles…) fonctionne via les routes
//! `routes_messages`, l'accès étant accordé par `permissions::dm_permissions`.

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::state::AppState;
use crate::util::parse_i64;
use axum::extract::{Path, State};
use axum::Json;
use ozone_proto::dto::{CreateDM, DMChannel, User};
use ozone_proto::Snowflake;
use serde_json::{json, Value};
use sqlx::Row;

// ───────────────────────────── Helpers ─────────────────────────────

async fn user_exists(st: &AppState, id: i64) -> AppResult<bool> {
    Ok(sqlx::query("SELECT 1 FROM users WHERE id = ?")
        .bind(id)
        .fetch_optional(&st.pool)
        .await?
        .is_some())
}

async fn is_recipient(st: &AppState, cid: i64, uid: i64) -> AppResult<bool> {
    Ok(
        sqlx::query("SELECT 1 FROM dm_recipients WHERE channel_id = ? AND user_id = ?")
            .bind(cid)
            .bind(uid)
            .fetch_optional(&st.pool)
            .await?
            .is_some(),
    )
}

async fn build_dm_channel(st: &AppState, cid: i64) -> AppResult<DMChannel> {
    let row = sqlx::query("SELECT type, name, owner_id FROM channels WHERE id = ?")
        .bind(cid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("salon introuvable"))?;
    let kind = row.get::<i64, _>("type") as u8;
    let name_raw: String = row.get("name");
    let name = if name_raw.is_empty() {
        None
    } else {
        Some(name_raw)
    };
    let owner_id: Option<i64> = row.get("owner_id");

    let recs = sqlx::query(
        "SELECT u.id, u.username, u.display_name, u.avatar_id \
         FROM dm_recipients r JOIN users u ON u.id = r.user_id WHERE r.channel_id = ? ORDER BY u.id",
    )
    .bind(cid)
    .fetch_all(&st.pool)
    .await?;
    let recipients = recs
        .into_iter()
        .map(|r| User {
            id: Snowflake::from_i64(r.get::<i64, _>("id")),
            username: r.get("username"),
            display_name: r.get("display_name"),
            avatar_id: r.get("avatar_id"),
            email: None,
        })
        .collect();

    let last: Option<i64> = sqlx::query("SELECT MAX(id) AS m FROM messages WHERE channel_id = ?")
        .bind(cid)
        .fetch_one(&st.pool)
        .await?
        .get("m");

    Ok(DMChannel {
        id: Snowflake::from_i64(cid),
        kind,
        name,
        owner_id: owner_id.map(Snowflake::from_i64),
        recipients,
        last_message_id: last.map(Snowflake::from_i64),
    })
}

// ───────────────────────────── Handlers ─────────────────────────────

/// `POST /users/@me/channels` — ouvre un MP (1 destinataire) ou crée un groupe (2 à 9).
pub async fn open_or_create_dm(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateDM>,
) -> AppResult<Json<DMChannel>> {
    let me = user.id.as_i64();
    let mut recips: Vec<i64> = req
        .recipients
        .iter()
        .map(|s| s.as_i64())
        .filter(|&r| r != me)
        .collect();
    recips.sort_unstable();
    recips.dedup();
    if recips.is_empty() {
        return Err(AppError::bad_request("au moins un destinataire requis"));
    }
    if recips.len() > 9 {
        return Err(AppError::bad_request(
            "un groupe est limité à 10 participants",
        ));
    }
    for &r in &recips {
        if !user_exists(&st, r).await? {
            return Err(AppError::not_found("destinataire introuvable"));
        }
    }

    if recips.len() == 1 {
        let target = recips[0];
        // Déduplication : réutilise le MP 1:1 existant s'il y en a un.
        let existing: Option<i64> = sqlx::query(
            "SELECT c.id FROM channels c WHERE c.type = 1 AND c.guild_id IS NULL \
             AND EXISTS (SELECT 1 FROM dm_recipients r WHERE r.channel_id = c.id AND r.user_id = ?) \
             AND EXISTS (SELECT 1 FROM dm_recipients r WHERE r.channel_id = c.id AND r.user_id = ?) \
             AND (SELECT COUNT(*) FROM dm_recipients r WHERE r.channel_id = c.id) = 2 LIMIT 1",
        )
        .bind(me)
        .bind(target)
        .fetch_optional(&st.pool)
        .await?
        .map(|r| r.get("id"));

        let cid = match existing {
            Some(cid) => cid,
            None => {
                let id = st.ids.next();
                sqlx::query(
                    "INSERT INTO channels (id, guild_id, type, name, topic, position, parent_id, owner_id, created_at) \
                     VALUES (?, NULL, 1, '', NULL, 0, NULL, NULL, ?)",
                )
                .bind(id.as_i64())
                .bind(now_ms())
                .execute(&st.pool)
                .await?;
                for u in [me, target] {
                    sqlx::query(
                        "INSERT OR IGNORE INTO dm_recipients (channel_id, user_id) VALUES (?, ?)",
                    )
                    .bind(id.as_i64())
                    .bind(u)
                    .execute(&st.pool)
                    .await?;
                }
                id.as_i64()
            }
        };
        Ok(Json(build_dm_channel(&st, cid).await?))
    } else {
        // Groupe de MP (propriétaire = créateur).
        let id = st.ids.next();
        sqlx::query(
            "INSERT INTO channels (id, guild_id, type, name, topic, position, parent_id, owner_id, created_at) \
             VALUES (?, NULL, 3, '', NULL, 0, NULL, ?, ?)",
        )
        .bind(id.as_i64())
        .bind(me)
        .bind(now_ms())
        .execute(&st.pool)
        .await?;
        for u in std::iter::once(me).chain(recips.iter().copied()) {
            sqlx::query("INSERT OR IGNORE INTO dm_recipients (channel_id, user_id) VALUES (?, ?)")
                .bind(id.as_i64())
                .bind(u)
                .execute(&st.pool)
                .await?;
        }
        Ok(Json(build_dm_channel(&st, id.as_i64()).await?))
    }
}

/// `GET /users/@me/channels` — liste les MP et groupes de l'utilisateur.
pub async fn list_dm_channels(
    State(st): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<DMChannel>>> {
    let me = user.id.as_i64();
    let rows = sqlx::query("SELECT channel_id FROM dm_recipients WHERE user_id = ?")
        .bind(me)
        .fetch_all(&st.pool)
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(build_dm_channel(&st, r.get::<i64, _>("channel_id")).await?);
    }
    out.sort_by(|a, b| {
        let bl = b.last_message_id.map(|s| s.get()).unwrap_or(0);
        let al = a.last_message_id.map(|s| s.get()).unwrap_or(0);
        bl.cmp(&al)
    });
    Ok(Json(out))
}

/// `PUT /channels/:channel_id/recipients/:user_id` — ajoute un membre à un groupe.
pub async fn add_recipient(
    State(st): State<AppState>,
    user: AuthUser,
    Path((cid, target)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let cid = parse_i64(&cid)?;
    let target = parse_i64(&target)?;
    if !is_recipient(&st, cid, user.id.as_i64()).await? {
        return Err(AppError::forbidden(
            "vous ne faites pas partie de ce groupe",
        ));
    }
    let kind: i64 = sqlx::query("SELECT type FROM channels WHERE id = ? AND guild_id IS NULL")
        .bind(cid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("groupe introuvable"))?
        .get("type");
    if kind != 3 {
        return Err(AppError::bad_request("opération réservée aux groupes"));
    }
    if !user_exists(&st, target).await? {
        return Err(AppError::not_found("destinataire introuvable"));
    }
    let count: i64 = sqlx::query("SELECT COUNT(*) AS c FROM dm_recipients WHERE channel_id = ?")
        .bind(cid)
        .fetch_one(&st.pool)
        .await?
        .get("c");
    if count >= 10 {
        return Err(AppError::bad_request(
            "groupe complet (10 participants maximum)",
        ));
    }
    sqlx::query("INSERT OR IGNORE INTO dm_recipients (channel_id, user_id) VALUES (?, ?)")
        .bind(cid)
        .bind(target)
        .execute(&st.pool)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

/// `DELETE /channels/:channel_id/recipients/:user_id` — quitte un groupe (soi-même)
/// ou en retire un membre (propriétaire uniquement).
pub async fn remove_recipient(
    State(st): State<AppState>,
    user: AuthUser,
    Path((cid, target)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let me = user.id.as_i64();
    let cid = parse_i64(&cid)?;
    let target = parse_i64(&target)?;
    if !is_recipient(&st, cid, me).await? {
        return Err(AppError::forbidden(
            "vous ne faites pas partie de ce groupe",
        ));
    }
    let row = sqlx::query("SELECT type, owner_id FROM channels WHERE id = ? AND guild_id IS NULL")
        .bind(cid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("groupe introuvable"))?;
    let kind: i64 = row.get("type");
    let owner: Option<i64> = row.get("owner_id");
    if kind != 3 {
        return Err(AppError::bad_request("opération réservée aux groupes"));
    }
    if target != me && owner != Some(me) {
        return Err(AppError::forbidden(
            "seul le propriétaire peut retirer un autre membre",
        ));
    }

    sqlx::query("DELETE FROM dm_recipients WHERE channel_id = ? AND user_id = ?")
        .bind(cid)
        .bind(target)
        .execute(&st.pool)
        .await?;

    // Si le propriétaire part, on transfère la propriété (ou on supprime le groupe vide).
    if Some(target) == owner {
        let next: Option<i64> = sqlx::query(
            "SELECT user_id FROM dm_recipients WHERE channel_id = ? ORDER BY user_id LIMIT 1",
        )
        .bind(cid)
        .fetch_optional(&st.pool)
        .await?
        .map(|r| r.get("user_id"));
        match next {
            Some(n) => {
                sqlx::query("UPDATE channels SET owner_id = ? WHERE id = ?")
                    .bind(n)
                    .bind(cid)
                    .execute(&st.pool)
                    .await?;
            }
            None => {
                sqlx::query("DELETE FROM reactions WHERE message_id IN (SELECT id FROM messages WHERE channel_id = ?)")
                    .bind(cid)
                    .execute(&st.pool)
                    .await?;
                sqlx::query("DELETE FROM messages WHERE channel_id = ?")
                    .bind(cid)
                    .execute(&st.pool)
                    .await?;
                sqlx::query("DELETE FROM channels WHERE id = ?")
                    .bind(cid)
                    .execute(&st.pool)
                    .await?;
            }
        }
    }
    Ok(Json(json!({ "ok": true })))
}
