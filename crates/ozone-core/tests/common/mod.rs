//! Utilitaires partagés des tests d'intégration : démarre une vraie instance `ozone-api` sur un
//! port éphémère et fournit des aides d'authentification. Inclus via `mod common;`.
#![allow(dead_code)] // chaque binaire de test n'utilise qu'un sous-ensemble de ces aides

use ozone_api::config::Config;
use ozone_api::{bootstrap_state, build_app};
use ozone_core::ApiClient;

fn unique_tag() -> String {
    format!(
        "{}_{:?}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        std::thread::current().id()
    )
}

/// Démarre un serveur (inscription ouverte, sans porte) et renvoie sa base d'API.
pub async fn spawn_server() -> String {
    spawn_server_cfg("open", None).await
}

/// Démarre un serveur avec une politique d'inscription et un mot de passe d'instance optionnels.
pub async fn spawn_server_cfg(
    registration_policy: &str,
    instance_password: Option<&str>,
) -> String {
    let path = std::env::temp_dir().join(format!("ozone-it-{}.db", unique_tag()));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "IT".into(),
        instance_description: None,
        registration_policy: registration_policy.into(),
        instance_password: instance_password.map(|s| s.to_string()),
        version: "0.1.0-test".into(),
    };
    let state = bootstrap_state(&cfg).await.expect("bootstrap");
    let app = build_app(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

/// Inscrit un utilisateur et renvoie un client **authentifié** (jeton d'accès porté).
pub async fn register(base: &str, username: &str) -> ApiClient {
    let mut client = ApiClient::new(base);
    let tokens = client
        .register(username, &format!("{username}@it.test"), "motdepasse-123")
        .await
        .expect("register");
    client.set_token(Some(tokens.access_token));
    client
}

/// Inscrit un utilisateur, crée une guilde, et renvoie `(client authentifié, guilde)`.
pub async fn register_with_guild(
    base: &str,
    username: &str,
    guild_name: &str,
) -> (ApiClient, ozone_core::proto::dto::Guild) {
    let client = register(base, username).await;
    let guild = client.create_guild(guild_name).await.expect("create_guild");
    (client, guild)
}
