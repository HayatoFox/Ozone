//! Sons de la soundboard d'une guilde (liste, création, modification, suppression).
//! Cf. `docs/features/11-expressions.md`.

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::permissions as pg;
use crate::state::{AppState, EventScope};
use crate::util::parse_i64;
use axum::extract::{Path, State};
use axum::Json;
use ozone_proto::dto::{CreateSound, SoundboardSound, UpdateSound};
use ozone_proto::{perms, Snowflake};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

// ───────────────────────── Helpers privés ─────────────────────────

/// Convertit une ligne SQLite en DTO [`SoundboardSound`].
fn row_to_sound(r: SqliteRow) -> SoundboardSound {
    SoundboardSound {
        id: Snowflake::from_i64(r.get::<i64, _>("id")),
        guild_id: Snowflake::from_i64(r.get::<i64, _>("guild_id")),
        name: r.get("name"),
        sound_id: r.get("sound_id"),
        volume: r.get::<f64, _>("volume"),
        emoji: r.get("emoji"),
        created_by: Snowflake::from_i64(r.get::<i64, _>("created_by")),
        available: r.get::<i64, _>("available") != 0,
    }
}

/// Charge un son par `(guild_id, sound_id)` ; renvoie 404 s'il est absent.
async fn fetch_sound(st: &AppState, gid: i64, sid: i64) -> AppResult<SqliteRow> {
    sqlx::query(
        "SELECT id, guild_id, name, sound_id, volume, emoji, created_by, available \
         FROM soundboard_sounds WHERE id = ? AND guild_id = ?",
    )
    .bind(sid)
    .bind(gid)
    .fetch_optional(&st.pool)
    .await?
    .ok_or_else(|| AppError::not_found("son introuvable"))
}

// ───────────────────────── Handlers publics ─────────────────────────

/// `GET /guilds/:guild_id/soundboard-sounds` — Liste tous les sons de la soundboard d'une guilde.
///
/// Exige d'être membre (`VIEW_CHANNEL`).
pub async fn list_sounds(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
) -> AppResult<Json<Vec<SoundboardSound>>> {
    let gid = parse_i64(&gid)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::VIEW_CHANNEL).await?;
    let rows = sqlx::query(
        "SELECT id, guild_id, name, sound_id, volume, emoji, created_by, available \
         FROM soundboard_sounds WHERE guild_id = ? ORDER BY id",
    )
    .bind(gid)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(rows.into_iter().map(row_to_sound).collect()))
}

/// `POST /guilds/:guild_id/soundboard-sounds` — Ajoute un son à la soundboard de la guilde.
///
/// Exige `CREATE_GUILD_EXPRESSIONS` ou `MANAGE_GUILD_EXPRESSIONS`.
/// Le nom est normalisé (trim) et validé (2–32 car.), `sound_id` ne peut pas être vide,
/// l'emoji (si fourni) est limité à 64 caractères, le volume est borné à `[0.0, 1.0]`.
pub async fn create_sound(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
    Json(req): Json<CreateSound>,
) -> AppResult<Json<SoundboardSound>> {
    let gid = parse_i64(&gid)?;
    let uid = user.id.as_i64();
    pg::require_expression_create(&st.pool, gid, uid).await?;

    let name = req.name.trim().to_string();
    let name_len = name.len();
    if !(2..=32).contains(&name_len) {
        return Err(AppError::bad_request(
            "le nom du son doit faire entre 2 et 32 caractères",
        ));
    }

    if req.sound_id.is_empty() {
        return Err(AppError::bad_request("sound_id ne peut pas être vide"));
    }

    if let Some(ref emoji) = req.emoji {
        if emoji.len() > 64 {
            return Err(AppError::bad_request(
                "l'emoji ne peut pas dépasser 64 caractères",
            ));
        }
    }

    let volume = req.volume.clamp(0.0, 1.0);
    let id = st.ids.next();
    let now = now_ms();

    sqlx::query(
        "INSERT INTO soundboard_sounds \
         (id, guild_id, name, sound_id, volume, emoji, created_by, available, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?)",
    )
    .bind(id.as_i64())
    .bind(gid)
    .bind(&name)
    .bind(&req.sound_id)
    .bind(volume)
    .bind(&req.emoji)
    .bind(uid)
    .bind(now)
    .execute(&st.pool)
    .await?;

    st.publish(
        EventScope::Guild(gid),
        "GUILD_SOUNDBOARD_UPDATE",
        serde_json::json!({ "guild_id": gid.to_string() }),
    );

    Ok(Json(SoundboardSound {
        id,
        guild_id: Snowflake::from_i64(gid),
        name,
        sound_id: req.sound_id,
        volume,
        emoji: req.emoji,
        created_by: Snowflake::from_i64(uid),
        available: true,
    }))
}

/// `PATCH /guilds/:guild_id/soundboard-sounds/:sound_id` — Modifie un son de la soundboard.
///
/// Exige `MANAGE_GUILD_EXPRESSIONS`, ou `CREATE_GUILD_EXPRESSIONS` si l'auteur
/// est l'utilisateur courant. Les champs absents conservent leur valeur existante.
pub async fn update_sound(
    State(st): State<AppState>,
    user: AuthUser,
    Path((gid, sid)): Path<(String, String)>,
    Json(req): Json<UpdateSound>,
) -> AppResult<Json<SoundboardSound>> {
    let gid = parse_i64(&gid)?;
    let sid = parse_i64(&sid)?;
    let uid = user.id.as_i64();

    let row = fetch_sound(&st, gid, sid).await?;
    let created_by: i64 = row.get("created_by");
    pg::require_expression_manage(&st.pool, gid, uid, created_by).await?;

    // Calcule les nouvelles valeurs (ou conserve les existantes).
    let name = if let Some(new_name) = req.name {
        let trimmed = new_name.trim().to_string();
        let len = trimmed.len();
        if !(2..=32).contains(&len) {
            return Err(AppError::bad_request(
                "le nom du son doit faire entre 2 et 32 caractères",
            ));
        }
        trimmed
    } else {
        row.get("name")
    };

    let volume = if let Some(v) = req.volume {
        v.clamp(0.0, 1.0)
    } else {
        row.get::<f64, _>("volume")
    };

    // `emoji` : si le champ est présent dans la requête on l'applique (y compris `null`
    // pour effacer l'emoji) ; s'il est absent on conserve la valeur existante.
    let emoji: Option<String> = if req.emoji.is_some() {
        if let Some(ref e) = req.emoji {
            if e.len() > 64 {
                return Err(AppError::bad_request(
                    "l'emoji ne peut pas dépasser 64 caractères",
                ));
            }
        }
        req.emoji
    } else {
        row.get("emoji")
    };

    sqlx::query(
        "UPDATE soundboard_sounds SET name = ?, volume = ?, emoji = ? \
         WHERE id = ? AND guild_id = ?",
    )
    .bind(&name)
    .bind(volume)
    .bind(&emoji)
    .bind(sid)
    .bind(gid)
    .execute(&st.pool)
    .await?;

    st.publish(
        EventScope::Guild(gid),
        "GUILD_SOUNDBOARD_UPDATE",
        serde_json::json!({ "guild_id": gid.to_string() }),
    );

    Ok(Json(row_to_sound(fetch_sound(&st, gid, sid).await?)))
}

/// `DELETE /guilds/:guild_id/soundboard-sounds/:sound_id` — Supprime un son de la soundboard.
///
/// Exige `MANAGE_GUILD_EXPRESSIONS`, ou `CREATE_GUILD_EXPRESSIONS` si l'auteur
/// est l'utilisateur courant.
pub async fn delete_sound(
    State(st): State<AppState>,
    user: AuthUser,
    Path((gid, sid)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let gid = parse_i64(&gid)?;
    let sid = parse_i64(&sid)?;
    let uid = user.id.as_i64();

    let row = fetch_sound(&st, gid, sid).await?;
    let created_by: i64 = row.get("created_by");
    pg::require_expression_manage(&st.pool, gid, uid, created_by).await?;

    sqlx::query("DELETE FROM soundboard_sounds WHERE id = ? AND guild_id = ?")
        .bind(sid)
        .bind(gid)
        .execute(&st.pool)
        .await?;

    st.publish(
        EventScope::Guild(gid),
        "GUILD_SOUNDBOARD_UPDATE",
        serde_json::json!({ "guild_id": gid.to_string() }),
    );

    Ok(Json(serde_json::json!({ "ok": true })))
}

// ───────────────────────── Handlers publics exposés ─────────────────────────
// list_sounds, create_sound, update_sound, delete_sound
