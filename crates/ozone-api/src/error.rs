//! Type d'erreur applicatif → réponse JSON (codes stables inspirés de Discord).

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug)]
pub struct AppError {
    pub status: StatusCode,
    pub code: u32,
    pub message: String,
    /// Délai conseillé avant nouvelle tentative (secondes) — émis en header `Retry-After`
    /// pour les réponses 429 (rate-limiting).
    pub retry_after: Option<u64>,
}

impl AppError {
    pub fn new(status: StatusCode, code: u32, message: impl Into<String>) -> Self {
        AppError {
            status,
            code,
            message: message.into(),
            retry_after: None,
        }
    }
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, 40000, msg)
    }
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, 40001, msg)
    }
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, 50013, msg)
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, 10004, msg)
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, 40002, msg)
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, 50000, msg)
    }
    pub fn too_many(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::TOO_MANY_REQUESTS, 20016, msg)
    }
    /// 429 avec un délai d'attente conseillé (`Retry-After`).
    pub fn rate_limited(secs: u64) -> Self {
        let mut e = Self::new(
            StatusCode::TOO_MANY_REQUESTS,
            20016,
            "trop de requêtes — réessaie dans un instant",
        );
        e.retry_after = Some(secs);
        e
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let body = Json(json!({
            "code": self.code,
            "message": self.message,
            "retry_after": self.retry_after,
        }));
        match self.retry_after {
            Some(secs) => (
                self.status,
                [(axum::http::header::RETRY_AFTER, secs.to_string())],
                body,
            )
                .into_response(),
            None => (self.status, body).into_response(),
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::internal(format!("erreur base de données : {e}"))
    }
}

pub type AppResult<T> = Result<T, AppError>;
