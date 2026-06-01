//! E2E : sondages (création, consultation, vote) via `ApiClient`. Suit le patron de `tests/guild.rs`.

mod common;
use common::{register_with_guild, spawn_server};
use ozone_core::proto::dto::{CastVote, CreatePoll};

#[tokio::test]
async fn poll_lifecycle_e2e() {
    let base = spawn_server().await;
    let (client, guild) = register_with_guild(&base, "gildane", "Forge").await;

    // Salon texte par défaut (« général », type 0) créé avec la guilde.
    let channels = client.list_channels(guild.id).await.unwrap();
    let channel = channels
        .into_iter()
        .find(|c| c.kind == 0)
        .expect("salon texte par défaut");

    // Création d'un sondage (le serveur insère un message porteur et renvoie le sondage).
    let poll = client
        .create_poll(
            channel.id,
            &CreatePoll {
                question: "Quel langage ?".into(),
                answers: vec!["Rust".into(), "Go".into(), "Zig".into()],
                multiselect: false,
                duration_hours: Some(24),
            },
        )
        .await
        .unwrap();
    assert_eq!(poll.channel_id, channel.id);
    assert_eq!(poll.question, "Quel langage ?");
    assert!(!poll.multiselect);
    assert!(!poll.finished);
    assert_eq!(poll.answers.len(), 3);
    // Les réponses sont numérotées à partir de 1 et sans vote au départ.
    assert_eq!(poll.answers[0].answer_id, 1);
    assert_eq!(poll.answers[0].text, "Rust");
    assert!(poll
        .answers
        .iter()
        .all(|a| a.vote_count == 0 && !a.me_voted));

    // L'identifiant du message porteur du sondage sert de clé pour les autres requêtes.
    let message_id = poll.message_id;

    // Consultation : on retrouve le même sondage.
    let fetched = client.get_poll(channel.id, message_id).await.unwrap();
    assert_eq!(fetched.message_id, message_id);
    assert_eq!(fetched.answers.len(), 3);

    // Vote pour la première réponse (answer_id == 1).
    let voted = client
        .cast_vote(
            channel.id,
            message_id,
            &CastVote {
                answer_ids: vec![1],
            },
        )
        .await
        .unwrap();
    let a1 = voted
        .answers
        .iter()
        .find(|a| a.answer_id == 1)
        .expect("réponse 1");
    assert_eq!(a1.vote_count, 1);
    assert!(a1.me_voted);

    // get_poll reflète le décompte de vote.
    let after = client.get_poll(channel.id, message_id).await.unwrap();
    let a1 = after
        .answers
        .iter()
        .find(|a| a.answer_id == 1)
        .expect("réponse 1");
    assert_eq!(a1.vote_count, 1);
    assert!(a1.me_voted);
    // Les autres réponses restent à zéro.
    assert!(after
        .answers
        .iter()
        .filter(|a| a.answer_id != 1)
        .all(|a| a.vote_count == 0 && !a.me_voted));
}
