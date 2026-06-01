//! Bindings `ApiClient` — **pièces jointes** : téléversement multipart + téléchargement binaire.
//! Cf. routes `routes_attachments`.

use crate::proto::dto::Attachment;
use crate::proto::Snowflake;
use crate::ApiClient;
use anyhow::{anyhow, Result};

impl ApiClient {
    /// `POST /channels/:id/attachments` — téléverse un fichier (champ multipart `file`).
    /// Renvoie la pièce jointe créée (à attacher ensuite à un message via `attachment_ids`).
    pub async fn upload_attachment(
        &self,
        channel_id: Snowflake,
        filename: &str,
        content_type: &str,
        bytes: Vec<u8>,
    ) -> Result<Attachment> {
        let part = reqwest::multipart::Part::bytes(bytes)
            .file_name(filename.to_string())
            .mime_str(content_type)?;
        let form = reqwest::multipart::Form::new().part("file", part);
        let rb = self
            .http()
            .post(self.url(&format!("/channels/{channel_id}/attachments")))
            .multipart(form);
        self.send_json(rb).await
    }

    /// `GET /attachments/:id/:filename` — télécharge le contenu binaire d'une pièce jointe.
    /// `path` est le `url` renvoyé par [`ApiClient::upload_attachment`] (`/attachments/<id>/<nom>`).
    pub async fn download_attachment(&self, path: &str) -> Result<Vec<u8>> {
        let resp = self.auth(self.http().get(self.url(path))).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("HTTP {status} : {body}"));
        }
        Ok(resp.bytes().await?.to_vec())
    }
}
