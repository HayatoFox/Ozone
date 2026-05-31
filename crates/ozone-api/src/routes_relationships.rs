//! Relations entre utilisateurs : demandes d'ami, blocage et notes personnelles.
//! Cf. schéma DB : `relationships(user_id, target_id, type, since)` et
//! `user_notes(user_id, target_id, note)`.

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::state::{AppState, EventScope};
use crate::util::parse_i64;
use axum::extract::{Path, State};
use axum::Json;
use ozone_proto::dto::{AddRelationship, Relationship, RelationshipType, UpdateNote, User};
use ozone_proto::Snowflake;
use sqlx::Row;

// ─────────────────────────── Helpers privés ───────────────────────────

/// Insère ou met à jour une ligne dans `relationships`.
async fn upsert_relationship(
    st: &AppState,
    user_id: i64,
    target_id: i64,
    kind: &str,
    since: i64,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO relationships (user_id, target_id, type, since) VALUES (?, ?, ?, ?) \
         ON CONFLICT(user_id, target_id) DO UPDATE SET type = excluded.type, since = excluded.since",
    )
    .bind(user_id)
    .bind(target_id)
    .bind(kind)
    .bind(since)
    .execute(&st.pool)
    .await?;
    Ok(())
}

/// Gère l'envoi ou l'acceptation d'une demande d'ami entre `me` et `target`.
///
/// - Si `target` a bloqué `me` → `forbidden`.
/// - Si `me` a bloqué `target` → `forbidden`.
/// - Si relation existante == `friend` → no-op.
/// - Si relation existante == `incoming` → acceptation mutuelle (`friend` / `friend`).
/// - Sinon → envoi de la demande (`outgoing` / `incoming`).
async fn friend_request(st: &AppState, me: i64, target: i64) -> AppResult<serde_json::Value> {
    // Durcissement : pas d'auto-relation, et la cible doit exister
    // (couvre l'accès par identifiant, non validé par nom d'utilisateur).
    if me == target {
        return Err(AppError::bad_request("impossible avec soi-même"));
    }
    let target_exists = sqlx::query("SELECT 1 FROM users WHERE id = ?")
        .bind(target)
        .fetch_optional(&st.pool)
        .await?
        .is_some();
    if !target_exists {
        return Err(AppError::not_found("utilisateur introuvable"));
    }

    // Vérifie que `target` n'a pas bloqué `me`.
    let blocked_by_target = sqlx::query(
        "SELECT 1 FROM relationships WHERE user_id = ? AND target_id = ? AND type = 'blocked'",
    )
    .bind(target)
    .bind(me)
    .fetch_optional(&st.pool)
    .await?
    .is_some();
    if blocked_by_target {
        return Err(AppError::forbidden("cet utilisateur vous a bloqué"));
    }

    // Vérifie que `me` n'a pas bloqué `target`.
    let i_blocked = sqlx::query(
        "SELECT 1 FROM relationships WHERE user_id = ? AND target_id = ? AND type = 'blocked'",
    )
    .bind(me)
    .bind(target)
    .fetch_optional(&st.pool)
    .await?
    .is_some();
    if i_blocked {
        return Err(AppError::forbidden("vous avez bloqué cet utilisateur"));
    }

    // Relation existante côté `me` → `target`.
    let existing: Option<String> =
        sqlx::query("SELECT type FROM relationships WHERE user_id = ? AND target_id = ?")
            .bind(me)
            .bind(target)
            .fetch_optional(&st.pool)
            .await?
            .map(|r| r.get("type"));

    let now = now_ms();
    match existing.as_deref() {
        Some("friend") => {
            // Déjà amis, rien à faire.
        }
        Some("incoming") => {
            // L'autre avait déjà envoyé une demande : on accepte mutuellement.
            upsert_relationship(st, me, target, "friend", now).await?;
            upsert_relationship(st, target, me, "friend", now).await?;
            st.publish(
                EventScope::User(target),
                "RELATIONSHIP_ADD",
                serde_json::json!({ "user_id": me.to_string(), "type": "friend" }),
            );
            st.publish(
                EventScope::User(me),
                "RELATIONSHIP_ADD",
                serde_json::json!({ "user_id": target.to_string(), "type": "friend" }),
            );
        }
        _ => {
            // Nouvelle demande sortante.
            upsert_relationship(st, me, target, "outgoing", now).await?;
            upsert_relationship(st, target, me, "incoming", now).await?;
            // Notifie la cible (demande entrante) et ses propres sessions (sortante).
            st.publish(
                EventScope::User(target),
                "RELATIONSHIP_ADD",
                serde_json::json!({ "user_id": me.to_string(), "type": "incoming" }),
            );
            st.publish(
                EventScope::User(me),
                "RELATIONSHIP_ADD",
                serde_json::json!({ "user_id": target.to_string(), "type": "outgoing" }),
            );
        }
    }

    Ok(serde_json::json!({ "ok": true }))
}

// ──────────────────────── Handlers publics ────────────────────────

/// `GET /relationships` — liste toutes les relations de l'utilisateur courant.
pub async fn list_relationships(
    State(st): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<Relationship>>> {
    let me = user.id.as_i64();
    let rows = sqlx::query(
        "SELECT r.target_id, r.type, r.since, u.username, u.display_name, u.avatar_id \
         FROM relationships r \
         JOIN users u ON u.id = r.target_id \
         WHERE r.user_id = ? \
         ORDER BY r.since",
    )
    .bind(me)
    .fetch_all(&st.pool)
    .await?;

    let relationships = rows
        .into_iter()
        .map(|r| {
            let target_id: i64 = r.get("target_id");
            let kind_str: String = r.get("type");
            Relationship {
                id: Snowflake::from_i64(target_id),
                kind: RelationshipType::parse(&kind_str),
                user: User {
                    id: Snowflake::from_i64(target_id),
                    username: r.get("username"),
                    display_name: r.get("display_name"),
                    avatar_id: r.get("avatar_id"),
                    email: None,
                },
                since: r.get::<i64, _>("since") as u64,
            }
        })
        .collect();

    Ok(Json(relationships))
}

/// `POST /relationships` — envoie une demande d'ami ou bloque un utilisateur par nom.
pub async fn add_relationship(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<AddRelationship>,
) -> AppResult<Json<serde_json::Value>> {
    let me = user.id.as_i64();
    let normalized = req.username.trim().to_lowercase();

    // Recherche de la cible par nom d'utilisateur.
    let target_row = sqlx::query("SELECT id FROM users WHERE username = ?")
        .bind(&normalized)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("utilisateur introuvable"))?;
    let target: i64 = target_row.get("id");

    if target == me {
        return Err(AppError::bad_request("impossible avec soi-même"));
    }

    if req.block {
        // Blocage : upsert de la relation côté `me`, puis suppression de la relation inverse.
        let now = now_ms();
        upsert_relationship(&st, me, target, "blocked", now).await?;
        sqlx::query("DELETE FROM relationships WHERE user_id = ? AND target_id = ?")
            .bind(target)
            .bind(me)
            .execute(&st.pool)
            .await?;
        // Le blocage ne notifie PAS la cible (Discord ne révèle pas un blocage) : sync de soi seulement.
        st.publish(
            EventScope::User(me),
            "RELATIONSHIP_ADD",
            serde_json::json!({ "user_id": target.to_string(), "type": "blocked" }),
        );
        return Ok(Json(serde_json::json!({ "ok": true })));
    }

    // Demande d'ami.
    Ok(Json(friend_request(&st, me, target).await?))
}

/// `PUT /relationships/:user_id` — accepte une demande d'ami entrante (ou envoie si absente).
pub async fn accept_relationship(
    State(st): State<AppState>,
    user: AuthUser,
    Path(path): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let me = user.id.as_i64();
    let target = parse_i64(&path)?;
    Ok(Json(friend_request(&st, me, target).await?))
}

/// `DELETE /relationships/:user_id` — supprime une relation dans les deux sens.
pub async fn remove_relationship(
    State(st): State<AppState>,
    user: AuthUser,
    Path(path): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let me = user.id.as_i64();
    let target = parse_i64(&path)?;

    sqlx::query("DELETE FROM relationships WHERE user_id = ? AND target_id = ?")
        .bind(me)
        .bind(target)
        .execute(&st.pool)
        .await?;

    sqlx::query("DELETE FROM relationships WHERE user_id = ? AND target_id = ?")
        .bind(target)
        .bind(me)
        .execute(&st.pool)
        .await?;

    st.publish(
        EventScope::User(target),
        "RELATIONSHIP_REMOVE",
        serde_json::json!({ "user_id": me.to_string() }),
    );
    st.publish(
        EventScope::User(me),
        "RELATIONSHIP_REMOVE",
        serde_json::json!({ "user_id": target.to_string() }),
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `GET /users/:user_id/note` — lit la note personnelle sur un utilisateur.
///
/// Retourne `{ "note": "<texte>" }` ou `{ "note": null }` si aucune note n'existe.
pub async fn get_note(
    State(st): State<AppState>,
    user: AuthUser,
    Path(path): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let me = user.id.as_i64();
    let target = parse_i64(&path)?;

    let note: Option<String> =
        sqlx::query("SELECT note FROM user_notes WHERE user_id = ? AND target_id = ?")
            .bind(me)
            .bind(target)
            .fetch_optional(&st.pool)
            .await?
            .map(|r| r.get("note"));

    Ok(Json(serde_json::json!({ "note": note })))
}

/// `PUT /users/:user_id/note` — crée ou remplace la note personnelle sur un utilisateur.
///
/// La note est limitée à 256 caractères Unicode.
pub async fn put_note(
    State(st): State<AppState>,
    user: AuthUser,
    Path(path): Path<String>,
    Json(req): Json<UpdateNote>,
) -> AppResult<Json<serde_json::Value>> {
    let me = user.id.as_i64();
    let target = parse_i64(&path)?;

    if req.note.chars().count() > 256 {
        return Err(AppError::bad_request("note trop longue (max 256)"));
    }

    sqlx::query(
        "INSERT INTO user_notes (user_id, target_id, note) VALUES (?, ?, ?) \
         ON CONFLICT(user_id, target_id) DO UPDATE SET note = excluded.note",
    )
    .bind(me)
    .bind(target)
    .bind(&req.note)
    .execute(&st.pool)
    .await?;

    Ok(Json(serde_json::json!({ "ok": true })))
}
