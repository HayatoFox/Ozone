//! Routes d'authentification : inscription, connexion, refresh, profil (`@me`).
//! Applique le **gate d'instance** et la **politique d'inscription**.

use crate::crypto;
use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::state::AppState;
use axum::extract::State;
use axum::Json;
use ozone_proto::dto::{
    LoginRequest, RefreshRequest, RegisterRequest, RegistrationPolicy, TokenPair, User,
};
use ozone_proto::Snowflake;
use sqlx::Row;

const ACCESS_TTL: u64 = 600; // 10 minutes
const REFRESH_TTL_MS: i64 = 30 * 24 * 3600 * 1000; // 30 jours

fn check_gate(st: &AppState, gate_token: &Option<String>) -> AppResult<()> {
    if st.instance.gate_enabled {
        let ok = gate_token
            .as_deref()
            .and_then(|t| crypto::jwt_verify(&st.jwt_secret, t, "gate"))
            .is_some();
        if !ok {
            return Err(AppError::unauthorized(
                "jeton de gate d'instance requis ou invalide",
            ));
        }
    }
    Ok(())
}

async fn issue_tokens(st: &AppState, user_id: Snowflake) -> AppResult<TokenPair> {
    let access = crypto::jwt_encode(&st.jwt_secret, &user_id.to_string(), "access", ACCESS_TTL);
    let refresh = crypto::random_token();
    let refresh_hash = crypto::sha256_hex(&refresh);
    let sid = st.ids.next();
    let created = now_ms();
    sqlx::query(
        "INSERT INTO sessions (id, user_id, refresh_hash, device, created_at, expires_at) VALUES (?, ?, ?, NULL, ?, ?)",
    )
    .bind(sid.as_i64())
    .bind(user_id.as_i64())
    .bind(&refresh_hash)
    .bind(created)
    .bind(created + REFRESH_TTL_MS)
    .execute(&st.pool)
    .await?;
    Ok(TokenPair {
        access_token: access,
        refresh_token: refresh,
        token_type: "Bearer".into(),
        expires_in: ACCESS_TTL,
    })
}

/// `POST /auth/register`
pub async fn register(
    State(st): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> AppResult<Json<TokenPair>> {
    check_gate(&st, &req.gate_token)?;

    match st.instance.registration_policy {
        RegistrationPolicy::Closed => {
            return Err(AppError::forbidden(
                "les inscriptions sont fermées sur cette instance",
            ))
        }
        RegistrationPolicy::Invite => {
            if req.invite_code.as_deref().unwrap_or("").is_empty() {
                return Err(AppError::forbidden("code d'invitation d'instance requis"));
            }
            // TODO(phase 5) : valider le code contre instance_invites.
        }
        RegistrationPolicy::Open => {}
    }

    let username = req.username.trim().to_lowercase();
    if username.len() < 2 || username.len() > 32 {
        return Err(AppError::bad_request("pseudo invalide (2 à 32 caractères)"));
    }
    if req.password.len() < 8 {
        return Err(AppError::bad_request(
            "mot de passe trop court (8 caractères minimum)",
        ));
    }
    let email = req.email.trim().to_lowercase();
    if !email.contains('@') {
        return Err(AppError::bad_request("e-mail invalide"));
    }

    let exists = sqlx::query("SELECT 1 FROM users WHERE username = ? OR email = ?")
        .bind(&username)
        .bind(&email)
        .fetch_optional(&st.pool)
        .await?;
    if exists.is_some() {
        return Err(AppError::conflict("pseudo ou e-mail déjà utilisé"));
    }

    let id = st.ids.next();
    let pw = crypto::hash_password(&req.password).map_err(AppError::internal)?;
    sqlx::query(
        "INSERT INTO users (id, username, display_name, email, password_hash, avatar_id, created_at) VALUES (?, ?, ?, ?, ?, NULL, ?)",
    )
    .bind(id.as_i64())
    .bind(&username)
    .bind(req.display_name.as_deref())
    .bind(&email)
    .bind(&pw)
    .bind(now_ms())
    .execute(&st.pool)
    .await?;

    // Le premier compte devient propriétaire de l'instance (bootstrap).
    let count: i64 = sqlx::query("SELECT COUNT(*) AS c FROM users")
        .fetch_one(&st.pool)
        .await?
        .get("c");
    let role = if count == 1 { "owner" } else { "user" };
    sqlx::query("INSERT INTO instance_roles (user_id, role) VALUES (?, ?)")
        .bind(id.as_i64())
        .bind(role)
        .execute(&st.pool)
        .await?;
    if role == "owner" {
        tracing::info!("Compte propriétaire de l'instance créé : « {} »", username);
    }

    Ok(Json(issue_tokens(&st, id).await?))
}

/// `POST /auth/login`
pub async fn login(
    State(st): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> AppResult<Json<TokenPair>> {
    check_gate(&st, &req.gate_token)?;
    let login = req.login.trim();
    let row = sqlx::query("SELECT id, password_hash FROM users WHERE username = ? OR email = ?")
        .bind(login.to_lowercase())
        .bind(login.to_lowercase())
        .fetch_optional(&st.pool)
        .await?;
    let Some(row) = row else {
        return Err(AppError::unauthorized("identifiants invalides"));
    };
    let hash: String = row.get("password_hash");
    if !crypto::verify_password(&req.password, &hash) {
        return Err(AppError::unauthorized("identifiants invalides"));
    }
    let id = Snowflake::from_i64(row.get::<i64, _>("id"));
    Ok(Json(issue_tokens(&st, id).await?))
}

/// `POST /auth/token/refresh` — rotation du refresh token.
pub async fn refresh(
    State(st): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> AppResult<Json<TokenPair>> {
    let hash = crypto::sha256_hex(&req.refresh_token);
    let row = sqlx::query("SELECT id, user_id, expires_at FROM sessions WHERE refresh_hash = ?")
        .bind(&hash)
        .fetch_optional(&st.pool)
        .await?;
    let Some(row) = row else {
        return Err(AppError::unauthorized("refresh token invalide"));
    };
    let expires_at: i64 = row.get("expires_at");
    if expires_at < now_ms() {
        return Err(AppError::unauthorized("session expirée"));
    }
    let sid: i64 = row.get("id");
    let user_id = Snowflake::from_i64(row.get::<i64, _>("user_id"));
    sqlx::query("DELETE FROM sessions WHERE id = ?")
        .bind(sid)
        .execute(&st.pool)
        .await?;
    Ok(Json(issue_tokens(&st, user_id).await?))
}

/// `GET /users/@me`
pub async fn me(State(st): State<AppState>, user: AuthUser) -> AppResult<Json<User>> {
    let row =
        sqlx::query("SELECT id, username, display_name, avatar_id, email FROM users WHERE id = ?")
            .bind(user.id.as_i64())
            .fetch_optional(&st.pool)
            .await?
            .ok_or_else(|| AppError::not_found("utilisateur introuvable"))?;
    Ok(Json(User {
        id: Snowflake::from_i64(row.get::<i64, _>("id")),
        username: row.get("username"),
        display_name: row.get("display_name"),
        avatar_id: row.get("avatar_id"),
        email: Some(row.get("email")),
    }))
}
