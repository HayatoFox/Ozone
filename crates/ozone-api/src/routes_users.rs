//! Profil public (lecture/édition de son propre profil) et réglages client (blob JSON privé).
//! Cf. docs/features/08-profil.md, 15-parametres-utilisateur.md.

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::state::AppState;
use crate::util::parse_i64;
use axum::extract::{Path, State};
use axum::Json;
use ozone_proto::dto::{
    EncryptionKeys, PublicKey, SetPublicKey, UpdateProfile, UserProfile, UserSettings,
};
use ozone_proto::Snowflake;
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const PROFILE_SELECT: &str = "SELECT id, username, display_name, avatar_id, bio, pronouns, banner_id, accent_color, created_at FROM users";
const MAX_SETTINGS_BYTES: usize = 64 * 1024;

// Anti-spam du profil : chaque PATCH déclenche un USER_UPDATE en fan-out (amis, MP, guildes
// partagées) → **10 par fenêtre glissante de 10 min et par utilisateur** (même posture que
// l'icône de guilde, §88/§95).
static PROFILE_UPDATE_HITS: OnceLock<Mutex<HashMap<i64, Vec<i64>>>> = OnceLock::new();
fn profile_update_allowed(uid: i64) -> bool {
    let now = now_ms();
    let lock = PROFILE_UPDATE_HITS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = lock.lock().unwrap_or_else(|e| e.into_inner());
    let hits = map.entry(uid).or_default();
    hits.retain(|t| now - *t < 600_000);
    if hits.len() >= 10 {
        return false;
    }
    hits.push(now);
    true
}

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
    if !profile_update_allowed(uid) {
        return Err(AppError::too_many(
            "trop de modifications du profil — réessaie dans quelques minutes",
        ));
    }
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

    // Propage le nouveau profil public (pseudo/avatar) EN DIRECT : soi, amis, MP, guildes partagées.
    crate::gateway::broadcast_user_update(&st, uid).await;

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

/// `GET /users/:user_id/mutual` — guildes & amis **en commun** entre l'appelant et la cible.
/// Sert le panneau de profil des MP (« Serveurs en commun », « Amis en commun »).
pub async fn get_mutual(
    State(st): State<AppState>,
    user: AuthUser,
    Path(target): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let me = user.id.as_i64();
    let target = parse_i64(&target)?;

    let guilds = sqlx::query(
        "SELECT g.id, g.name, g.icon_id FROM guilds g \
         JOIN guild_members m1 ON m1.guild_id = g.id AND m1.user_id = ? \
         JOIN guild_members m2 ON m2.guild_id = g.id AND m2.user_id = ?",
    )
    .bind(me)
    .bind(target)
    .fetch_all(&st.pool)
    .await?;
    let guilds: Vec<serde_json::Value> = guilds
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<i64, _>("id").to_string(),
                "name": r.get::<String, _>("name"),
                "icon_id": r.get::<Option<String>, _>("icon_id"),
            })
        })
        .collect();

    // Amis communs : utilisateurs amis (`type='friend'`) à la fois de l'appelant ET de la cible.
    let friends = sqlx::query(
        "SELECT u.id, u.username, u.display_name, u.avatar_id \
         FROM relationships r1 \
         JOIN relationships r2 ON r1.target_id = r2.target_id \
         JOIN users u ON u.id = r1.target_id \
         WHERE r1.user_id = ? AND r1.type = 'friend' \
           AND r2.user_id = ? AND r2.type = 'friend'",
    )
    .bind(me)
    .bind(target)
    .fetch_all(&st.pool)
    .await?;
    let friends: Vec<serde_json::Value> = friends
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<i64, _>("id").to_string(),
                "username": r.get::<String, _>("username"),
                "display_name": r.get::<Option<String>, _>("display_name"),
                "avatar_id": r.get::<Option<String>, _>("avatar_id"),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "guilds": guilds, "friends": friends })))
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

// ─────────────────────── Clés de chiffrement DM (E2EE) ───────────────────────
// La clé PRIVÉE ne quitte jamais le client. Le serveur ne stocke/expose que la clé PUBLIQUE
// (P-256 ECDH, SPKI base64) afin que chaque participant puisse dériver le secret partagé.

/// Taille max de la clé publique SPKI base64 (P-256 ≈ 120 octets bruts → ~160 en base64 ; marge large).
const MAX_PUBLIC_KEY_LEN: usize = 1024;

/// `PUT /users/@me/keys` — publie SA clé publique de chiffrement DM.
pub async fn put_public_key(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<SetPublicKey>,
) -> AppResult<Json<PublicKey>> {
    let key = req.public_key.trim();
    if key.is_empty() || key.len() > MAX_PUBLIC_KEY_LEN {
        return Err(AppError::bad_request("clé publique invalide"));
    }
    sqlx::query("UPDATE users SET dm_public_key = ? WHERE id = ?")
        .bind(key)
        .bind(user.id.as_i64())
        .execute(&st.pool)
        .await?;
    Ok(Json(PublicKey {
        public_key: Some(key.to_string()),
    }))
}

/// `GET /users/@me/encryption` — matériel de chiffrement DM de l'utilisateur courant : clé publique
/// + clé privée EMBALLÉE (escrow). Le client la déballe localement avec la KEK dérivée du mot de passe.
pub async fn get_encryption(
    State(st): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<EncryptionKeys>> {
    let row = sqlx::query("SELECT dm_public_key, dm_priv_wrapped, pw_scheme FROM users WHERE id = ?")
        .bind(user.id.as_i64())
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("utilisateur introuvable"))?;
    Ok(Json(EncryptionKeys {
        public_key: row.get("dm_public_key"),
        priv_wrapped: row.get("dm_priv_wrapped"),
        pw_scheme: row.get::<i64, _>("pw_scheme") as u8,
    }))
}

/// `GET /users/:user_id/keys` — clé publique de chiffrement DM d'un utilisateur (peut être absente).
pub async fn get_public_key(
    State(st): State<AppState>,
    _user: AuthUser,
    Path(target): Path<String>,
) -> AppResult<Json<PublicKey>> {
    let target = parse_i64(&target)?;
    let public_key: Option<String> = sqlx::query("SELECT dm_public_key FROM users WHERE id = ?")
        .bind(target)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("utilisateur introuvable"))?
        .get("dm_public_key");
    Ok(Json(PublicKey { public_key }))
}
