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
pub mod presence;
pub mod routes_auth;
pub mod routes_chat;
pub mod routes_discovery;
pub mod routes_dms;
pub mod routes_emojis;
pub mod routes_events;
pub mod routes_guild;
pub mod routes_instance;
pub mod routes_instance_admin;
pub mod routes_messages;
pub mod routes_moderation;
pub mod routes_notifications;
pub mod routes_presence;
pub mod routes_relationships;
pub mod routes_roles;
pub mod routes_soundboard;
pub mod routes_stickers;
pub mod routes_users;
pub mod routes_voice;
pub mod routes_webhooks;
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
        // Administration d'instance (self-hoster)
        .route(
            "/instance/admin/config",
            get(routes_instance_admin::get_config),
        )
        .route(
            "/instance/admin/invites",
            get(routes_instance_admin::list_invites).post(routes_instance_admin::create_invite),
        )
        .route(
            "/instance/admin/invites/:code",
            delete(routes_instance_admin::revoke_invite),
        )
        .route(
            "/instance/admin/users",
            get(routes_instance_admin::list_users),
        )
        .route(
            "/instance/admin/users/:user_id",
            patch(routes_instance_admin::set_suspended),
        )
        .route(
            "/instance/admin/users/:user_id/role",
            put(routes_instance_admin::set_role),
        )
        // Authentification
        .route("/auth/register", post(routes_auth::register))
        .route("/auth/login", post(routes_auth::login))
        .route("/auth/token/refresh", post(routes_auth::refresh))
        .route(
            "/users/@me",
            get(routes_auth::me).patch(routes_users::update_profile),
        )
        .route(
            "/users/@me/settings",
            get(routes_users::get_settings).put(routes_users::put_settings),
        )
        .route("/users/@me/presence", put(routes_presence::set_presence))
        .route("/users/:user_id/profile", get(routes_users::get_profile))
        // Relations (amis / blocages / notes)
        .route(
            "/users/@me/relationships",
            get(routes_relationships::list_relationships)
                .post(routes_relationships::add_relationship),
        )
        .route(
            "/users/@me/relationships/:user_id",
            put(routes_relationships::accept_relationship)
                .delete(routes_relationships::remove_relationship),
        )
        .route(
            "/users/@me/notes/:user_id",
            get(routes_relationships::get_note).put(routes_relationships::put_note),
        )
        // Messages privés & groupes
        .route(
            "/users/@me/channels",
            get(routes_dms::list_dm_channels).post(routes_dms::open_or_create_dm),
        )
        .route(
            "/channels/:channel_id/recipients/:user_id",
            put(routes_dms::add_recipient).delete(routes_dms::remove_recipient),
        )
        // Guildes / salons / messages
        .route(
            "/guilds",
            post(routes_chat::create_guild).get(routes_chat::list_guilds),
        )
        .route(
            "/guilds/:guild_id",
            get(routes_chat::get_guild)
                .patch(routes_chat::update_guild)
                .delete(routes_chat::delete_guild),
        )
        .route(
            "/guilds/:guild_id/channels",
            post(routes_chat::create_channel)
                .get(routes_chat::list_channels)
                .patch(routes_chat::reorder_channels),
        )
        .route(
            "/channels/:channel_id",
            get(routes_chat::get_channel)
                .patch(routes_chat::update_channel)
                .delete(routes_chat::delete_channel),
        )
        .route(
            "/channels/:channel_id/messages",
            get(routes_messages::list_messages).post(routes_messages::create_message),
        )
        .route(
            "/channels/:channel_id/messages/bulk-delete",
            post(routes_messages::bulk_delete),
        )
        .route(
            "/channels/:channel_id/messages/:message_id",
            patch(routes_messages::edit_message).delete(routes_messages::delete_message),
        )
        .route(
            "/channels/:channel_id/messages/:message_id/reactions/:emoji/@me",
            put(routes_messages::add_reaction).delete(routes_messages::remove_reaction),
        )
        .route(
            "/channels/:channel_id/pins",
            get(routes_messages::list_pins),
        )
        .route(
            "/channels/:channel_id/pins/:message_id",
            put(routes_messages::pin_message).delete(routes_messages::unpin_message),
        )
        .route(
            "/channels/:channel_id/typing",
            post(routes_messages::typing),
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
            "/guilds/:guild_id/members/@me",
            delete(routes_guild::leave_guild),
        )
        .route(
            "/guilds/:guild_id/presences",
            get(routes_presence::list_presences),
        )
        // Signalisation vocale
        .route(
            "/guilds/:guild_id/voice-states",
            get(routes_voice::list_voice_states),
        )
        .route(
            "/guilds/:guild_id/voice-states/@me",
            patch(routes_voice::update_own_voice_state).delete(routes_voice::leave_voice),
        )
        .route(
            "/guilds/:guild_id/voice-states/:user_id",
            patch(routes_voice::moderate_voice_state),
        )
        .route("/voice/regions", get(routes_voice::voice_regions))
        .route(
            "/guilds/:guild_id/members/:user_id",
            patch(routes_moderation::update_member).delete(routes_guild::kick_member),
        )
        // Modération
        .route(
            "/guilds/:guild_id/bans/:user_id",
            put(routes_moderation::ban_member).delete(routes_moderation::unban_member),
        )
        .route("/guilds/:guild_id/bans", get(routes_moderation::list_bans))
        .route(
            "/guilds/:guild_id/audit-logs",
            get(routes_moderation::list_audit_logs),
        )
        .route(
            "/guilds/:guild_id/invites",
            get(routes_guild::list_invites).post(routes_guild::create_invite),
        )
        .route(
            "/invites/:code",
            post(routes_guild::join_invite)
                .get(routes_guild::preview_invite)
                .delete(routes_guild::revoke_invite),
        )
        // Découverte de guildes publiques
        .route("/discovery/guilds", get(routes_discovery::list_discovery))
        .route(
            "/discovery/guilds/:guild_id/join",
            post(routes_discovery::join_discovery),
        )
        // Expressions (emojis / stickers / soundboard)
        .route(
            "/guilds/:guild_id/emojis",
            get(routes_emojis::list_emojis).post(routes_emojis::create_emoji),
        )
        .route(
            "/guilds/:guild_id/emojis/:emoji_id",
            patch(routes_emojis::update_emoji).delete(routes_emojis::delete_emoji),
        )
        .route(
            "/guilds/:guild_id/stickers",
            get(routes_stickers::list_stickers).post(routes_stickers::create_sticker),
        )
        .route(
            "/guilds/:guild_id/stickers/:sticker_id",
            patch(routes_stickers::update_sticker).delete(routes_stickers::delete_sticker),
        )
        .route(
            "/guilds/:guild_id/soundboard",
            get(routes_soundboard::list_sounds).post(routes_soundboard::create_sound),
        )
        .route(
            "/guilds/:guild_id/soundboard/:sound_id",
            patch(routes_soundboard::update_sound).delete(routes_soundboard::delete_sound),
        )
        // Marqueurs de lecture & notifications
        .route(
            "/channels/:channel_id/messages/:message_id/ack",
            post(routes_notifications::ack_message),
        )
        .route(
            "/guilds/:guild_id/ack",
            post(routes_notifications::ack_guild),
        )
        .route(
            "/users/@me/read-states",
            get(routes_notifications::list_read_states),
        )
        .route("/users/@me/mentions", get(routes_messages::mentions_inbox))
        .route(
            "/users/@me/notification-settings",
            get(routes_notifications::list_notification_settings),
        )
        .route(
            "/users/@me/notification-settings/guild/:guild_id",
            put(routes_notifications::set_guild_notification),
        )
        .route(
            "/users/@me/notification-settings/channel/:channel_id",
            put(routes_notifications::set_channel_notification),
        )
        // Recherche de messages (FTS5)
        .route(
            "/guilds/:guild_id/messages/search",
            get(routes_messages::search_guild),
        )
        .route(
            "/channels/:channel_id/messages/search",
            get(routes_messages::search_channel),
        )
        // Webhooks (gestion + exécution par jeton)
        .route(
            "/channels/:channel_id/webhooks",
            get(routes_webhooks::list_channel_webhooks).post(routes_webhooks::create_webhook),
        )
        .route(
            "/guilds/:guild_id/webhooks",
            get(routes_webhooks::list_guild_webhooks),
        )
        .route(
            "/webhooks/:webhook_id",
            patch(routes_webhooks::update_webhook)
                .delete(routes_webhooks::delete_webhook)
                .post(routes_webhooks::regenerate_token),
        )
        .route(
            "/webhooks/:webhook_id/:token",
            post(routes_webhooks::execute_webhook),
        )
        // Événements programmés
        .route(
            "/guilds/:guild_id/events",
            get(routes_events::list_events).post(routes_events::create_event),
        )
        .route(
            "/guilds/:guild_id/events/:event_id",
            get(routes_events::get_event)
                .patch(routes_events::update_event)
                .delete(routes_events::delete_event),
        )
        .route(
            "/guilds/:guild_id/events/:event_id/interested",
            put(routes_events::rsvp_event).delete(routes_events::unrsvp_event),
        )
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
