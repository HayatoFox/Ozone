//! Signalisation vocale : rejoindre/quitter/déplacer un salon vocal, mute/deaf (soi + modération),
//! états vocaux. Émet `VOICE_STATE_UPDATE` (portée guilde) et `VOICE_SERVER_UPDATE` (portée user).
//!
//! Le **transport média** (SFU, SRTP, ICE) et le **chiffrement E2EE** (DAVE/MLS) sont un
//! sous-projet média distinct — cf. docs/06-infrastructure-vocale.md. `endpoint` ci-dessous est
//! un emplacement à configurer une fois un nœud SFU déployé.

use crate::crypto;
use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::permissions as pg;
use crate::state::{AppState, EventScope};
use crate::util::parse_i64;
use axum::extract::{Path, State};
use axum::Json;
use ozone_proto::dto::{
    ModerateVoiceState, UpdateVoiceState, VoiceConnectionInfo, VoiceJoinResponse, VoiceRegion,
    VoiceState,
};
use ozone_proto::{perms, Snowflake};
use serde_json::{json, Value};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

/// Emplacement du nœud média — à configurer lors du déploiement d'un SFU (placeholder).
const VOICE_ENDPOINT: &str = "wss://voice.ozone.local/sfu";
const VOICE_TOKEN_TTL: u64 = 3600;
const VS_SELECT: &str = "SELECT user_id, guild_id, channel_id, session_id, self_mute, self_deaf, \
     self_video, self_stream, mute, deaf, suppress FROM voice_states";

fn row_to_voice_state(r: &SqliteRow) -> VoiceState {
    VoiceState {
        user_id: Snowflake::from_i64(r.get::<i64, _>("user_id")),
        guild_id: Snowflake::from_i64(r.get::<i64, _>("guild_id")),
        channel_id: Some(Snowflake::from_i64(r.get::<i64, _>("channel_id"))),
        session_id: r.get("session_id"),
        self_mute: r.get::<i64, _>("self_mute") != 0,
        self_deaf: r.get::<i64, _>("self_deaf") != 0,
        self_video: r.get::<i64, _>("self_video") != 0,
        self_stream: r.get::<i64, _>("self_stream") != 0,
        mute: r.get::<i64, _>("mute") != 0,
        deaf: r.get::<i64, _>("deaf") != 0,
        suppress: r.get::<i64, _>("suppress") != 0,
    }
}

async fn fetch_voice_state(st: &AppState, uid: i64) -> AppResult<Option<VoiceState>> {
    let row = sqlx::query(&format!("{VS_SELECT} WHERE user_id = ?"))
        .bind(uid)
        .fetch_optional(&st.pool)
        .await?;
    Ok(row.as_ref().map(row_to_voice_state))
}

/// Vérifie que `cid` est bien un salon **vocal** (type 2) ou **stage** (type 13) de la guilde.
async fn ensure_voice_channel(st: &AppState, gid: i64, cid: i64) -> AppResult<()> {
    let row = sqlx::query("SELECT type, guild_id FROM channels WHERE id = ?")
        .bind(cid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("salon introuvable"))?;
    let kind: i64 = row.get("type");
    let row_gid: Option<i64> = row.get("guild_id");
    if row_gid != Some(gid) || !(kind == 2 || kind == 13) {
        return Err(AppError::bad_request(
            "le salon cible n'est pas un salon vocal de cette guilde",
        ));
    }
    Ok(())
}

async fn emit_state(st: &AppState, gid: i64, vs: &VoiceState) {
    st.publish(
        EventScope::Guild(gid),
        "VOICE_STATE_UPDATE",
        serde_json::to_value(vs).unwrap_or_default(),
    );
}

/// Émet un `VOICE_STATE_UPDATE` de **départ** (channel_id nul).
async fn emit_left(st: &AppState, gid: i64, uid: i64) {
    st.publish(
        EventScope::Guild(gid),
        "VOICE_STATE_UPDATE",
        json!({ "user_id": uid.to_string(), "guild_id": gid.to_string(), "channel_id": null }),
    );
}

/// `PATCH /guilds/:guild_id/voice-states/@me` — rejoindre/déplacer (avec `channel_id`)
/// ou mettre à jour ses indicateurs (sans `channel_id`).
pub async fn update_own_voice_state(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
    Json(req): Json<UpdateVoiceState>,
) -> AppResult<Json<Value>> {
    let gid = parse_i64(&gid)?;
    let me = user.id.as_i64();
    pg::require_guild_member(&st.pool, gid, me).await?;

    if let Some(target) = req.channel_id {
        // Rejoindre / se déplacer.
        let cid = target.as_i64();
        ensure_voice_channel(&st, gid, cid).await?;
        pg::require_channel_perm(&st.pool, cid, me, perms::VIEW_CHANNEL | perms::CONNECT).await?;
        let session_id = crypto::random_token();
        let now = now_ms();
        sqlx::query(
            "INSERT INTO voice_states (user_id, guild_id, channel_id, session_id, self_mute, self_deaf, self_video, self_stream, mute, deaf, suppress, joined_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, 0, 0, ?) \
             ON CONFLICT(user_id) DO UPDATE SET guild_id = excluded.guild_id, channel_id = excluded.channel_id, \
                 session_id = excluded.session_id, self_mute = excluded.self_mute, self_deaf = excluded.self_deaf, \
                 self_video = excluded.self_video, self_stream = excluded.self_stream, joined_at = excluded.joined_at",
        )
        .bind(me)
        .bind(gid)
        .bind(cid)
        .bind(&session_id)
        .bind(req.self_mute.unwrap_or(false) as i64)
        .bind(req.self_deaf.unwrap_or(false) as i64)
        .bind(req.self_video.unwrap_or(false) as i64)
        .bind(req.self_stream.unwrap_or(false) as i64)
        .bind(now)
        .execute(&st.pool)
        .await?;

        let vs = fetch_voice_state(&st, me)
            .await?
            .ok_or_else(|| AppError::internal("état vocal introuvable après insertion"))?;
        emit_state(&st, gid, &vs).await;

        // Jeton vocal : signé avec le secret partagé SFU ; `sub = "<user>.<channel>"` pour que
        // le SFU vérifie l'identité ET que le salon demandé correspond.
        let token = crypto::jwt_encode(
            &st.voice_secret,
            &format!("{me}.{cid}"),
            "voice",
            VOICE_TOKEN_TTL,
        );
        let connection = VoiceConnectionInfo {
            token,
            endpoint: VOICE_ENDPOINT.to_string(),
            guild_id: Snowflake::from_i64(gid),
            channel_id: target,
            session_id,
        };
        // Informations du nœud média : uniquement à l'intéressé.
        st.publish(
            EventScope::User(me),
            "VOICE_SERVER_UPDATE",
            serde_json::to_value(&connection).unwrap_or_default(),
        );
        Ok(Json(
            serde_json::to_value(VoiceJoinResponse {
                voice_state: vs,
                connection,
            })
            .unwrap_or_default(),
        ))
    } else {
        // Simple mise à jour des indicateurs (doit être déjà connecté à CETTE guilde).
        let existing = fetch_voice_state(&st, me)
            .await?
            .filter(|v| v.guild_id.as_i64() == gid)
            .ok_or_else(|| {
                AppError::bad_request("vous n'êtes pas connecté au vocal de cette guilde")
            })?;
        let self_mute = req.self_mute.unwrap_or(existing.self_mute) as i64;
        let self_deaf = req.self_deaf.unwrap_or(existing.self_deaf) as i64;
        let self_video = req.self_video.unwrap_or(existing.self_video) as i64;
        let self_stream = req.self_stream.unwrap_or(existing.self_stream) as i64;
        sqlx::query(
            "UPDATE voice_states SET self_mute = ?, self_deaf = ?, self_video = ?, self_stream = ? WHERE user_id = ?",
        )
        .bind(self_mute)
        .bind(self_deaf)
        .bind(self_video)
        .bind(self_stream)
        .bind(me)
        .execute(&st.pool)
        .await?;
        let vs = fetch_voice_state(&st, me)
            .await?
            .ok_or_else(|| AppError::internal("état vocal introuvable"))?;
        emit_state(&st, gid, &vs).await;
        Ok(Json(serde_json::to_value(vs).unwrap_or_default()))
    }
}

/// `DELETE /guilds/:guild_id/voice-states/@me` — quitter le vocal.
pub async fn leave_voice(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
) -> AppResult<Json<Value>> {
    let gid = parse_i64(&gid)?;
    let me = user.id.as_i64();
    pg::require_guild_member(&st.pool, gid, me).await?;
    let res = sqlx::query("DELETE FROM voice_states WHERE user_id = ? AND guild_id = ?")
        .bind(me)
        .bind(gid)
        .execute(&st.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("vous n'êtes pas connecté au vocal"));
    }
    emit_left(&st, gid, me).await;
    Ok(Json(json!({ "ok": true })))
}

/// `GET /guilds/:guild_id/voice-states` — états vocaux des membres (membres uniquement).
pub async fn list_voice_states(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
) -> AppResult<Json<Vec<VoiceState>>> {
    let gid = parse_i64(&gid)?;
    pg::require_guild_member(&st.pool, gid, user.id.as_i64()).await?;
    let rows = sqlx::query(&format!("{VS_SELECT} WHERE guild_id = ?"))
        .bind(gid)
        .fetch_all(&st.pool)
        .await?;
    Ok(Json(rows.iter().map(row_to_voice_state).collect()))
}

/// `PATCH /guilds/:guild_id/voice-states/:user_id` — modération vocale (mute/deaf/déplacer/déconnecter).
pub async fn moderate_voice_state(
    State(st): State<AppState>,
    user: AuthUser,
    Path((gid, target)): Path<(String, String)>,
    Json(req): Json<ModerateVoiceState>,
) -> AppResult<Json<Value>> {
    let gid = parse_i64(&gid)?;
    let target = parse_i64(&target)?;
    let me = user.id.as_i64();
    pg::require_guild_member(&st.pool, gid, me).await?;

    // La cible doit être connectée au vocal de cette guilde.
    let existing = fetch_voice_state(&st, target)
        .await?
        .filter(|v| v.guild_id.as_i64() == gid)
        .ok_or_else(|| AppError::not_found("ce membre n'est pas connecté au vocal"))?;

    // Le propriétaire ne peut pas être modéré en vocal.
    let owner = pg::guild_owner(&st.pool, gid)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?;
    if target == owner {
        return Err(AppError::forbidden(
            "impossible de modérer le propriétaire en vocal",
        ));
    }

    if req.mute.is_some() {
        pg::require_guild_perm(&st.pool, gid, me, perms::MUTE_MEMBERS).await?;
    }
    if req.deaf.is_some() {
        pg::require_guild_perm(&st.pool, gid, me, perms::DEAFEN_MEMBERS).await?;
    }
    if req.channel_id.is_some() || req.disconnect == Some(true) {
        pg::require_guild_perm(&st.pool, gid, me, perms::MOVE_MEMBERS).await?;
    }

    if req.disconnect == Some(true) {
        sqlx::query("DELETE FROM voice_states WHERE user_id = ? AND guild_id = ?")
            .bind(target)
            .bind(gid)
            .execute(&st.pool)
            .await?;
        emit_left(&st, gid, target).await;
        return Ok(Json(json!({ "ok": true })));
    }

    let new_channel = match req.channel_id {
        Some(c) => {
            ensure_voice_channel(&st, gid, c.as_i64()).await?;
            c.as_i64()
        }
        None => existing.channel_id.map(|s| s.as_i64()).unwrap_or(0),
    };
    let mute = req.mute.unwrap_or(existing.mute) as i64;
    let deaf = req.deaf.unwrap_or(existing.deaf) as i64;
    sqlx::query("UPDATE voice_states SET mute = ?, deaf = ?, channel_id = ? WHERE user_id = ? AND guild_id = ?")
        .bind(mute)
        .bind(deaf)
        .bind(new_channel)
        .bind(target)
        .bind(gid)
        .execute(&st.pool)
        .await?;
    let vs = fetch_voice_state(&st, target)
        .await?
        .ok_or_else(|| AppError::internal("état vocal introuvable"))?;
    emit_state(&st, gid, &vs).await;
    Ok(Json(serde_json::to_value(vs).unwrap_or_default()))
}

/// `GET /voice/regions` — régions vocales disponibles.
pub async fn voice_regions(
    State(_st): State<AppState>,
    _user: AuthUser,
) -> AppResult<Json<Vec<VoiceRegion>>> {
    Ok(Json(vec![
        VoiceRegion {
            id: "auto".into(),
            name: "Automatique".into(),
            optimal: true,
        },
        VoiceRegion {
            id: "local".into(),
            name: "Local".into(),
            optimal: false,
        },
    ]))
}

/// Déconnecte un utilisateur du vocal (appelé à la fermeture de sa session Gateway).
pub async fn disconnect_all_voice(st: &AppState, uid: i64) {
    if let Ok(Some(vs)) = fetch_voice_state(st, uid).await {
        let gid = vs.guild_id.as_i64();
        let _ = sqlx::query("DELETE FROM voice_states WHERE user_id = ?")
            .bind(uid)
            .execute(&st.pool)
            .await;
        emit_left(st, gid, uid).await;
    }
}
