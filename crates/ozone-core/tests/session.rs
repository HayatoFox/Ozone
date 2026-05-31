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

/// Pompe les événements jusqu'à recevoir un `MESSAGE_CREATE` du contenu attendu (sinon panique).
async fn wait_for_message(sess: &mut Session, content: &str) {
    for _ in 0..50 {
        match tokio::time::timeout(std::time::Duration::from_secs(5), sess.poll_event()).await {
            Ok(Some(ev)) => {
                if ev.kind() == Some("MESSAGE_CREATE") {
                    let c = ev
                        .frame
                        .d
                        .as_ref()
                        .and_then(|d| d.get("content"))
                        .and_then(|v| v.as_str());
                    if c == Some(content) {
                        return;
                    }
                }
            }
            Ok(None) => break,
            Err(_) => break,
        }
    }
    panic!("événement attendu '{content}' non reçu");
}

#[tokio::test]
async fn session_resume_replays_events_missed_during_outage() {
    let base = spawn_server().await;
    let mut sess = Session::new(InstanceRef::new(&base));
    sess.register("erin", "erin@s.fr", "motdepasse")
        .await
        .expect("register");
    let _guild = sess.api.create_guild("Resume").await.expect("guild");
    sess.bootstrap().await.expect("bootstrap");
    let cid = sess.store.channels.values().next().expect("salon").id;

    sess.connect_gateway().await.expect("gateway");
    sess.open_channel(cid).await.expect("open");

    // Message reçu en direct → last_seq avance (l'accusé de réception suit la consommation).
    sess.send_message(cid, "avant").await.expect("send avant");
    wait_for_message(&mut sess, "avant").await;
    assert_eq!(sess.gateway_seq(), 1, "un seul événement consommé");

    // Coupure réseau simulée : la session reste résumable côté serveur (fenêtre de grâce).
    sess.abort_gateway();

    // Pendant la coupure, un message est publié → il doit être bufferisé côté serveur.
    sess.send_message(cid, "pendant coupure")
        .await
        .expect("send pendant");

    // RESUME : accepté, et l'événement manqué est rejoué.
    let resumed = sess.reconnect().await.expect("reconnect");
    assert!(resumed, "RESUME accepté (pas de re-IDENTIFY)");

    wait_for_message(&mut sess, "pendant coupure").await;
    assert!(
        sess.store
            .messages_of(cid)
            .iter()
            .any(|m| m.content == "pendant coupure"),
        "le message manqué est appliqué au Store après RESUME"
    );
}

#[tokio::test]
async fn resume_rejects_another_users_session() {
    let base = spawn_server().await;

    // Gita ouvre une session Gateway → on récupère son session_id.
    let mut gita = Session::new(InstanceRef::new(&base));
    gita.register("gita", "gita@s.fr", "motdepasse")
        .await
        .expect("register gita");
    gita.connect_gateway().await.expect("gita gateway");
    let sid = gita
        .ready
        .as_ref()
        .and_then(|v| v.get("session_id"))
        .and_then(|v| v.as_str())
        .expect("session_id")
        .to_string();

    // Hugo, avec SON jeton, tente de reprendre la session de Gita → doit être refusé (isolation).
    let mut hugo = Session::new(InstanceRef::new(&base));
    hugo.register("hugo", "hugo@s.fr", "motdepasse")
        .await
        .expect("register hugo");
    let hugo_access = hugo.access_token().expect("token").to_string();

    match ozone_core::gateway::connect_resume(&base, &hugo_access, &sid, 0)
        .await
        .expect("resume call")
    {
        ozone_core::gateway::Resumed::Invalid => {}
        ozone_core::gateway::Resumed::Ok(_) => {
            panic!("un utilisateur ne doit jamais reprendre la session d'un autre")
        }
    }
}

#[tokio::test]
async fn resume_unknown_session_is_refused() {
    let base = spawn_server().await;
    let mut sess = Session::new(InstanceRef::new(&base));
    sess.register("frank", "frank@s.fr", "motdepasse")
        .await
        .expect("register");
    let access = sess.access_token().expect("token").to_string();

    // Session inexistante → INVALID_SESSION (le client devra faire un IDENTIFY complet).
    match ozone_core::gateway::connect_resume(&base, &access, "123456789", 0)
        .await
        .expect("resume call")
    {
        ozone_core::gateway::Resumed::Invalid => {}
        ozone_core::gateway::Resumed::Ok(_) => {
            panic!("une session inconnue ne doit pas être reprise")
        }
    }
}
