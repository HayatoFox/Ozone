//! E2E : messages privés (MP 1:1) via `ApiClient`, avec deux utilisateurs (patron `guild.rs`).

mod common;
use common::{register, spawn_server};
use ozone_core::proto::dto::CreateDM;

#[tokio::test]
async fn open_dm_and_list() {
    let base = spawn_server().await;
    let alice = register(&base, "alice").await;
    let bob = register(&base, "bob").await;

    // On apprend l'id de Bob via `GET /users/@me` (un MP se crée par id de destinataire).
    let bob_id = bob.me().await.unwrap().id;

    // Alice ouvre un MP 1:1 avec Bob.
    let dm = alice
        .open_or_create_dm(&CreateDM {
            recipients: vec![bob_id],
        })
        .await
        .unwrap();
    assert_eq!(dm.kind, 1, "un MP 1:1 doit être de type 1");
    assert!(
        dm.recipients.iter().any(|u| u.id == bob_id),
        "Bob doit figurer parmi les destinataires du MP"
    );

    // Déduplication : rouvrir le MP renvoie le même salon.
    let dm2 = alice
        .open_or_create_dm(&CreateDM {
            recipients: vec![bob_id],
        })
        .await
        .unwrap();
    assert_eq!(dm2.id, dm.id, "le MP 1:1 doit être dédupliqué");

    // Le MP apparaît dans la liste des salons de MP d'Alice.
    let channels = alice.list_dm_channels().await.unwrap();
    assert!(
        channels.iter().any(|c| c.id == dm.id),
        "le MP doit apparaître dans la liste d'Alice"
    );

    // Et aussi dans celle de Bob (les deux participants le voient).
    assert!(
        bob.list_dm_channels()
            .await
            .unwrap()
            .iter()
            .any(|c| c.id == dm.id),
        "le MP doit apparaître dans la liste de Bob"
    );
}
