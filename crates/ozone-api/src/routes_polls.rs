//! Sondages : un sondage est porté par un message du salon. Création, vote (mono/multi), résultats.
//! Cf. docs/features/04-messagerie.md.

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::permissions as pg;
use crate::routes_messages::insert_text_message;
use crate::state::{AppState, EventScope};
use crate::util::parse_i64;
use axum::extract::{Path, State};
use axum::Json;
use ozone_proto::dto::{CastVote, CreatePoll, Poll, PollAnswer};
use ozone_proto::perms;
use ozone_proto::Snowflake;
use serde_json::json;
use sqlx::Row;

const MAX_ANSWERS: usize = 10;
const MAX_DURATION_HOURS: i64 = 768; // 32 jours

/// Construit le DTO `Poll` (réponses + décomptes + vote de l'utilisateur courant).
async fn build_poll(st: &AppState, mid: i64, viewer: i64) -> AppResult<Poll> {
    let p = sqlx::query(
        "SELECT channel_id, question, multiselect, expires_at FROM polls WHERE message_id = ?",
    )
    .bind(mid)
    .fetch_optional(&st.pool)
    .await?
    .ok_or_else(|| AppError::not_found("sondage introuvable"))?;
    let rows = sqlx::query(
        "SELECT answer_id, text FROM poll_answers WHERE message_id = ? ORDER BY answer_id",
    )
    .bind(mid)
    .fetch_all(&st.pool)
    .await?;
    let mut answers = Vec::with_capacity(rows.len());
    for a in rows {
        let aid: i64 = a.get("answer_id");
        let vote_count: i64 = sqlx::query(
            "SELECT COUNT(*) AS c FROM poll_votes WHERE message_id = ? AND answer_id = ?",
        )
        .bind(mid)
        .bind(aid)
        .fetch_one(&st.pool)
        .await?
        .get("c");
        let me_voted = sqlx::query(
            "SELECT 1 FROM poll_votes WHERE message_id = ? AND answer_id = ? AND user_id = ?",
        )
        .bind(mid)
        .bind(aid)
        .bind(viewer)
        .fetch_optional(&st.pool)
        .await?
        .is_some();
        answers.push(PollAnswer {
            answer_id: aid as i32,
            text: a.get("text"),
            vote_count,
            me_voted,
        });
    }
    let expires_at: Option<i64> = p.get("expires_at");
    Ok(Poll {
        message_id: Snowflake::from_i64(mid),
        channel_id: Snowflake::from_i64(p.get::<i64, _>("channel_id")),
        question: p.get("question"),
        multiselect: p.get::<i64, _>("multiselect") != 0,
        expires_at,
        finished: expires_at.map(|e| e < now_ms()).unwrap_or(false),
        answers,
    })
}

/// `POST /channels/:channel_id/polls` — crée un sondage (message porteur + réponses).
pub async fn create_poll(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
    Json(req): Json<CreatePoll>,
) -> AppResult<Json<Poll>> {
    let cid = parse_i64(&cid)?;
    pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::SEND_MESSAGES).await?;

    let question = req.question.trim().to_string();
    if question.is_empty() || question.chars().count() > 300 {
        return Err(AppError::bad_request(
            "question invalide (1 à 300 caractères)",
        ));
    }
    let answers: Vec<String> = req
        .answers
        .iter()
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .collect();
    if answers.is_empty() || answers.len() > MAX_ANSWERS {
        return Err(AppError::bad_request("entre 1 et 10 réponses"));
    }
    if answers.iter().any(|a| a.chars().count() > 55) {
        return Err(AppError::bad_request(
            "réponse trop longue (max 55 caractères)",
        ));
    }
    let dur = req.duration_hours.unwrap_or(24);
    let expires_at = if dur <= 0 {
        None
    } else {
        Some(now_ms() + dur.clamp(1, MAX_DURATION_HOURS) * 3600 * 1000)
    };

    // Message porteur du sondage (apparaît dans le salon, diffuse MESSAGE_CREATE).
    let msg = insert_text_message(&st, cid, user.id.as_i64(), &question).await?;
    let mid = msg.id.as_i64();
    sqlx::query(
        "INSERT INTO polls (message_id, channel_id, question, multiselect, expires_at, created_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(mid)
    .bind(cid)
    .bind(&question)
    .bind(req.multiselect as i64)
    .bind(expires_at)
    .bind(now_ms())
    .execute(&st.pool)
    .await?;
    for (i, text) in answers.iter().enumerate() {
        sqlx::query("INSERT INTO poll_answers (message_id, answer_id, text) VALUES (?, ?, ?)")
            .bind(mid)
            .bind((i + 1) as i64)
            .bind(text)
            .execute(&st.pool)
            .await?;
    }
    Ok(Json(build_poll(&st, mid, user.id.as_i64()).await?))
}

/// `GET /channels/:channel_id/polls/:message_id` — résultats d'un sondage.
pub async fn get_poll(
    State(st): State<AppState>,
    user: AuthUser,
    Path((cid, mid)): Path<(String, String)>,
) -> AppResult<Json<Poll>> {
    let cid = parse_i64(&cid)?;
    let mid = parse_i64(&mid)?;
    pg::require_channel_perm(
        &st.pool,
        cid,
        user.id.as_i64(),
        perms::VIEW_CHANNEL | perms::READ_MESSAGE_HISTORY,
    )
    .await?;
    // Le sondage doit appartenir à ce salon.
    let exists = sqlx::query("SELECT 1 FROM polls WHERE message_id = ? AND channel_id = ?")
        .bind(mid)
        .bind(cid)
        .fetch_optional(&st.pool)
        .await?
        .is_some();
    if !exists {
        return Err(AppError::not_found("sondage introuvable"));
    }
    Ok(Json(build_poll(&st, mid, user.id.as_i64()).await?))
}

/// `PUT /channels/:channel_id/polls/:message_id/votes` — (re)définit ses votes.
pub async fn cast_vote(
    State(st): State<AppState>,
    user: AuthUser,
    Path((cid, mid)): Path<(String, String)>,
    Json(req): Json<CastVote>,
) -> AppResult<Json<Poll>> {
    let cid = parse_i64(&cid)?;
    let mid = parse_i64(&mid)?;
    let me = user.id.as_i64();
    let (gid, _owner, _p) =
        pg::require_channel_perm(&st.pool, cid, me, perms::VIEW_CHANNEL).await?;

    let poll = sqlx::query(
        "SELECT multiselect, expires_at FROM polls WHERE message_id = ? AND channel_id = ?",
    )
    .bind(mid)
    .bind(cid)
    .fetch_optional(&st.pool)
    .await?
    .ok_or_else(|| AppError::not_found("sondage introuvable"))?;
    if let Some(exp) = poll.get::<Option<i64>, _>("expires_at") {
        if exp < now_ms() {
            return Err(AppError::forbidden("sondage terminé"));
        }
    }
    let multiselect = poll.get::<i64, _>("multiselect") != 0;

    // Réponses valides du sondage.
    let valid: Vec<i64> = sqlx::query("SELECT answer_id FROM poll_answers WHERE message_id = ?")
        .bind(mid)
        .fetch_all(&st.pool)
        .await?
        .into_iter()
        .map(|r| r.get::<i64, _>("answer_id"))
        .collect();
    let mut ids: Vec<i64> = req.answer_ids.iter().map(|a| *a as i64).collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.iter().any(|a| !valid.contains(a)) {
        return Err(AppError::bad_request("réponse de sondage invalide"));
    }
    if !multiselect && ids.len() > 1 {
        return Err(AppError::bad_request(
            "ce sondage n'autorise qu'une seule réponse",
        ));
    }

    // Remplace les votes de l'utilisateur.
    sqlx::query("DELETE FROM poll_votes WHERE message_id = ? AND user_id = ?")
        .bind(mid)
        .bind(me)
        .execute(&st.pool)
        .await?;
    for aid in &ids {
        sqlx::query("INSERT INTO poll_votes (message_id, answer_id, user_id) VALUES (?, ?, ?)")
            .bind(mid)
            .bind(aid)
            .bind(me)
            .execute(&st.pool)
            .await?;
    }

    st.publish(
        EventScope::channel(gid, cid),
        "MESSAGE_POLL_VOTE",
        json!({ "channel_id": cid.to_string(), "message_id": mid.to_string(), "user_id": me.to_string() }),
    );
    Ok(Json(build_poll(&st, mid, me).await?))
}
