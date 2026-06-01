//! E2E : actions sur les messages (édition, réactions, épingles, frappe, suppression en masse)
//! via `ApiClient`. Suit le patron de `tests/guild.rs`.

mod common;
use common::{register_with_guild, spawn_server};
use ozone_core::proto::dto::{BulkDelete, EditMessage};

#[tokio::test]
async fn message_actions_e2e() {
    let base = spawn_server().await;
    let (client, guild) = register_with_guild(&base, "gildane", "Forge").await;

    // Salon texte par défaut (« général », type 0) créé avec la guilde.
    let channels = client.list_channels(guild.id).await.unwrap();
    let channel = channels
        .into_iter()
        .find(|c| c.kind == 0)
        .expect("salon texte par défaut");

    // Envoi d'un message.
    let msg = client.send_message(channel.id, "bonjour").await.unwrap();
    assert_eq!(msg.content, "bonjour");

    // Édition (le contenu change).
    let edited = client
        .edit_message(
            channel.id,
            msg.id,
            &EditMessage {
                content: "bonjour, édité".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(edited.id, msg.id);
    assert_eq!(edited.content, "bonjour, édité");

    // Réaction (emoji unicode, percent-encodé dans le chemin) : ajout puis retrait.
    client.add_reaction(channel.id, msg.id, "👍").await.unwrap();
    client
        .remove_reaction(channel.id, msg.id, "👍")
        .await
        .unwrap();

    // Épingle : pin → présent dans la liste → unpin.
    client.pin_message(channel.id, msg.id).await.unwrap();
    let pins = client.list_pins(channel.id).await.unwrap();
    assert!(
        pins.iter().any(|m| m.id == msg.id),
        "le message épinglé doit figurer dans les épingles"
    );
    client.unpin_message(channel.id, msg.id).await.unwrap();
    assert!(client
        .list_pins(channel.id)
        .await
        .unwrap()
        .iter()
        .all(|m| m.id != msg.id));

    // Indicateur de frappe (succès sans corps).
    client.typing(channel.id).await.unwrap();

    // Suppression en masse de quelques messages.
    let m1 = client.send_message(channel.id, "un").await.unwrap();
    let m2 = client.send_message(channel.id, "deux").await.unwrap();
    client
        .bulk_delete(
            channel.id,
            &BulkDelete {
                messages: vec![m1.id, m2.id],
            },
        )
        .await
        .unwrap();
    let remaining = client.list_messages(channel.id).await.unwrap();
    assert!(remaining.iter().all(|m| m.id != m1.id && m.id != m2.id));
}
