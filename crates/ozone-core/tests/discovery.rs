//! E2E : découverte de guildes via `ApiClient` (annuaire public opt-in + adhésion directe).
//! Suit le patron des tests `client_*` (cf. `guild.rs`).

mod common;
use common::{register, register_with_guild, spawn_server};
use ozone_core::proto::dto::UpdateGuild;

#[tokio::test]
async fn discovery_list_and_join() {
    let base = spawn_server().await;

    // Alice crée une guilde et l'ouvre à la découverte.
    let (alice, guild) = register_with_guild(&base, "alice", "Place publique").await;
    alice
        .update_guild(
            guild.id,
            &UpdateGuild {
                discoverable: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // La guilde apparaît dans l'annuaire de découverte.
    assert!(alice
        .list_discovery()
        .await
        .unwrap()
        .iter()
        .any(|g| g.id == guild.id));

    // Bob (second utilisateur) découvre puis rejoint directement la guilde.
    let bob = register(&base, "bob").await;
    let joined = bob.join_discovery(guild.id).await.unwrap();
    assert_eq!(joined.id, guild.id);

    // Bob est désormais membre : la guilde figure dans ses guildes.
    assert!(bob
        .list_guilds()
        .await
        .unwrap()
        .iter()
        .any(|g| g.id == guild.id));
}
