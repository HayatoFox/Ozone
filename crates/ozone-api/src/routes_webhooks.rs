//! Webhooks entrants : gestion (permission `MANAGE_WEBHOOKS`) et **exécution**
//! authentifiée par jeton dans l'URL (sans session). Cf. docs/features/17-webhooks-bots-integrations.md.

use crate::crypto;
use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::permissions as pg;
use crate::routes_messages::insert_webhook_message;
use crate::state::AppState;
use crate::util::parse_i64;
use axum::extract::{Path, Query, State};
use axum::Json;
use ozone_proto::dto::{CreateWebhook, ExecuteWebhook, Webhook};
use ozone_proto::{perms, Snowflake};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

const WH_SELECT: &str =
    "SELECT id, channel_id, guild_id, name, avatar_id, token, created_by, created_at FROM webhooks";

fn row_to_webhook(r: &SqliteRow, with_token: bool) -> Webhook {
    Webhook {
        id: Snowflake::from_i64(r.get::<i64, _>("id")),
        channel_id: Snowflake::from_i64(r.get::<i64, _>("channel_id")),
        guild_id: Snowflake::from_i64(r.get::<i64, _>("guild_id")),
        name: r.get("name"),
        avatar_id: r.get("avatar_id"),
        created_by: Snowflake::from_i64(r.get::<i64, _>("created_by")),
        created_at: r.get::<i64, _>("created_at") as u64,
        token: if with_token {
            Some(r.get("token"))
        } else {
            None
        },
    }
}

/// Valide un nom de webhook (ou un nom d'affichage de remplacement).
fn validate_name(name: &str) -> AppResult<String> {
    let n = name.trim().to_string();
    if n.is_empty() || n.chars().count() > 80 {
        return Err(AppError::bad_request(
            "nom de webhook invalide (1 à 80 caractères)",
        ));
    }
    let lower = n.to_lowercase();
    if lower.contains("clyde") || lower.contains("discord") {
        return Err(AppError::bad_request("nom de webhook réservé"));
    }
    Ok(n)
}

/// Charge un webhook par identifiant (404 si absent).
async fn fetch_webhook(st: &AppState, id: i64) -> AppResult<SqliteRow> {
    sqlx::query(&format!("{WH_SELECT} WHERE id = ?"))
        .bind(id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("webhook introuvable"))
}

// ───────────────────────────── Gestion (session + MANAGE_WEBHOOKS) ─────────────────────────────

/// `POST /channels/:channel_id/webhooks`
pub async fn create_webhook(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
    Json(req): Json<CreateWebhook>,
) -> AppResult<Json<Webhook>> {
    let cid = parse_i64(&cid)?;
    let (gid, _owner, _p) =
        pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::MANAGE_WEBHOOKS).await?;
    if gid == 0 {
        return Err(AppError::bad_request(
            "les webhooks ne sont pas disponibles en messages privés",
        ));
    }
    let name = validate_name(&req.name)?;
    let id = st.ids.next();
    let token = crypto::random_token();
    let now = now_ms();
    sqlx::query(
        "INSERT INTO webhooks (id, channel_id, guild_id, name, avatar_id, token, created_by, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.as_i64())
    .bind(cid)
    .bind(gid)
    .bind(&name)
    .bind(req.avatar_id.as_deref())
    .bind(&token)
    .bind(user.id.as_i64())
    .bind(now)
    .execute(&st.pool)
    .await?;
    Ok(Json(Webhook {
        id,
        channel_id: Snowflake::from_i64(cid),
        guild_id: Snowflake::from_i64(gid),
        name,
        avatar_id: req.avatar_id,
        created_by: user.id,
        created_at: now as u64,
        token: Some(token),
    }))
}

/// `GET /channels/:channel_id/webhooks`
pub async fn list_channel_webhooks(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
) -> AppResult<Json<Vec<Webhook>>> {
    let cid = parse_i64(&cid)?;
    let (gid, _owner, _p) =
        pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::MANAGE_WEBHOOKS).await?;
    if gid == 0 {
        return Err(AppError::bad_request(
            "les webhooks ne sont pas disponibles en messages privés",
        ));
    }
    let rows = sqlx::query(&format!(
        "{WH_SELECT} WHERE channel_id = ? ORDER BY id DESC"
    ))
    .bind(cid)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.iter().map(|r| row_to_webhook(r, false)).collect(),
    ))
}

/// `GET /guilds/:guild_id/webhooks`
pub async fn list_guild_webhooks(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
) -> AppResult<Json<Vec<Webhook>>> {
    let gid = parse_i64(&gid)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::MANAGE_WEBHOOKS).await?;
    let rows = sqlx::query(&format!("{WH_SELECT} WHERE guild_id = ? ORDER BY id DESC"))
        .bind(gid)
        .fetch_all(&st.pool)
        .await?;
    Ok(Json(
        rows.iter().map(|r| row_to_webhook(r, false)).collect(),
    ))
}

/// `PATCH /webhooks/:webhook_id`
pub async fn update_webhook(
    State(st): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    Json(req): Json<ozone_proto::dto::UpdateWebhook>,
) -> AppResult<Json<Webhook>> {
    let wid = parse_i64(&wid)?;
    let wh = fetch_webhook(&st, wid).await?;
    let channel_id: i64 = wh.get("channel_id");
    let guild_id: i64 = wh.get("guild_id");
    // Droit de gestion sur le salon courant du webhook.
    pg::require_channel_perm(
        &st.pool,
        channel_id,
        user.id.as_i64(),
        perms::MANAGE_WEBHOOKS,
    )
    .await?;

    let new_name = match &req.name {
        Some(n) => validate_name(n)?,
        None => wh.get("name"),
    };
    // Déplacement éventuel : salon cible dans la **même guilde** + droit sur la cible.
    let new_channel = match req.channel_id {
        Some(target) => {
            let target = target.as_i64();
            let (tgid, _o, _p) = pg::require_channel_perm(
                &st.pool,
                target,
                user.id.as_i64(),
                perms::MANAGE_WEBHOOKS,
            )
            .await?;
            if tgid != guild_id {
                return Err(AppError::bad_request(
                    "le salon cible doit appartenir à la même guilde",
                ));
            }
            target
        }
        None => channel_id,
    };
    let new_avatar = req.avatar_id.or_else(|| wh.get("avatar_id"));

    sqlx::query("UPDATE webhooks SET name = ?, avatar_id = ?, channel_id = ? WHERE id = ?")
        .bind(&new_name)
        .bind(new_avatar.as_deref())
        .bind(new_channel)
        .bind(wid)
        .execute(&st.pool)
        .await?;
    let row = fetch_webhook(&st, wid).await?;
    Ok(Json(row_to_webhook(&row, false)))
}

/// `POST /webhooks/:webhook_id` — régénère le jeton secret.
pub async fn regenerate_token(
    State(st): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
) -> AppResult<Json<Webhook>> {
    let wid = parse_i64(&wid)?;
    let wh = fetch_webhook(&st, wid).await?;
    let channel_id: i64 = wh.get("channel_id");
    pg::require_channel_perm(
        &st.pool,
        channel_id,
        user.id.as_i64(),
        perms::MANAGE_WEBHOOKS,
    )
    .await?;
    let token = crypto::random_token();
    sqlx::query("UPDATE webhooks SET token = ? WHERE id = ?")
        .bind(&token)
        .bind(wid)
        .execute(&st.pool)
        .await?;
    let row = fetch_webhook(&st, wid).await?;
    let mut out = row_to_webhook(&row, false);
    out.token = Some(token);
    Ok(Json(out))
}

/// `DELETE /webhooks/:webhook_id`
pub async fn delete_webhook(
    State(st): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
) -> AppResult<Json<Value>> {
    let wid = parse_i64(&wid)?;
    let wh = fetch_webhook(&st, wid).await?;
    let channel_id: i64 = wh.get("channel_id");
    pg::require_channel_perm(
        &st.pool,
        channel_id,
        user.id.as_i64(),
        perms::MANAGE_WEBHOOKS,
    )
    .await?;
    sqlx::query("DELETE FROM webhooks WHERE id = ?")
        .bind(wid)
        .execute(&st.pool)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

// ───────────────────────────── Exécution (jeton, sans session) ─────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ExecQuery {
    #[serde(default)]
    wait: Option<bool>,
}

/// `POST /webhooks/:webhook_id/:token` — exécute le webhook (aucune session requise).
pub async fn execute_webhook(
    State(st): State<AppState>,
    Path((wid, token)): Path<(String, String)>,
    Query(q): Query<ExecQuery>,
    Json(req): Json<ExecuteWebhook>,
) -> AppResult<Json<Value>> {
    let wid = parse_i64(&wid)?;
    let wh = fetch_webhook(&st, wid).await.map_err(|_| {
        // Ne pas distinguer « id inconnu » de « jeton invalide » : message uniforme.
        AppError::unauthorized("webhook ou jeton invalide")
    })?;
    let real: String = wh.get("token");
    if real.as_bytes() != token.as_bytes() {
        return Err(AppError::unauthorized("webhook ou jeton invalide"));
    }
    let channel_id: i64 = wh.get("channel_id");
    let created_by: i64 = wh.get("created_by");

    // Le salon doit toujours exister.
    if pg::channel_guild(&st.pool, channel_id).await?.is_none() {
        return Err(AppError::not_found("salon du webhook introuvable"));
    }

    let content = req.content.trim_end();
    if content.is_empty() || content.chars().count() > 4000 {
        return Err(AppError::bad_request(
            "contenu de message invalide (1 à 4000 caractères)",
        ));
    }
    // Surcharge facultative du nom d'affichage (validée comme un nom de webhook).
    let name_override = match req.username.as_deref() {
        Some(u) => Some(validate_name(u)?),
        None => Some(wh.get::<String, _>("name")),
    };
    let avatar_override = req
        .avatar_id
        .or_else(|| wh.get::<Option<String>, _>("avatar_id"));

    let msg = insert_webhook_message(
        &st,
        channel_id,
        wid,
        created_by,
        name_override.as_deref(),
        avatar_override.as_deref(),
        content,
    )
    .await?;

    if q.wait.unwrap_or(false) {
        Ok(Json(serde_json::to_value(&msg).unwrap_or_default()))
    } else {
        Ok(Json(json!({ "ok": true })))
    }
}
