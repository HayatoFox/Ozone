//! E2E : marqueurs de lecture, réglages de notification et boîte de mentions via `ApiClient`.

mod common;
use common::{register_with_guild, spawn_server};
use ozone_core::proto::dto::SetNotificationSetting;

#[tokio::test]
async fn read_states_notifications_and_mentions() {
    let base = spawn_server().await;
    let (client, guild) = register_with_guild(&base, "vigie", "Tour de guet").await;

    // Salon texte (type 0) créé automatiquement avec la guilde.
    let channels = client.list_channels(guild.id).await.unwrap();
    let text = channels
        .iter()
        .find(|c| c.kind == 0)
        .expect("salon texte par défaut");

    // Envoi d'un message puis acquittement jusqu'à celui-ci.
    let msg = client.send_message(text.id, "présent").await.unwrap();
    let rs = client.ack_message(text.id, msg.id).await.unwrap();
    assert_eq!(rs.channel_id, text.id);
    assert_eq!(rs.last_read_id, msg.id);
    assert_eq!(rs.mention_count, 0);

    // La liste des états de lecture reflète l'acquittement.
    let states = client.list_read_states().await.unwrap();
    let found = states
        .iter()
        .find(|s| s.channel_id == text.id)
        .expect("état de lecture du salon");
    assert_eq!(found.last_read_id, msg.id);

    // Réglage de notification de guilde : niveau 1 (@mentions uniquement).
    let g_set = client
        .set_guild_notification(
            guild.id,
            &SetNotificationSetting {
                level: Some(1),
                mute_seconds: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(g_set.scope_type, 0);
    assert_eq!(g_set.scope_id, guild.id);
    assert_eq!(g_set.level, 1);

    // Réglage de notification de salon : niveau 2 (rien).
    let c_set = client
        .set_channel_notification(
            text.id,
            &SetNotificationSetting {
                level: Some(2),
                mute_seconds: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(c_set.scope_type, 1);
    assert_eq!(c_set.scope_id, text.id);
    assert_eq!(c_set.level, 2);

    // La liste des réglages reflète les deux portées.
    let settings = client.list_notification_settings().await.unwrap();
    assert!(settings
        .iter()
        .any(|s| s.scope_type == 0 && s.scope_id == guild.id && s.level == 1));
    assert!(settings
        .iter()
        .any(|s| s.scope_type == 1 && s.scope_id == text.id && s.level == 2));

    // La boîte de mentions répond (vide ici : pas d'auto-mention).
    let mentions = client.mentions_inbox().await.unwrap();
    assert!(mentions.is_empty());

    // Acquittement de toute la guilde.
    client.ack_guild(guild.id).await.unwrap();
}
