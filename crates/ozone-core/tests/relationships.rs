//! E2E : relations (amis / notes) via `ApiClient`, avec deux utilisateurs (patron `guild.rs`).

mod common;
use common::*;
use ozone_core::proto::dto::{AddRelationship, RelationshipType};

#[tokio::test]
async fn friend_request_accept_and_note_flow() {
    let base = spawn_server().await;
    let alice = register(&base, "alice").await;
    let bob = register(&base, "bob").await;

    // Alice envoie une demande d'ami à Bob (par nom d'utilisateur).
    alice
        .add_relationship(&AddRelationship {
            username: "bob".into(),
            block: false,
        })
        .await
        .unwrap();

    // Côté Alice : relation sortante vers Bob → on récupère l'id de Bob.
    let alice_rels = alice.list_relationships().await.unwrap();
    let bob_rel = alice_rels
        .iter()
        .find(|r| r.kind == RelationshipType::Outgoing)
        .expect("demande sortante vers Bob");
    assert_eq!(bob_rel.user.username, "bob");
    let bob_id = bob_rel.id;

    // Côté Bob : la demande apparaît comme entrante → on récupère l'id d'Alice.
    let bob_rels = bob.list_relationships().await.unwrap();
    let alice_rel = bob_rels
        .iter()
        .find(|r| r.kind == RelationshipType::Incoming)
        .expect("demande entrante d'Alice");
    assert_eq!(alice_rel.user.username, "alice");
    let alice_id = alice_rel.id;

    // Bob accepte la demande d'Alice.
    bob.accept_relationship(alice_id).await.unwrap();

    // Les deux sont désormais amis.
    assert!(alice
        .list_relationships()
        .await
        .unwrap()
        .iter()
        .any(|r| r.id == bob_id && r.kind == RelationshipType::Friend));
    assert!(bob
        .list_relationships()
        .await
        .unwrap()
        .iter()
        .any(|r| r.id == alice_id && r.kind == RelationshipType::Friend));

    // Notes : absente au départ, puis posée et relue.
    assert_eq!(alice.get_note(bob_id).await.unwrap(), None);
    alice.put_note(bob_id, "copain de la forge").await.unwrap();
    assert_eq!(
        alice.get_note(bob_id).await.unwrap().as_deref(),
        Some("copain de la forge")
    );

    // Suppression de la relation dans les deux sens.
    alice.remove_relationship(bob_id).await.unwrap();
    assert!(alice.list_relationships().await.unwrap().is_empty());
    assert!(bob.list_relationships().await.unwrap().is_empty());
}
