//! Présence & statut : définir son statut, lister les présences d'une guilde.
//! Cf. docs/features/13-notifications.md. Registre en mémoire (`crate::presence`).

use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::permissions as pg;
use crate::presence::Registry;
use crate::state::AppState;
use crate::util::parse_i64;
use axum::extract::{Path, State};
use axum::Json;
use ozone_proto::dto::{PresenceView, SetPresence};
use ozone_proto::Snowflake;
use sqlx::Row;

/// `PUT /users/@me/presence` — définit son statut (et statut personnalisé).
pub async fn set_presence(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<SetPresence>,
) -> AppResult<Json<PresenceView>> {
    if !Registry::valid_status(&req.status) {
        return Err(AppError::bad_request(
            "statut invalide (online | idle | dnd | invisible)",
        ));
    }
    let uid = user.id.as_i64();
    // Sémantique à 3 états sur custom_status :
    //   None              → champ absent : PRÉSERVER le statut perso existant.
    //   Some(None)        → null explicite : EFFACER.
    //   Some(Some(texte)) → DÉFINIR (validé/borné, vide ⇒ effacer).
    let custom = match req.custom_status {
        None => st.presence.current_custom(uid),
        Some(None) => None,
        Some(Some(s)) => {
            let t = s.trim();
            if t.is_empty() {
                None
            } else if t.chars().count() > 128 {
                return Err(AppError::bad_request(
                    "statut personnalisé trop long (max 128)",
                ));
            } else {
                Some(t.to_string())
            }
        }
    };
    st.presence.set_status(uid, &req.status, custom.clone());
    // Diffuse le nouveau statut effectif aux guildes partagées.
    crate::gateway::broadcast_presence(&st, uid).await;
    Ok(Json(PresenceView {
        user_id: user.id,
        status: req.status,
        custom_status: custom,
    }))
}

/// `GET /guilds/:guild_id/presences` — présences (non hors-ligne) des membres de la guilde.
pub async fn list_presences(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
) -> AppResult<Json<Vec<PresenceView>>> {
    let gid = parse_i64(&gid)?;
    pg::require_guild_member(&st.pool, gid, user.id.as_i64()).await?;
    let members = sqlx::query("SELECT user_id FROM guild_members WHERE guild_id = ?")
        .bind(gid)
        .fetch_all(&st.pool)
        .await?;
    let mut out = Vec::new();
    for m in members {
        let uid: i64 = m.get("user_id");
        let (status, custom_status) = st.presence.effective(uid);
        if status != "offline" {
            out.push(PresenceView {
                user_id: Snowflake::from_i64(uid),
                status,
                custom_status,
            });
        }
    }
    Ok(Json(out))
}
