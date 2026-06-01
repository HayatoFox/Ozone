//! E2E : CRUD guildes/salons/fils via `ApiClient` (patron des tests `client_*`).

mod common;
use common::{register_with_guild, spawn_server};
use ozone_core::proto::dto::{CreateChannel, UpdateChannel, UpdateGuild};

#[tokio::test]
async fn guild_channel_thread_crud() {
    let base = spawn_server().await;
    let (client, guild) = register_with_guild(&base, "gildane", "Forge").await;

    // GET + PATCH guilde.
    assert_eq!(client.get_guild(guild.id).await.unwrap().id, guild.id);
    let updated = client
        .update_guild(
            guild.id,
            &UpdateGuild {
                name: Some("Forge++".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(updated.name, "Forge++");

    // Création + lecture + maj d'un salon.
    let ch = client
        .create_channel(
            guild.id,
            &CreateChannel {
                name: "atelier".into(),
                kind: 0,
                topic: None,
                parent_id: None,
                nsfw: None,
                rate_limit_per_user: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(ch.name, "atelier");
    assert_eq!(client.get_channel(ch.id).await.unwrap().id, ch.id);
    let uc = client
        .update_channel(
            ch.id,
            &UpdateChannel {
                name: Some("atelier-2".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(uc.name, "atelier-2");

    // Fil (thread) sous le salon.
    let thread = client.create_thread(ch.id, "fil").await.unwrap();
    assert_eq!(thread.kind, 11);
    assert!(client
        .list_threads(ch.id)
        .await
        .unwrap()
        .iter()
        .any(|t| t.id == thread.id));

    // Suppressions.
    client.delete_channel(ch.id).await.unwrap();
    client.delete_guild(guild.id).await.unwrap();
    assert!(client.get_guild(guild.id).await.is_err());
}
