//! E2E : cycle de vie d'un webhook via `ApiClient` — création, listes, mise à jour, régénération de
//! jeton, exécution (non authentifiée), suppression. Suit le patron de `tests/guild.rs`.

mod common;
use common::{register_with_guild, spawn_server};
use ozone_core::proto::dto::{CreateWebhook, ExecuteWebhook, UpdateWebhook};

#[tokio::test]
async fn webhook_lifecycle() {
    let base = spawn_server().await;
    let (client, guild) = register_with_guild(&base, "webmestre", "Atelier").await;

    // Premier salon texte de la guilde (kind == 0).
    let channels = client.list_channels(guild.id).await.unwrap();
    let text_channel = channels
        .iter()
        .find(|c| c.kind == 0)
        .expect("un salon texte par défaut");

    // Création : la réponse porte le jeton secret.
    let created = client
        .create_webhook(
            text_channel.id,
            &CreateWebhook {
                name: "Annonceur".into(),
                avatar_id: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(created.name, "Annonceur");
    assert_eq!(created.channel_id, text_channel.id);
    assert_eq!(created.guild_id, guild.id);
    let first_token = created.token.clone().expect("jeton à la création");

    // Listes (salon + guilde) : le webhook y figure (jeton masqué côté liste).
    let by_channel = client.list_channel_webhooks(text_channel.id).await.unwrap();
    assert!(by_channel.iter().any(|w| w.id == created.id));
    assert!(by_channel.iter().all(|w| w.token.is_none()));
    let by_guild = client.list_guild_webhooks(guild.id).await.unwrap();
    assert!(by_guild.iter().any(|w| w.id == created.id));

    // Mise à jour : renommage.
    let renamed = client
        .update_webhook(
            created.id,
            &UpdateWebhook {
                name: Some("Crieur".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(renamed.name, "Crieur");
    assert_eq!(renamed.id, created.id);

    // Régénération du jeton : le secret change.
    let regenerated = client.regenerate_token(created.id).await.unwrap();
    let new_token = regenerated.token.clone().expect("nouveau jeton");
    assert_ne!(new_token, first_token);

    // Exécution (non authentifiée — jeton dans l'URL) : poste un message via le webhook.
    let message = client
        .execute_webhook(
            created.id,
            &new_token,
            &ExecuteWebhook {
                content: "Bonjour depuis un webhook !".into(),
                username: None,
                avatar_id: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(message.content, "Bonjour depuis un webhook !");
    assert_eq!(message.channel_id, text_channel.id);
    assert_eq!(message.webhook_id, Some(created.id));

    // L'ancien jeton ne doit plus permettre l'exécution.
    assert!(client
        .execute_webhook(
            created.id,
            &first_token,
            &ExecuteWebhook {
                content: "Avec un jeton périmé".into(),
                username: None,
                avatar_id: None,
            },
        )
        .await
        .is_err());

    // Suppression.
    client.delete_webhook(created.id).await.unwrap();
    assert!(client
        .list_channel_webhooks(text_channel.id)
        .await
        .unwrap()
        .iter()
        .all(|w| w.id != created.id));
}
