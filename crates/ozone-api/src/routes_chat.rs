//! Routes guildes & salons (création, lecture, mise à jour, suppression, réordonnancement,
//! catégories, slowmode/NSFW). Applique les permissions. Cf. docs/features/03-salons.md.

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::permissions as pg;
use crate::state::{AppState, EventScope};
use crate::util::parse_i64;
use axum::extract::{Path, State};
use axum::Json;
use ozone_proto::dto::{
    Channel, ChannelPosition, CreateChannel, CreateGuild, CreateThread, Guild, UpdateChannel,
    UpdateGuild,
};
use ozone_proto::{perms, Snowflake};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const CHANNEL_SELECT: &str =
    "SELECT id, guild_id, type AS kind, name, topic, position, parent_id, nsfw, rate_limit_per_user, \
     bitrate, user_limit, rtc_region, video_quality_mode, default_auto_archive, archived, locked, \
     (SELECT MAX(id) FROM messages WHERE messages.channel_id = channels.id) AS last_message_id \
     FROM channels";
const GUILD_SELECT: &str =
    "SELECT id, name, owner_id, icon_id, description, discoverable, banner_color, banner_id, games, private_profile, system_channel_id, default_message_notifications, afk_channel_id, afk_timeout, vanity_code FROM guilds";

const MAX_GAMES: usize = 12;

/// Parse le tableau JSON de jeux stocké en base (tolérant : liste vide si illisible).
fn parse_games(raw: Option<String>) -> Vec<String> {
    raw.and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default()
}

/// Anti-spam des mises à jour de guilde : **5 par fenêtre glissante de 10 min et par guilde**.
/// L'icône/bannière se propagent à tous via `GUILD_UPDATE` ; on évite le matraquage.
static GUILD_UPDATE_HITS: OnceLock<Mutex<HashMap<i64, Vec<i64>>>> = OnceLock::new();
fn guild_update_allowed(gid: i64) -> bool {
    let now = now_ms();
    let lock = GUILD_UPDATE_HITS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = lock.lock().unwrap_or_else(|e| e.into_inner());
    let hits = map.entry(gid).or_default();
    hits.retain(|t| now - *t < 600_000); // 10 minutes
    if hits.len() >= 5 {
        return false;
    }
    hits.push(now);
    true
}
const ALLOWED_KINDS: [u8; 7] = [0, 2, 4, 5, 13, 15, 16];
const MAX_SLOWMODE: i32 = 21_600; // 6 h
const MIN_BITRATE: i32 = 8_000;
const MAX_BITRATE: i32 = 512_000; // on autorise bien plus que Discord (96 kbps)
const MAX_USER_LIMIT: i32 = 99;
const AUTO_ARCHIVE_VALUES: [i32; 4] = [60, 1_440, 4_320, 10_080]; // 1 h / 1 j / 3 j / 7 j

fn row_to_guild(r: &SqliteRow) -> Guild {
    Guild {
        id: Snowflake::from_i64(r.get::<i64, _>("id")),
        name: r.get("name"),
        owner_id: Snowflake::from_i64(r.get::<i64, _>("owner_id")),
        icon_id: r.get("icon_id"),
        description: r.get("description"),
        discoverable: r.get::<i64, _>("discoverable") != 0,
        banner_color: r.get("banner_color"),
        banner_id: r.get("banner_id"),
        games: parse_games(r.get("games")),
        private_profile: r.get::<i64, _>("private_profile") != 0,
        system_channel_id: r
            .get::<Option<i64>, _>("system_channel_id")
            .map(Snowflake::from_i64),
        default_message_notifications: r.get::<i64, _>("default_message_notifications") as u8,
        afk_channel_id: r
            .get::<Option<i64>, _>("afk_channel_id")
            .map(Snowflake::from_i64),
        afk_timeout: r.get::<i64, _>("afk_timeout"),
        vanity_code: r.get("vanity_code"),
    }
}

/// Récupère une guilde complète (tous les champs du DTO). Réutilisé par les chemins d'adhésion
/// et de transfert pour éviter de reconstruire `Guild` à la main (et oublier un champ).
pub async fn fetch_guild_full(st: &AppState, gid: i64) -> AppResult<Option<Guild>> {
    let row = sqlx::query(&format!("{GUILD_SELECT} WHERE id = ?"))
        .bind(gid)
        .fetch_optional(&st.pool)
        .await?;
    Ok(row.as_ref().map(row_to_guild))
}

// ───────────────────────────── Guildes ─────────────────────────────

/// `POST /guilds` — crée une guilde (+ rôle @everyone + salon « général »).
pub async fn create_guild(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateGuild>,
) -> AppResult<Json<Guild>> {
    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(AppError::bad_request(
            "nom de guilde invalide (1 à 100 caractères)",
        ));
    }
    let id = st.ids.next();
    let now = now_ms();
    sqlx::query(
        "INSERT INTO guilds (id, name, owner_id, icon_id, created_at) VALUES (?, ?, ?, NULL, ?)",
    )
    .bind(id.as_i64())
    .bind(name)
    .bind(user.id.as_i64())
    .bind(now)
    .execute(&st.pool)
    .await?;
    sqlx::query(
        "INSERT INTO roles (id, guild_id, name, color, hoist, position, permissions, mentionable, managed, created_at) \
         VALUES (?, ?, '@everyone', 0, 0, 0, ?, 0, 1, ?)",
    )
    .bind(id.as_i64())
    .bind(id.as_i64())
    .bind(perms::DEFAULT_EVERYONE as i64)
    .bind(now)
    .execute(&st.pool)
    .await?;
    sqlx::query(
        "INSERT INTO guild_members (guild_id, user_id, nick, joined_at) VALUES (?, ?, NULL, ?)",
    )
    .bind(id.as_i64())
    .bind(user.id.as_i64())
    .bind(now)
    .execute(&st.pool)
    .await?;
    // Rôle @everyone (id == guild_id) attribué dès l'arrivée → cohérence avec les autres rôles.
    sqlx::query("INSERT OR IGNORE INTO member_roles (guild_id, user_id, role_id) VALUES (?, ?, ?)")
        .bind(id.as_i64())
        .bind(user.id.as_i64())
        .bind(id.as_i64())
        .execute(&st.pool)
        .await?;
    let chan = st.ids.next();
    sqlx::query(
        "INSERT INTO channels (id, guild_id, type, name, topic, position, parent_id, created_at) VALUES (?, ?, 0, 'général', NULL, 0, NULL, ?)",
    )
    .bind(chan.as_i64())
    .bind(id.as_i64())
    .bind(now)
    .execute(&st.pool)
    .await?;
    // #général devient le salon système par défaut (messages d'arrivée).
    sqlx::query("UPDATE guilds SET system_channel_id = ? WHERE id = ?")
        .bind(chan.as_i64())
        .bind(id.as_i64())
        .execute(&st.pool)
        .await?;
    let guild = Guild {
        id,
        name: name.to_string(),
        owner_id: user.id,
        icon_id: None,
        description: None,
        discoverable: false,
        banner_color: None,
        banner_id: None,
        games: Vec::new(),
        private_profile: false,
        system_channel_id: Some(chan),
        default_message_notifications: 0,
        afk_channel_id: None,
        afk_timeout: 300,
        vanity_code: None,
    };
    // Notifie le créateur (ses sessions) de la nouvelle guilde.
    st.publish(
        EventScope::User(user.id.as_i64()),
        "GUILD_CREATE",
        serde_json::to_value(&guild).unwrap_or_default(),
    );
    Ok(Json(guild))
}

/// `GET /guilds/:guild_id` — détail d'une guilde (membres uniquement).
pub async fn get_guild(
    State(st): State<AppState>,
    user: AuthUser,
    Path(guild_id): Path<String>,
) -> AppResult<Json<Guild>> {
    let gid = parse_i64(&guild_id)?;
    pg::require_guild_member(&st.pool, gid, user.id.as_i64()).await?;
    let g = sqlx::query(&format!("{GUILD_SELECT} WHERE id = ?"))
        .bind(gid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?;
    Ok(Json(row_to_guild(&g)))
}

/// `PATCH /guilds/:guild_id` — renommer, icône, description, découverte (`MANAGE_GUILD`).
pub async fn update_guild(
    State(st): State<AppState>,
    user: AuthUser,
    Path(guild_id): Path<String>,
    Json(req): Json<UpdateGuild>,
) -> AppResult<Json<Guild>> {
    let gid = parse_i64(&guild_id)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::MANAGE_GUILD).await?;
    if !guild_update_allowed(gid) {
        return Err(AppError::too_many(
            "trop de modifications du serveur — réessaie dans quelques minutes",
        ));
    }
    let g = sqlx::query(&format!("{GUILD_SELECT} WHERE id = ?"))
        .bind(gid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?;

    let name = match req.name {
        Some(n) => {
            let t = n.trim().to_string();
            if t.is_empty() || t.chars().count() > 100 {
                return Err(AppError::bad_request(
                    "nom de guilde invalide (1 à 100 caractères)",
                ));
            }
            t
        }
        None => g.get("name"),
    };
    let icon_id = match req.icon_id {
        Some(s) if s.trim().is_empty() => None,
        Some(s) => Some(s),
        None => g.get("icon_id"),
    };
    let description = match req.description {
        Some(s) if s.trim().is_empty() => None,
        Some(s) => {
            if s.chars().count() > 300 {
                return Err(AppError::bad_request("description trop longue (max 300)"));
            }
            Some(s)
        }
        None => g.get("description"),
    };
    let discoverable = req
        .discoverable
        .unwrap_or_else(|| g.get::<i64, _>("discoverable") != 0);

    // Bannière : couleur (dégradé) et/ou image téléversée.
    let banner_color = match req.banner_color {
        Some(c) => Some(c),
        None => g.get::<Option<i64>, _>("banner_color"),
    };
    let banner_id = match req.banner_id {
        Some(s) if s.trim().is_empty() => None,
        Some(s) => Some(s),
        None => g.get::<Option<String>, _>("banner_id"),
    };
    // Jeux : liste bornée, clés filtrées (alphanum/-/_ ≤ 40) ⇒ ni injection ni surcharge.
    let games_json: Option<String> = match req.games {
        Some(list) => {
            let clean: Vec<String> = list
                .into_iter()
                .filter(|k| {
                    !k.is_empty()
                        && k.chars().count() <= 40
                        && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                })
                .take(MAX_GAMES)
                .collect();
            Some(serde_json::to_string(&clean).unwrap_or_else(|_| "[]".into()))
        }
        None => g.get::<Option<String>, _>("games"),
    };
    let private_profile = req
        .private_profile
        .unwrap_or_else(|| g.get::<i64, _>("private_profile") != 0);

    // Salon système : "0" = désactiver ; sinon le salon doit être un salon TEXTE de CETTE guilde.
    let system_channel_id: Option<i64> = match req.system_channel_id {
        Some(s) if s.as_i64() == 0 => None,
        Some(s) => {
            let cid = s.as_i64();
            let ok = sqlx::query(
                "SELECT 1 FROM channels WHERE id = ? AND guild_id = ? AND type IN (0, 5)",
            )
            .bind(cid)
            .bind(gid)
            .fetch_optional(&st.pool)
            .await?
            .is_some();
            if !ok {
                return Err(AppError::bad_request(
                    "salon système invalide (salon texte de cette guilde requis)",
                ));
            }
            Some(cid)
        }
        None => g.get::<Option<i64>, _>("system_channel_id"),
    };

    let default_notif: i64 = match req.default_message_notifications {
        Some(n) if n <= 1 => n as i64,
        Some(_) => return Err(AppError::bad_request("niveau par défaut invalide (0 ou 1)")),
        None => g.get::<i64, _>("default_message_notifications"),
    };

    // Salon AFK : "0" = désactiver ; sinon salon VOCAL de cette guilde.
    let afk_channel_id: Option<i64> = match req.afk_channel_id {
        Some(s) if s.as_i64() == 0 => None,
        Some(s) => {
            let cid = s.as_i64();
            let ok = sqlx::query("SELECT 1 FROM channels WHERE id = ? AND guild_id = ? AND type = 2")
                .bind(cid)
                .bind(gid)
                .fetch_optional(&st.pool)
                .await?
                .is_some();
            if !ok {
                return Err(AppError::bad_request(
                    "salon AFK invalide (salon vocal de cette guilde requis)",
                ));
            }
            Some(cid)
        }
        None => g.get::<Option<i64>, _>("afk_channel_id"),
    };
    let afk_timeout: i64 = req
        .afk_timeout
        .map(|t| t.clamp(60, 3600))
        .unwrap_or_else(|| g.get::<i64, _>("afk_timeout"));

    // Code vanity : "" = retirer ; sinon [a-z0-9-]{2,32}, unique parmi les guildes.
    let vanity_code: Option<String> = match &req.vanity_code {
        Some(c) if c.trim().is_empty() => None,
        Some(c) => {
            let code = c.trim().to_lowercase();
            if code.len() < 2
                || code.len() > 32
                || !code.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
            {
                return Err(AppError::bad_request(
                    "code vanity invalide (2 à 32 caractères : a-z, 0-9, -)",
                ));
            }
            let taken = sqlx::query("SELECT 1 FROM guilds WHERE vanity_code = ? AND id != ?")
                .bind(&code)
                .bind(gid)
                .fetch_optional(&st.pool)
                .await?
                .is_some();
            if taken {
                return Err(AppError::bad_request("ce code vanity est déjà pris"));
            }
            Some(code)
        }
        None => g.get::<Option<String>, _>("vanity_code"),
    };

    sqlx::query(
        "UPDATE guilds SET name = ?, icon_id = ?, description = ?, discoverable = ?, \
         banner_color = ?, banner_id = ?, games = ?, private_profile = ?, system_channel_id = ?, \
         default_message_notifications = ?, afk_channel_id = ?, afk_timeout = ?, vanity_code = ? WHERE id = ?",
    )
    .bind(&name)
    .bind(icon_id.as_deref())
    .bind(description.as_deref())
    .bind(discoverable as i64)
    .bind(banner_color)
    .bind(banner_id.as_deref())
    .bind(games_json.as_deref())
    .bind(private_profile as i64)
    .bind(system_channel_id)
    .bind(default_notif)
    .bind(afk_channel_id)
    .bind(afk_timeout)
    .bind(vanity_code.as_deref())
    .bind(gid)
    .execute(&st.pool)
    .await?;
    let guild = Guild {
        id: Snowflake::from_i64(gid),
        name,
        owner_id: Snowflake::from_i64(g.get::<i64, _>("owner_id")),
        icon_id,
        description,
        discoverable,
        banner_color,
        banner_id,
        games: parse_games(games_json),
        private_profile,
        system_channel_id: system_channel_id.map(Snowflake::from_i64),
        default_message_notifications: default_notif as u8,
        afk_channel_id: afk_channel_id.map(Snowflake::from_i64),
        afk_timeout,
        vanity_code,
    };
    crate::routes_moderation::audit_named(&st, gid, user.id.as_i64(), "guild_update", &guild.name)
        .await;
    st.publish(
        EventScope::Guild(gid),
        "GUILD_UPDATE",
        serde_json::to_value(&guild).unwrap_or_default(),
    );
    Ok(Json(guild))
}

/// `DELETE /guilds/:guild_id` — supprime la guilde et toutes ses données (propriétaire uniquement).
pub async fn delete_guild(
    State(st): State<AppState>,
    user: AuthUser,
    Path(guild_id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let gid = parse_i64(&guild_id)?;
    let owner = pg::require_guild_owner_id(&st.pool, gid).await?;
    if owner != user.id.as_i64() {
        return Err(AppError::forbidden(
            "seul le propriétaire peut supprimer la guilde",
        ));
    }
    // Membres avant suppression (pour notifier ensuite via portée individuelle).
    let members: Vec<i64> = sqlx::query("SELECT user_id FROM guild_members WHERE guild_id = ?")
        .bind(gid)
        .fetch_all(&st.pool)
        .await?
        .into_iter()
        .map(|r| r.get::<i64, _>("user_id"))
        .collect();

    // Cascade atomique. `IN (SELECT id FROM channels WHERE guild_id = ?)` cible les salons de la guilde.
    let ch = "(SELECT id FROM channels WHERE guild_id = ?)";
    let mut tx = st.pool.begin().await?;
    // Messages & dérivés (les déclencheurs FTS se synchronisent à la suppression des messages).
    for sql in [
        format!("DELETE FROM reactions WHERE message_id IN (SELECT id FROM messages WHERE channel_id IN {ch})"),
        format!("DELETE FROM mentions WHERE channel_id IN {ch}"),
        format!("DELETE FROM read_states WHERE channel_id IN {ch}"),
        format!("DELETE FROM messages WHERE channel_id IN {ch}"),
        format!("DELETE FROM channel_overwrites WHERE channel_id IN {ch}"),
        format!("DELETE FROM notification_settings WHERE (scope_type = 1 AND scope_id IN {ch}) OR (scope_type = 0 AND scope_id = ?)"),
        "DELETE FROM webhooks WHERE guild_id = ?".to_string(),
        "DELETE FROM event_interested WHERE event_id IN (SELECT id FROM scheduled_events WHERE guild_id = ?)".to_string(),
        "DELETE FROM scheduled_events WHERE guild_id = ?".to_string(),
        "DELETE FROM channels WHERE guild_id = ?".to_string(),
        "DELETE FROM emojis WHERE guild_id = ?".to_string(),
        "DELETE FROM stickers WHERE guild_id = ?".to_string(),
        "DELETE FROM soundboard_sounds WHERE guild_id = ?".to_string(),
        "DELETE FROM member_roles WHERE guild_id = ?".to_string(),
        "DELETE FROM roles WHERE guild_id = ?".to_string(),
        "DELETE FROM invites WHERE guild_id = ?".to_string(),
        "DELETE FROM guild_bans WHERE guild_id = ?".to_string(),
        "DELETE FROM audit_log WHERE guild_id = ?".to_string(),
        "DELETE FROM guild_members WHERE guild_id = ?".to_string(),
        "DELETE FROM guilds WHERE id = ?".to_string(),
    ] {
        // Chaque requête de cette cascade ne référence `gid` qu'une seule fois, sauf la ligne
        // `notification_settings` qui le lie deux fois (salons puis guilde).
        let binds = sql.matches('?').count();
        let mut q = sqlx::query(&sql);
        for _ in 0..binds {
            q = q.bind(gid);
        }
        q.execute(&mut *tx).await?;
    }
    tx.commit().await?;

    // Notifie chaque ancien membre (la guilde n'existe plus → portée individuelle).
    for uid in members {
        st.publish(
            EventScope::User(uid),
            "GUILD_DELETE",
            serde_json::json!({ "id": gid.to_string() }),
        );
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `GET /guilds` — guildes dont l'utilisateur est membre.
pub async fn list_guilds(
    State(st): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<Guild>>> {
    let rows = sqlx::query(
        "SELECT g.id, g.name, g.owner_id, g.icon_id, g.description, g.discoverable, \
         g.banner_color, g.banner_id, g.games, g.private_profile, g.system_channel_id, \
         g.default_message_notifications, g.afk_channel_id, g.afk_timeout, g.vanity_code FROM guilds g \
         JOIN guild_members m ON m.guild_id = g.id WHERE m.user_id = ? ORDER BY g.id",
    )
    .bind(user.id.as_i64())
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(rows.iter().map(row_to_guild).collect()))
}

// ───────────────────────────── Salons ─────────────────────────────

/// `POST /guilds/:guild_id/channels`
pub async fn create_channel(
    State(st): State<AppState>,
    user: AuthUser,
    Path(guild_id): Path<String>,
    Json(req): Json<CreateChannel>,
) -> AppResult<Json<Channel>> {
    let gid = parse_i64(&guild_id)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::MANAGE_CHANNELS).await?;
    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(AppError::bad_request("nom de salon invalide"));
    }
    if !ALLOWED_KINDS.contains(&req.kind) {
        return Err(AppError::bad_request("type de salon non supporté"));
    }
    validate_topic(&req.topic)?;
    let parent_id = match req.parent_id {
        Some(p) => {
            if req.kind == 4 {
                return Err(AppError::bad_request(
                    "une catégorie ne peut pas avoir de parent",
                ));
            }
            ensure_category(&st, gid, p.as_i64()).await?;
            Some(p.as_i64())
        }
        None => None,
    };
    let nsfw = req.nsfw.unwrap_or(false) as i64;
    let rate = req.rate_limit_per_user.unwrap_or(0).clamp(0, MAX_SLOWMODE) as i64;

    let maxpos: i64 =
        sqlx::query("SELECT COALESCE(MAX(position), 0) AS m FROM channels WHERE guild_id = ?")
            .bind(gid)
            .fetch_one(&st.pool)
            .await?
            .get("m");
    let position = maxpos + 1;
    let id = st.ids.next();
    sqlx::query(
        "INSERT INTO channels (id, guild_id, type, name, topic, position, parent_id, nsfw, rate_limit_per_user, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.as_i64())
    .bind(gid)
    .bind(req.kind as i64)
    .bind(name)
    .bind(req.topic.as_deref())
    .bind(position)
    .bind(parent_id)
    .bind(nsfw)
    .bind(rate)
    .bind(now_ms())
    .execute(&st.pool)
    .await?;

    // Héritage : un salon créé sous une catégorie COPIE ses surcharges de permission —
    // sinon un salon né sous une catégorie privée serait visible de tous.
    if let Some(parent) = parent_id {
        copy_parent_overwrites(&st, id.as_i64(), parent).await?;
    }

    crate::routes_moderation::audit_named(&st, gid, user.id.as_i64(), "channel_create", name).await;

    let ch = fetch_channel(&st, id.as_i64()).await?;
    st.publish(
        EventScope::Channel {
            guild_id: gid,
            channel_id: id.as_i64(),
        },
        "CHANNEL_CREATE",
        serde_json::to_value(&ch).unwrap_or_default(),
    );
    Ok(Json(ch))
}

/// Remplace les surcharges d'un salon par une copie de celles de sa catégorie parente.
async fn copy_parent_overwrites(st: &AppState, cid: i64, parent: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM channel_overwrites WHERE channel_id = ?")
        .bind(cid)
        .execute(&st.pool)
        .await?;
    sqlx::query(
        "INSERT INTO channel_overwrites (channel_id, target_id, target_type, allow, deny) \
         SELECT ?, target_id, target_type, allow, deny FROM channel_overwrites WHERE channel_id = ?",
    )
    .bind(cid)
    .bind(parent)
    .execute(&st.pool)
    .await?;
    Ok(())
}

/// `POST /channels/:channel_id/sync-permissions` — re-copie les surcharges de la catégorie
/// parente sur le salon (gate `MANAGE_ROLES` sur le salon, comme l'édition d'overwrites).
pub async fn sync_channel_permissions(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let cid = parse_i64(&cid)?;
    let (gid, _owner, _perms) =
        pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::MANAGE_ROLES).await?;
    if gid == 0 {
        return Err(AppError::bad_request("salon hors guilde"));
    }
    let parent: Option<i64> = sqlx::query("SELECT parent_id FROM channels WHERE id = ?")
        .bind(cid)
        .fetch_optional(&st.pool)
        .await?
        .and_then(|r| r.get("parent_id"));
    let parent =
        parent.ok_or_else(|| AppError::bad_request("ce salon n'a pas de catégorie parente"))?;
    copy_parent_overwrites(&st, cid, parent).await?;
    let ch = fetch_channel(&st, cid).await?;
    st.publish(
        EventScope::Channel {
            guild_id: gid,
            channel_id: cid,
        },
        "CHANNEL_UPDATE",
        serde_json::to_value(&ch).unwrap_or_default(),
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `PUT /channels/:channel_id/thread-members/@me` — rejoindre un fil (abonnement).
pub async fn join_thread(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let cid = parse_i64(&cid)?;
    pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::VIEW_CHANNEL).await?;
    sqlx::query(
        "INSERT OR IGNORE INTO thread_members (channel_id, user_id, joined_at) VALUES (?, ?, ?)",
    )
    .bind(cid)
    .bind(user.id.as_i64())
    .bind(now_ms())
    .execute(&st.pool)
    .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `DELETE /channels/:channel_id/thread-members/@me` — quitter un fil.
pub async fn leave_thread(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let cid = parse_i64(&cid)?;
    sqlx::query("DELETE FROM thread_members WHERE channel_id = ? AND user_id = ?")
        .bind(cid)
        .bind(user.id.as_i64())
        .execute(&st.pool)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `GET /guilds/:guild_id/channels` — uniquement les salons visibles.
pub async fn list_channels(
    State(st): State<AppState>,
    user: AuthUser,
    Path(guild_id): Path<String>,
) -> AppResult<Json<Vec<Channel>>> {
    let gid = parse_i64(&guild_id)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::VIEW_CHANNEL).await?;
    let owner = pg::require_guild_owner_id(&st.pool, gid).await?;
    let rows = sqlx::query(&format!(
        "{CHANNEL_SELECT} WHERE guild_id = ? ORDER BY position, id"
    ))
    .bind(gid)
    .fetch_all(&st.pool)
    .await?;
    let mut out = Vec::new();
    for r in rows {
        let ch = row_to_channel(r);
        let p =
            pg::channel_permissions(&st.pool, gid, owner, ch.id.as_i64(), user.id.as_i64()).await?;
        if perms::has(p, perms::VIEW_CHANNEL) {
            out.push(ch);
        }
    }
    Ok(Json(out))
}

/// `GET /channels/:channel_id`
pub async fn get_channel(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
) -> AppResult<Json<Channel>> {
    let cid = parse_i64(&cid)?;
    pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::VIEW_CHANNEL).await?;
    Ok(Json(fetch_channel(&st, cid).await?))
}

/// `PATCH /channels/:channel_id`
pub async fn update_channel(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
    Json(req): Json<UpdateChannel>,
) -> AppResult<Json<Channel>> {
    let cid = parse_i64(&cid)?;
    let (gid, _owner, _p) =
        pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::MANAGE_CHANNELS).await?;
    let cur = fetch_channel(&st, cid).await?;

    let name = match req.name {
        Some(n) => {
            let n = n.trim().to_string();
            if n.is_empty() || n.chars().count() > 100 {
                return Err(AppError::bad_request("nom de salon invalide"));
            }
            n
        }
        None => cur.name.clone(),
    };
    if req.topic.is_some() {
        validate_topic(&req.topic)?;
    }
    let topic = req.topic.or(cur.topic.clone());
    let nsfw = req.nsfw.unwrap_or(cur.nsfw) as i64;
    let rate = req
        .rate_limit_per_user
        .unwrap_or(cur.rate_limit_per_user)
        .clamp(0, MAX_SLOWMODE) as i64;
    let position = req.position.unwrap_or(cur.position) as i64;
    let parent_id = match req.parent_id {
        Some(p) => {
            if cur.kind == 4 {
                return Err(AppError::bad_request(
                    "une catégorie ne peut pas avoir de parent",
                ));
            }
            ensure_category(&st, gid, p.as_i64()).await?;
            Some(p.as_i64())
        }
        None => cur.parent_id.map(|s| s.as_i64()),
    };

    // Paramètres vocaux (bornés) et texte.
    let bitrate = req
        .bitrate
        .map(|b| b.clamp(MIN_BITRATE, MAX_BITRATE))
        .unwrap_or(cur.bitrate);
    let user_limit = req
        .user_limit
        .map(|u| u.clamp(0, MAX_USER_LIMIT))
        .unwrap_or(cur.user_limit);
    let rtc_region = match req.rtc_region {
        Some(s) => {
            let s = s.trim();
            if s.is_empty() {
                None // automatique
            } else if s.chars().count() > 32
                || !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                return Err(AppError::bad_request("région invalide"));
            } else {
                Some(s.to_string())
            }
        }
        None => cur.rtc_region.clone(),
    };
    let video_quality_mode = match req.video_quality_mode {
        Some(v) if v == 1 || v == 2 => v,
        Some(_) => return Err(AppError::bad_request("qualité vidéo invalide")),
        None => cur.video_quality_mode,
    };
    let default_auto_archive = match req.default_auto_archive {
        Some(v) if AUTO_ARCHIVE_VALUES.contains(&v) => v,
        Some(_) => return Err(AppError::bad_request("durée de masquage invalide")),
        None => cur.default_auto_archive,
    };
    // Archivage / verrouillage : réservé aux fils (type 11/12).
    let is_thread = cur.kind == 11 || cur.kind == 12;
    let archived = match req.archived {
        Some(_) if !is_thread => {
            return Err(AppError::bad_request("seuls les fils peuvent être archivés"))
        }
        Some(a) => a,
        None => cur.archived,
    };
    let locked = match req.locked {
        Some(_) if !is_thread => {
            return Err(AppError::bad_request("seuls les fils peuvent être verrouillés"))
        }
        Some(l) => l,
        None => cur.locked,
    };
    let archived_at = if archived { Some(now_ms()) } else { None };

    sqlx::query(
        "UPDATE channels SET name = ?, topic = ?, nsfw = ?, rate_limit_per_user = ?, position = ?, parent_id = ?, \
         bitrate = ?, user_limit = ?, rtc_region = ?, video_quality_mode = ?, default_auto_archive = ?, \
         archived = ?, locked = ?, archived_at = ? WHERE id = ?",
    )
    .bind(&name)
    .bind(topic.as_deref())
    .bind(nsfw)
    .bind(rate)
    .bind(position)
    .bind(parent_id)
    .bind(bitrate as i64)
    .bind(user_limit as i64)
    .bind(rtc_region.as_deref())
    .bind(video_quality_mode as i64)
    .bind(default_auto_archive as i64)
    .bind(archived as i64)
    .bind(locked as i64)
    .bind(archived_at)
    .bind(cid)
    .execute(&st.pool)
    .await?;

    let ch = fetch_channel(&st, cid).await?;
    crate::routes_moderation::audit_named(&st, gid, user.id.as_i64(), "channel_update", &ch.name)
        .await;
    st.publish(
        EventScope::Channel {
            guild_id: gid,
            channel_id: cid,
        },
        "CHANNEL_UPDATE",
        serde_json::to_value(&ch).unwrap_or_default(),
    );
    Ok(Json(ch))
}

/// `DELETE /channels/:channel_id`
pub async fn delete_channel(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let cid = parse_i64(&cid)?;
    let (gid, _owner, _p) =
        pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::MANAGE_CHANNELS).await?;
    // Nom capturé avant suppression (pour l'audit).
    let chan_name: String = sqlx::query("SELECT name FROM channels WHERE id = ?")
        .bind(cid)
        .fetch_optional(&st.pool)
        .await?
        .map(|r| r.get("name"))
        .unwrap_or_default();
    // Les salons enfants d'une catégorie supprimée sont détachés (parent → NULL).
    let child_ids: Vec<i64> = sqlx::query("SELECT id FROM channels WHERE parent_id = ?")
        .bind(cid)
        .fetch_all(&st.pool)
        .await?
        .into_iter()
        .map(|r| r.get::<i64, _>("id"))
        .collect();
    sqlx::query("UPDATE channels SET parent_id = NULL WHERE parent_id = ?")
        .bind(cid)
        .execute(&st.pool)
        .await?;
    sqlx::query(
        "DELETE FROM reactions WHERE message_id IN (SELECT id FROM messages WHERE channel_id = ?)",
    )
    .bind(cid)
    .execute(&st.pool)
    .await?;
    sqlx::query("DELETE FROM messages WHERE channel_id = ?")
        .bind(cid)
        .execute(&st.pool)
        .await?;
    sqlx::query("DELETE FROM channel_overwrites WHERE channel_id = ?")
        .bind(cid)
        .execute(&st.pool)
        .await?;
    sqlx::query("DELETE FROM channels WHERE id = ?")
        .bind(cid)
        .execute(&st.pool)
        .await?;
    crate::routes_moderation::audit_named(&st, gid, user.id.as_i64(), "channel_delete", &chan_name)
        .await;
    st.publish(
        EventScope::Guild(gid),
        "CHANNEL_DELETE",
        serde_json::json!({ "id": cid.to_string(), "guild_id": gid.to_string() }),
    );
    // Notifie EN DIRECT les clients que les salons enfants sont désormais à la racine
    // (sinon, leur parent pointant vers une catégorie supprimée, ils disparaîtraient de la liste).
    for child in child_ids {
        if let Ok(ch) = fetch_channel(&st, child).await {
            st.publish(
                EventScope::Channel {
                    guild_id: gid,
                    channel_id: child,
                },
                "CHANNEL_UPDATE",
                serde_json::to_value(&ch).unwrap_or_default(),
            );
        }
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `PATCH /guilds/:guild_id/channels` — réordonnancement / déplacement entre catégories.
pub async fn reorder_channels(
    State(st): State<AppState>,
    user: AuthUser,
    Path(guild_id): Path<String>,
    Json(items): Json<Vec<ChannelPosition>>,
) -> AppResult<Json<serde_json::Value>> {
    let gid = parse_i64(&guild_id)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::MANAGE_CHANNELS).await?;
    if items.len() > 500 {
        return Err(AppError::bad_request("trop d'éléments"));
    }
    for it in &items {
        let cid = it.id.as_i64();
        let in_guild = sqlx::query("SELECT 1 FROM channels WHERE id = ? AND guild_id = ?")
            .bind(cid)
            .bind(gid)
            .fetch_optional(&st.pool)
            .await?
            .is_some();
        if !in_guild {
            return Err(AppError::not_found("salon hors de cette guilde"));
        }
        match it.parent_id {
            // Sentinelle « 0 » = racine : on sort le salon de toute catégorie (parent → NULL).
            Some(p) if p.as_i64() == 0 => {
                sqlx::query(
                    "UPDATE channels SET position = ?, parent_id = NULL WHERE id = ? AND guild_id = ?",
                )
                .bind(it.position as i64)
                .bind(cid)
                .bind(gid)
                .execute(&st.pool)
                .await?;
            }
            Some(p) => {
                ensure_category(&st, gid, p.as_i64()).await?;
                sqlx::query(
                    "UPDATE channels SET position = ?, parent_id = ? WHERE id = ? AND guild_id = ?",
                )
                .bind(it.position as i64)
                .bind(p.as_i64())
                .bind(cid)
                .bind(gid)
                .execute(&st.pool)
                .await?;
            }
            None => {
                sqlx::query("UPDATE channels SET position = ? WHERE id = ? AND guild_id = ?")
                    .bind(it.position as i64)
                    .bind(cid)
                    .bind(gid)
                    .execute(&st.pool)
                    .await?;
            }
        }
    }
    // Propage le nouvel ordre EN DIRECT à tous les membres de la guilde.
    let positions: Vec<serde_json::Value> = items
        .iter()
        .map(|it| {
            let parent = match it.parent_id {
                Some(p) if p.as_i64() != 0 => Some(p.to_string()),
                _ => None,
            };
            serde_json::json!({ "id": it.id.to_string(), "position": it.position, "parent_id": parent })
        })
        .collect();
    st.publish(
        EventScope::Guild(gid),
        "CHANNELS_REORDER",
        serde_json::json!({ "guild_id": gid.to_string(), "positions": positions }),
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ───────────────────────────── Fils (threads) ─────────────────────────────

/// `POST /channels/:channel_id/threads` — démarre un fil sous un salon texte/annonces.
/// Le fil (type 11) **hérite des surcharges de permission de son salon parent**.
pub async fn create_thread(
    State(st): State<AppState>,
    user: AuthUser,
    Path(parent): Path<String>,
    Json(req): Json<CreateThread>,
) -> AppResult<Json<Channel>> {
    let parent = parse_i64(&parent)?;
    let (gid, _owner, _p) = pg::require_channel_perm(
        &st.pool,
        parent,
        user.id.as_i64(),
        perms::VIEW_CHANNEL | perms::CREATE_PUBLIC_THREADS,
    )
    .await?;
    if gid == 0 {
        return Err(AppError::bad_request(
            "les fils ne sont pas disponibles en message privé",
        ));
    }
    let parent_ch = fetch_channel(&st, parent).await?;
    if !(parent_ch.kind == 0 || parent_ch.kind == 5) {
        return Err(AppError::bad_request(
            "un fil ne peut être créé que sous un salon texte ou d'annonces",
        ));
    }
    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(AppError::bad_request(
            "nom de fil invalide (1 à 100 caractères)",
        ));
    }
    let id = st.ids.next();
    sqlx::query(
        "INSERT INTO channels (id, guild_id, type, name, topic, position, parent_id, nsfw, rate_limit_per_user, created_at) \
         VALUES (?, ?, 11, ?, NULL, 0, ?, 0, 0, ?)",
    )
    .bind(id.as_i64())
    .bind(gid)
    .bind(name)
    .bind(parent)
    .bind(now_ms())
    .execute(&st.pool)
    .await?;
    let ch = fetch_channel(&st, id.as_i64()).await?;
    st.publish(
        EventScope::Channel {
            guild_id: gid,
            channel_id: id.as_i64(),
        },
        "THREAD_CREATE",
        serde_json::to_value(&ch).unwrap_or_default(),
    );
    Ok(Json(ch))
}

/// `GET /channels/:channel_id/threads` — liste les fils visibles d'un salon.
pub async fn list_threads(
    State(st): State<AppState>,
    user: AuthUser,
    Path(parent): Path<String>,
) -> AppResult<Json<Vec<Channel>>> {
    let parent = parse_i64(&parent)?;
    let (gid, owner, _p) =
        pg::require_channel_perm(&st.pool, parent, user.id.as_i64(), perms::VIEW_CHANNEL).await?;
    let rows = sqlx::query(&format!(
        "{CHANNEL_SELECT} WHERE parent_id = ? AND type IN (11, 12) ORDER BY id DESC"
    ))
    .bind(parent)
    .fetch_all(&st.pool)
    .await?;
    let mut out = Vec::new();
    for r in rows {
        let ch = row_to_channel(r);
        // Le fil hérite des surcharges du parent : on filtre sur la visibilité effective.
        let p =
            pg::channel_permissions(&st.pool, gid, owner, ch.id.as_i64(), user.id.as_i64()).await?;
        if perms::has(p, perms::VIEW_CHANNEL) {
            out.push(ch);
        }
    }
    Ok(Json(out))
}

// ───────────────────────────── Helpers ─────────────────────────────

fn validate_topic(topic: &Option<String>) -> AppResult<()> {
    if let Some(t) = topic {
        if t.chars().count() > 1024 {
            return Err(AppError::bad_request("sujet trop long (max 1024)"));
        }
    }
    Ok(())
}

async fn ensure_category(st: &AppState, gid: i64, parent_id: i64) -> AppResult<()> {
    let row = sqlx::query("SELECT type FROM channels WHERE id = ? AND guild_id = ?")
        .bind(parent_id)
        .bind(gid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("catégorie parente introuvable"))?;
    if row.get::<i64, _>("type") != 4 {
        return Err(AppError::bad_request("le parent n'est pas une catégorie"));
    }
    Ok(())
}

async fn fetch_channel(st: &AppState, cid: i64) -> AppResult<Channel> {
    let row = sqlx::query(&format!("{CHANNEL_SELECT} WHERE id = ?"))
        .bind(cid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("salon introuvable"))?;
    Ok(row_to_channel(row))
}

fn row_to_channel(r: SqliteRow) -> Channel {
    Channel {
        id: Snowflake::from_i64(r.get::<i64, _>("id")),
        guild_id: r.get::<Option<i64>, _>("guild_id").map(Snowflake::from_i64),
        kind: r.get::<i64, _>("kind") as u8,
        name: r.get("name"),
        topic: r.get("topic"),
        position: r.get::<i64, _>("position") as i32,
        parent_id: r
            .get::<Option<i64>, _>("parent_id")
            .map(Snowflake::from_i64),
        nsfw: r.get::<i64, _>("nsfw") != 0,
        rate_limit_per_user: r.get::<i64, _>("rate_limit_per_user") as i32,
        bitrate: r.get::<i64, _>("bitrate") as i32,
        user_limit: r.get::<i64, _>("user_limit") as i32,
        rtc_region: r.get::<Option<String>, _>("rtc_region"),
        video_quality_mode: r.get::<i64, _>("video_quality_mode") as u8,
        default_auto_archive: r.get::<i64, _>("default_auto_archive") as i32,
        last_message_id: r
            .get::<Option<i64>, _>("last_message_id")
            .map(Snowflake::from_i64),
        archived: r.get::<i64, _>("archived") != 0,
        locked: r.get::<i64, _>("locked") != 0,
    }
}
