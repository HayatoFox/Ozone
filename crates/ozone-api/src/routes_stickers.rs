//! Stickers de guilde : liste, création, mise à jour, suppression.
//! Cf. `docs/features/` — expressions (emojis / stickers / sons).

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::permissions as pg;
use crate::state::AppState;
use crate::util::parse_i64;
use axum::extract::{Path, State};
use axum::Json;
use ozone_proto::dto::{CreateSticker, Sticker, UpdateSticker};
use ozone_proto::{perms, Snowflake};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

// ─────────────────────────── Helpers internes ───────────────────────────

/// Convertit une ligne SQLite en DTO [`Sticker`].
fn row_to_sticker(r: SqliteRow) -> Sticker {
    Sticker {
        id: Snowflake::from_i64(r.get::<i64, _>("id")),
        guild_id: Snowflake::from_i64(r.get::<i64, _>("guild_id")),
        name: r.get("name"),
        description: r.get("description"),
        tags: r.get("tags"),
        format_type: r.get::<i64, _>("format_type") as u8,
        asset_id: r.get("asset_id"),
        created_by: Snowflake::from_i64(r.get::<i64, _>("created_by")),
        available: r.get::<i64, _>("available") != 0,
    }
}

/// Récupère un sticker par `id` **et** `guild_id` ; renvoie 404 s'il n'existe pas.
async fn fetch_sticker(st: &AppState, gid: i64, sid: i64) -> AppResult<SqliteRow> {
    sqlx::query("SELECT * FROM stickers WHERE id = ? AND guild_id = ?")
        .bind(sid)
        .bind(gid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("sticker introuvable"))
}

// ─────────────────────────── Handlers publics ───────────────────────────

/// `GET /guilds/:guild_id/stickers` — liste tous les stickers de la guilde.
///
/// Exige `VIEW_CHANNEL` sur la guilde (membre visible).
pub async fn list_stickers(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
) -> AppResult<Json<Vec<Sticker>>> {
    let gid = parse_i64(&gid)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::VIEW_CHANNEL).await?;
    let rows = sqlx::query("SELECT * FROM stickers WHERE guild_id = ? ORDER BY id")
        .bind(gid)
        .fetch_all(&st.pool)
        .await?;
    Ok(Json(rows.into_iter().map(row_to_sticker).collect()))
}

/// `POST /guilds/:guild_id/stickers` — crée un sticker dans la guilde.
///
/// Exige `CREATE_GUILD_EXPRESSIONS` ou `MANAGE_GUILD_EXPRESSIONS`.
/// Valide : `name` 2–30 caractères (après trim), `description` ≤ 100,
/// `tags` ≤ 200, `asset_id` non vide, `format_type` ∈ 1..=4.
pub async fn create_sticker(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
    Json(req): Json<CreateSticker>,
) -> AppResult<Json<Sticker>> {
    let gid = parse_i64(&gid)?;
    let uid = user.id.as_i64();
    pg::require_expression_create(&st.pool, gid, uid).await?;

    // --- Validations ---
    let name = req.name.trim().to_owned();
    if !(2..=30).contains(&name.len()) {
        return Err(AppError::bad_request(
            "le nom du sticker doit comporter entre 2 et 30 caractères",
        ));
    }
    if let Some(ref d) = req.description {
        if d.len() > 100 {
            return Err(AppError::bad_request(
                "la description ne peut pas dépasser 100 caractères",
            ));
        }
    }
    if let Some(ref t) = req.tags {
        if t.len() > 200 {
            return Err(AppError::bad_request(
                "les tags ne peuvent pas dépasser 200 caractères",
            ));
        }
    }
    if req.asset_id.trim().is_empty() {
        return Err(AppError::bad_request(
            "l'identifiant d'asset ne peut pas être vide",
        ));
    }
    if !(1..=4).contains(&req.format_type) {
        return Err(AppError::bad_request(
            "format_type invalide (valeurs acceptées : 1 png, 2 apng, 3 lottie, 4 gif)",
        ));
    }

    let id = st.ids.next();
    let now = now_ms();

    sqlx::query(
        "INSERT INTO stickers \
         (id, guild_id, name, description, tags, format_type, asset_id, created_by, available, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?)",
    )
    .bind(id.as_i64())
    .bind(gid)
    .bind(&name)
    .bind(&req.description)
    .bind(&req.tags)
    .bind(req.format_type as i64)
    .bind(&req.asset_id)
    .bind(uid)
    .bind(now)
    .execute(&st.pool)
    .await?;

    Ok(Json(Sticker {
        id,
        guild_id: Snowflake::from_i64(gid),
        name,
        description: req.description,
        tags: req.tags,
        format_type: req.format_type,
        asset_id: req.asset_id,
        created_by: Snowflake::from_i64(uid),
        available: true,
    }))
}

/// `PATCH /guilds/:guild_id/stickers/:sticker_id` — met à jour un sticker.
///
/// Seuls les champs `name`, `description` et `tags` sont modifiables.
/// Exige `MANAGE_GUILD_EXPRESSIONS`, ou `CREATE_GUILD_EXPRESSIONS` si l'appelant
/// est l'auteur du sticker.
pub async fn update_sticker(
    State(st): State<AppState>,
    user: AuthUser,
    Path((gid, sid)): Path<(String, String)>,
    Json(req): Json<UpdateSticker>,
) -> AppResult<Json<Sticker>> {
    let gid = parse_i64(&gid)?;
    let sid = parse_i64(&sid)?;
    let uid = user.id.as_i64();

    let row = fetch_sticker(&st, gid, sid).await?;
    let created_by: i64 = row.get("created_by");
    pg::require_expression_manage(&st.pool, gid, uid, created_by).await?;

    // --- Validations des champs fournis ---
    if let Some(ref n) = req.name {
        let n = n.trim();
        if !(2..=30).contains(&n.len()) {
            return Err(AppError::bad_request(
                "le nom du sticker doit comporter entre 2 et 30 caractères",
            ));
        }
    }
    if let Some(ref d) = req.description {
        if d.len() > 100 {
            return Err(AppError::bad_request(
                "la description ne peut pas dépasser 100 caractères",
            ));
        }
    }
    if let Some(ref t) = req.tags {
        if t.len() > 200 {
            return Err(AppError::bad_request(
                "les tags ne peuvent pas dépasser 200 caractères",
            ));
        }
    }

    // --- Application (patch partiel) ---
    let name: String = req
        .name
        .as_deref()
        .map(str::trim)
        .map(str::to_owned)
        .unwrap_or_else(|| row.get("name"));
    let description: Option<String> = match req.description {
        Some(d) => Some(d),
        None => row.get("description"),
    };
    let tags: Option<String> = match req.tags {
        Some(t) => Some(t),
        None => row.get("tags"),
    };

    sqlx::query(
        "UPDATE stickers SET name = ?, description = ?, tags = ? WHERE id = ? AND guild_id = ?",
    )
    .bind(&name)
    .bind(&description)
    .bind(&tags)
    .bind(sid)
    .bind(gid)
    .execute(&st.pool)
    .await?;

    Ok(Json(row_to_sticker(fetch_sticker(&st, gid, sid).await?)))
}

/// `DELETE /guilds/:guild_id/stickers/:sticker_id` — supprime un sticker.
///
/// Exige `MANAGE_GUILD_EXPRESSIONS`, ou `CREATE_GUILD_EXPRESSIONS` si l'appelant
/// est l'auteur du sticker.
pub async fn delete_sticker(
    State(st): State<AppState>,
    user: AuthUser,
    Path((gid, sid)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let gid = parse_i64(&gid)?;
    let sid = parse_i64(&sid)?;
    let uid = user.id.as_i64();

    let row = fetch_sticker(&st, gid, sid).await?;
    let created_by: i64 = row.get("created_by");
    pg::require_expression_manage(&st.pool, gid, uid, created_by).await?;

    sqlx::query("DELETE FROM stickers WHERE id = ? AND guild_id = ?")
        .bind(sid)
        .bind(gid)
        .execute(&st.pool)
        .await?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

// ─────────────────────────── Handlers exportés ───────────────────────────
// list_stickers, create_sticker, update_sticker, delete_sticker
