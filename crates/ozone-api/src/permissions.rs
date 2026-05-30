//! Calcul des permissions effectives (base de guilde → overwrites de salon).
//! Cf. `docs/features/10-roles-permissions.md` (algorithme).

use crate::error::{AppError, AppResult};
use ozone_proto::perms;
use sqlx::{Row, SqlitePool};

/// Propriétaire d'une guilde (`None` si la guilde n'existe pas).
pub async fn guild_owner(pool: &SqlitePool, guild_id: i64) -> AppResult<Option<i64>> {
    let row = sqlx::query("SELECT owner_id FROM guilds WHERE id = ?")
        .bind(guild_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.get::<i64, _>("owner_id")))
}

/// `guild_id` du salon (`None` si MP, erreur 404 gérée par l'appelant si le salon n'existe pas).
pub async fn channel_guild(pool: &SqlitePool, channel_id: i64) -> AppResult<Option<Option<i64>>> {
    let row = sqlx::query("SELECT guild_id FROM channels WHERE id = ?")
        .bind(channel_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.get::<Option<i64>, _>("guild_id")))
}

/// Identifiants des rôles d'un membre (hors `@everyone`).
pub async fn member_role_ids(
    pool: &SqlitePool,
    guild_id: i64,
    user_id: i64,
) -> AppResult<Vec<i64>> {
    let rows = sqlx::query("SELECT role_id FROM member_roles WHERE guild_id = ? AND user_id = ?")
        .bind(guild_id)
        .bind(user_id)
        .fetch_all(pool)
        .await?;
    Ok(rows.iter().map(|r| r.get::<i64, _>("role_id")).collect())
}

async fn everyone_permissions(pool: &SqlitePool, guild_id: i64) -> AppResult<u64> {
    let row = sqlx::query("SELECT permissions FROM roles WHERE id = ?")
        .bind(guild_id)
        .fetch_optional(pool)
        .await?;
    Ok(row
        .map(|r| r.get::<i64, _>("permissions") as u64)
        .unwrap_or(0))
}

/// Permissions de base au niveau de la guilde (propriétaire / ADMINISTRATOR → toutes).
pub async fn guild_permissions(
    pool: &SqlitePool,
    guild_id: i64,
    owner_id: i64,
    user_id: i64,
) -> AppResult<u64> {
    if owner_id == user_id {
        return Ok(perms::ALL);
    }
    // Un non-membre n'a aucune permission (le rôle @everyone ne s'applique qu'aux membres).
    let is_member = sqlx::query("SELECT 1 FROM guild_members WHERE guild_id = ? AND user_id = ?")
        .bind(guild_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .is_some();
    if !is_member {
        return Ok(0);
    }
    let mut acc = everyone_permissions(pool, guild_id).await?;
    let role_ids = member_role_ids(pool, guild_id, user_id).await?;
    if !role_ids.is_empty() {
        let rows = sqlx::query("SELECT id, permissions FROM roles WHERE guild_id = ?")
            .bind(guild_id)
            .fetch_all(pool)
            .await?;
        for r in rows {
            if role_ids.contains(&r.get::<i64, _>("id")) {
                acc |= r.get::<i64, _>("permissions") as u64;
            }
        }
    }
    if acc & perms::ADMINISTRATOR != 0 {
        return Ok(perms::ALL);
    }
    Ok(acc)
}

/// Permissions effectives dans un salon (applique les surcharges).
/// Si `VIEW_CHANNEL` n'est pas accordé, renvoie `0` (le salon est invisible).
pub async fn channel_permissions(
    pool: &SqlitePool,
    guild_id: i64,
    owner_id: i64,
    channel_id: i64,
    user_id: i64,
) -> AppResult<u64> {
    let base = guild_permissions(pool, guild_id, owner_id, user_id).await?;
    if base == perms::ALL {
        return Ok(perms::ALL);
    }
    let role_ids = member_role_ids(pool, guild_id, user_id).await?;
    let ows = sqlx::query(
        "SELECT target_id, target_type, allow, deny FROM channel_overwrites WHERE channel_id = ?",
    )
    .bind(channel_id)
    .fetch_all(pool)
    .await?;

    let mut perms_acc = base;

    // 1) surcharge @everyone (target = guild_id, type rôle)
    for r in &ows {
        if r.get::<i64, _>("target_type") == 0 && r.get::<i64, _>("target_id") == guild_id {
            perms_acc =
                (perms_acc & !(r.get::<i64, _>("deny") as u64)) | (r.get::<i64, _>("allow") as u64);
        }
    }
    // 2) surcharges de rôles (cumulées)
    let mut allow = 0u64;
    let mut deny = 0u64;
    for r in &ows {
        if r.get::<i64, _>("target_type") == 0 {
            let tid: i64 = r.get("target_id");
            if tid != guild_id && role_ids.contains(&tid) {
                allow |= r.get::<i64, _>("allow") as u64;
                deny |= r.get::<i64, _>("deny") as u64;
            }
        }
    }
    perms_acc = (perms_acc & !deny) | allow;
    // 3) surcharge membre
    for r in &ows {
        if r.get::<i64, _>("target_type") == 1 && r.get::<i64, _>("target_id") == user_id {
            perms_acc =
                (perms_acc & !(r.get::<i64, _>("deny") as u64)) | (r.get::<i64, _>("allow") as u64);
        }
    }

    if perms_acc & perms::VIEW_CHANNEL == 0 {
        return Ok(0);
    }
    Ok(perms_acc)
}

/// Position du rôle le plus haut d'un membre (propriétaire → `i32::MAX`).
pub async fn highest_role_position(
    pool: &SqlitePool,
    guild_id: i64,
    owner_id: i64,
    user_id: i64,
) -> AppResult<i32> {
    if owner_id == user_id {
        return Ok(i32::MAX);
    }
    let role_ids = member_role_ids(pool, guild_id, user_id).await?;
    if role_ids.is_empty() {
        return Ok(0);
    }
    let rows = sqlx::query("SELECT id, position FROM roles WHERE guild_id = ?")
        .bind(guild_id)
        .fetch_all(pool)
        .await?;
    let mut max = 0i32;
    for r in rows {
        if role_ids.contains(&r.get::<i64, _>("id")) {
            max = max.max(r.get::<i64, _>("position") as i32);
        }
    }
    Ok(max)
}

/// Exige une permission au niveau **guilde** ; renvoie les permissions calculées.
pub async fn require_guild_perm(
    pool: &SqlitePool,
    guild_id: i64,
    user_id: i64,
    needed: u64,
) -> AppResult<u64> {
    let owner = guild_owner(pool, guild_id)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?;
    let p = guild_permissions(pool, guild_id, owner, user_id).await?;
    if !perms::has(p, needed) {
        return Err(AppError::forbidden("permissions insuffisantes"));
    }
    Ok(p)
}

/// Permissions accordées dans un MP / groupe (aucune gestion de type serveur).
pub const DM_PERMS: u64 = perms::VIEW_CHANNEL
    | perms::SEND_MESSAGES
    | perms::READ_MESSAGE_HISTORY
    | perms::ADD_REACTIONS
    | perms::EMBED_LINKS
    | perms::ATTACH_FILES
    | perms::USE_EXTERNAL_EMOJIS
    | perms::USE_EXTERNAL_STICKERS
    | perms::MENTION_EVERYONE
    | perms::PIN_MESSAGES
    | perms::SEND_VOICE_MESSAGES;

/// Permissions effectives dans un salon de MP / groupe. `0` si l'utilisateur n'est pas
/// destinataire. En MP 1:1, si l'un a bloqué l'autre, l'envoi est retiré (lecture seule).
pub async fn dm_permissions(pool: &SqlitePool, channel_id: i64, user_id: i64) -> AppResult<u64> {
    let is_recipient =
        sqlx::query("SELECT 1 FROM dm_recipients WHERE channel_id = ? AND user_id = ?")
            .bind(channel_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await?
            .is_some();
    if !is_recipient {
        return Ok(0);
    }
    let mut p = DM_PERMS;
    let kind: i64 = sqlx::query("SELECT type FROM channels WHERE id = ?")
        .bind(channel_id)
        .fetch_one(pool)
        .await?
        .get("type");
    if kind == 1 {
        // MP 1:1 : retire l'envoi en cas de blocage dans un sens ou l'autre.
        let other: Option<i64> = sqlx::query(
            "SELECT user_id FROM dm_recipients WHERE channel_id = ? AND user_id <> ? LIMIT 1",
        )
        .bind(channel_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .map(|r| r.get("user_id"));
        if let Some(other) = other {
            let blocked = sqlx::query(
                "SELECT 1 FROM relationships WHERE type = 'blocked' AND \
                 ((user_id = ? AND target_id = ?) OR (user_id = ? AND target_id = ?))",
            )
            .bind(user_id)
            .bind(other)
            .bind(other)
            .bind(user_id)
            .fetch_optional(pool)
            .await?
            .is_some();
            if blocked {
                p &= !(perms::SEND_MESSAGES | perms::SEND_VOICE_MESSAGES | perms::ADD_REACTIONS);
            }
        }
    }
    Ok(p)
}

/// Exige une permission au niveau **salon** (guilde ou MP) ; renvoie `(guild_id, owner_id, permissions)`.
/// Pour un MP / groupe, `guild_id` et `owner_id` valent `0`.
pub async fn require_channel_perm(
    pool: &SqlitePool,
    channel_id: i64,
    user_id: i64,
    needed: u64,
) -> AppResult<(i64, i64, u64)> {
    let maybe_guild = channel_guild(pool, channel_id)
        .await?
        .ok_or_else(|| AppError::not_found("salon introuvable"))?;
    match maybe_guild {
        Some(guild_id) => {
            let owner = guild_owner(pool, guild_id)
                .await?
                .ok_or_else(|| AppError::not_found("guilde introuvable"))?;
            let p = channel_permissions(pool, guild_id, owner, channel_id, user_id).await?;
            if !perms::has(p, needed) {
                return Err(AppError::forbidden("permissions insuffisantes"));
            }
            Ok((guild_id, owner, p))
        }
        None => {
            let p = dm_permissions(pool, channel_id, user_id).await?;
            if !perms::has(p, needed) {
                return Err(AppError::forbidden("accès refusé à ce message privé"));
            }
            Ok((0, 0, p))
        }
    }
}
