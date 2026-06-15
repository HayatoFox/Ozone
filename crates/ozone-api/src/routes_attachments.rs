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
use tokio::io::AsyncWriteExt;

/// Taille maximale d'une pièce jointe (1 Go). Le fichier est STREAMÉ vers le disque (jamais
/// bufferisé entièrement en mémoire) → un upload de 1 Go ne consomme pas 1 Go de RAM.
pub const MAX_ATTACHMENT_SIZE: usize = 1024 * 1024 * 1024;

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

    let id = st.ids.next();
    let path = st.upload_dir.join(id.as_i64().to_string());

    let mut filename = None;
    let mut content_type = None;
    let mut size: usize = 0;
    let mut wrote_file = false;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::bad_request("requête multipart invalide"))?
    {
        if field.file_name().is_none() {
            continue;
        }
        filename = field.file_name().map(|s| s.to_string());
        content_type = field.content_type().map(|s| s.to_string());
        // STREAMING : on écrit chaque morceau au fil de l'eau, en bornant la taille — pas de
        // bufferisation complète en mémoire (indispensable pour des fichiers jusqu'à 1 Go).
        let mut file = tokio::fs::File::create(&path)
            .await
            .map_err(|e| AppError::internal(format!("création du fichier : {e}")))?;
        loop {
            let chunk = match field.chunk().await {
                Ok(Some(c)) => c,
                Ok(None) => break,
                Err(_) => {
                    let _ = tokio::fs::remove_file(&path).await;
                    return Err(AppError::bad_request("lecture du fichier échouée"));
                }
            };
            size += chunk.len();
            if size > MAX_ATTACHMENT_SIZE {
                drop(file);
                let _ = tokio::fs::remove_file(&path).await;
                return Err(AppError::bad_request("fichier trop volumineux (max 1 Go)"));
            }
            if file.write_all(&chunk).await.is_err() {
                let _ = tokio::fs::remove_file(&path).await;
                return Err(AppError::internal("écriture du fichier échouée"));
            }
        }
        let _ = file.flush().await;
        wrote_file = true;
        break;
    }
    if !wrote_file {
        return Err(AppError::bad_request("aucun fichier fourni"));
    }
    if size == 0 {
        let _ = tokio::fs::remove_file(&path).await;
        return Err(AppError::bad_request("fichier vide"));
    }
    let filename = sanitize_filename(&filename.unwrap_or_default());
    let content_type = content_type.unwrap_or_else(|| "application/octet-stream".to_string());
    let size = size as i64;

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
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|_| AppError::not_found("fichier introuvable"))?;
    let file_len = file
        .metadata()
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    // STREAMING : on lit le fichier par morceaux de 64 Kio et on les pousse dans le corps de la
    // réponse → un téléchargement de 1 Go ne charge jamais 1 Go en mémoire.
    let stream = futures_util::stream::unfold(
        (file, vec![0u8; 64 * 1024]),
        |(mut f, mut buf)| async move {
            use tokio::io::AsyncReadExt;
            match f.read(&mut buf).await {
                Ok(0) => None,
                Ok(n) => Some((
                    Ok::<_, std::io::Error>(axum::body::Bytes::copy_from_slice(&buf[..n])),
                    (f, buf),
                )),
                Err(e) => Some((Err(e), (f, buf))),
            }
        },
    );
    let body = Body::from_stream(stream);

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

    let mut resp = Response::new(body);
    let headers = resp.headers_mut();
    if file_len > 0 {
        if let Ok(v) = header::HeaderValue::from_str(&file_len.to_string()) {
            headers.insert(header::CONTENT_LENGTH, v);
        }
    }
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
