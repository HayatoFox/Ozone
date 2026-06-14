//! E2E : présence, profil, réglages & compte via `ApiClient` (patron des tests `client_*`).

mod common;
use common::{register, register_with_guild, spawn_server};
use ozone_core::proto::dto::{
    ChangeEmail, ChangePassword, DeleteAccount, SetPresence, UpdateProfile, UserSettings,
};
use ozone_core::ApiClient;

#[tokio::test]
async fn presence_profile_settings_account_flow() {
    let base = spawn_server().await;
    let client = register(&base, "alice").await;

    // ── Présence : passer en « dnd » avec un statut personnalisé. ──
    let pres = client
        .set_presence(&SetPresence {
            status: "dnd".into(),
            custom_status: Some(Some("en réunion".into())),
        })
        .await
        .unwrap();
    assert_eq!(pres.status, "dnd");
    assert_eq!(pres.custom_status.as_deref(), Some("en réunion"));

    // ── Profil : définir le nom affiché, vérifier via la valeur renvoyée et via `me()`. ──
    let profile = client
        .update_profile(&UpdateProfile {
            display_name: Some("Alice L.".into()),
            bio: Some("salut".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(profile.display_name.as_deref(), Some("Alice L."));
    assert_eq!(profile.bio.as_deref(), Some("salut"));
    let me = client.me().await.unwrap();
    assert_eq!(me.display_name.as_deref(), Some("Alice L."));

    // ── Réglages : aller-retour GET puis PUT d'un blob JSON. ──
    let initial = client.get_settings().await.unwrap();
    assert!(initial.data.is_object());
    let saved = client
        .put_settings(&UserSettings {
            data: serde_json::json!({ "theme": "dark", "compact": true }),
        })
        .await
        .unwrap();
    assert_eq!(saved.data["theme"], "dark");
    // Relecture : le serveur a bien persisté le blob.
    let reread = client.get_settings().await.unwrap();
    assert_eq!(reread.data["theme"], "dark");
    assert_eq!(reread.data["compact"], true);

    // ── Changement d'e-mail (ré-auth par le mot de passe d'inscription). ──
    let updated = client
        .change_email(&ChangeEmail {
            password: "motdepasse-123".into(),
            new_email: "alice2@it.test".into(),
        })
        .await
        .unwrap();
    assert_eq!(updated.email.as_deref(), Some("alice2@it.test"));
    // Mauvais mot de passe ⇒ refus côté serveur.
    assert!(client
        .change_email(&ChangeEmail {
            password: "mauvais".into(),
            new_email: "alice3@it.test".into(),
        })
        .await
        .is_err());

    // ── Changement de mot de passe : le serveur révoque les sessions. ──
    client
        .change_password(&ChangePassword {
            current_password: "motdepasse-123".into(),
            new_password: "nouveau-motdepasse-456".into(),
        })
        .await
        .unwrap();

    // L'ancien mot de passe ne fonctionne plus ; le nouveau, oui (via un client neuf).
    let fresh = ApiClient::new(&base);
    assert!(fresh.login("alice", "motdepasse-123").await.is_err());
    let tokens = fresh
        .login("alice", "nouveau-motdepasse-456")
        .await
        .unwrap();
    assert!(!tokens.access_token.is_empty());
}

#[tokio::test]
async fn list_presences_returns_vec() {
    let base = spawn_server().await;
    let (client, guild) = register_with_guild(&base, "bob", "Salon").await;
    // L'appel doit aboutir (le statut par défaut hors-ligne peut être filtré : Vec éventuellement vide).
    let presences = client.list_presences(guild.id).await.unwrap();
    let _: usize = presences.len();
}

#[tokio::test]
async fn delete_account_requires_password_and_invalidates_user() {
    let base = spawn_server().await;
    let client = register(&base, "carol").await;

    // Mauvais mot de passe ⇒ la suppression échoue (ré-auth côté serveur).
    assert!(client
        .delete_account(&DeleteAccount {
            password: "mauvais".into(),
        })
        .await
        .is_err());

    // Bon mot de passe ⇒ suppression (anonymisation) réussie.
    client
        .delete_account(&DeleteAccount {
            password: "motdepasse-123".into(),
        })
        .await
        .unwrap();

    // Le compte supprimé ne peut plus se reconnecter.
    let fresh = ApiClient::new(&base);
    assert!(fresh.login("carol", "motdepasse-123").await.is_err());
}
