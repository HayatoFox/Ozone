//! Routes d'authentification : inscription, connexion, refresh, profil (`@me`).
//! Applique le **gate d'instance** et la **politique d'inscription**.

use crate::crypto;
use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::{AuthUser, ClientIp};
use crate::ratelimit;
use crate::state::AppState;
use axum::extract::State;
use axum::Json;
use ozone_proto::dto::{
    ChangeEmail, ChangePassword, DeleteAccount, LoginRequest, PreloginRequest, PreloginResponse,
    RefreshRequest, RegisterRequest, RegistrationPolicy, TokenPair, UpgradeEncryption, User,
};
use ozone_proto::Snowflake;
use sqlx::Row;

const ACCESS_TTL: u64 = 600; // 10 minutes
const REFRESH_TTL_MS: i64 = 30 * 24 * 3600 * 1000; // 30 jours
const MAX_SESSIONS_PER_USER: i64 = 10; // R9 — plafond d'appareils/sessions actives par compte

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
    // R9 — plafond de sessions par utilisateur : on conserve les MAX_SESSIONS plus récentes
    // (déconnecte les plus anciennes au-delà). Borne l'accumulation d'acteurs/sessions.
    // Tri départagé par `id DESC` (Snowflake monotone) : à `created_at` égal (reconnexions dans
    // la même milliseconde), la session qu'on vient d'émettre — la plus récente — reste toujours
    // dans le top N et n'est jamais supprimée juste après l'émission de son refresh token.
    sqlx::query(
        "DELETE FROM sessions WHERE user_id = ?1 AND id NOT IN \
         (SELECT id FROM sessions WHERE user_id = ?1 ORDER BY created_at DESC, id DESC LIMIT ?2)",
    )
    .bind(user_id.as_i64())
    .bind(MAX_SESSIONS_PER_USER)
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
    ClientIp(ip): ClientIp,
    Json(req): Json<RegisterRequest>,
) -> AppResult<Json<TokenPair>> {
    st.rate
        .check(ratelimit::REGISTER, &ip)
        .map_err(AppError::rate_limited)?;
    check_gate(&st, &req.gate_token)?;

    // Le tout premier compte (futur propriétaire) contourne la politique d'inscription.
    let is_first = sqlx::query("SELECT 1 FROM users LIMIT 1")
        .fetch_optional(&st.pool)
        .await?
        .is_none();
    // En politique « invite », l'invitation d'instance est validée puis consommée après création.
    let mut invite_to_consume: Option<String> = None;
    if !is_first {
        match st.instance.registration_policy {
            RegistrationPolicy::Closed => {
                return Err(AppError::forbidden(
                    "les inscriptions sont fermées sur cette instance",
                ))
            }
            RegistrationPolicy::Invite => {
                let code = req.invite_code.as_deref().unwrap_or("").trim().to_string();
                if code.is_empty() {
                    return Err(AppError::forbidden("code d'invitation d'instance requis"));
                }
                let row = sqlx::query(
                    "SELECT max_uses, uses, expires_at FROM instance_invites WHERE code = ?",
                )
                .bind(&code)
                .fetch_optional(&st.pool)
                .await?
                .ok_or_else(|| AppError::forbidden("invitation d'instance invalide"))?;
                if let Some(exp) = row.get::<Option<i64>, _>("expires_at") {
                    if exp < now_ms() {
                        return Err(AppError::forbidden("invitation d'instance expirée"));
                    }
                }
                // R5 — consommation ATOMIQUE : l'incrément conditionnel sert de verrou.
                // `WHERE … (max_uses = 0 OR uses < max_uses)` ⇒ deux inscriptions concurrentes
                // sur une invitation à usage unique ne peuvent pas toutes deux réussir (SQLite
                // sérialise les écritures ; `rows_affected == 0` signale l'épuisement).
                let claimed = sqlx::query(
                    "UPDATE instance_invites SET uses = uses + 1 \
                     WHERE code = ? AND (max_uses = 0 OR uses < max_uses)",
                )
                .bind(&code)
                .execute(&st.pool)
                .await?
                .rows_affected();
                if claimed == 0 {
                    return Err(AppError::forbidden("invitation d'instance épuisée"));
                }
                // Le slot est réservé ; on le rend si la création échoue plus bas.
                invite_to_consume = Some(code);
            }
            RegistrationPolicy::Open => {}
        }
    }

    // Rembourse le slot d'invitation réservé (R5) si l'inscription échoue après la réservation.
    let refund = |st: AppState, code: Option<String>| async move {
        if let Some(c) = code {
            let _ = sqlx::query("UPDATE instance_invites SET uses = MAX(0, uses - 1) WHERE code = ?")
                .bind(&c)
                .execute(&st.pool)
                .await;
        }
    };

    let username = req.username.trim().to_lowercase();
    if username.len() < 2 || username.len() > 32 {
        refund(st.clone(), invite_to_consume).await;
        return Err(AppError::bad_request("pseudo invalide (2 à 32 caractères)"));
    }
    // v2 (zero-knowledge) : `password` porte l'`authSecret` dérivé + dépôt de l'escrow chiffré.
    let is_v2 = req.public_key.is_some() && req.priv_wrapped.is_some();
    // R3 — politique de mot de passe : appliquée en legacy uniquement (en v2, le serveur ne voit que
    // l'authSecret ; la robustesse du VRAI mot de passe est vérifiée côté client avant dérivation).
    if !is_v2 {
        if let Err(e) = validate_password_strength(&req.password, &username) {
            refund(st.clone(), invite_to_consume).await;
            return Err(e);
        }
    } else {
        let pk = req.public_key.as_deref().unwrap_or("");
        let blob = req.priv_wrapped.as_deref().unwrap_or("");
        if pk.is_empty() || pk.len() > 1024 || blob.is_empty() || blob.len() > 8192 {
            refund(st.clone(), invite_to_consume).await;
            return Err(AppError::bad_request("matériel de chiffrement invalide"));
        }
    }
    let email = req.email.trim().to_lowercase();
    if !email.contains('@') {
        refund(st.clone(), invite_to_consume).await;
        return Err(AppError::bad_request("e-mail invalide"));
    }

    let exists = sqlx::query("SELECT 1 FROM users WHERE username = ? OR email = ?")
        .bind(&username)
        .bind(&email)
        .fetch_optional(&st.pool)
        .await?;
    if exists.is_some() {
        refund(st.clone(), invite_to_consume).await;
        return Err(AppError::conflict("pseudo ou e-mail déjà utilisé"));
    }

    let id = st.ids.next();
    // Création du compte. TOUTE erreur ici (hash, INSERT en collision sur une course concurrente,
    // rôle d'instance, émission des jetons) doit REMBOURSER le slot d'invitation réservé plus haut
    // (R5) — sinon une inscription échouée consommerait définitivement un usage.
    let outcome: AppResult<TokenPair> = async {
        let pw = crypto::hash_password(&req.password).map_err(AppError::internal)?;
        let scheme = if is_v2 { 2 } else { 1 };
        sqlx::query(
            "INSERT INTO users (id, username, display_name, email, password_hash, avatar_id, created_at, dm_public_key, dm_priv_wrapped, pw_scheme, kdf_salt) \
             VALUES (?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?)",
        )
        .bind(id.as_i64())
        .bind(&username)
        .bind(req.display_name.as_deref())
        .bind(&email)
        .bind(&pw)
        .bind(now_ms())
        .bind(req.public_key.as_deref())
        .bind(req.priv_wrapped.as_deref())
        .bind(scheme)
        .bind(req.kdf_salt.as_deref())
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
        issue_tokens(&st, id).await
    }
    .await;

    match outcome {
        Ok(tokens) => Ok(Json(tokens)),
        Err(e) => {
            refund(st.clone(), invite_to_consume).await;
            Err(e)
        }
    }
}

/// Politique de mot de passe (R3) : ≥ 8 caractères, pas dans la denylist des mots de passe
/// les plus courants, et ne contenant pas le pseudo. Sans dépendance externe (pas de zxcvbn/HIBP).
fn validate_password_strength(password: &str, username: &str) -> AppResult<()> {
    if password.len() < 8 {
        return Err(AppError::bad_request(
            "mot de passe trop court (8 caractères minimum)",
        ));
    }
    let lower = password.to_lowercase();
    const COMMON: &[&str] = &[
        "password", "motdepasse", "12345678", "123456789", "1234567890", "azerty123",
        "qwerty123", "11111111", "00000000", "iloveyou", "admin123", "letmein1", "password1",
        "azertyuiop", "qwertyuiop", "motdepasse1",
    ];
    if COMMON.contains(&lower.as_str()) {
        return Err(AppError::bad_request(
            "mot de passe trop courant — choisis-en un plus robuste",
        ));
    }
    if username.len() >= 3 && lower.contains(&username.to_lowercase()) {
        return Err(AppError::bad_request(
            "le mot de passe ne doit pas contenir ton pseudo",
        ));
    }
    Ok(())
}

/// `POST /auth/login`
pub async fn login(
    State(st): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(req): Json<LoginRequest>,
) -> AppResult<Json<TokenPair>> {
    st.rate
        .check(ratelimit::LOGIN, &ip)
        .map_err(AppError::rate_limited)?;
    check_gate(&st, &req.gate_token)?;
    let login = req.login.trim();
    let row = sqlx::query(
        "SELECT id, password_hash, suspended, deleted FROM users WHERE username = ? OR email = ?",
    )
    .bind(login.to_lowercase())
    .bind(login.to_lowercase())
    .fetch_optional(&st.pool)
    .await?;
    let Some(row) = row else {
        return Err(AppError::unauthorized("identifiants invalides"));
    };
    // Un compte supprimé n'existe plus (on ne révèle pas la suppression).
    if row.get::<i64, _>("deleted") != 0 {
        return Err(AppError::unauthorized("identifiants invalides"));
    }
    let hash: String = row.get("password_hash");
    if !crypto::verify_password(&req.password, &hash) {
        return Err(AppError::unauthorized("identifiants invalides"));
    }
    if row.get::<i64, _>("suspended") != 0 {
        return Err(AppError::forbidden("compte suspendu"));
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
    let row = sqlx::query(
        "SELECT s.id, s.user_id, s.expires_at, u.suspended \
         FROM sessions s JOIN users u ON u.id = s.user_id WHERE s.refresh_hash = ?",
    )
    .bind(&hash)
    .fetch_optional(&st.pool)
    .await?;
    let Some(row) = row else {
        return Err(AppError::unauthorized("refresh token invalide"));
    };
    let user_id = Snowflake::from_i64(row.get::<i64, _>("user_id"));
    // Un compte suspendu ne peut pas renouveler ses jetons : on révoque toutes ses sessions.
    if row.get::<i64, _>("suspended") != 0 {
        sqlx::query("DELETE FROM sessions WHERE user_id = ?")
            .bind(user_id.as_i64())
            .execute(&st.pool)
            .await?;
        return Err(AppError::forbidden("compte suspendu"));
    }
    let expires_at: i64 = row.get("expires_at");
    if expires_at < now_ms() {
        return Err(AppError::unauthorized("session expirée"));
    }
    let sid: i64 = row.get("id");
    sqlx::query("DELETE FROM sessions WHERE id = ?")
        .bind(sid)
        .execute(&st.pool)
        .await?;
    Ok(Json(issue_tokens(&st, user_id).await?))
}

/// `GET /users/@me`
pub async fn me(State(st): State<AppState>, user: AuthUser) -> AppResult<Json<User>> {
    let row = sqlx::query(
        "SELECT id, username, display_name, avatar_id, email, name_style FROM users WHERE id = ?",
    )
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
        name_style: crate::util::parse_name_style(row.get("name_style")),
    }))
}

/// `PATCH /users/@me/password` — change le mot de passe (ré-auth requise) et révoque les sessions.
pub async fn change_password(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<ChangePassword>,
) -> AppResult<Json<serde_json::Value>> {
    let uid = user.id.as_i64();
    let row = sqlx::query("SELECT password_hash, pw_scheme FROM users WHERE id = ?")
        .bind(uid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("utilisateur introuvable"))?;
    let hash: String = row.get("password_hash");
    let scheme: i64 = row.get("pw_scheme");
    if !crypto::verify_password(&req.current_password, &hash) {
        return Err(AppError::unauthorized("mot de passe actuel invalide"));
    }
    // En v2, `new_password` est l'authSecret (robustesse vérifiée côté client) ; sinon politique R3.
    if scheme != 2 {
        validate_password_strength(&req.new_password, "")?;
    }
    let new_hash = crypto::hash_password(&req.new_password).map_err(AppError::internal)?;
    // Re-emballe l'escrow avec la NOUVELLE KEK (sans quoi la clé privée deviendrait indéchiffrable).
    match req.priv_wrapped.as_deref() {
        Some(blob) => {
            if blob.is_empty() || blob.len() > 8192 {
                return Err(AppError::bad_request("clé emballée invalide"));
            }
            sqlx::query("UPDATE users SET password_hash = ?, dm_priv_wrapped = ? WHERE id = ?")
                .bind(&new_hash)
                .bind(blob)
                .bind(uid)
                .execute(&st.pool)
                .await?;
        }
        None => {
            sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
                .bind(&new_hash)
                .bind(uid)
                .execute(&st.pool)
                .await?;
        }
    }
    // Sécurité : révoque toutes les sessions (refresh tokens) — reconnexion requise partout.
    sqlx::query("DELETE FROM sessions WHERE user_id = ?")
        .bind(uid)
        .execute(&st.pool)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `POST /users/@me/encryption/upgrade` — bascule un compte legacy (v1) vers le schéma zero-knowledge
/// (v2). On prouve la possession du mot de passe BRUT actuel une dernière fois, puis le serveur ne
/// stocke plus que `Argon2(authSecret)` et l'escrow chiffré (la session courante reste valide).
pub async fn upgrade_encryption(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<UpgradeEncryption>,
) -> AppResult<Json<serde_json::Value>> {
    let uid = user.id.as_i64();
    let row = sqlx::query("SELECT password_hash, pw_scheme FROM users WHERE id = ?")
        .bind(uid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("utilisateur introuvable"))?;
    let hash: String = row.get("password_hash");
    if row.get::<i64, _>("pw_scheme") == 2 {
        return Err(AppError::bad_request("chiffrement déjà migré"));
    }
    if !crypto::verify_password(&req.current_password, &hash) {
        return Err(AppError::unauthorized("mot de passe invalide"));
    }
    if req.public_key.is_empty()
        || req.public_key.len() > 1024
        || req.priv_wrapped.is_empty()
        || req.priv_wrapped.len() > 8192
        || req.auth_secret.is_empty()
        || req.auth_secret.len() > 1024
        || req.kdf_salt.is_empty()
        || req.kdf_salt.len() > 128
    {
        return Err(AppError::bad_request("matériel de chiffrement invalide"));
    }
    let new_hash = crypto::hash_password(&req.auth_secret).map_err(AppError::internal)?;
    sqlx::query(
        "UPDATE users SET password_hash = ?, pw_scheme = 2, dm_public_key = ?, dm_priv_wrapped = ?, kdf_salt = ? WHERE id = ?",
    )
    .bind(&new_hash)
    .bind(&req.public_key)
    .bind(&req.priv_wrapped)
    .bind(&req.kdf_salt)
    .bind(uid)
    .execute(&st.pool)
    .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `POST /auth/prelogin` — restitue le sel KDF + le schéma pour un identifiant, AVANT le login, afin
/// que le client dérive `authSecret`/KEK (login par e-mail OU pseudo). Anti-énumération : pour un
/// compte inexistant (ou v1 sans sel), renvoie un sel DÉTERMINISTE factice (HMAC-like via le secret
/// JWT) indistinguable d'un vrai sel, avec `pw_scheme = 2`.
pub async fn prelogin(
    State(st): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(req): Json<PreloginRequest>,
) -> AppResult<Json<PreloginResponse>> {
    st.rate
        .check(ratelimit::LOGIN, &ip)
        .map_err(AppError::rate_limited)?;
    let login = req.login.trim().to_lowercase();
    let row = sqlx::query("SELECT pw_scheme, kdf_salt FROM users WHERE username = ? OR email = ?")
        .bind(&login)
        .bind(&login)
        .fetch_optional(&st.pool)
        .await?;
    // Sel factice déterministe (compte inexistant ou v1) : SHA-256(secret JWT | login) en hex (64
    // car.), indistinguable d'un vrai sel client (32 octets aléatoires → hex). Lié au secret serveur
    // ⇒ non précalculable par un attaquant, donc pas d'énumération.
    let fake_salt = || {
        let secret = String::from_utf8_lossy(st.jwt_secret.as_ref());
        crypto::sha256_hex(&format!("{secret}|prelogin|{login}"))
    };
    let (scheme, salt) = match row {
        Some(r) => {
            let scheme = r.get::<i64, _>("pw_scheme") as u8;
            let salt: Option<String> = r.get("kdf_salt");
            (scheme, salt.unwrap_or_else(fake_salt))
        }
        None => (2, fake_salt()),
    };
    Ok(Json(PreloginResponse {
        kdf_salt: salt,
        pw_scheme: scheme,
    }))
}

/// `PATCH /users/@me/email` — change l'e-mail (ré-auth requise, unicité vérifiée).
pub async fn change_email(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<ChangeEmail>,
) -> AppResult<Json<User>> {
    let uid = user.id.as_i64();
    let hash: String = sqlx::query("SELECT password_hash FROM users WHERE id = ?")
        .bind(uid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("utilisateur introuvable"))?
        .get("password_hash");
    if !crypto::verify_password(&req.password, &hash) {
        return Err(AppError::unauthorized("mot de passe invalide"));
    }
    let email = req.new_email.trim().to_lowercase();
    if !email.contains('@') || email.len() > 254 {
        return Err(AppError::bad_request("e-mail invalide"));
    }
    let taken = sqlx::query("SELECT 1 FROM users WHERE email = ? AND id <> ?")
        .bind(&email)
        .bind(uid)
        .fetch_optional(&st.pool)
        .await?
        .is_some();
    if taken {
        return Err(AppError::conflict("e-mail déjà utilisé"));
    }
    sqlx::query("UPDATE users SET email = ? WHERE id = ?")
        .bind(&email)
        .bind(uid)
        .execute(&st.pool)
        .await?;
    let row = sqlx::query(
        "SELECT id, username, display_name, avatar_id, email, name_style FROM users WHERE id = ?",
    )
    .bind(uid)
    .fetch_one(&st.pool)
    .await?;
    Ok(Json(User {
        id: Snowflake::from_i64(row.get::<i64, _>("id")),
        username: row.get("username"),
        display_name: row.get("display_name"),
        avatar_id: row.get("avatar_id"),
        email: Some(row.get("email")),
        name_style: crate::util::parse_name_style(row.get("name_style")),
    }))
}

/// `DELETE /users/@me` — supprime (anonymise) son propre compte (ré-auth requise).
/// La ligne `users` est conservée mais vidée de toute donnée personnelle : les messages publiés
/// restent attribués à un « utilisateur supprimé ». Impossible si l'on possède encore des guildes.
pub async fn delete_account(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<DeleteAccount>,
) -> AppResult<Json<serde_json::Value>> {
    let uid = user.id.as_i64();
    let hash: String = sqlx::query("SELECT password_hash FROM users WHERE id = ?")
        .bind(uid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("utilisateur introuvable"))?
        .get("password_hash");
    if !crypto::verify_password(&req.password, &hash) {
        return Err(AppError::unauthorized("mot de passe invalide"));
    }
    let owned: i64 = sqlx::query("SELECT COUNT(*) AS c FROM guilds WHERE owner_id = ?")
        .bind(uid)
        .fetch_one(&st.pool)
        .await?
        .get("c");
    if owned > 0 {
        return Err(AppError::bad_request(
            "supprimez ou transférez vos guildes avant de supprimer votre compte",
        ));
    }

    let mut tx = st.pool.begin().await?;
    for sql in [
        "DELETE FROM sessions WHERE user_id = ?",
        "DELETE FROM relationships WHERE user_id = ? OR target_id = ?",
        "DELETE FROM user_notes WHERE user_id = ? OR target_id = ?",
        "DELETE FROM read_states WHERE user_id = ?",
        "DELETE FROM notification_settings WHERE user_id = ?",
        "DELETE FROM instance_roles WHERE user_id = ?",
        "DELETE FROM voice_states WHERE user_id = ?",
        "DELETE FROM mentions WHERE user_id = ?",
        "DELETE FROM dm_recipients WHERE user_id = ?",
        "DELETE FROM member_roles WHERE user_id = ?",
        "DELETE FROM guild_members WHERE user_id = ?",
        "DELETE FROM user_settings WHERE user_id = ?",
        "DELETE FROM poll_votes WHERE user_id = ?",
        "DELETE FROM reactions WHERE user_id = ?",
        "DELETE FROM attachments WHERE uploader_id = ? AND message_id IS NULL",
    ] {
        let binds = sql.matches('?').count();
        let mut q = sqlx::query(sql);
        for _ in 0..binds {
            q = q.bind(uid);
        }
        q.execute(&mut *tx).await?;
    }
    // Anonymise la ligne (conservée pour l'attribution des messages). Mot de passe rendu inutilisable.
    sqlx::query(
        "UPDATE users SET username = ?, email = ?, display_name = NULL, avatar_id = NULL, \
         bio = NULL, pronouns = NULL, banner_id = NULL, accent_color = NULL, \
         password_hash = 'DELETED', deleted = 1 WHERE id = ?",
    )
    .bind(format!("deleted_{uid}"))
    .bind(format!("deleted_{uid}@deleted.invalid"))
    .bind(uid)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
