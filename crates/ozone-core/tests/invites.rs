//! E2E : invitations de guilde via `ApiClient` (création, aperçu, jonction, liste, révocation).
//! Suit le patron de `tests/guild.rs`.
//!
//! Scénario à deux utilisateurs : alice (propriétaire) crée une invitation, bob l'aperçoit puis
//! la rejoint, alice la retrouve dans sa liste et la révoque.

mod common;
use common::{register, register_with_guild, spawn_server};
use ozone_core::proto::dto::CreateInvite;

#[tokio::test]
async fn invite_create_preview_join_list_revoke() {
    let base = spawn_server().await;

    // alice possède la guilde : elle détient CREATE_INSTANT_INVITE et MANAGE_GUILD.
    let (alice, guild) = register_with_guild(&base, "alice", "Forge").await;

    // Création d'une invitation illimitée et sans expiration (0 = illimité / jamais).
    let invite = alice
        .create_invite(
            guild.id,
            &CreateInvite {
                max_uses: 0,
                max_age: 0,
            },
        )
        .await
        .unwrap();
    assert_eq!(invite.guild_id, guild.id);
    assert_eq!(invite.uses, 0);
    let code = invite.code.clone();

    // Aperçu (n'importe quel utilisateur authentifié) : expose le nom/identifiant de la guilde.
    let bob = register(&base, "bob").await;
    let preview = bob.preview_invite(&code).await.unwrap();
    assert_eq!(preview.guild_id, guild.id);
    assert_eq!(preview.guild_name, "Forge");
    assert_eq!(preview.code, code);

    // Avant jonction : bob n'est membre d'aucune guilde.
    assert!(bob.list_guilds().await.unwrap().is_empty());

    // bob rejoint via l'invitation ; la guilde renvoyée est bien celle visée.
    let joined = bob.join_invite(&code).await.unwrap();
    assert_eq!(joined.id, guild.id);

    // bob est désormais membre : la guilde figure dans ses guildes.
    assert!(
        bob.list_guilds()
            .await
            .unwrap()
            .iter()
            .any(|g| g.id == guild.id),
        "bob doit être membre de la guilde après la jonction"
    );

    // Côté alice : l'invitation apparaît dans la liste, et l'usage a été incrémenté.
    let invites = alice.list_invites(guild.id).await.unwrap();
    let listed = invites
        .iter()
        .find(|i| i.code == code)
        .expect("l'invitation doit figurer dans la liste");
    assert_eq!(listed.uses, 1);

    // Révocation par alice (créatrice) : l'invitation n'est plus utilisable ni aperçue.
    alice.revoke_invite(&code).await.unwrap();
    assert!(
        bob.preview_invite(&code).await.is_err(),
        "une invitation révoquée ne doit plus être aperçue"
    );
}
