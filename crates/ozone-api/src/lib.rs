//! `ozone-api` — serveur Ozone (API REST + Gateway WS), mode tout-en-un SQLite.
//!
//! Cf. `docs/01-architecture.md`, `docs/04-api-rest.md`, `docs/05-gateway-temps-reel.md`.

pub mod config;
pub mod crypto;
pub mod db;
pub mod error;
pub mod extract;
pub mod gateway;
pub mod permissions;
pub mod routes_auth;
pub mod routes_chat;
pub mod routes_guild;
pub mod routes_instance;
pub mod routes_roles;
pub mod state;
pub mod util;

use axum::routing::{delete, get, patch, post, put};
use axum::Router;
use config::Config;
use state::AppState;

pub use db::bootstrap_state;

/// Construit le routeur axum avec l'état applicatif.
pub fn build_app(state: AppState) -> Router {
    Router::new()
        // Instance (point d'entrée self-host)
        .route("/instance", get(routes_instance::get_instance))
        .route("/instance/health", get(routes_instance::health))
        .route("/instance/gate", post(routes_instance::gate))
        // Authentification
        .route("/auth/register", post(routes_auth::register))
        .route("/auth/login", post(routes_auth::login))
        .route("/auth/token/refresh", post(routes_auth::refresh))
        .route("/users/@me", get(routes_auth::me))
        // Guildes / salons / messages
        .route(
            "/guilds",
            post(routes_chat::create_guild).get(routes_chat::list_guilds),
        )
        .route(
            "/guilds/:guild_id/channels",
            post(routes_chat::create_channel).get(routes_chat::list_channels),
        )
        .route(
            "/channels/:channel_id/messages",
            get(routes_chat::list_messages).post(routes_chat::create_message),
        )
        // Rôles & permissions
        .route(
            "/guilds/:guild_id/roles",
            get(routes_roles::list_roles).post(routes_roles::create_role),
        )
        .route(
            "/guilds/:guild_id/roles/:role_id",
            patch(routes_roles::update_role).delete(routes_roles::delete_role),
        )
        .route(
            "/guilds/:guild_id/members/:user_id/roles/:role_id",
            put(routes_roles::add_member_role).delete(routes_roles::remove_member_role),
        )
        .route(
            "/channels/:channel_id/permissions/:overwrite_id",
            put(routes_roles::set_overwrite).delete(routes_roles::delete_overwrite),
        )
        // Membres & invitations
        .route("/guilds/:guild_id/members", get(routes_guild::list_members))
        .route(
            "/guilds/:guild_id/members/:user_id",
            delete(routes_guild::kick_member),
        )
        .route(
            "/guilds/:guild_id/invites",
            get(routes_guild::list_invites).post(routes_guild::create_invite),
        )
        .route("/invites/:code", post(routes_guild::join_invite))
        // Gateway temps réel
        .route("/gateway", get(gateway::ws_handler))
        .with_state(state)
}

/// Démarre le serveur (lecture de la config depuis l'environnement, bootstrap, écoute).
pub async fn run() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cfg = Config::from_env();
    let bind = cfg.bind.clone();
    let state = bootstrap_state(&cfg).await?;
    let app = build_app(state);

    let listener = tokio::net::TcpListener::bind(bind.as_str()).await?;
    tracing::info!("API Ozone à l'écoute sur http://{bind}  (gateway : ws://{bind}/gateway)");
    axum::serve(listener, app).await?;
    Ok(())
}
