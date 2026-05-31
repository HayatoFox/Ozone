//! Binaire du nœud média SFU Ozone : signalisation HTTP (offre/réponse SDP) + authentification
//! par jeton vocal, au-dessus du cœur SFU. Processus **séparé** de l'API.
//!
//! Variables d'environnement :
//! - `OZONE_SFU_BIND` (défaut `127.0.0.1:8081`).
//! - `OZONE_VOICE_SECRET` : secret partagé avec l'API pour vérifier les jetons vocaux.
//!   **Requis** — sans lui, le SFU refuse toute connexion (fail-closed).

use ozone_sfu::room::Sfu;
use ozone_sfu::server::{build_router, AppState};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let voice_secret = std::env::var("OZONE_VOICE_SECRET")
        .ok()
        .map(|s| s.into_bytes());
    if voice_secret.is_none() {
        tracing::warn!(
            "OZONE_VOICE_SECRET non défini : le SFU refusera toute connexion (fail-closed). \
             Définissez le même secret que l'API."
        );
    }

    let state = AppState {
        sfu: Sfu::new()?,
        voice_secret: Arc::new(voice_secret),
    };
    let app = build_router(state);

    let bind = std::env::var("OZONE_SFU_BIND").unwrap_or_else(|_| "127.0.0.1:8081".to_string());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!("Nœud média SFU Ozone à l'écoute sur http://{bind}");
    axum::serve(listener, app).await?;
    Ok(())
}
