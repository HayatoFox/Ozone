//! Test d'intégration de la **porte d'accès d'instance** (mot de passe d'instance) contre une
//! vraie instance `ozone-api` configurée avec un gate.

use ozone_api::config::Config;
use ozone_api::{bootstrap_state, build_app};
use ozone_core::ApiClient;

/// Démarre un serveur **protégé par mot de passe d'instance** et renvoie sa base d'API.
async fn spawn_gated_server(instance_password: &str) -> String {
    let unique = format!(
        "{}_{:?}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        std::thread::current().id()
    );
    let path = std::env::temp_dir().join(format!("ozone-gate-it-{unique}.db"));
    let cfg = Config {
        bind: "127.0.0.1:0".into(),
        db_path: path.to_string_lossy().to_string(),
        instance_name: "Gated".into(),
        instance_description: None,
        registration_policy: "open".into(),
        instance_password: Some(instance_password.to_string()),
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
async fn gate_required_blocks_until_password_passed() {
    let base = spawn_gated_server("ouvre-toi").await;

    // 1) Les métadonnées publiques annoncent un gate requis.
    let info = ApiClient::new(&base)
        .instance_info()
        .await
        .expect("instance_info");
    assert!(
        info.access_gate.required,
        "le gate doit être annoncé requis"
    );

    // 2) Sans jeton de gate, l'inscription est refusée.
    let no_gate = ApiClient::new(&base);
    assert!(
        no_gate
            .register("alice", "alice@g.fr", "motdepasse")
            .await
            .is_err(),
        "inscription refusée sans jeton de gate"
    );

    // 3) Mauvais mot de passe d'instance → refus du gate.
    assert!(
        ApiClient::new(&base).gate("mauvais").await.is_err(),
        "mauvais mot de passe d'instance rejeté"
    );

    // 4) Bon mot de passe → jeton de gate → inscription acceptée.
    let mut client = ApiClient::new(&base);
    let gate_token = client.gate("ouvre-toi").await.expect("gate ok");
    assert!(!gate_token.is_empty());
    client.set_gate_token(Some(gate_token));
    let tokens = client
        .register("alice", "alice@g.fr", "motdepasse")
        .await
        .expect("inscription avec gate");
    assert!(!tokens.access_token.is_empty());

    // 5) Le même client (jeton de gate en mémoire) peut aussi se connecter.
    let fresh = ApiClient::new(&base);
    let gt2 = fresh.gate("ouvre-toi").await.expect("gate ok");
    let mut login_client = ApiClient::new(&base);
    login_client.set_gate_token(Some(gt2));
    assert!(
        login_client.login("alice", "motdepasse").await.is_ok(),
        "connexion acceptée avec jeton de gate"
    );
}
