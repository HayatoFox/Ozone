//! Messages : liste paginée, envoi (avec réponse), édition, suppression, réactions,
//! épingles, suppression en masse, indicateur de frappe. Cf. docs/features/04-messagerie.md.

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::permissions as pg;
use crate::state::{AppState, EventScope, HubEvent};
use crate::util::parse_i64;
use axum::extract::{Path, Query, State};
use axum::Json;
use crate::routes_polls::build_poll;
use ozone_proto::dto::{
    Attachment, BulkDelete, CreateMessage, EditMessage, Message, Reaction, SearchResponse, User,
};
use ozone_proto::{perms, Snowflake};
use serde::Deserialize;
use serde_json::json;
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use std::collections::HashMap;

const MAX_PINS: i64 = 250;

const MSG_SELECT: &str =
    "SELECT m.id, m.channel_id, m.author_id, m.content, m.type AS kind, m.nonce, \
     m.created_at, m.edited_at, m.reference_id, m.pinned, m.webhook_id, \
     m.author_name AS wh_name, m.author_avatar AS wh_avatar, \
     m.sticker_id, stk.name AS sticker_name, stk.format_type AS sticker_format, m.embeds, m.cipher, \
     u.username, u.display_name, u.avatar_id \
     FROM messages m JOIN users u ON u.id = m.author_id \
     LEFT JOIN stickers stk ON stk.id = m.sticker_id";

async fn emit(st: &AppState, channel_id: i64, t: &str, d: serde_json::Value) {
    // Portée pub/sub = ce salon (MP ou salon de guilde), qui existe au moment de l'émission.
    let scope = match crate::permissions::channel_guild(&st.pool, channel_id).await {
        Ok(Some(Some(gid))) => EventScope::Channel {
            guild_id: gid,
            channel_id,
        },
        _ => EventScope::Dm(channel_id),
    };
    let _ = st.hub.send(HubEvent {
        t: t.to_string(),
        d,
        scope,
    });
}

fn row_to_message_basic(r: SqliteRow) -> Message {
    let webhook_id = r
        .get::<Option<i64>, _>("webhook_id")
        .map(Snowflake::from_i64);
    // Pour un message de webhook, le nom/avatar de remplacement priment sur ceux de l'auteur réel.
    let wh_name: Option<String> = r.get("wh_name");
    let wh_avatar: Option<String> = r.get("wh_avatar");
    let (display_name, avatar_id) = if webhook_id.is_some() {
        (
            wh_name.or_else(|| r.get("display_name")),
            wh_avatar.or_else(|| r.get("avatar_id")),
        )
    } else {
        (r.get("display_name"), r.get("avatar_id"))
    };
    Message {
        id: Snowflake::from_i64(r.get::<i64, _>("id")),
        channel_id: Snowflake::from_i64(r.get::<i64, _>("channel_id")),
        author: User {
            id: Snowflake::from_i64(r.get::<i64, _>("author_id")),
            username: r.get("username"),
            display_name,
            avatar_id,
            email: None,
        },
        content: r.get("content"),
        kind: r.get::<i64, _>("kind") as u8,
        created_at: r.get::<i64, _>("created_at") as u64,
        edited_at: r.get::<Option<i64>, _>("edited_at").map(|v| v as u64),
        pinned: r.get::<i64, _>("pinned") != 0,
        reactions: Vec::new(),
        reference_id: r
            .get::<Option<i64>, _>("reference_id")
            .map(Snowflake::from_i64),
        referenced_message: None,
        nonce: r.get("nonce"),
        webhook_id,
        attachments: Vec::new(),
        poll: None,
        // Le sticker peut avoir été supprimé depuis : le LEFT JOIN renvoie alors des NULL
        // (on n'affiche que les stickers encore résolus).
        sticker: r.get::<Option<i64>, _>("sticker_id").and_then(|sid| {
            let name: Option<String> = r.get("sticker_name");
            name.map(|name| ozone_proto::dto::MessageSticker {
                id: Snowflake::from_i64(sid),
                name,
                format_type: r.get::<Option<i64>, _>("sticker_format").unwrap_or(1) as u8,
            })
        }),
        embeds: r
            .get::<Option<String>, _>("embeds")
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        cipher: r.get("cipher"),
    }
}

/// Assainit et borne une liste d'embeds, renvoyant un JSON prêt à stocker (ou `None` si vide).
/// Bornes : ≤ 10 embeds, titre ≤ 256, description ≤ 4096, ≤ 25 champs (name ≤ 256, value ≤ 1024),
/// footer ≤ 2048 ; URLs forcées en http(s) (sinon retirées) ; couleur masquée à 24 bits.
pub fn sanitize_embeds(embeds: Vec<ozone_proto::dto::MessageEmbed>) -> Option<String> {
    fn clip(s: Option<String>, max: usize) -> Option<String> {
        s.map(|t| t.chars().take(max).collect::<String>())
            .filter(|t| !t.is_empty())
    }
    fn safe_url(u: Option<String>) -> Option<String> {
        u.filter(|s| s.starts_with("http://") || s.starts_with("https://"))
            .map(|s| s.chars().take(2048).collect())
    }
    if embeds.is_empty() {
        return None;
    }
    let cleaned: Vec<ozone_proto::dto::MessageEmbed> = embeds
        .into_iter()
        .take(10)
        .map(|e| ozone_proto::dto::MessageEmbed {
            title: clip(e.title, 256),
            description: clip(e.description, 4096),
            url: safe_url(e.url),
            color: e.color.map(|c| c & 0xFF_FFFF),
            fields: e
                .fields
                .into_iter()
                .take(25)
                .filter_map(|f| {
                    let name: String = f.name.chars().take(256).collect();
                    let value: String = f.value.chars().take(1024).collect();
                    if name.is_empty() || value.is_empty() {
                        None
                    } else {
                        Some(ozone_proto::dto::EmbedField {
                            name,
                            value,
                            inline: f.inline,
                        })
                    }
                })
                .collect(),
            image_url: safe_url(e.image_url),
            footer: clip(e.footer, 2048),
        })
        // Retire les embeds totalement vides.
        .filter(|e| {
            e.title.is_some()
                || e.description.is_some()
                || !e.fields.is_empty()
                || e.image_url.is_some()
        })
        .collect();
    if cleaned.is_empty() {
        None
    } else {
        serde_json::to_string(&cleaned).ok()
    }
}

/// Récupère un message **dans un salon donné** (pour inliner le message cité sans fuite inter-salons).
async fn fetch_referenced(st: &AppState, cid: i64, mid: i64) -> AppResult<Option<Message>> {
    let row = sqlx::query(&format!("{MSG_SELECT} WHERE m.id = ? AND m.channel_id = ?"))
        .bind(mid)
        .bind(cid)
        .fetch_optional(&st.pool)
        .await?;
    Ok(row.map(row_to_message_basic))
}

async fn fetch_message_in_channel(st: &AppState, cid: i64, mid: i64) -> AppResult<Message> {
    let row = sqlx::query(&format!("{MSG_SELECT} WHERE m.id = ? AND m.channel_id = ?"))
        .bind(mid)
        .bind(cid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("message introuvable"))?;
    Ok(row_to_message_basic(row))
}

/// Charge les agrégats de réactions pour un ensemble de messages.
async fn load_reactions(
    st: &AppState,
    ids: &[i64],
    user_id: i64,
) -> AppResult<HashMap<i64, Vec<Reaction>>> {
    let mut map: HashMap<i64, Vec<Reaction>> = HashMap::new();
    if ids.is_empty() {
        return Ok(map);
    }
    let list = ids
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT message_id, emoji, COUNT(*) AS count, MAX(CASE WHEN user_id = ? THEN 1 ELSE 0 END) AS me \
         FROM reactions WHERE message_id IN ({list}) GROUP BY message_id, emoji ORDER BY MIN(created_at)"
    );
    let rows = sqlx::query(&sql).bind(user_id).fetch_all(&st.pool).await?;
    for r in rows {
        map.entry(r.get::<i64, _>("message_id"))
            .or_default()
            .push(Reaction {
                emoji: r.get("emoji"),
                count: r.get::<i64, _>("count"),
                me: r.get::<i64, _>("me") != 0,
            });
    }
    Ok(map)
}

/// Charge les pièces jointes (avec leur URL de téléchargement) pour un ensemble de messages.
async fn load_attachments(st: &AppState, ids: &[i64]) -> AppResult<HashMap<i64, Vec<Attachment>>> {
    let mut map: HashMap<i64, Vec<Attachment>> = HashMap::new();
    if ids.is_empty() {
        return Ok(map);
    }
    let list = ids
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT id, message_id, filename, content_type, size FROM attachments \
         WHERE message_id IN ({list}) ORDER BY id"
    );
    let rows = sqlx::query(&sql).fetch_all(&st.pool).await?;
    for r in rows {
        let mid: i64 = r.get("message_id");
        let id: i64 = r.get("id");
        let filename: String = r.get("filename");
        map.entry(mid).or_default().push(Attachment {
            id: Snowflake::from_i64(id),
            url: format!("/attachments/{id}/{filename}"),
            filename,
            content_type: r.get("content_type"),
            size: r.get::<i64, _>("size"),
        });
    }
    Ok(map)
}

/// Identifiants de messages (dans l'ensemble donné) qui portent un sondage.
async fn load_poll_ids(st: &AppState, ids: &[i64]) -> AppResult<Vec<i64>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let list = ids
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!("SELECT message_id FROM polls WHERE message_id IN ({list})");
    let rows = sqlx::query(&sql).fetch_all(&st.pool).await?;
    Ok(rows.into_iter().map(|r| r.get::<i64, _>("message_id")).collect())
}

async fn hydrate(st: &AppState, mut msg: Message, user_id: i64) -> AppResult<Message> {
    let map = load_reactions(st, &[msg.id.as_i64()], user_id).await?;
    if let Some(rs) = map.get(&msg.id.as_i64()) {
        msg.reactions = rs.clone();
    }
    let amap = load_attachments(st, &[msg.id.as_i64()]).await?;
    if let Some(a) = amap.get(&msg.id.as_i64()) {
        msg.attachments = a.clone();
    }
    if !load_poll_ids(st, &[msg.id.as_i64()]).await?.is_empty() {
        msg.poll = Some(build_poll(st, msg.id.as_i64(), user_id).await?);
    }
    if let Some(ref_id) = msg.reference_id {
        msg.referenced_message = fetch_referenced(st, msg.channel_id.as_i64(), ref_id.as_i64())
            .await?
            .map(Box::new);
    }
    Ok(msg)
}

/// Ré-hydrate un message et diffuse un `MESSAGE_UPDATE` (utilisé après création d'un sondage,
/// pour que le sondage apparaisse en temps réel chez les autres clients).
pub async fn emit_message_update(st: &AppState, cid: i64, mid: i64, viewer: i64) -> AppResult<()> {
    let msg = hydrate(st, fetch_message_in_channel(st, cid, mid).await?, viewer).await?;
    emit(st, cid, "MESSAGE_UPDATE", serde_json::to_value(&msg).unwrap_or_default()).await;
    Ok(())
}

// ───────────────────────────── Mentions ─────────────────────────────

/// Extrait les identifiants mentionnés (`<@123>` ou `<@!123>`), dédupliqués.
fn parse_mention_ids(content: &str) -> Vec<i64> {
    let bytes = content.as_bytes();
    let mut ids = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'<' && bytes[i + 1] == b'@' {
            let mut j = i + 2;
            if j < bytes.len() && bytes[j] == b'!' {
                j += 1;
            }
            let start = j;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'>' && j > start {
                if let Ok(id) = content[start..j].parse::<i64>() {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    ids
}

/// Enregistre les mentions d'un message **pour les seuls destinataires autorisés à voir le salon**
/// (anti-notification fantôme) et incrémente leur compteur de mentions non lues.
async fn process_mentions(
    st: &AppState,
    channel_id: i64,
    message_id: i64,
    author_id: i64,
    content: &str,
) -> AppResult<()> {
    let ids = parse_mention_ids(content);
    if ids.is_empty() {
        return Ok(());
    }
    let guild = match pg::channel_guild(&st.pool, channel_id).await? {
        Some(g) => g,
        None => return Ok(()),
    };
    let now = now_ms();
    for uid in ids {
        if uid == author_id {
            continue;
        }
        let can_view = match guild {
            Some(gid) => match pg::guild_owner(&st.pool, gid).await? {
                Some(owner) => {
                    let p = pg::channel_permissions(&st.pool, gid, owner, channel_id, uid).await?;
                    perms::has(p, perms::VIEW_CHANNEL)
                }
                None => false,
            },
            None => sqlx::query("SELECT 1 FROM dm_recipients WHERE channel_id = ? AND user_id = ?")
                .bind(channel_id)
                .bind(uid)
                .fetch_optional(&st.pool)
                .await?
                .is_some(),
        };
        if !can_view {
            continue;
        }
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO mentions (user_id, message_id, channel_id, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(uid)
        .bind(message_id)
        .bind(channel_id)
        .bind(now)
        .execute(&st.pool)
        .await?
        .rows_affected();
        if inserted > 0 {
            sqlx::query(
                "INSERT INTO read_states (user_id, channel_id, last_read_id, mention_count) VALUES (?, ?, 0, 1) \
                 ON CONFLICT(user_id, channel_id) DO UPDATE SET mention_count = mention_count + 1",
            )
            .bind(uid)
            .bind(channel_id)
            .execute(&st.pool)
            .await?;
        }
    }
    Ok(())
}

// ───────────────────────────── Liste paginée ─────────────────────────────

#[derive(Debug, Deserialize)]
pub struct MsgQuery {
    limit: Option<i64>,
    before: Option<String>,
    after: Option<String>,
    around: Option<String>,
}

/// `GET /channels/:channel_id/messages?limit=&before=&after=&around=`
pub async fn list_messages(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
    Query(q): Query<MsgQuery>,
) -> AppResult<Json<Vec<Message>>> {
    let cid = parse_i64(&cid)?;
    pg::require_channel_perm(
        &st.pool,
        cid,
        user.id.as_i64(),
        perms::VIEW_CHANNEL | perms::READ_MESSAGE_HISTORY,
    )
    .await?;
    let limit = q.limit.unwrap_or(50).clamp(1, 100);

    let rows = if let Some(before) = q.before {
        let b = parse_i64(&before)?;
        sqlx::query(&format!(
            "{MSG_SELECT} WHERE m.channel_id = ? AND m.id < ? ORDER BY m.id DESC LIMIT ?"
        ))
        .bind(cid)
        .bind(b)
        .bind(limit)
        .fetch_all(&st.pool)
        .await?
    } else if let Some(after) = q.after {
        let a = parse_i64(&after)?;
        sqlx::query(&format!(
            "{MSG_SELECT} WHERE m.channel_id = ? AND m.id > ? ORDER BY m.id ASC LIMIT ?"
        ))
        .bind(cid)
        .bind(a)
        .bind(limit)
        .fetch_all(&st.pool)
        .await?
    } else if let Some(around) = q.around {
        let a = parse_i64(&around)?;
        let half = (limit / 2).max(1);
        let mut rows = sqlx::query(&format!(
            "{MSG_SELECT} WHERE m.channel_id = ? AND m.id <= ? ORDER BY m.id DESC LIMIT ?"
        ))
        .bind(cid)
        .bind(a)
        .bind(half + 1)
        .fetch_all(&st.pool)
        .await?;
        let mut after_rows = sqlx::query(&format!(
            "{MSG_SELECT} WHERE m.channel_id = ? AND m.id > ? ORDER BY m.id ASC LIMIT ?"
        ))
        .bind(cid)
        .bind(a)
        .bind(half)
        .fetch_all(&st.pool)
        .await?;
        rows.append(&mut after_rows);
        rows
    } else {
        sqlx::query(&format!(
            "{MSG_SELECT} WHERE m.channel_id = ? ORDER BY m.id DESC LIMIT ?"
        ))
        .bind(cid)
        .bind(limit)
        .fetch_all(&st.pool)
        .await?
    };

    let mut msgs: Vec<Message> = rows.into_iter().map(row_to_message_basic).collect();
    msgs.sort_by_key(|m| m.id.as_i64());

    let ids: Vec<i64> = msgs.iter().map(|m| m.id.as_i64()).collect();
    let reactions = load_reactions(&st, &ids, user.id.as_i64()).await?;
    let attachments = load_attachments(&st, &ids).await?;
    let poll_ids: std::collections::HashSet<i64> =
        load_poll_ids(&st, &ids).await?.into_iter().collect();
    for m in &mut msgs {
        if let Some(rs) = reactions.get(&m.id.as_i64()) {
            m.reactions = rs.clone();
        }
        if let Some(a) = attachments.get(&m.id.as_i64()) {
            m.attachments = a.clone();
        }
        if poll_ids.contains(&m.id.as_i64()) {
            m.poll = Some(build_poll(&st, m.id.as_i64(), user.id.as_i64()).await?);
        }
        if let Some(ref_id) = m.reference_id {
            m.referenced_message = fetch_referenced(&st, m.channel_id.as_i64(), ref_id.as_i64())
                .await?
                .map(Box::new);
        }
    }
    Ok(Json(msgs))
}

// ───────────────────────────── Envoi / édition / suppression ─────────────────────────────

/// `POST /channels/:channel_id/messages`
pub async fn create_message(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
    Json(req): Json<CreateMessage>,
) -> AppResult<Json<Message>> {
    let cid = parse_i64(&cid)?;
    // Anti-spam global par utilisateur (en plus du slowmode par salon, qui reste par-salon).
    st.rate
        .check(crate::ratelimit::MESSAGE, &user.id.to_string())
        .map_err(AppError::rate_limited)?;
    let (gid, _owner, perms_acc) =
        pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::SEND_MESSAGES).await?;
    let content = req.content.trim_end();
    // Chiffrement de bout en bout (MP 1:1) : blob opaque « iv|ciphertext » base64. Le serveur le
    // stocke tel quel et ne voit jamais le texte clair → inaccessible à l'admin de l'instance.
    // Réservé aux MP (pas de guilde) ; borné pour éviter l'abus de stockage.
    let cipher = req.cipher.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if let Some(c) = cipher {
        if gid != 0 {
            return Err(AppError::bad_request(
                "le chiffrement de bout en bout n'est disponible qu'en message privé",
            ));
        }
        if c.len() > 64 * 1024 {
            return Err(AppError::bad_request("charge chiffrée trop volumineuse"));
        }
    }
    // Garantie E2EE : dès qu'un cipher est présent, le serveur IGNORE tout texte clair fourni — il
    // ne le stocke pas, ne l'indexe pas (FTS), ne le passe ni à l'auto-mod ni aux mentions.
    let content: &str = if cipher.is_some() { "" } else { content };
    // Embeds : nécessite EMBED_LINKS, assainis et bornés. Un message peut n'être QUE des embeds.
    let embeds_json = if req.embeds.is_empty() {
        None
    } else {
        if !perms::has(perms_acc, perms::EMBED_LINKS) {
            return Err(AppError::forbidden(
                "permission « Intégrer des liens » requise pour les embeds",
            ));
        }
        sanitize_embeds(req.embeds.clone())
    };
    // Un message peut être vide s'il porte au moins une pièce jointe, un sticker, un embed OU un cipher.
    if (content.is_empty()
        && req.attachments.is_empty()
        && req.sticker_id.is_none()
        && embeds_json.is_none()
        && cipher.is_none())
        || content.chars().count() > 4000
    {
        return Err(AppError::bad_request(
            "contenu de message invalide (1 à 4000 caractères, ou une pièce jointe / un sticker / un embed)",
        ));
    }
    // Sticker : doit appartenir à la guilde du salon (pas de stickers en MP).
    let sticker_id = match req.sticker_id {
        Some(s) => {
            let sid = s.as_i64();
            let ok = sqlx::query("SELECT 1 FROM stickers WHERE id = ? AND guild_id = ?")
                .bind(sid)
                .bind(gid)
                .fetch_optional(&st.pool)
                .await?
                .is_some();
            if !ok {
                return Err(AppError::bad_request("sticker introuvable dans cette guilde"));
            }
            Some(sid)
        }
        None => None,
    };
    // Fil verrouillé : seuls les modérateurs (MANAGE_CHANNELS) peuvent écrire. Un fil archivé
    // (non verrouillé) est réactivé automatiquement à l'écriture (comportement Discord).
    let thread_state: Option<(i64, i64, i64)> = sqlx::query(
        "SELECT type AS kind, archived, locked FROM channels WHERE id = ?",
    )
    .bind(cid)
    .fetch_optional(&st.pool)
    .await?
    .map(|r| {
        (
            r.get::<i64, _>("kind"),
            r.get::<i64, _>("archived"),
            r.get::<i64, _>("locked"),
        )
    });
    if let Some((kind, archived, locked)) = thread_state {
        if kind == 11 || kind == 12 {
            if locked != 0 && !perms::has(perms_acc, perms::MANAGE_CHANNELS) {
                return Err(AppError::forbidden("ce fil est verrouillé"));
            }
            if archived != 0 {
                let _ = sqlx::query("UPDATE channels SET archived = 0, archived_at = NULL WHERE id = ?")
                    .bind(cid)
                    .execute(&st.pool)
                    .await;
            }
        }
    }

    // Timeout : un membre en sourdine ne peut pas écrire dans un salon de guilde.
    if gid != 0 {
        let until: Option<i64> = sqlx::query(
            "SELECT communication_disabled_until FROM guild_members WHERE guild_id = ? AND user_id = ?",
        )
        .bind(gid)
        .bind(user.id.as_i64())
        .fetch_optional(&st.pool)
        .await?
        .and_then(|r| r.get("communication_disabled_until"));
        if let Some(until) = until {
            if until > now_ms() {
                return Err(AppError::forbidden(
                    "vous êtes temporairement en sourdine (timeout)",
                ));
            }
        }
    }
    // Slowmode (sauf MANAGE_MESSAGES / MANAGE_CHANNELS / BYPASS_SLOWMODE).
    if !(perms::has(perms_acc, perms::MANAGE_MESSAGES)
        || perms::has(perms_acc, perms::MANAGE_CHANNELS)
        || perms::has(perms_acc, perms::BYPASS_SLOWMODE))
    {
        let rate: i64 = sqlx::query("SELECT rate_limit_per_user FROM channels WHERE id = ?")
            .bind(cid)
            .fetch_one(&st.pool)
            .await?
            .get("rate_limit_per_user");
        if rate > 0 {
            let last: Option<i64> = sqlx::query(
                "SELECT created_at FROM messages WHERE channel_id = ? AND author_id = ? ORDER BY id DESC LIMIT 1",
            )
            .bind(cid)
            .bind(user.id.as_i64())
            .fetch_optional(&st.pool)
            .await?
            .map(|r| r.get::<i64, _>("created_at"));
            if let Some(last) = last {
                if now_ms() - last < rate * 1000 {
                    return Err(AppError::too_many("slowmode actif, réessayez plus tard"));
                }
            }
        }
    }

    // Auto-modération : une règle `block` refuse le message (les gestionnaires sont exemptés).
    if !content.is_empty() {
        if let crate::routes_automod::AutomodVerdict::Block(rule) =
            crate::routes_automod::check_message(&st, gid, cid, user.id.as_i64(), content, perms_acc)
                .await
        {
            return Err(AppError::forbidden(format!(
                "message bloqué par l'auto-modération ({rule})"
            )));
        }
    }

    let reference_id = match req.reply_to {
        Some(s) => {
            let rid = s.as_i64();
            let exists = sqlx::query("SELECT 1 FROM messages WHERE id = ? AND channel_id = ?")
                .bind(rid)
                .bind(cid)
                .fetch_optional(&st.pool)
                .await?;
            if exists.is_none() {
                return Err(AppError::bad_request("message de réponse introuvable"));
            }
            Some(rid)
        }
        None => None,
    };

    let id = st.ids.next();
    let now = now_ms();
    sqlx::query(
        "INSERT INTO messages (id, channel_id, author_id, content, type, nonce, reference_id, pinned, created_at, edited_at, sticker_id, embeds, cipher) \
         VALUES (?, ?, ?, ?, 0, ?, ?, 0, ?, NULL, ?, ?, ?)",
    )
    .bind(id.as_i64())
    .bind(cid)
    .bind(user.id.as_i64())
    .bind(content)
    .bind(req.nonce.as_deref())
    .bind(reference_id)
    .bind(now)
    .bind(sticker_id)
    .bind(embeds_json.as_deref())
    .bind(cipher)
    .execute(&st.pool)
    .await?;

    process_mentions(&st, cid, id.as_i64(), user.id.as_i64(), content).await?;

    // Lie les pièces jointes déjà téléversées : uniquement les siennes, en attente, du même salon.
    for att in &req.attachments {
        sqlx::query(
            "UPDATE attachments SET message_id = ? WHERE id = ? AND uploader_id = ? AND channel_id = ? AND message_id IS NULL",
        )
        .bind(id.as_i64())
        .bind(att.as_i64())
        .bind(user.id.as_i64())
        .bind(cid)
        .execute(&st.pool)
        .await?;
    }

    let msg = hydrate(
        &st,
        fetch_message_in_channel(&st, cid, id.as_i64()).await?,
        user.id.as_i64(),
    )
    .await?;
    emit(
        &st,
        cid,
        "MESSAGE_CREATE",
        serde_json::to_value(&msg).unwrap_or_default(),
    )
    .await;
    Ok(Json(msg))
}

/// Insère un message texte simple (p. ex. message porteur d'un **sondage**) et diffuse
/// `MESSAGE_CREATE`. Renvoie le message hydraté.
pub async fn insert_text_message(
    st: &AppState,
    channel_id: i64,
    author_id: i64,
    content: &str,
) -> AppResult<Message> {
    let id = st.ids.next();
    let now = now_ms();
    sqlx::query(
        "INSERT INTO messages (id, channel_id, author_id, content, type, nonce, reference_id, pinned, created_at, edited_at) \
         VALUES (?, ?, ?, ?, 0, NULL, NULL, 0, ?, NULL)",
    )
    .bind(id.as_i64())
    .bind(channel_id)
    .bind(author_id)
    .bind(content)
    .bind(now)
    .execute(&st.pool)
    .await?;
    let msg = hydrate(
        st,
        fetch_message_in_channel(st, channel_id, id.as_i64()).await?,
        author_id,
    )
    .await?;
    emit(
        st,
        channel_id,
        "MESSAGE_CREATE",
        serde_json::to_value(&msg).unwrap_or_default(),
    )
    .await;
    Ok(msg)
}

/// Insère un message **système** (ex. type 7 = arrivée d'un membre) et diffuse `MESSAGE_CREATE`.
/// Best-effort : n'échoue jamais l'opération appelante (l'adhésion prime sur le message).
pub async fn insert_system_message(st: &AppState, channel_id: i64, author_id: i64, kind: i64) {
    let id = st.ids.next();
    let now = now_ms();
    let ins = sqlx::query(
        "INSERT INTO messages (id, channel_id, author_id, content, type, nonce, reference_id, pinned, created_at, edited_at) \
         VALUES (?, ?, ?, '', ?, NULL, NULL, 0, ?, NULL)",
    )
    .bind(id.as_i64())
    .bind(channel_id)
    .bind(author_id)
    .bind(kind)
    .bind(now)
    .execute(&st.pool)
    .await;
    if ins.is_err() {
        return;
    }
    let msg = match fetch_message_in_channel(st, channel_id, id.as_i64()).await {
        Ok(row) => match hydrate(st, row, author_id).await {
            Ok(m) => m,
            Err(_) => return,
        },
        Err(_) => return,
    };
    emit(
        st,
        channel_id,
        "MESSAGE_CREATE",
        serde_json::to_value(&msg).unwrap_or_default(),
    )
    .await;
}

/// Insère un message **émis par un webhook** et diffuse `MESSAGE_CREATE`.
/// `author_id` = créateur du webhook (pour la jointure `users`) ; le nom/avatar de
/// remplacement priment à l'affichage. Renvoie le message hydraté.
#[allow(clippy::too_many_arguments)] // helper interne : surcharges nom/avatar/embeds explicites
pub async fn insert_webhook_message(
    st: &AppState,
    channel_id: i64,
    webhook_id: i64,
    author_id: i64,
    name_override: Option<&str>,
    avatar_override: Option<&str>,
    content: &str,
    embeds: Vec<ozone_proto::dto::MessageEmbed>,
) -> AppResult<Message> {
    let id = st.ids.next();
    let now = now_ms();
    let embeds_json = sanitize_embeds(embeds);
    sqlx::query(
        "INSERT INTO messages (id, channel_id, author_id, content, type, nonce, reference_id, pinned, webhook_id, author_name, author_avatar, created_at, edited_at, embeds) \
         VALUES (?, ?, ?, ?, 0, NULL, NULL, 0, ?, ?, ?, ?, NULL, ?)",
    )
    .bind(id.as_i64())
    .bind(channel_id)
    .bind(author_id)
    .bind(content)
    .bind(webhook_id)
    .bind(name_override)
    .bind(avatar_override)
    .bind(now)
    .bind(embeds_json.as_deref())
    .execute(&st.pool)
    .await?;
    process_mentions(st, channel_id, id.as_i64(), author_id, content).await?;
    let msg = hydrate(
        st,
        fetch_message_in_channel(st, channel_id, id.as_i64()).await?,
        author_id,
    )
    .await?;
    emit(
        st,
        channel_id,
        "MESSAGE_CREATE",
        serde_json::to_value(&msg).unwrap_or_default(),
    )
    .await;
    Ok(msg)
}

/// `PATCH /channels/:channel_id/messages/:message_id`
pub async fn edit_message(
    State(st): State<AppState>,
    user: AuthUser,
    Path((cid, mid)): Path<(String, String)>,
    Json(req): Json<EditMessage>,
) -> AppResult<Json<Message>> {
    let cid = parse_i64(&cid)?;
    let mid = parse_i64(&mid)?;
    let (gid, _owner, _perms_acc) =
        pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::VIEW_CHANNEL).await?;
    let existing = fetch_message_in_channel(&st, cid, mid).await?;
    if existing.author.id.as_i64() != user.id.as_i64() {
        return Err(AppError::forbidden(
            "vous n'êtes pas l'auteur de ce message",
        ));
    }
    // Édition chiffrée (MP 1:1) : on remplace le blob opaque et on garde `content` vide. Réservé aux MP.
    let cipher = req.cipher.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if let Some(c) = cipher {
        if gid != 0 {
            return Err(AppError::bad_request(
                "le chiffrement de bout en bout n'est disponible qu'en message privé",
            ));
        }
        if c.len() > 64 * 1024 {
            return Err(AppError::bad_request("charge chiffrée trop volumineuse"));
        }
        sqlx::query("UPDATE messages SET content = '', cipher = ?, edited_at = ? WHERE id = ?")
            .bind(c)
            .bind(now_ms())
            .bind(mid)
            .execute(&st.pool)
            .await?;
    } else {
        let content = req.content.trim_end();
        if content.is_empty() || content.chars().count() > 4000 {
            return Err(AppError::bad_request(
                "contenu de message invalide (1 à 4000 caractères)",
            ));
        }
        sqlx::query("UPDATE messages SET content = ?, edited_at = ? WHERE id = ?")
            .bind(content)
            .bind(now_ms())
            .bind(mid)
            .execute(&st.pool)
            .await?;
    }
    let msg = hydrate(
        &st,
        fetch_message_in_channel(&st, cid, mid).await?,
        user.id.as_i64(),
    )
    .await?;
    emit(
        &st,
        cid,
        "MESSAGE_UPDATE",
        serde_json::to_value(&msg).unwrap_or_default(),
    )
    .await;
    Ok(Json(msg))
}

/// `DELETE /channels/:channel_id/messages/:message_id`
pub async fn delete_message(
    State(st): State<AppState>,
    user: AuthUser,
    Path((cid, mid)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let cid = parse_i64(&cid)?;
    let mid = parse_i64(&mid)?;
    let (_gid, _owner, perms_acc) =
        pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::VIEW_CHANNEL).await?;
    let existing = fetch_message_in_channel(&st, cid, mid).await?;
    let is_author = existing.author.id.as_i64() == user.id.as_i64();
    if !is_author && !perms::has(perms_acc, perms::MANAGE_MESSAGES) {
        return Err(AppError::forbidden(
            "permissions insuffisantes pour supprimer ce message",
        ));
    }
    sqlx::query("DELETE FROM messages WHERE id = ?")
        .bind(mid)
        .execute(&st.pool)
        .await?;
    sqlx::query("DELETE FROM reactions WHERE message_id = ?")
        .bind(mid)
        .execute(&st.pool)
        .await?;
    emit(
        &st,
        cid,
        "MESSAGE_DELETE",
        json!({ "id": mid.to_string(), "channel_id": cid.to_string() }),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

/// `POST /channels/:channel_id/messages/bulk-delete`
pub async fn bulk_delete(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
    Json(req): Json<BulkDelete>,
) -> AppResult<Json<serde_json::Value>> {
    let cid = parse_i64(&cid)?;
    pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::MANAGE_MESSAGES).await?;
    if req.messages.is_empty() || req.messages.len() > 100 {
        return Err(AppError::bad_request(
            "entre 1 et 100 messages par suppression en masse",
        ));
    }
    let mut deleted = Vec::new();
    for m in &req.messages {
        let mid = m.as_i64();
        let res = sqlx::query("DELETE FROM messages WHERE id = ? AND channel_id = ?")
            .bind(mid)
            .bind(cid)
            .execute(&st.pool)
            .await?;
        if res.rows_affected() > 0 {
            sqlx::query("DELETE FROM reactions WHERE message_id = ?")
                .bind(mid)
                .execute(&st.pool)
                .await?;
            deleted.push(mid.to_string());
        }
    }
    emit(
        &st,
        cid,
        "MESSAGE_DELETE_BULK",
        json!({ "channel_id": cid.to_string(), "ids": deleted }),
    )
    .await;
    Ok(Json(json!({ "deleted": deleted.len() })))
}

// ───────────────────────────── Réactions ─────────────────────────────

/// `PUT /channels/:channel_id/messages/:message_id/reactions/:emoji/@me`
pub async fn add_reaction(
    State(st): State<AppState>,
    user: AuthUser,
    Path((cid, mid, emoji)): Path<(String, String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let cid = parse_i64(&cid)?;
    let mid = parse_i64(&mid)?;
    pg::require_channel_perm(
        &st.pool,
        cid,
        user.id.as_i64(),
        perms::VIEW_CHANNEL | perms::READ_MESSAGE_HISTORY | perms::ADD_REACTIONS,
    )
    .await?;
    if emoji.is_empty() || emoji.chars().count() > 64 {
        return Err(AppError::bad_request("emoji invalide"));
    }
    fetch_message_in_channel(&st, cid, mid).await?; // existence
    sqlx::query("INSERT OR IGNORE INTO reactions (message_id, emoji, user_id, created_at) VALUES (?, ?, ?, ?)")
        .bind(mid)
        .bind(&emoji)
        .bind(user.id.as_i64())
        .bind(now_ms())
        .execute(&st.pool)
        .await?;
    emit(
        &st,
        cid,
        "MESSAGE_REACTION_ADD",
        json!({ "channel_id": cid.to_string(), "message_id": mid.to_string(), "user_id": user.id.to_string(), "emoji": emoji }),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

/// `DELETE /channels/:channel_id/messages/:message_id/reactions/:emoji/@me`
pub async fn remove_reaction(
    State(st): State<AppState>,
    user: AuthUser,
    Path((cid, mid, emoji)): Path<(String, String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let cid = parse_i64(&cid)?;
    let mid = parse_i64(&mid)?;
    pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::VIEW_CHANNEL).await?;
    sqlx::query("DELETE FROM reactions WHERE message_id = ? AND emoji = ? AND user_id = ?")
        .bind(mid)
        .bind(&emoji)
        .bind(user.id.as_i64())
        .execute(&st.pool)
        .await?;
    emit(
        &st,
        cid,
        "MESSAGE_REACTION_REMOVE",
        json!({ "channel_id": cid.to_string(), "message_id": mid.to_string(), "user_id": user.id.to_string(), "emoji": emoji }),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

// ───────────────────────────── Épingles ─────────────────────────────

/// `GET /channels/:channel_id/pins`
pub async fn list_pins(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
) -> AppResult<Json<Vec<Message>>> {
    let cid = parse_i64(&cid)?;
    pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::VIEW_CHANNEL).await?;
    let rows = sqlx::query(&format!(
        "{MSG_SELECT} WHERE m.channel_id = ? AND m.pinned = 1 ORDER BY m.id DESC"
    ))
    .bind(cid)
    .fetch_all(&st.pool)
    .await?;
    let mut msgs: Vec<Message> = rows.into_iter().map(row_to_message_basic).collect();
    let ids: Vec<i64> = msgs.iter().map(|m| m.id.as_i64()).collect();
    let reactions = load_reactions(&st, &ids, user.id.as_i64()).await?;
    for m in &mut msgs {
        if let Some(rs) = reactions.get(&m.id.as_i64()) {
            m.reactions = rs.clone();
        }
    }
    Ok(Json(msgs))
}

/// `PUT /channels/:channel_id/pins/:message_id`
pub async fn pin_message(
    State(st): State<AppState>,
    user: AuthUser,
    Path((cid, mid)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let cid = parse_i64(&cid)?;
    let mid = parse_i64(&mid)?;
    pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::PIN_MESSAGES).await?;
    fetch_message_in_channel(&st, cid, mid).await?;
    let count: i64 =
        sqlx::query("SELECT COUNT(*) AS c FROM messages WHERE channel_id = ? AND pinned = 1")
            .bind(cid)
            .fetch_one(&st.pool)
            .await?
            .get("c");
    if count >= MAX_PINS {
        return Err(AppError::bad_request(
            "nombre maximal de messages épinglés atteint (250)",
        ));
    }
    sqlx::query("UPDATE messages SET pinned = 1 WHERE id = ? AND channel_id = ?")
        .bind(mid)
        .bind(cid)
        .execute(&st.pool)
        .await?;
    // `message_id` + `pinned` : permet aux clients de mettre à jour le drapeau EN DIRECT.
    emit(
        &st,
        cid,
        "CHANNEL_PINS_UPDATE",
        json!({ "channel_id": cid.to_string(), "message_id": mid.to_string(), "pinned": true }),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

/// `DELETE /channels/:channel_id/pins/:message_id`
pub async fn unpin_message(
    State(st): State<AppState>,
    user: AuthUser,
    Path((cid, mid)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let cid = parse_i64(&cid)?;
    let mid = parse_i64(&mid)?;
    pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::PIN_MESSAGES).await?;
    sqlx::query("UPDATE messages SET pinned = 0 WHERE id = ? AND channel_id = ?")
        .bind(mid)
        .bind(cid)
        .execute(&st.pool)
        .await?;
    emit(
        &st,
        cid,
        "CHANNEL_PINS_UPDATE",
        json!({ "channel_id": cid.to_string(), "message_id": mid.to_string(), "pinned": false }),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

// ───────────────────────────── Frappe ─────────────────────────────

/// `POST /channels/:channel_id/typing`
pub async fn typing(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let cid = parse_i64(&cid)?;
    pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::SEND_MESSAGES).await?;
    emit(
        &st,
        cid,
        "TYPING_START",
        json!({ "channel_id": cid.to_string(), "user_id": user.id.to_string(), "timestamp": now_ms() }),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

// ───────────────────────────── Recherche (FTS5) ─────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// Texte recherché (plein-texte). Vide = pas de filtre textuel.
    q: Option<String>,
    /// Restreindre à un salon (recherche de guilde).
    channel_id: Option<String>,
    author_id: Option<String>,
    /// `link` = messages contenant un lien (autres valeurs : non encore indexées → 0 résultat).
    has: Option<String>,
    pinned: Option<bool>,
    before: Option<String>,
    after: Option<String>,
    /// Tri : `recent` (défaut) | `old` | `relevance` (avec `q`).
    sort: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

/// Transforme le texte utilisateur en requête FTS5 sûre (chaque terme est une phrase entre
/// guillemets ⇒ aucun opérateur FTS injectable, aucune erreur de syntaxe). `None` si vide.
fn fts_query(q: &str) -> Option<String> {
    let terms: Vec<String> = q
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
}

/// Identifiants des salons d'une guilde que l'utilisateur peut **lire** (VIEW + historique).
async fn viewable_channel_ids(
    st: &AppState,
    guild_id: i64,
    owner_id: i64,
    user_id: i64,
) -> AppResult<Vec<i64>> {
    let rows = sqlx::query("SELECT id FROM channels WHERE guild_id = ?")
        .bind(guild_id)
        .fetch_all(&st.pool)
        .await?;
    let mut ids = Vec::new();
    for r in rows {
        let cid = r.get::<i64, _>("id");
        let p = pg::channel_permissions(&st.pool, guild_id, owner_id, cid, user_id).await?;
        if perms::has(p, perms::VIEW_CHANNEL | perms::READ_MESSAGE_HISTORY) {
            ids.push(cid);
        }
    }
    Ok(ids)
}

/// Exécute la recherche sur un ensemble de salons **déjà autorisés**.
async fn run_search(
    st: &AppState,
    channel_ids: &[i64],
    q: &SearchQuery,
    user_id: i64,
) -> AppResult<SearchResponse> {
    if channel_ids.is_empty() {
        return Ok(SearchResponse {
            total: 0,
            messages: Vec::new(),
        });
    }
    // `has` non pris en charge (hors `link`) : aucune pièce jointe indexée pour l'instant.
    let has_link = match q.has.as_deref() {
        None => None,
        Some("link") => Some(true),
        Some(_) => {
            return Ok(SearchResponse {
                total: 0,
                messages: Vec::new(),
            })
        }
    };
    let fts = q.q.as_deref().and_then(fts_query);
    let id_list = channel_ids
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",");

    // Filtres entiers : validés en i64 (donc sûrs à interpoler). Le texte FTS est **lié**.
    let mut filters = format!(" WHERE m.channel_id IN ({id_list})");
    if fts.is_some() {
        filters.push_str(" AND messages_fts MATCH ?");
    }
    if let Some(a) = &q.author_id {
        filters.push_str(&format!(" AND m.author_id = {}", parse_i64(a)?));
    }
    if let Some(b) = &q.before {
        filters.push_str(&format!(" AND m.id < {}", parse_i64(b)?));
    }
    if let Some(a) = &q.after {
        filters.push_str(&format!(" AND m.id > {}", parse_i64(a)?));
    }
    match q.pinned {
        Some(true) => filters.push_str(" AND m.pinned = 1"),
        Some(false) => filters.push_str(" AND m.pinned = 0"),
        None => {}
    }
    if has_link.is_some() {
        filters.push_str(" AND m.content LIKE '%http%'");
    }
    let join_fts = if fts.is_some() {
        " JOIN messages_fts ON messages_fts.rowid = m.id"
    } else {
        ""
    };

    // Total (mêmes filtres, sans tri ni pagination).
    let count_sql = format!("SELECT COUNT(*) AS c FROM messages m{join_fts}{filters}");
    let mut cq = sqlx::query(&count_sql);
    if let Some(f) = &fts {
        cq = cq.bind(f);
    }
    let total: i64 = cq.fetch_one(&st.pool).await?.get("c");

    let order = match q.sort.as_deref() {
        Some("old") => " ORDER BY m.id ASC",
        Some("relevance") if fts.is_some() => " ORDER BY messages_fts.rank",
        _ => " ORDER BY m.id DESC",
    };
    let limit = q.limit.unwrap_or(25).clamp(1, 50);
    let offset = q.offset.unwrap_or(0).max(0);
    let sql = format!("{MSG_SELECT}{join_fts}{filters}{order} LIMIT {limit} OFFSET {offset}");
    let mut query = sqlx::query(&sql);
    if let Some(f) = &fts {
        query = query.bind(f);
    }
    let rows = query.fetch_all(&st.pool).await?;
    let mut msgs: Vec<Message> = rows.into_iter().map(row_to_message_basic).collect();
    let ids: Vec<i64> = msgs.iter().map(|m| m.id.as_i64()).collect();
    let reactions = load_reactions(st, &ids, user_id).await?;
    for m in &mut msgs {
        if let Some(rs) = reactions.get(&m.id.as_i64()) {
            m.reactions = rs.clone();
        }
    }
    Ok(SearchResponse {
        total,
        messages: msgs,
    })
}

/// `GET /guilds/:guild_id/messages/search` — recherche sur toute la guilde, filtrée par permissions.
pub async fn search_guild(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
    Query(q): Query<SearchQuery>,
) -> AppResult<Json<SearchResponse>> {
    let gid = parse_i64(&gid)?;
    pg::require_guild_member(&st.pool, gid, user.id.as_i64()).await?;
    let owner = pg::guild_owner(&st.pool, gid)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?;
    let mut channels = viewable_channel_ids(&st, gid, owner, user.id.as_i64()).await?;
    // Restriction facultative à un salon (qui doit être dans l'ensemble autorisé).
    if let Some(c) = &q.channel_id {
        let target = parse_i64(c)?;
        channels.retain(|&id| id == target);
    }
    Ok(Json(
        run_search(&st, &channels, &q, user.id.as_i64()).await?,
    ))
}

/// `GET /channels/:channel_id/messages/search` — recherche dans un seul salon (guilde ou MP).
pub async fn search_channel(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
    Query(q): Query<SearchQuery>,
) -> AppResult<Json<SearchResponse>> {
    let cid = parse_i64(&cid)?;
    // Doit pouvoir lire le salon (gère salons de guilde et MP, avec surcharges).
    pg::require_channel_perm(
        &st.pool,
        cid,
        user.id.as_i64(),
        perms::VIEW_CHANNEL | perms::READ_MESSAGE_HISTORY,
    )
    .await?;
    Ok(Json(run_search(&st, &[cid], &q, user.id.as_i64()).await?))
}

// ───────────────────────────── Boîte de mentions ─────────────────────────────

/// L'utilisateur peut-il **actuellement** lire ce salon ? (salon supprimé → `false`).
async fn can_view_channel(st: &AppState, cid: i64, uid: i64) -> AppResult<bool> {
    match pg::channel_guild(&st.pool, cid).await? {
        None => Ok(false),
        Some(Some(gid)) => match pg::guild_owner(&st.pool, gid).await? {
            Some(owner) => {
                let p = pg::channel_permissions(&st.pool, gid, owner, cid, uid).await?;
                Ok(perms::has(
                    p,
                    perms::VIEW_CHANNEL | perms::READ_MESSAGE_HISTORY,
                ))
            }
            None => Ok(false),
        },
        Some(None) => Ok(sqlx::query(
            "SELECT 1 FROM dm_recipients WHERE channel_id = ? AND user_id = ?",
        )
        .bind(cid)
        .bind(uid)
        .fetch_optional(&st.pool)
        .await?
        .is_some()),
    }
}

#[derive(Debug, Deserialize)]
pub struct InboxQuery {
    limit: Option<i64>,
}

/// `GET /users/@me/mentions?limit=` — messages récents qui me mentionnent, **filtrés aux salons
/// que je peux encore lire** (et messages non supprimés).
pub async fn mentions_inbox(
    State(st): State<AppState>,
    user: AuthUser,
    Query(q): Query<InboxQuery>,
) -> AppResult<Json<Vec<Message>>> {
    let uid = user.id.as_i64();
    let limit = q.limit.unwrap_or(25).clamp(1, 50);
    // On lit plus large que `limit` pour compenser le filtrage (permissions / suppressions).
    let rows = sqlx::query(
        "SELECT message_id, channel_id FROM mentions WHERE user_id = ? ORDER BY message_id DESC LIMIT ?",
    )
    .bind(uid)
    .bind(limit * 4)
    .fetch_all(&st.pool)
    .await?;

    let mut viewable: HashMap<i64, bool> = HashMap::new();
    let mut out: Vec<Message> = Vec::new();
    for r in rows {
        if out.len() as i64 >= limit {
            break;
        }
        let cid: i64 = r.get("channel_id");
        let mid: i64 = r.get("message_id");
        let can = match viewable.get(&cid) {
            Some(v) => *v,
            None => {
                let v = can_view_channel(&st, cid, uid).await?;
                viewable.insert(cid, v);
                v
            }
        };
        if !can {
            continue;
        }
        if let Some(row) = sqlx::query(&format!("{MSG_SELECT} WHERE m.id = ? AND m.channel_id = ?"))
            .bind(mid)
            .bind(cid)
            .fetch_optional(&st.pool)
            .await?
        {
            let mut msg = row_to_message_basic(row);
            let map = load_reactions(&st, &[mid], uid).await?;
            if let Some(rs) = map.get(&mid) {
                msg.reactions = rs.clone();
            }
            out.push(msg);
        }
    }
    Ok(Json(out))
}
