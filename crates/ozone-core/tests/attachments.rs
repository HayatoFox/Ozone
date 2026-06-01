//! E2E : téléversement + téléchargement d'une pièce jointe via `ApiClient` (multipart).

mod common;
use common::{register_with_guild, spawn_server};

#[tokio::test]
async fn upload_then_download_roundtrip() {
    let base = spawn_server().await;
    let (client, guild) = register_with_guild(&base, "archiviste", "Dépôt").await;
    let cid = client
        .list_channels(guild.id)
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.kind == 0)
        .expect("salon texte")
        .id;

    let content = b"bonjour, ceci est un fichier joint".to_vec();
    let att = client
        .upload_attachment(cid, "note.txt", "text/plain", content.clone())
        .await
        .expect("upload");
    assert_eq!(att.filename, "note.txt");
    assert_eq!(att.size, content.len() as i64);
    assert!(att.url.starts_with("/attachments/"));

    // Le téléchargement renvoie exactement les octets téléversés.
    let bytes = client
        .download_attachment(&att.url)
        .await
        .expect("download");
    assert_eq!(bytes, content);
}
