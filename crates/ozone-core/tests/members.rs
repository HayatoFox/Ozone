//! E2E : membres & modération via `ApiClient` (liste, auto-pseudo, bannissements, audit).
//! Suit le patron de `tests/guild.rs`.
//!
//! Test auto-suffisant : le propriétaire est l'unique membre. On vérifie les chemins de
//! **lecture** et d'**auto-mise à jour** ; l'expulsion/le bannissement exigent un second membre
//! ayant rejoint la guilde (via invitation), hors périmètre de ce test mono-domaine.

mod common;
use common::{register_with_guild, spawn_server};
use ozone_core::proto::dto::UpdateMember;

#[tokio::test]
async fn members_and_moderation_reads() {
    let base = spawn_server().await;
    // Le créateur de la guilde en est propriétaire et son unique membre : il possède toutes les
    // permissions, donc aucune restriction d'anti-escalade ne s'applique ici.
    let (client, guild) = register_with_guild(&base, "gildane", "Forge").await;

    // La liste des membres contient le propriétaire (joint d'office à la création).
    let members = client.list_members(guild.id).await.unwrap();
    assert!(
        members.iter().any(|m| m.user.id == guild.owner_id),
        "le propriétaire doit figurer parmi les membres"
    );

    // Auto-définition de son propre pseudo de serveur (CHANGE_NICKNAME, possédée par le proprio).
    client
        .update_member(
            guild.id,
            guild.owner_id,
            &UpdateMember {
                nick: Some("Forgeron".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    // Le pseudo est bien persisté côté serveur.
    let members = client.list_members(guild.id).await.unwrap();
    let me = members
        .iter()
        .find(|m| m.user.id == guild.owner_id)
        .expect("propriétaire présent");
    assert_eq!(me.nick.as_deref(), Some("Forgeron"));

    // Aucune sanction encore prononcée : la liste des bannissements est vide.
    assert!(client.list_bans(guild.id).await.unwrap().is_empty());

    // Le journal d'audit est lisible (VIEW_AUDIT_LOG, possédée par le proprio).
    assert!(client.list_audit_logs(guild.id).await.is_ok());
}
