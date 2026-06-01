//! E2E : recherche de messages (FTS5) par salon et par guilde via `ApiClient`.
//! Suit le patron des tests `client_*` (cf. `guild.rs`).

mod common;
use common::{register_with_guild, spawn_server};

#[tokio::test]
async fn search_channel_and_guild_find_message() {
    let base = spawn_server().await;
    let (client, guild) = register_with_guild(&base, "merline", "Sortilege").await;

    // Premier salon texte (type 0) de la guilde par défaut.
    let channels = client.list_channels(guild.id).await.unwrap();
    let cid = channels
        .iter()
        .find(|c| c.kind == 0)
        .expect("un salon texte")
        .id;

    // Message contenant un mot distinctif (token simple, en minuscules, pour FTS).
    let sent = client
        .send_message(cid, "voici une licorne magique")
        .await
        .unwrap();

    // Recherche dans le salon : doit retrouver le message.
    let in_channel = client.search_channel(cid, "licorne").await.unwrap();
    assert!(in_channel.total >= 1);
    assert!(in_channel.messages.iter().any(|m| m.id == sent.id));

    // Recherche sur toute la guilde : doit aussi le retrouver.
    let in_guild = client.search_guild(guild.id, "licorne").await.unwrap();
    assert!(in_guild.total >= 1);
    assert!(in_guild.messages.iter().any(|m| m.id == sent.id));

    // Vérif cyber : un terme contenant `&`/`=`/espace (caractères de séparation de la chaîne de
    // requête) ne doit PAS s'évader pour injecter un autre paramètre. Le percent-encoding le
    // confine à la valeur de `q` ; FTS ne trouve simplement rien, sans erreur ni effet de bord.
    let injection = client
        .search_channel(cid, "licorne&author_id=1 OR limit=999")
        .await
        .unwrap();
    assert_eq!(injection.total, 0);
    assert!(injection.messages.is_empty());
}
