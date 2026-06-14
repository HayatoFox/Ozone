//! Auto-modération : règles par guilde (mots filtrés, anti-spam de mentions).
//! La vérification (`check_message`) est appelée par `create_message`/`execute_webhook`
//! AVANT l'insertion : une règle `block` refuse le message (403), une règle `alert` le
//! laisse passer mais notifie le salon d'alerte. Cf. docs/features/11-moderation-securite.md.

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::permissions as pg;
use crate::state::{AppState, EventScope};
use crate::util::parse_i64;
use axum::extract::{Path, State};
use axum::Json;
use ozone_proto::dto::{AutomodRule, CreateAutomodRule, UpdateAutomodRule};
use ozone_proto::{perms, Snowflake};
use serde_json::json;
use sqlx::Row;

fn parse_json_vec(s: &str) -> Vec<String> {
    serde_json::from_str(s).unwrap_or_default()
}

fn row_to_rule(r: &sqlx::sqlite::SqliteRow) -> AutomodRule {
    AutomodRule {
        id: Snowflake::from_i64(r.get::<i64, _>("id")),
        guild_id: Snowflake::from_i64(r.get::<i64, _>("guild_id")),
        name: r.get("name"),
        trigger_type: r.get("trigger_type"),
        keywords: parse_json_vec(&r.get::<String, _>("keywords")),
        mention_limit: r.get("mention_limit"),
        action: r.get("action"),
        alert_channel_id: r
            .get::<Option<i64>, _>("alert_channel_id")
            .map(Snowflake::from_i64),
        exempt_roles: parse_json_vec(&r.get::<String, _>("exempt_roles"))
            .into_iter()
            .filter_map(|s| s.parse::<i64>().ok().map(Snowflake::from_i64))
            .collect(),
        enabled: r.get::<i64, _>("enabled") != 0,
    }
}

fn valid_trigger(t: &str) -> bool {
    matches!(t, "keyword" | "mention_spam")
}
fn valid_action(a: &str) -> bool {
    matches!(a, "block" | "alert")
}

/// `GET /guilds/:guild_id/automod/rules`
pub async fn list_rules(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
) -> AppResult<Json<Vec<AutomodRule>>> {
    let gid = parse_i64(&gid)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::MANAGE_GUILD).await?;
    let rows = sqlx::query("SELECT * FROM automod_rules WHERE guild_id = ? ORDER BY id")
        .bind(gid)
        .fetch_all(&st.pool)
        .await?;
    Ok(Json(rows.iter().map(row_to_rule).collect()))
}

/// `POST /guilds/:guild_id/automod/rules`
pub async fn create_rule(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
    Json(req): Json<CreateAutomodRule>,
) -> AppResult<Json<AutomodRule>> {
    let gid = parse_i64(&gid)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::MANAGE_GUILD).await?;
    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > 60 {
        return Err(AppError::bad_request("nom de règle invalide (1 à 60)"));
    }
    if !valid_trigger(&req.trigger_type) {
        return Err(AppError::bad_request(
            "déclencheur invalide (keyword | mention_spam)",
        ));
    }
    if !valid_action(&req.action) {
        return Err(AppError::bad_request("action invalide (block | alert)"));
    }
    // Mots-clés : bornés (≤ 100 entrées, ≤ 60 caractères chacun), minuscule.
    let keywords: Vec<String> = req
        .keywords
        .into_iter()
        .map(|k| k.trim().to_lowercase())
        .filter(|k| !k.is_empty() && k.chars().count() <= 60)
        .take(100)
        .collect();
    let exempt: Vec<String> = req.exempt_roles.iter().map(|r| r.to_string()).collect();
    let alert_cid = req.alert_channel_id.map(|c| c.as_i64());
    let mention_limit = req.mention_limit.clamp(1, 50);
    let id = st.ids.next();
    sqlx::query(
        "INSERT INTO automod_rules (id, guild_id, name, trigger_type, keywords, mention_limit, action, alert_channel_id, exempt_roles, enabled, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?)",
    )
    .bind(id.as_i64())
    .bind(gid)
    .bind(name)
    .bind(&req.trigger_type)
    .bind(serde_json::to_string(&keywords).unwrap_or_else(|_| "[]".into()))
    .bind(mention_limit)
    .bind(&req.action)
    .bind(alert_cid)
    .bind(serde_json::to_string(&exempt).unwrap_or_else(|_| "[]".into()))
    .bind(now_ms())
    .execute(&st.pool)
    .await?;
    crate::routes_moderation::audit_named(&st, gid, user.id.as_i64(), "automod_rule_create", name)
        .await;
    let row = sqlx::query("SELECT * FROM automod_rules WHERE id = ?")
        .bind(id.as_i64())
        .fetch_one(&st.pool)
        .await?;
    Ok(Json(row_to_rule(&row)))
}

/// `PATCH /guilds/:guild_id/automod/rules/:rule_id`
pub async fn update_rule(
    State(st): State<AppState>,
    user: AuthUser,
    Path((gid, rid)): Path<(String, String)>,
    Json(req): Json<UpdateAutomodRule>,
) -> AppResult<Json<AutomodRule>> {
    let gid = parse_i64(&gid)?;
    let rid = parse_i64(&rid)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::MANAGE_GUILD).await?;
    let cur = sqlx::query("SELECT * FROM automod_rules WHERE id = ? AND guild_id = ?")
        .bind(rid)
        .bind(gid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("règle introuvable"))?;
    let mut rule = row_to_rule(&cur);
    if let Some(n) = req.name {
        let t = n.trim().to_string();
        if t.is_empty() || t.chars().count() > 60 {
            return Err(AppError::bad_request("nom de règle invalide (1 à 60)"));
        }
        rule.name = t;
    }
    if let Some(kw) = req.keywords {
        rule.keywords = kw
            .into_iter()
            .map(|k| k.trim().to_lowercase())
            .filter(|k| !k.is_empty() && k.chars().count() <= 60)
            .take(100)
            .collect();
    }
    if let Some(ml) = req.mention_limit {
        rule.mention_limit = ml.clamp(1, 50);
    }
    if let Some(a) = req.action {
        if !valid_action(&a) {
            return Err(AppError::bad_request("action invalide (block | alert)"));
        }
        rule.action = a;
    }
    if let Some(c) = req.alert_channel_id {
        rule.alert_channel_id = if c.as_i64() == 0 { None } else { Some(c) };
    }
    if let Some(roles) = req.exempt_roles {
        rule.exempt_roles = roles;
    }
    if let Some(e) = req.enabled {
        rule.enabled = e;
    }
    let exempt: Vec<String> = rule.exempt_roles.iter().map(|r| r.to_string()).collect();
    sqlx::query(
        "UPDATE automod_rules SET name = ?, keywords = ?, mention_limit = ?, action = ?, alert_channel_id = ?, exempt_roles = ?, enabled = ? WHERE id = ? AND guild_id = ?",
    )
    .bind(&rule.name)
    .bind(serde_json::to_string(&rule.keywords).unwrap_or_else(|_| "[]".into()))
    .bind(rule.mention_limit)
    .bind(&rule.action)
    .bind(rule.alert_channel_id.map(|c| c.as_i64()))
    .bind(serde_json::to_string(&exempt).unwrap_or_else(|_| "[]".into()))
    .bind(rule.enabled as i64)
    .bind(rid)
    .bind(gid)
    .execute(&st.pool)
    .await?;
    Ok(Json(rule))
}

/// `DELETE /guilds/:guild_id/automod/rules/:rule_id`
pub async fn delete_rule(
    State(st): State<AppState>,
    user: AuthUser,
    Path((gid, rid)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let gid = parse_i64(&gid)?;
    let rid = parse_i64(&rid)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::MANAGE_GUILD).await?;
    let res = sqlx::query("DELETE FROM automod_rules WHERE id = ? AND guild_id = ?")
        .bind(rid)
        .bind(gid)
        .execute(&st.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("règle introuvable"));
    }
    Ok(Json(json!({ "ok": true })))
}

/// Décision d'auto-modération pour un message entrant.
pub enum AutomodVerdict {
    /// Le message passe (aucune règle déclenchée, ou seulement des règles `alert`).
    Allow,
    /// Le message est refusé (règle `block`), avec le nom de la règle déclenchée.
    Block(String),
}

/// Vérifie un message contre les règles actives de la guilde. Les membres exemptés (par rôle)
/// ou disposant de `MANAGE_MESSAGES` ne sont pas filtrés. Best-effort : toute erreur interne
/// laisse passer le message (l'auto-mod ne doit pas casser l'envoi).
pub async fn check_message(
    st: &AppState,
    gid: i64,
    channel_id: i64,
    author_id: i64,
    content: &str,
    author_perms: u64,
) -> AutomodVerdict {
    if gid == 0 {
        return AutomodVerdict::Allow;
    }
    // Les gestionnaires de messages ne sont jamais filtrés (cohérent avec le slowmode).
    if perms::has(author_perms, perms::MANAGE_MESSAGES) {
        return AutomodVerdict::Allow;
    }
    let rows = match sqlx::query("SELECT * FROM automod_rules WHERE guild_id = ? AND enabled = 1")
        .bind(gid)
        .fetch_all(&st.pool)
        .await
    {
        Ok(r) => r,
        Err(_) => return AutomodVerdict::Allow,
    };
    if rows.is_empty() {
        return AutomodVerdict::Allow;
    }
    let author_roles = pg::member_role_ids(&st.pool, gid, author_id)
        .await
        .unwrap_or_default();
    let lower = content.to_lowercase();
    let mention_count = count_mentions(content);

    for r in &rows {
        let rule = row_to_rule(r);
        // Exemption par rôle.
        if rule
            .exempt_roles
            .iter()
            .any(|er| author_roles.contains(&er.as_i64()))
        {
            continue;
        }
        let triggered = match rule.trigger_type.as_str() {
            "keyword" => rule.keywords.iter().any(|k| lower.contains(k)),
            "mention_spam" => mention_count >= rule.mention_limit,
            _ => false,
        };
        if !triggered {
            continue;
        }
        // Notifie le salon d'alerte. PORTÉE = ce salon d'alerte (EventScope::Channel) et NON
        // toute la guilde : l'aperçu du message peut provenir d'un salon privé ; seuls les
        // membres habilités à voir le salon d'alerte doivent recevoir le contenu (cf. should_deliver
        // qui impose VIEW_CHANNEL pour Channel, pas pour Guild).
        if let Some(alert) = rule.alert_channel_id {
            let preview: String = content.chars().take(80).collect();
            st.publish(
                EventScope::Channel {
                    guild_id: gid,
                    channel_id: alert.as_i64(),
                },
                "AUTOMOD_ACTION",
                json!({
                    "guild_id": gid.to_string(),
                    "channel_id": channel_id.to_string(),
                    "rule_id": rule.id.to_string(),
                    "rule_name": rule.name,
                    "action": rule.action,
                    "user_id": author_id.to_string(),
                    "preview": preview,
                    "alert_channel_id": alert.to_string(),
                }),
            );
        }
        crate::routes_moderation::record_audit_changes(
            st,
            gid,
            author_id,
            Some(author_id),
            "automod_trigger",
            None,
            Some(json!({ "name": rule.name, "action": rule.action })),
        )
        .await;
        if rule.action == "block" {
            return AutomodVerdict::Block(rule.name);
        }
    }
    AutomodVerdict::Allow
}

/// Compte les mentions « notifiantes » d'un message (utilisateurs, rôles, @everyone/@here).
fn count_mentions(content: &str) -> i64 {
    let users = content.matches("<@").count();
    let everyone = content.matches("@everyone").count() + content.matches("@here").count();
    (users + everyone) as i64
}
