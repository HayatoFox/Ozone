//! E2E : administration d'instance via `ApiClient` (patron `tests/guild.rs`).
//!
//! **Détermination de l'admin (côté serveur).** Le tout premier compte inscrit sur une instance
//! devient automatiquement le **propriétaire** (`owner`) de l'instance : le handler `register`
//! (cf. `routes_auth`) compte les comptes après insertion et attribue le rôle d'instance `owner`
//! si `count == 1`, sinon `user`. Les routes `/instance/admin/*` exigent `require_instance_admin`
//! (rôles `owner` ou `admin`), et `set_instance_role` exige `require_instance_owner` (rôle `owner`
//! uniquement). Le premier utilisateur étant `owner`, il a accès à toutes ces routes.
//!
//! Scénario : alice (1ʳᵉ inscrite ⇒ propriétaire) lit la config, se voit dans la liste des
//! comptes, crée puis révoque une invitation d'instance ; un second compte (bob) est inscrit, sur
//! lequel alice exerce suspension et changement de rôle. Enfin, on vérifie que bob (simple `user`)
//! se voit refuser (403) l'accès à ces routes privilégiées.

mod common;
use common::{register, spawn_server};
use ozone_core::proto::dto::{CreateInstanceInvite, SetInstanceRole, SetSuspended};

#[tokio::test]
async fn instance_admin_config_invites_users_flow() {
    let base = spawn_server().await;

    // 1ᵉʳ compte inscrit ⇒ propriétaire d'instance (admin de fait).
    let admin = register(&base, "alice").await;

    // Config accessible au propriétaire ; expose le nom de l'instance défini dans la config de test.
    let config = admin.get_config().await.unwrap();
    assert_eq!(config["name"], "IT");
    assert!(config.get("registration_policy").is_some());

    // La liste des comptes contient le propriétaire avec le rôle « owner ».
    let users = admin.list_instance_users().await.unwrap();
    let alice_view = users
        .iter()
        .find(|u| u.user.username == "alice")
        .expect("le propriétaire doit figurer dans la liste des comptes");
    assert_eq!(alice_view.role, "owner");
    assert!(!alice_view.suspended);

    // Création d'une invitation d'instance illimitée et sans expiration (0 = illimité / jamais).
    let invite = admin
        .create_instance_invite(&CreateInstanceInvite {
            max_uses: 0,
            max_age: 0,
        })
        .await
        .unwrap();
    assert_eq!(invite.uses, 0);
    assert_eq!(invite.created_by, alice_view.user.id);
    let code = invite.code.clone();

    // L'invitation figure dans la liste, puis sa révocation la fait disparaître.
    assert!(
        admin
            .list_instance_invites()
            .await
            .unwrap()
            .iter()
            .any(|i| i.code == code),
        "l'invitation d'instance doit figurer dans la liste"
    );
    admin.revoke_instance_invite(&code).await.unwrap();
    assert!(
        !admin
            .list_instance_invites()
            .await
            .unwrap()
            .iter()
            .any(|i| i.code == code),
        "l'invitation révoquée ne doit plus figurer dans la liste"
    );
    // Révoquer une invitation inexistante renvoie une erreur (404 côté serveur).
    assert!(admin.revoke_instance_invite(&code).await.is_err());

    // Second compte (simple utilisateur) ; on récupère son id via la liste admin.
    let bob = register(&base, "bob").await;
    let bob_id = admin
        .list_instance_users()
        .await
        .unwrap()
        .into_iter()
        .find(|u| u.user.username == "bob")
        .expect("bob doit figurer dans la liste des comptes")
        .user
        .id;

    // Routes privilégiées refusées à un simple utilisateur (bob) : 403 → Err côté client.
    assert!(
        bob.get_config().await.is_err(),
        "un simple utilisateur ne doit pas lire la config d'instance"
    );
    assert!(bob.list_instance_users().await.is_err());
    assert!(bob.list_instance_invites().await.is_err());
    assert!(bob
        .create_instance_invite(&CreateInstanceInvite::default())
        .await
        .is_err());
    assert!(bob
        .set_suspended(bob_id, &SetSuspended { suspended: true })
        .await
        .is_err());
    assert!(bob
        .set_instance_role(
            bob_id,
            &SetInstanceRole {
                role: "admin".into()
            }
        )
        .await
        .is_err());

    // Le propriétaire change le rôle d'instance de bob (set_instance_role exige « owner »).
    admin
        .set_instance_role(
            bob_id,
            &SetInstanceRole {
                role: "moderator".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        admin
            .list_instance_users()
            .await
            .unwrap()
            .iter()
            .find(|u| u.user.id == bob_id)
            .unwrap()
            .role,
        "moderator"
    );

    // Le propriétaire suspend puis réactive le compte de bob.
    admin
        .set_suspended(bob_id, &SetSuspended { suspended: true })
        .await
        .unwrap();
    assert!(
        admin
            .list_instance_users()
            .await
            .unwrap()
            .iter()
            .find(|u| u.user.id == bob_id)
            .unwrap()
            .suspended,
        "bob doit être marqué suspendu"
    );
    admin
        .set_suspended(bob_id, &SetSuspended { suspended: false })
        .await
        .unwrap();
    assert!(
        !admin
            .list_instance_users()
            .await
            .unwrap()
            .iter()
            .find(|u| u.user.id == bob_id)
            .unwrap()
            .suspended,
        "bob doit être réactivé"
    );
}
