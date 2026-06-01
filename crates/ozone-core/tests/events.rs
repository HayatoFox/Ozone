//! E2E : cycle de vie d'un événement programmé via `ApiClient` (cf. `routes_events`).

mod common;
use common::{register_with_guild, spawn_server};
use ozone_core::proto::dto::{CreateEvent, UpdateEvent};

#[tokio::test]
async fn scheduled_event_lifecycle() {
    let base = spawn_server().await;
    let (client, guild) = register_with_guild(&base, "evna", "Agora").await;

    // Création d'un événement externe (type 3) : seul un lieu est requis, pas de salon.
    // `scheduled_start` est un horodatage Unix en millisecondes (i64).
    let start: i64 = 4_102_444_800_000; // 2100-01-01T00:00:00Z, en ms.
    let created = client
        .create_event(
            guild.id,
            &CreateEvent {
                name: "Réunion".into(),
                entity_type: 3,
                location: Some("Salle des fêtes".into()),
                description: Some("Ordre du jour".into()),
                scheduled_start: start,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(created.name, "Réunion");
    assert_eq!(created.entity_type, 3);
    assert_eq!(created.scheduled_start, start);
    assert_eq!(created.interested_count, 0);

    // Présent dans la liste, et lecture unitaire cohérente.
    assert!(client
        .list_events(guild.id)
        .await
        .unwrap()
        .iter()
        .any(|e| e.id == created.id));
    assert_eq!(
        client.get_event(guild.id, created.id).await.unwrap().id,
        created.id
    );

    // Renommage via PATCH.
    let updated = client
        .update_event(
            guild.id,
            created.id,
            &UpdateEvent {
                name: Some("Réunion++".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(updated.name, "Réunion++");

    // RSVP : intérêt puis retrait (idempotents).
    client.rsvp_event(guild.id, created.id).await.unwrap();
    assert_eq!(
        client
            .get_event(guild.id, created.id)
            .await
            .unwrap()
            .interested_count,
        1
    );
    client.unrsvp_event(guild.id, created.id).await.unwrap();
    assert_eq!(
        client
            .get_event(guild.id, created.id)
            .await
            .unwrap()
            .interested_count,
        0
    );

    // Suppression : l'événement disparaît.
    client.delete_event(guild.id, created.id).await.unwrap();
    assert!(client.get_event(guild.id, created.id).await.is_err());
}
