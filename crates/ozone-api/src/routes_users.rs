//! Profil public (lecture/édition de son propre profil) et réglages client (blob JSON privé).
//! Cf. docs/features/08-profil.md, 15-parametres-utilisateur.md.

use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::state::AppState;
use crate::util::parse_i64;
use axum::extract::{Path, State};
use axum::Json;
use ozone_proto::dto::{UpdateProfile, UserProfile, UserSettings};
use ozone_proto::Snowflake;
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

const PROFILE_SELECT: &str = "SELECT id, username, display_name, avatar_id, bio, pronouns, banner_id, accent_color, created_at FROM users";
const MAX_SETTINGS_BYTES: usize = 64 * 1024;

fn row_to_profile(r: SqliteRow) -> UserProfile {
    UserProfile {
        id: Snowflake::from_i64(r.get::<i64, _>("id")),
        username: r.get("username"),
        display_name: r.get("display_name"),
        avatar_id: r.get("avatar_id"),
        bio: r.get("bio"),
        pronouns: r.get("pronouns"),
        banner_id: r.get("banner_id"),
        accent_color: r.get::<Option<i64>, _>("accent_color").map(|c| c as u32),
        created_at: r.get::<i64, _>("created_at") as u64,
    }
}

async fn fetch_profile(st: &AppState, uid: i64) -> AppResult<UserProfile> {
    let row = sqlx::query(&format!("{PROFILE_SELECT} WHERE id = ?"))
        .bind(uid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("utilisateur introuvable"))?;
    Ok(row_to_profile(row))
}

/// Champ texte optionnel : `None` = inchangé, `Some("")` = effacé, sinon validé/tronqué.
fn text_field(
    new: &Option<String>,
    current: Option<String>,
    max: usize,
    label: &str,
) -> AppResult<Option<String>> {
    match new {
        None => Ok(current),
        Some(s) => {
            let t = s.trim();
            if t.is_empty() {
                return Ok(None);
            }
            if t.chars().count() > max {
                return Err(AppError::bad_request(format!(
                    "{label} trop long (max {max})"
                )));
            }
            Ok(Some(t.to_string()))
        }
    }
}

/// `PATCH /users/@me` — édite son propre profil.
pub async fn update_profile(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<UpdateProfile>,
) -> AppResult<Json<UserProfile>> {
    let uid = user.id.as_i64();
    let cur = fetch_profile(&st, uid).await?;

    let display_name = text_field(&req.display_name, cur.display_name, 32, "nom affiché")?;
    let bio = text_field(&req.bio, cur.bio, 190, "bio")?;
    let pronouns = text_field(&req.pronouns, cur.pronouns, 40, "pronoms")?;
    let avatar_id = text_field(&req.avatar_id, cur.avatar_id, 256, "avatar")?;
    let banner_id = text_field(&req.banner_id, cur.banner_id, 256, "bannière")?;
    let accent_color = match req.accent_color {
        Some(c) => {
            if c > 0xFF_FFFF {
                return Err(AppError::bad_request(
                    "couleur d'accent invalide (0..=0xFFFFFF)",
                ));
            }
            Some(c as i64)
        }
        None => cur.accent_color.map(|c| c as i64),
    };

    sqlx::query(
        "UPDATE users SET display_name = ?, avatar_id = ?, bio = ?, pronouns = ?, banner_id = ?, accent_color = ? WHERE id = ?",
    )
    .bind(display_name.as_deref())
    .bind(avatar_id.as_deref())
    .bind(bio.as_deref())
    .bind(pronouns.as_deref())
    .bind(banner_id.as_deref())
    .bind(accent_color)
    .bind(uid)
    .execute(&st.pool)
    .await?;

    Ok(Json(fetch_profile(&st, uid).await?))
}

/// `GET /users/:user_id/profile` — profil public d'un utilisateur (sans e-mail).
pub async fn get_profile(
    State(st): State<AppState>,
    _user: AuthUser,
    Path(target): Path<String>,
) -> AppResult<Json<UserProfile>> {
    let target = parse_i64(&target)?;
    Ok(Json(fetch_profile(&st, target).await?))
}

// ───────────────────────────── Réglages client ─────────────────────────────

/// `GET /users/@me/settings`
pub async fn get_settings(
    State(st): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<UserSettings>> {
    let row = sqlx::query("SELECT data FROM user_settings WHERE user_id = ?")
        .bind(user.id.as_i64())
        .fetch_optional(&st.pool)
        .await?;
    let text: String = row
        .map(|r| r.get::<String, _>("data"))
        .unwrap_or_else(|| "{}".into());
    let data = serde_json::from_str(&text).unwrap_or(serde_json::Value::Object(Default::default()));
    Ok(Json(UserSettings { data }))
}

/// `PUT /users/@me/settings` — remplace le blob de réglages client.
pub async fn put_settings(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<UserSettings>,
) -> AppResult<Json<UserSettings>> {
    if !req.data.is_object() {
        return Err(AppError::bad_request(
            "les réglages doivent être un objet JSON",
        ));
    }
    let text = serde_json::to_string(&req.data).unwrap_or_else(|_| "{}".into());
    if text.len() > MAX_SETTINGS_BYTES {
        return Err(AppError::bad_request(
            "réglages trop volumineux (max 64 Ko)",
        ));
    }
    sqlx::query(
        "INSERT INTO user_settings (user_id, data) VALUES (?, ?) \
         ON CONFLICT(user_id) DO UPDATE SET data = excluded.data",
    )
    .bind(user.id.as_i64())
    .bind(&text)
    .execute(&st.pool)
    .await?;
    Ok(Json(req))
}
