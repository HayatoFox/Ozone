//! E2E : rôles, mise à jour, attribution, surcharges de permission de salon via `ApiClient`.
//! Suit le patron de `tests/guild.rs`.

mod common;
use common::{register_with_guild, spawn_server};
use ozone_core::proto::dto::{CreateChannel, CreateRole, SetOverwrite, UpdateRole};
use ozone_core::proto::perms;

#[tokio::test]
async fn roles_and_overwrites_crud() {
    let base = spawn_server().await;
    // Le créateur de la guilde en est propriétaire : il possède toutes les permissions,
    // donc l'anti-escalade côté serveur ne rogne rien dans ce test.
    let (client, guild) = register_with_guild(&base, "gildane", "Forge").await;

    // Création d'un rôle (avec une permission accordable).
    let role = client
        .create_role(
            guild.id,
            &CreateRole {
                name: Some("forgeron".into()),
                permissions: Some(perms::SEND_MESSAGES.to_string()),
                hoist: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(role.name, "forgeron");
    assert_eq!(role.guild_id, guild.id);
    assert_eq!(role.permissions, perms::SEND_MESSAGES.to_string());

    // Le rôle apparaît dans la liste (avec @everyone).
    let roles = client.list_roles(guild.id).await.unwrap();
    assert!(roles.iter().any(|r| r.id == role.id));

    // Mise à jour : renommage + nouvelles permissions.
    let updated = client
        .update_role(
            guild.id,
            role.id,
            &UpdateRole {
                name: Some("maître-forgeron".into()),
                permissions: Some((perms::SEND_MESSAGES | perms::ADD_REACTIONS).to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(updated.name, "maître-forgeron");
    assert_eq!(
        updated.permissions,
        (perms::SEND_MESSAGES | perms::ADD_REACTIONS).to_string()
    );

    // Création d'un salon pour y poser une surcharge.
    let channel = client
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

    // Surcharge ciblant le rôle (type 0) : autorise SEND_MESSAGES, refuse MENTION_EVERYONE.
    let ow = client
        .set_overwrite(
            channel.id,
            role.id,
            &SetOverwrite {
                kind: 0,
                allow: Some(perms::SEND_MESSAGES.to_string()),
                deny: Some(perms::MENTION_EVERYONE.to_string()),
            },
        )
        .await
        .unwrap();
    assert_eq!(ow.id, role.id);
    assert_eq!(ow.kind, 0);
    assert_eq!(ow.allow, perms::SEND_MESSAGES.to_string());
    assert_eq!(ow.deny, perms::MENTION_EVERYONE.to_string());

    // Suppression de la surcharge, puis du rôle.
    client.delete_overwrite(channel.id, role.id).await.unwrap();
    client.delete_role(guild.id, role.id).await.unwrap();
    assert!(client
        .list_roles(guild.id)
        .await
        .unwrap()
        .iter()
        .all(|r| r.id != role.id));
}
