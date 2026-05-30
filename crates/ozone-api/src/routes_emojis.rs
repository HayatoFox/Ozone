//! Emojis personnalisés d'une guilde (liste, création, modification, suppression).
//! Cf. `docs/features/11-expressions.md`.

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::permissions as pg;
use crate::state::AppState;
use crate::util::parse_i64;
use axum::extract::{Path, State};
use axum::Json;
use ozone_proto::dto::{CreateEmoji, Emoji, UpdateEmoji};
use ozone_proto::{perms, Snowflake};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

// ───────────────────────── Helpers privés ─────────────────────────

/// Convertit une ligne SQLite en DTO [`Emoji`].
fn row_to_emoji(r: SqliteRow) -> Emoji {
    Emoji {
        id: Snowflake::from_i64(r.get::<i64, _>("id")),
        guild_id: Snowflake::from_i64(r.get::<i64, _>("guild_id")),
        name: r.get("name"),
        animated: r.get::<i64, _>("animated") != 0,
        image_id: r.get("image_id"),
        created_by: Snowflake::from_i64(r.get::<i64, _>("created_by")),
        available: r.get::<i64, _>("available") != 0,
    }
}

/// Charge un emoji par `(guild_id, emoji_id)` ; renvoie 404 s'il est absent.
async fn fetch_emoji(st: &AppState, gid: i64, eid: i64) -> AppResult<SqliteRow> {
    sqlx::query(
        "SELECT id, guild_id, name, animated, image_id, created_by, available \
         FROM emojis WHERE id = ? AND guild_id = ?",
    )
    .bind(eid)
    .bind(gid)
    .fetch_optional(&st.pool)
    .await?
    .ok_or_else(|| AppError::not_found("emoji introuvable"))
}

/// Valide le nom d'un emoji : longueur 2–32 et caractères `[A-Za-z0-9_]` uniquement.
fn validate_name(name: &str) -> AppResult<()> {
    let len = name.len();
    if !(2..=32).contains(&len) {
        return Err(AppError::bad_request(
            "le nom de l'emoji doit faire entre 2 et 32 caractères",
        ));
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(AppError::bad_request(
            "le nom de l'emoji ne peut contenir que des lettres, chiffres et tirets bas",
        ));
    }
    Ok(())
}

// ───────────────────────── Handlers publics ─────────────────────────

/// `GET /guilds/:guild_id/emojis` — Liste tous les emojis d'une guilde.
///
/// Exige d'être membre (`VIEW_CHANNEL`).
pub async fn list_emojis(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
) -> AppResult<Json<Vec<Emoji>>> {
    let gid = parse_i64(&gid)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::VIEW_CHANNEL).await?;
    let rows = sqlx::query(
        "SELECT id, guild_id, name, animated, image_id, created_by, available \
         FROM emojis WHERE guild_id = ? ORDER BY id",
    )
    .bind(gid)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(rows.into_iter().map(row_to_emoji).collect()))
}

/// `POST /guilds/:guild_id/emojis` — Crée un nouvel emoji dans la guilde.
///
/// Exige `CREATE_GUILD_EXPRESSIONS` ou `MANAGE_GUILD_EXPRESSIONS`.
/// Le nom est normalisé (trim) puis validé.
pub async fn create_emoji(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
    Json(req): Json<CreateEmoji>,
) -> AppResult<Json<Emoji>> {
    let gid = parse_i64(&gid)?;
    let uid = user.id.as_i64();
    pg::require_expression_create(&st.pool, gid, uid).await?;

    let name = req.name.trim().to_string();
    validate_name(&name)?;

    if req.image_id.is_empty() {
        return Err(AppError::bad_request("image_id ne peut pas être vide"));
    }

    let id = st.ids.next();
    let animated = req.animated as i64;
    let now = now_ms();

    sqlx::query(
        "INSERT INTO emojis (id, guild_id, name, animated, image_id, created_by, available, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, 1, ?)",
    )
    .bind(id.as_i64())
    .bind(gid)
    .bind(&name)
    .bind(animated)
    .bind(&req.image_id)
    .bind(uid)
    .bind(now)
    .execute(&st.pool)
    .await?;

    Ok(Json(Emoji {
        id,
        guild_id: Snowflake::from_i64(gid),
        name,
        animated: req.animated,
        image_id: req.image_id,
        created_by: Snowflake::from_i64(uid),
        available: true,
    }))
}

/// `PATCH /guilds/:guild_id/emojis/:emoji_id` — Modifie le nom d'un emoji.
///
/// Exige `MANAGE_GUILD_EXPRESSIONS`, ou `CREATE_GUILD_EXPRESSIONS` si l'auteur
/// est l'utilisateur courant.
pub async fn update_emoji(
    State(st): State<AppState>,
    user: AuthUser,
    Path((gid, eid)): Path<(String, String)>,
    Json(req): Json<UpdateEmoji>,
) -> AppResult<Json<Emoji>> {
    let gid = parse_i64(&gid)?;
    let eid = parse_i64(&eid)?;
    let uid = user.id.as_i64();

    let row = fetch_emoji(&st, gid, eid).await?;
    let created_by: i64 = row.get("created_by");
    pg::require_expression_manage(&st.pool, gid, uid, created_by).await?;

    if let Some(new_name) = &req.name {
        let name = new_name.trim().to_string();
        validate_name(&name)?;
        sqlx::query("UPDATE emojis SET name = ? WHERE id = ? AND guild_id = ?")
            .bind(&name)
            .bind(eid)
            .bind(gid)
            .execute(&st.pool)
            .await?;
    }

    Ok(Json(row_to_emoji(fetch_emoji(&st, gid, eid).await?)))
}

/// `DELETE /guilds/:guild_id/emojis/:emoji_id` — Supprime un emoji.
///
/// Exige `MANAGE_GUILD_EXPRESSIONS`, ou `CREATE_GUILD_EXPRESSIONS` si l'auteur
/// est l'utilisateur courant.
pub async fn delete_emoji(
    State(st): State<AppState>,
    user: AuthUser,
    Path((gid, eid)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let gid = parse_i64(&gid)?;
    let eid = parse_i64(&eid)?;
    let uid = user.id.as_i64();

    let row = fetch_emoji(&st, gid, eid).await?;
    let created_by: i64 = row.get("created_by");
    pg::require_expression_manage(&st.pool, gid, uid, created_by).await?;

    sqlx::query("DELETE FROM emojis WHERE id = ? AND guild_id = ?")
        .bind(eid)
        .bind(gid)
        .execute(&st.pool)
        .await?;

    Ok(Json(serde_json::json!({ "ok": true })))
}
