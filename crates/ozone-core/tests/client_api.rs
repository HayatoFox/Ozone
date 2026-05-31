//! Test d'intégration **client ↔ serveur** : démarre une vraie instance `ozone-api` sur un port
//! éphémère et exerce le `ApiClient` (HTTP réel) de bout en bout.

use ozone_api::config::Config;
use ozone_api::{bootstrap_state, build_app};
use ozone_core::ApiClient;

/// Démarre le serveur sur un port libre et renvoie sa base d'API (`http://127.0.0.1:<port>`).
async fn spawn_server() -> String {
    let unique = format!(
        "{}_{:?}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        std::thread::current().id()
    );
    let path = std::env::temp_dir().join(format!("ozone-core-it-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "IT".into(),
        instance_description: None,
        registration_policy: "open".into(),
        instance_password: None,
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

#[tokio::test]
async fn client_round_trip() {
    let base = spawn_server().await;
    let mut client = ApiClient::new(&base);

    // Métadonnées d'instance (non authentifié).
    let info = client.instance_info().await.expect("instance_info");
    assert_eq!(info.name, "IT");

    // Inscription → jetons → on porte le jeton d'accès.
    let tokens = client
        .register("alice", "alice@core.fr", "motdepasse")
        .await
        .expect("register");
    assert!(!tokens.access_token.is_empty());
    client.set_token(Some(tokens.access_token));

    // Crée une guilde, la retrouve dans la liste.
    let guild = client
        .create_guild("Ma Guilde")
        .await
        .expect("create_guild");
    assert_eq!(guild.name, "Ma Guilde");
    let guilds = client.list_guilds().await.expect("list_guilds");
    assert_eq!(guilds.len(), 1);

    // Récupère le salon « général » et y envoie un message.
    let channels = client.list_channels(guild.id).await.expect("list_channels");
    assert!(!channels.is_empty());
    let cid = channels[0].id;
    let msg = client
        .send_message(cid, "bonjour depuis le client")
        .await
        .expect("send_message");
    assert_eq!(msg.content, "bonjour depuis le client");

    // L'historique reflète le message.
    let msgs = client.list_messages(cid).await.expect("list_messages");
    assert!(msgs.iter().any(|m| m.content == "bonjour depuis le client"));
}

#[tokio::test]
async fn login_rejects_bad_password() {
    let base = spawn_server().await;
    let client = ApiClient::new(&base);
    client
        .register("bob", "bob@core.fr", "motdepasse")
        .await
        .expect("register");
    // Mauvais mot de passe → erreur (HTTP 401 propagée).
    assert!(client.login("bob", "mauvais").await.is_err());
    // Bon mot de passe → ok.
    assert!(client.login("bob", "motdepasse").await.is_ok());
}
