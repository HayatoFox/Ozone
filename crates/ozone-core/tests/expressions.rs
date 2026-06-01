//! E2E : CRUD des expressions d'une guilde (emojis / stickers / sons de soundboard) via
//! `ApiClient`. Suit le patron de `tests/guild.rs`.

mod common;
use common::{register_with_guild, spawn_server};
use ozone_core::proto::dto::{
    CreateEmoji, CreateSound, CreateSticker, UpdateEmoji, UpdateSound, UpdateSticker,
};

#[tokio::test]
async fn expressions_crud() {
    let base = spawn_server().await;
    // Le créateur de la guilde en est propriétaire : il possède toutes les permissions
    // (création et gestion des expressions), donc aucune vérification ne le bloque ici.
    let (client, guild) = register_with_guild(&base, "gildane", "Forge").await;

    // ─────────────────────────────── Emoji ───────────────────────────────
    let emoji = client
        .create_emoji(
            guild.id,
            &CreateEmoji {
                name: "forgeron".into(),
                animated: false,
                image_id: "asset-emoji-1".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(emoji.name, "forgeron");
    assert_eq!(emoji.guild_id, guild.id);
    assert_eq!(emoji.image_id, "asset-emoji-1");
    assert!(emoji.available);
    // Présent dans la liste.
    assert!(client
        .list_emojis(guild.id)
        .await
        .unwrap()
        .iter()
        .any(|e| e.id == emoji.id));
    // Mise à jour (renommage).
    let emoji = client
        .update_emoji(
            guild.id,
            emoji.id,
            &UpdateEmoji {
                name: Some("maitre_forgeron".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(emoji.name, "maitre_forgeron");
    // Suppression : disparaît de la liste.
    client.delete_emoji(guild.id, emoji.id).await.unwrap();
    assert!(client
        .list_emojis(guild.id)
        .await
        .unwrap()
        .iter()
        .all(|e| e.id != emoji.id));

    // ─────────────────────────────── Sticker ───────────────────────────────
    let sticker = client
        .create_sticker(
            guild.id,
            &CreateSticker {
                name: "enclume".into(),
                description: Some("une enclume".into()),
                tags: Some("forge".into()),
                format_type: 1,
                asset_id: "asset-sticker-1".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(sticker.name, "enclume");
    assert_eq!(sticker.guild_id, guild.id);
    assert_eq!(sticker.asset_id, "asset-sticker-1");
    assert_eq!(sticker.format_type, 1);
    assert!(sticker.available);
    // Présent dans la liste.
    assert!(client
        .list_stickers(guild.id)
        .await
        .unwrap()
        .iter()
        .any(|s| s.id == sticker.id));
    // Mise à jour (renommage + description).
    let sticker = client
        .update_sticker(
            guild.id,
            sticker.id,
            &UpdateSticker {
                name: Some("enclume-2".into()),
                description: Some("une grande enclume".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(sticker.name, "enclume-2");
    assert_eq!(sticker.description.as_deref(), Some("une grande enclume"));
    // Suppression : disparaît de la liste.
    client.delete_sticker(guild.id, sticker.id).await.unwrap();
    assert!(client
        .list_stickers(guild.id)
        .await
        .unwrap()
        .iter()
        .all(|s| s.id != sticker.id));

    // ───────────────────────────── Son (soundboard) ─────────────────────────────
    let sound = client
        .create_sound(
            guild.id,
            &CreateSound {
                name: "marteau".into(),
                sound_id: "asset-sound-1".into(),
                volume: 0.8,
                emoji: Some("🔨".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(sound.name, "marteau");
    assert_eq!(sound.guild_id, guild.id);
    assert_eq!(sound.sound_id, "asset-sound-1");
    assert_eq!(sound.volume, 0.8);
    assert!(sound.available);
    // Présent dans la liste.
    assert!(client
        .list_sounds(guild.id)
        .await
        .unwrap()
        .iter()
        .any(|s| s.id == sound.id));
    // Mise à jour (renommage + volume).
    let sound = client
        .update_sound(
            guild.id,
            sound.id,
            &UpdateSound {
                name: Some("gros-marteau".into()),
                volume: Some(0.5),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(sound.name, "gros-marteau");
    assert_eq!(sound.volume, 0.5);
    // Suppression : disparaît de la liste.
    client.delete_sound(guild.id, sound.id).await.unwrap();
    assert!(client
        .list_sounds(guild.id)
        .await
        .unwrap()
        .iter()
        .all(|s| s.id != sound.id));
}
