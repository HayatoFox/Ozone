//! Emojis personnalisés d'une guilde (liste, création, modification, suppression).
//! Cf. `docs/features/11-expressions.md`.

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::permissions as pg;
use crate::state::{AppState, EventScope};
use crate::util::parse_i64;
use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::header;
use axum::response::Response;
use axum::Json;
use ozone_proto::dto::{CreateEmoji, Emoji, UpdateEmoji};
use ozone_proto::{perms, Snowflake};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

/// Taille maximale d'une image d'emoji (512 Kio).
const MAX_EMOJI_SIZE: usize = 512 * 1024;

/// Taille maximale d'une image d'autocollant (sticker) — 2 Mio.
const MAX_STICKER_SIZE: usize = 2 * 1024 * 1024;

/// Détecte le type MIME d'une image par ses octets magiques (PNG/GIF/WEBP/JPEG).
fn detect_image_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        Some("image/png")
    } else if bytes.starts_with(b"GIF8") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else {
        None
    }
}

/// En-tête `Content-Type` d'une image servie, restreint au jeu de types connu (défense en
/// profondeur) ; tout le reste retombe sur `application/octet-stream`.
fn image_content_type(ctype: &str) -> header::HeaderValue {
    header::HeaderValue::from_static(match ctype {
        "image/png" => "image/png",
        "image/gif" => "image/gif",
        "image/webp" => "image/webp",
        "image/jpeg" => "image/jpeg",
        _ => "application/octet-stream",
    })
}

/// Écrit `data` dans un nouveau fichier d'upload (nom = id Snowflake) et renvoie l'id généré.
async fn store_upload(st: &AppState, data: &[u8]) -> AppResult<Snowflake> {
    let id = st.ids.next();
    let path = st.upload_dir.join(id.as_i64().to_string());
    tokio::fs::write(&path, data)
        .await
        .map_err(|e| AppError::internal(format!("écriture du fichier : {e}")))?;
    Ok(id)
}

// ───────────────────────── Helpers privés ─────────────────────────

/// Convertit une ligne SQLite en DTO [`Emoji`].
fn row_to_emoji(r: SqliteRow) -> Emoji {
    Emoji {
        id: Snowflake::from_i64(r.get::<i64, _>("id")),
        guild_id: Snowflake::from_i64(r.get::<i64, _>("guild_id")),
        name: r.get("name"),
        animated: r.get::<i64, _>("animated") != 0,
        image_id: r.get("image_id"),
        created_by: Snowflake::from_i64(r.get::<i64, _>("created_by")),
        available: r.get::<i64, _>("available") != 0,
    }
}

/// Charge un emoji par `(guild_id, emoji_id)` ; renvoie 404 s'il est absent.
async fn fetch_emoji(st: &AppState, gid: i64, eid: i64) -> AppResult<SqliteRow> {
    sqlx::query(
        "SELECT id, guild_id, name, animated, image_id, created_by, available \
         FROM emojis WHERE id = ? AND guild_id = ?",
    )
    .bind(eid)
    .bind(gid)
    .fetch_optional(&st.pool)
    .await?
    .ok_or_else(|| AppError::not_found("emoji introuvable"))
}

/// Valide le nom d'un emoji : longueur 2–32 et caractères `[A-Za-z0-9_]` uniquement.
fn validate_name(name: &str) -> AppResult<()> {
    let len = name.len();
    if !(2..=32).contains(&len) {
        return Err(AppError::bad_request(
            "le nom de l'emoji doit faire entre 2 et 32 caractères",
        ));
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(AppError::bad_request(
            "le nom de l'emoji ne peut contenir que des lettres, chiffres et tirets bas",
        ));
    }
    Ok(())
}

// ───────────────────────── Handlers publics ─────────────────────────

/// `GET /guilds/:guild_id/emojis` — Liste tous les emojis d'une guilde.
///
/// Exige d'être membre (`VIEW_CHANNEL`).
pub async fn list_emojis(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
) -> AppResult<Json<Vec<Emoji>>> {
    let gid = parse_i64(&gid)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::VIEW_CHANNEL).await?;
    let rows = sqlx::query(
        "SELECT id, guild_id, name, animated, image_id, created_by, available \
         FROM emojis WHERE guild_id = ? ORDER BY id",
    )
    .bind(gid)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(rows.into_iter().map(row_to_emoji).collect()))
}

/// `POST /guilds/:guild_id/emojis` — Crée un nouvel emoji dans la guilde.
///
/// Exige `CREATE_GUILD_EXPRESSIONS` ou `MANAGE_GUILD_EXPRESSIONS`.
/// Le nom est normalisé (trim) puis validé.
pub async fn create_emoji(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
    Json(req): Json<CreateEmoji>,
) -> AppResult<Json<Emoji>> {
    let gid = parse_i64(&gid)?;
    let uid = user.id.as_i64();
    pg::require_expression_create(&st.pool, gid, uid).await?;

    let name = req.name.trim().to_string();
    validate_name(&name)?;

    if req.image_id.is_empty() {
        return Err(AppError::bad_request("image_id ne peut pas être vide"));
    }

    let id = st.ids.next();
    let animated = req.animated as i64;
    let now = now_ms();

    sqlx::query(
        "INSERT INTO emojis (id, guild_id, name, animated, image_id, created_by, available, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, 1, ?)",
    )
    .bind(id.as_i64())
    .bind(gid)
    .bind(&name)
    .bind(animated)
    .bind(&req.image_id)
    .bind(uid)
    .bind(now)
    .execute(&st.pool)
    .await?;

    // Les autres clients rafraîchissent leur picker en direct.
    st.publish(
        EventScope::Guild(gid),
        "GUILD_EMOJIS_UPDATE",
        serde_json::json!({ "guild_id": gid.to_string() }),
    );

    Ok(Json(Emoji {
        id,
        guild_id: Snowflake::from_i64(gid),
        name,
        animated: req.animated,
        image_id: req.image_id,
        created_by: Snowflake::from_i64(uid),
        available: true,
    }))
}

/// `PATCH /guilds/:guild_id/emojis/:emoji_id` — Modifie le nom d'un emoji.
///
/// Exige `MANAGE_GUILD_EXPRESSIONS`, ou `CREATE_GUILD_EXPRESSIONS` si l'auteur
/// est l'utilisateur courant.
pub async fn update_emoji(
    State(st): State<AppState>,
    user: AuthUser,
    Path((gid, eid)): Path<(String, String)>,
    Json(req): Json<UpdateEmoji>,
) -> AppResult<Json<Emoji>> {
    let gid = parse_i64(&gid)?;
    let eid = parse_i64(&eid)?;
    let uid = user.id.as_i64();

    let row = fetch_emoji(&st, gid, eid).await?;
    let created_by: i64 = row.get("created_by");
    pg::require_expression_manage(&st.pool, gid, uid, created_by).await?;

    if let Some(new_name) = &req.name {
        let name = new_name.trim().to_string();
        validate_name(&name)?;
        sqlx::query("UPDATE emojis SET name = ? WHERE id = ? AND guild_id = ?")
            .bind(&name)
            .bind(eid)
            .bind(gid)
            .execute(&st.pool)
            .await?;
        st.publish(
            EventScope::Guild(gid),
            "GUILD_EMOJIS_UPDATE",
            serde_json::json!({ "guild_id": gid.to_string() }),
        );
    }

    Ok(Json(row_to_emoji(fetch_emoji(&st, gid, eid).await?)))
}

/// `DELETE /guilds/:guild_id/emojis/:emoji_id` — Supprime un emoji.
///
/// Exige `MANAGE_GUILD_EXPRESSIONS`, ou `CREATE_GUILD_EXPRESSIONS` si l'auteur
/// est l'utilisateur courant.
pub async fn delete_emoji(
    State(st): State<AppState>,
    user: AuthUser,
    Path((gid, eid)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let gid = parse_i64(&gid)?;
    let eid = parse_i64(&eid)?;
    let uid = user.id.as_i64();

    let row = fetch_emoji(&st, gid, eid).await?;
    let created_by: i64 = row.get("created_by");
    pg::require_expression_manage(&st.pool, gid, uid, created_by).await?;

    sqlx::query("DELETE FROM emojis WHERE id = ? AND guild_id = ?")
        .bind(eid)
        .bind(gid)
        .execute(&st.pool)
        .await?;

    st.publish(
        EventScope::Guild(gid),
        "GUILD_EMOJIS_UPDATE",
        serde_json::json!({ "guild_id": gid.to_string() }),
    );

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `POST /guilds/:guild_id/emojis/image` — téléverse l'image d'un emoji (champ multipart `file`).
/// Renvoie `{ image_id }` à passer ensuite à `create_emoji`. Exige le droit de créer une expression.
pub async fn upload_emoji_image(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
    mut multipart: Multipart,
) -> AppResult<Json<serde_json::Value>> {
    let gid = parse_i64(&gid)?;
    pg::require_expression_create(&st.pool, gid, user.id.as_i64()).await?;

    let mut data = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::bad_request("requête multipart invalide"))?
    {
        if field.file_name().is_some() {
            data = Some(
                field
                    .bytes()
                    .await
                    .map_err(|_| AppError::bad_request("lecture du fichier échouée"))?,
            );
            break;
        }
    }
    let data = data.ok_or_else(|| AppError::bad_request("aucun fichier fourni"))?;
    if data.is_empty() {
        return Err(AppError::bad_request("fichier vide"));
    }
    if data.len() > MAX_EMOJI_SIZE {
        return Err(AppError::bad_request("image trop volumineuse (max 512 Kio)"));
    }
    // N'accepte que de vraies images (vérifiées par octets magiques).
    if detect_image_type(&data).is_none() {
        return Err(AppError::bad_request("format d'image non supporté (png/gif/webp/jpeg)"));
    }

    let id = store_upload(&st, &data).await?;

    Ok(Json(serde_json::json!({ "image_id": id.to_string() })))
}

/// `POST /guilds/:guild_id/stickers/image` — téléverse l'image d'un autocollant (champ multipart
/// `file`). Identique à l'upload d'emoji mais avec sa propre limite (2 Mio) : les stickers sont plus
/// grands. Renvoie `{ image_id }` à passer ensuite à `create_sticker` (champ `asset_id`).
pub async fn upload_sticker_image(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
    mut multipart: Multipart,
) -> AppResult<Json<serde_json::Value>> {
    let gid = parse_i64(&gid)?;
    pg::require_expression_create(&st.pool, gid, user.id.as_i64()).await?;

    let mut data = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::bad_request("requête multipart invalide"))?
    {
        if field.file_name().is_some() {
            data = Some(
                field
                    .bytes()
                    .await
                    .map_err(|_| AppError::bad_request("lecture du fichier échouée"))?,
            );
            break;
        }
    }
    let data = data.ok_or_else(|| AppError::bad_request("aucun fichier fourni"))?;
    if data.is_empty() {
        return Err(AppError::bad_request("fichier vide"));
    }
    if data.len() > MAX_STICKER_SIZE {
        return Err(AppError::bad_request("image trop volumineuse (max 2 Mio)"));
    }
    if detect_image_type(&data).is_none() {
        return Err(AppError::bad_request("format d'image non supporté (png/gif/webp/jpeg)"));
    }

    let id = store_upload(&st, &data).await?;

    Ok(Json(serde_json::json!({ "image_id": id.to_string() })))
}

/// `GET /emojis/:emoji_id` — sert l'image d'un emoji (**public**, décoratif ; type détecté par
/// octets magiques, `nosniff` + CSP neutralisante pour interdire toute exécution).
pub async fn serve_emoji(State(st): State<AppState>, Path(eid): Path<String>) -> AppResult<Response> {
    let eid = parse_i64(&eid)?;
    let image_id: String = sqlx::query("SELECT image_id FROM emojis WHERE id = ? AND available = 1")
        .bind(eid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("emoji introuvable"))?
        .get("image_id");
    // L'image est localisée par identifiant numérique uniquement (pas de traversée de chemin).
    let file_id = image_id
        .parse::<i64>()
        .map_err(|_| AppError::not_found("image d'emoji introuvable"))?;
    let path = st.upload_dir.join(file_id.to_string());
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| AppError::not_found("image d'emoji introuvable"))?;
    let ctype = detect_image_type(&bytes).unwrap_or("application/octet-stream");

    let mut resp = Response::new(Body::from(bytes));
    let h = resp.headers_mut();
    h.insert(header::CONTENT_TYPE, image_content_type(ctype));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        header::HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        header::HeaderValue::from_static("default-src 'none'; sandbox"),
    );
    h.insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("public, max-age=86400, immutable"),
    );
    Ok(resp)
}

// ───────────────────────── Images de guilde (icône / bannière) ─────────────────────────

/// Taille maximale d'une icône / bannière de serveur (2 Mio).
const MAX_GUILD_IMAGE: usize = 2 * 1024 * 1024;

/// Sert une image stockée par son `image_id` numérique (publique, décorative). L'identifiant
/// est purement numérique ⇒ aucune traversée de chemin ; `nosniff` + CSP neutralisante.
async fn serve_stored_image(st: &AppState, image_id: Option<String>) -> AppResult<Response> {
    let image_id = image_id.ok_or_else(|| AppError::not_found("image absente"))?;
    let file_id = image_id
        .parse::<i64>()
        .map_err(|_| AppError::not_found("image introuvable"))?;
    let path = st.upload_dir.join(file_id.to_string());
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| AppError::not_found("image introuvable"))?;
    let ctype = detect_image_type(&bytes).unwrap_or("application/octet-stream");
    let mut resp = Response::new(Body::from(bytes));
    let h = resp.headers_mut();
    h.insert(header::CONTENT_TYPE, image_content_type(ctype));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        header::HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        header::HeaderValue::from_static("default-src 'none'; sandbox"),
    );
    h.insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("public, max-age=3600"),
    );
    Ok(resp)
}

/// `POST /guilds/:guild_id/images` — téléverse une image d'icône/bannière (champ multipart `file`).
/// Renvoie `{ image_id }` à passer ensuite à `update_guild` (icon_id / banner_id). MANAGE_GUILD requis.
pub async fn upload_guild_image(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
    mut multipart: Multipart,
) -> AppResult<Json<serde_json::Value>> {
    let gid = parse_i64(&gid)?;
    pg::require_guild_perm(&st.pool, gid, user.id.as_i64(), perms::MANAGE_GUILD).await?;

    let mut data = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::bad_request("requête multipart invalide"))?
    {
        if field.file_name().is_some() {
            data = Some(
                field
                    .bytes()
                    .await
                    .map_err(|_| AppError::bad_request("lecture du fichier échouée"))?,
            );
            break;
        }
    }
    let data = data.ok_or_else(|| AppError::bad_request("aucun fichier fourni"))?;
    if data.is_empty() {
        return Err(AppError::bad_request("fichier vide"));
    }
    if data.len() > MAX_GUILD_IMAGE {
        return Err(AppError::bad_request("image trop volumineuse (max 2 Mio)"));
    }
    if detect_image_type(&data).is_none() {
        return Err(AppError::bad_request("format d'image non supporté (png/gif/webp/jpeg)"));
    }
    let id = store_upload(&st, &data).await?;
    Ok(Json(serde_json::json!({ "image_id": id.to_string() })))
}

/// `GET /stickers/:sticker_id` — sert l'image d'un sticker (publique, décorative).
pub async fn serve_sticker(
    State(st): State<AppState>,
    Path(sid): Path<String>,
) -> AppResult<Response> {
    let sid = parse_i64(&sid)?;
    let asset: Option<String> = sqlx::query("SELECT asset_id FROM stickers WHERE id = ?")
        .bind(sid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("sticker introuvable"))?
        .get("asset_id");
    serve_stored_image(&st, asset).await
}

// ───────────────────────── Audio du soundboard ─────────────────────────

/// Taille maximale d'un son de soundboard (1 Mio — quelques secondes d'audio).
const MAX_SOUND_SIZE: usize = 1024 * 1024;

/// Détecte le type MIME d'un fichier audio par ses octets magiques (MP3/OGG/WAV).
fn detect_audio_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"ID3") || (bytes.len() >= 2 && bytes[0] == 0xFF && (bytes[1] & 0xE0) == 0xE0)
    {
        Some("audio/mpeg")
    } else if bytes.starts_with(b"OggS") {
        Some("audio/ogg")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        Some("audio/wav")
    } else {
        None
    }
}

/// `POST /guilds/:guild_id/soundboard/audio` — téléverse le fichier audio d'un son
/// (multipart `file`, ≤ 1 Mio, mp3/ogg/wav vérifiés par octets magiques).
/// Renvoie le `sound_id` (asset) à poser via `POST /guilds/:id/soundboard`.
pub async fn upload_sound_audio(
    State(st): State<AppState>,
    user: AuthUser,
    Path(gid): Path<String>,
    mut multipart: Multipart,
) -> AppResult<Json<serde_json::Value>> {
    let gid = parse_i64(&gid)?;
    pg::require_expression_create(&st.pool, gid, user.id.as_i64()).await?;

    let mut data = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::bad_request("requête multipart invalide"))?
    {
        if field.file_name().is_some() {
            data = Some(
                field
                    .bytes()
                    .await
                    .map_err(|_| AppError::bad_request("lecture du fichier échouée"))?,
            );
            break;
        }
    }
    let data = data.ok_or_else(|| AppError::bad_request("aucun fichier fourni"))?;
    if data.is_empty() {
        return Err(AppError::bad_request("fichier vide"));
    }
    if data.len() > MAX_SOUND_SIZE {
        return Err(AppError::bad_request("son trop volumineux (max 1 Mio)"));
    }
    if detect_audio_type(&data).is_none() {
        return Err(AppError::bad_request(
            "format audio non supporté (mp3/ogg/wav)",
        ));
    }
    let id = store_upload(&st, &data).await?;
    Ok(Json(serde_json::json!({ "sound_id": id.to_string() })))
}

/// `GET /soundboard-sounds/:id/audio` — sert le fichier audio d'un son de soundboard.
/// Le type est forcé en liste blanche audio + `nosniff` (même posture que les images).
pub async fn serve_sound_audio(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Response> {
    let id = parse_i64(&id)?;
    let asset: Option<String> = sqlx::query("SELECT sound_id FROM soundboard_sounds WHERE id = ?")
        .bind(id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("son introuvable"))?
        .get("sound_id");
    let asset = asset.ok_or_else(|| AppError::not_found("audio absent"))?;
    let file_id = asset
        .parse::<i64>()
        .map_err(|_| AppError::not_found("audio introuvable"))?;
    let path = st.upload_dir.join(file_id.to_string());
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| AppError::not_found("audio introuvable"))?;
    let ctype = detect_audio_type(&bytes).ok_or_else(|| AppError::not_found("audio invalide"))?;
    let mut resp = Response::new(Body::from(bytes));
    let h = resp.headers_mut();
    h.insert(header::CONTENT_TYPE, header::HeaderValue::from_static(ctype));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        header::HeaderValue::from_static("nosniff"),
    );
    Ok(resp)
}

/// `POST /users/@me/images` — téléverse une image de profil (avatar / bannière) pour
/// l'utilisateur courant. Mêmes gardes que les images de guilde : taille ≤ 2 Mio, type
/// d'image vérifié par octets magiques. Renvoie l'`image_id` à poser via PATCH /users/@me.
pub async fn upload_user_image(
    State(st): State<AppState>,
    _user: AuthUser,
    mut multipart: Multipart,
) -> AppResult<Json<serde_json::Value>> {
    let mut data = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::bad_request("requête multipart invalide"))?
    {
        if field.file_name().is_some() {
            data = Some(
                field
                    .bytes()
                    .await
                    .map_err(|_| AppError::bad_request("lecture du fichier échouée"))?,
            );
            break;
        }
    }
    let data = data.ok_or_else(|| AppError::bad_request("aucun fichier fourni"))?;
    if data.is_empty() {
        return Err(AppError::bad_request("fichier vide"));
    }
    if data.len() > MAX_GUILD_IMAGE {
        return Err(AppError::bad_request("image trop volumineuse (max 2 Mio)"));
    }
    if detect_image_type(&data).is_none() {
        return Err(AppError::bad_request(
            "format d'image non supporté (png/gif/webp/jpeg)",
        ));
    }
    let id = store_upload(&st, &data).await?;
    Ok(Json(serde_json::json!({ "image_id": id.to_string() })))
}

/// `GET /users/:user_id/avatar` — sert l'avatar d'un utilisateur (public, décoratif).
pub async fn serve_user_avatar(
    State(st): State<AppState>,
    Path(uid): Path<String>,
) -> AppResult<Response> {
    let uid = parse_i64(&uid)?;
    let avatar: Option<String> = sqlx::query("SELECT avatar_id FROM users WHERE id = ?")
        .bind(uid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("utilisateur introuvable"))?
        .get("avatar_id");
    serve_stored_image(&st, avatar).await
}

/// `GET /users/:user_id/banner` — sert la bannière de profil d'un utilisateur.
pub async fn serve_user_banner(
    State(st): State<AppState>,
    Path(uid): Path<String>,
) -> AppResult<Response> {
    let uid = parse_i64(&uid)?;
    let banner: Option<String> = sqlx::query("SELECT banner_id FROM users WHERE id = ?")
        .bind(uid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("utilisateur introuvable"))?
        .get("banner_id");
    serve_stored_image(&st, banner).await
}

// `GET /guilds/:guild_id/icon` — sert l'icône du serveur (publique, décorative).
pub async fn serve_guild_icon(
    State(st): State<AppState>,
    Path(gid): Path<String>,
) -> AppResult<Response> {
    let gid = parse_i64(&gid)?;
    let icon: Option<String> = sqlx::query("SELECT icon_id FROM guilds WHERE id = ?")
        .bind(gid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?
        .get("icon_id");
    serve_stored_image(&st, icon).await
}

/// `GET /guilds/:guild_id/banner` — sert la bannière (image) du serveur (publique, décorative).
pub async fn serve_guild_banner(
    State(st): State<AppState>,
    Path(gid): Path<String>,
) -> AppResult<Response> {
    let gid = parse_i64(&gid)?;
    let banner: Option<String> = sqlx::query("SELECT banner_id FROM guilds WHERE id = ?")
        .bind(gid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::not_found("guilde introuvable"))?
        .get("banner_id");
    serve_stored_image(&st, banner).await
}
