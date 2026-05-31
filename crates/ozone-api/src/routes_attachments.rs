//! Pièces jointes : téléversement (multipart) et téléchargement (gardé par permission de salon).
//! Stockage fichier local (un fichier nommé par l'identifiant — pas de traversée de chemin).
//! Cf. docs/features/04-messagerie.md.

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::extract::AuthUser;
use crate::permissions as pg;
use crate::state::AppState;
use crate::util::parse_i64;
use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::header;
use axum::response::Response;
use axum::Json;
use ozone_proto::dto::Attachment;
use ozone_proto::perms;
use sqlx::Row;

/// Taille maximale d'une pièce jointe (25 Mo).
pub const MAX_ATTACHMENT_SIZE: usize = 25 * 1024 * 1024;

/// Nettoie un nom de fichier pour l'affichage / l'en-tête `Content-Disposition`
/// (le fichier sur disque est nommé par identifiant, jamais par ce nom → pas de traversée).
fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| !c.is_control() && *c != '"' && *c != '\\' && *c != '/')
        .take(200)
        .collect();
    let t = cleaned.trim();
    if t.is_empty() {
        "fichier".to_string()
    } else {
        t.to_string()
    }
}

/// `POST /channels/:channel_id/attachments` — téléverse un fichier (champ multipart `file`).
pub async fn upload_attachment(
    State(st): State<AppState>,
    user: AuthUser,
    Path(cid): Path<String>,
    mut multipart: Multipart,
) -> AppResult<Json<Attachment>> {
    let cid = parse_i64(&cid)?;
    pg::require_channel_perm(
        &st.pool,
        cid,
        user.id.as_i64(),
        perms::VIEW_CHANNEL | perms::SEND_MESSAGES | perms::ATTACH_FILES,
    )
    .await?;

    let mut filename = None;
    let mut content_type = None;
    let mut data = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::bad_request("requête multipart invalide"))?
    {
        if field.file_name().is_some() {
            filename = field.file_name().map(|s| s.to_string());
            content_type = field.content_type().map(|s| s.to_string());
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
    if data.len() > MAX_ATTACHMENT_SIZE {
        return Err(AppError::bad_request("fichier trop volumineux (max 25 Mo)"));
    }
    let filename = sanitize_filename(&filename.unwrap_or_default());
    let content_type = content_type.unwrap_or_else(|| "application/octet-stream".to_string());
    let size = data.len() as i64;

    let id = st.ids.next();
    let path = st.upload_dir.join(id.as_i64().to_string());
    tokio::fs::write(&path, &data)
        .await
        .map_err(|e| AppError::internal(format!("écriture du fichier : {e}")))?;

    sqlx::query(
        "INSERT INTO attachments (id, channel_id, uploader_id, message_id, filename, content_type, size, created_at) \
         VALUES (?, ?, ?, NULL, ?, ?, ?, ?)",
    )
    .bind(id.as_i64())
    .bind(cid)
    .bind(user.id.as_i64())
    .bind(&filename)
    .bind(&content_type)
    .bind(size)
    .bind(now_ms())
    .execute(&st.pool)
    .await?;

    Ok(Json(Attachment {
        url: format!("/attachments/{}/{}", id.as_i64(), filename),
        id,
        filename,
        content_type,
        size,
    }))
}

/// `GET /attachments/:id/:filename` — télécharge une pièce jointe (membre du salon uniquement).
pub async fn serve_attachment(
    State(st): State<AppState>,
    user: AuthUser,
    Path((id, _filename)): Path<(String, String)>,
) -> AppResult<Response> {
    let id = parse_i64(&id)?;
    let row =
        sqlx::query("SELECT channel_id, filename, content_type FROM attachments WHERE id = ?")
            .bind(id)
            .fetch_optional(&st.pool)
            .await?
            .ok_or_else(|| AppError::not_found("pièce jointe introuvable"))?;
    let cid: i64 = row.get("channel_id");
    // Accès réservé à qui peut voir le salon.
    pg::require_channel_perm(&st.pool, cid, user.id.as_i64(), perms::VIEW_CHANNEL).await?;

    // Le fichier est localisé par identifiant (jamais par le nom fourni) → pas de traversée.
    let path = st.upload_dir.join(id.to_string());
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| AppError::not_found("fichier introuvable"))?;

    let stored_name: String = row.get("filename");
    let ctype: String = row.get("content_type");
    // Anti-XSS sur contenu téléversé : `nosniff` + CSP neutralisante, et affichage *inline*
    // réservé aux médias sûrs (image/audio/vidéo, texte brut) ; tout le reste est forcé en
    // téléchargement (`attachment`) pour qu'un HTML/JS téléversé ne s'exécute jamais dans l'origine.
    let inline_ok = ctype.starts_with("image/")
        || ctype.starts_with("audio/")
        || ctype.starts_with("video/")
        || ctype == "text/plain";
    let disposition = if inline_ok { "inline" } else { "attachment" };

    let mut resp = Response::new(Body::from(bytes));
    let headers = resp.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_str(&ctype)
            .unwrap_or_else(|_| header::HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        header::HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        header::HeaderValue::from_static("default-src 'none'; sandbox"),
    );
    if let Ok(v) =
        header::HeaderValue::from_str(&format!("{disposition}; filename=\"{stored_name}\""))
    {
        headers.insert(header::CONTENT_DISPOSITION, v);
    }
    Ok(resp)
}
