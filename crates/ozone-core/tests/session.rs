//! Test d'intégration de l'orchestrateur `Session` contre une vraie instance `ozone-api`.
//! Couvre : authentification → bootstrap REST → cache → temps réel Gateway → réhydratation hors-ligne.

use ozone_api::config::Config;
use ozone_api::{bootstrap_state, build_app};
use ozone_core::{InstanceRef, Session};

async fn spawn_server() -> String {
    let unique = format!(
        "{}_{:?}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        std::thread::current().id()
    );
    let path = std::env::temp_dir().join(format!("ozone-session-it-{unique}.db"));
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

fn temp_cache_path() -> String {
    let unique = format!(
        "{}_{:?}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        std::thread::current().id()
    );
    std::env::temp_dir()
        .join(format!("ozone-session-cache-{unique}.db"))
        .to_string_lossy()
        .to_string()
}

#[tokio::test]
async fn session_auth_bootstrap_cache_realtime_and_offline() {
    let base = spawn_server().await;
    let cache_path = temp_cache_path();

    // ── Session authentifiée + cache attaché ─────────────────────────────
    let cache = ozone_core::Cache::open(&cache_path).await.expect("cache");
    let mut sess = Session::new(InstanceRef::new(&base)).with_cache(cache);
    assert!(!sess.is_authenticated());

    sess.register("dave", "dave@s.fr", "motdepasse")
        .await
        .expect("register");
    assert!(sess.is_authenticated());
    assert!(sess.access_token().is_some() && sess.refresh_token().is_some());

    // Une guilde (via le client REST exposé) → bootstrap doit la voir.
    let guild = sess.api.create_guild("Atelier").await.expect("guild");
    sess.bootstrap().await.expect("bootstrap");
    assert!(sess.store.guilds.contains_key(&guild.id.as_i64()));
    let cid = {
        let chans: Vec<_> = sess.store.channels.values().collect();
        assert!(!chans.is_empty(), "le bootstrap a chargé les salons");
        chans[0].id
    };

    // ── Temps réel : un message envoyé arrive via la Gateway et met à jour le Store ──
    sess.connect_gateway().await.expect("gateway");
    assert!(sess.is_realtime());
    assert!(sess.ready.is_some());

    sess.open_channel(cid).await.expect("open_channel");
    sess.send_message(cid, "via session").await.expect("send");

    let mut got = false;
    for _ in 0..50 {
        match tokio::time::timeout(std::time::Duration::from_secs(5), sess.poll_event()).await {
            Ok(Some(ev)) => {
                if ev.kind() == Some("MESSAGE_CREATE") {
                    assert!(ev.changed);
                    got = true;
                    break;
                }
            }
            Ok(None) => break,
            Err(_) => break,
        }
    }
    assert!(got, "MESSAGE_CREATE reçu et appliqué");
    assert!(sess
        .store
        .messages_of(cid)
        .iter()
        .any(|m| m.content == "via session"));

    // ── Démarrage hors-ligne : une session neuve hydrate depuis le cache, sans réseau ──
    drop(sess); // libère le pool du cache
    let cache2 = ozone_core::Cache::open(&cache_path)
        .await
        .expect("reopen cache");
    let mut offline = Session::new(InstanceRef::new(&base)).with_cache(cache2);
    offline.hydrate_from_cache(100).await.expect("hydrate");
    assert!(
        offline.store.guilds.contains_key(&guild.id.as_i64()),
        "guilde restaurée depuis le cache"
    );
    assert!(
        offline
            .store
            .messages_of(cid)
            .iter()
            .any(|m| m.content == "via session"),
        "message restauré depuis le cache"
    );

    let _ = std::fs::remove_file(&cache_path);
}

#[tokio::test]
async fn session_without_token_cannot_connect_gateway() {
    let base = spawn_server().await;
    let mut sess = Session::new(InstanceRef::new(&base));
    // Pas d'authentification → la connexion Gateway doit échouer proprement (pas de panique).
    assert!(sess.connect_gateway().await.is_err());
    assert!(!sess.is_realtime());
    // Et poll_event sans Gateway renvoie None.
    assert!(sess.poll_event().await.is_none());
}
