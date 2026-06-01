//! Bindings `ApiClient` — **actions sur les messages** : édition, suppression (unitaire et en
//! masse), réactions, épingles, indicateur de frappe.
//! Cf. routes `routes_messages`. Suit le patron de `client_guild`.

use crate::proto::dto::{BulkDelete, EditMessage, Message};
use crate::proto::Snowflake;
use crate::ApiClient;
use anyhow::Result;

/// Encode un **segment de chemin** d'URL en percent-encoding (RFC 3986) : tous les octets sont
/// échappés sauf les caractères **non réservés** `A-Z a-z 0-9 - _ . ~`.
///
/// Indispensable pour le segment `:emoji` : un emoji unicode (« 👍 ») ou la forme `nom:id` d'un
/// emoji personnalisé contient des octets non sûrs dans un chemin (multi-octets UTF-8, `:`, et
/// potentiellement `/`, `?`, `#`, `%`…). Sans échappement, on risquerait une **injection de
/// chemin/requête** (p. ex. un `/`, un `..` ou un `?` traversant ou détournant la route) au lieu
/// d'un simple segment opaque. On encode donc octet par octet (sur les octets UTF-8), ce qui rend
/// le segment inviolable côté client ; le serveur le décode et le lie en requête paramétrée.
fn encode_path_segment(s: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => {
                out.push('%');
                out.push(HEX[(b >> 4) as usize] as char);
                out.push(HEX[(b & 0x0F) as usize] as char);
            }
        }
    }
    out
}

impl ApiClient {
    /// `PATCH /channels/:channel_id/messages/:message_id` — édite un message (auteur requis côté
    /// serveur). Renvoie le message mis à jour.
    pub async fn edit_message(
        &self,
        channel_id: Snowflake,
        message_id: Snowflake,
        edit: &EditMessage,
    ) -> Result<Message> {
        self.patch(
            &format!("/channels/{channel_id}/messages/{message_id}"),
            edit,
        )
        .await
    }

    /// `DELETE /channels/:channel_id/messages/:message_id` — supprime un message (auteur ou
    /// `MANAGE_MESSAGES`).
    pub async fn delete_message(&self, channel_id: Snowflake, message_id: Snowflake) -> Result<()> {
        self.delete_unit(&format!("/channels/{channel_id}/messages/{message_id}"))
            .await
    }

    /// `POST /channels/:channel_id/messages/bulk-delete` — supprime plusieurs messages d'un coup
    /// (1 à 100, `MANAGE_MESSAGES` requis).
    pub async fn bulk_delete(&self, channel_id: Snowflake, delete: &BulkDelete) -> Result<()> {
        self.post_unit(
            &format!("/channels/{channel_id}/messages/bulk-delete"),
            delete,
        )
        .await
    }

    /// `PUT /channels/:channel_id/messages/:message_id/reactions/:emoji/@me` — ajoute sa réaction.
    ///
    /// `emoji` est un emoji unicode (« 👍 ») ou la forme `nom:id` d'un emoji personnalisé ; il est
    /// **percent-encodé** dans le chemin (cf. [`encode_path_segment`]).
    pub async fn add_reaction(
        &self,
        channel_id: Snowflake,
        message_id: Snowflake,
        emoji: &str,
    ) -> Result<()> {
        let emoji = encode_path_segment(emoji);
        self.put_unit(
            &format!("/channels/{channel_id}/messages/{message_id}/reactions/{emoji}/@me"),
            serde_json::json!({}),
        )
        .await
    }

    /// `DELETE /channels/:channel_id/messages/:message_id/reactions/:emoji/@me` — retire sa
    /// réaction. `emoji` est **percent-encodé** dans le chemin (cf. [`encode_path_segment`]).
    pub async fn remove_reaction(
        &self,
        channel_id: Snowflake,
        message_id: Snowflake,
        emoji: &str,
    ) -> Result<()> {
        let emoji = encode_path_segment(emoji);
        self.delete_unit(&format!(
            "/channels/{channel_id}/messages/{message_id}/reactions/{emoji}/@me"
        ))
        .await
    }

    /// `GET /channels/:channel_id/pins` — liste les messages épinglés d'un salon.
    pub async fn list_pins(&self, channel_id: Snowflake) -> Result<Vec<Message>> {
        self.get(&format!("/channels/{channel_id}/pins")).await
    }

    /// `PUT /channels/:channel_id/pins/:message_id` — épingle un message (`PIN_MESSAGES` requis).
    pub async fn pin_message(&self, channel_id: Snowflake, message_id: Snowflake) -> Result<()> {
        self.put_unit(
            &format!("/channels/{channel_id}/pins/{message_id}"),
            serde_json::json!({}),
        )
        .await
    }

    /// `DELETE /channels/:channel_id/pins/:message_id` — désépingle un message (`PIN_MESSAGES`).
    pub async fn unpin_message(&self, channel_id: Snowflake, message_id: Snowflake) -> Result<()> {
        self.delete_unit(&format!("/channels/{channel_id}/pins/{message_id}"))
            .await
    }

    /// `POST /channels/:channel_id/typing` — signale que l'on est en train d'écrire.
    pub async fn typing(&self, channel_id: Snowflake) -> Result<()> {
        self.post_unit(
            &format!("/channels/{channel_id}/typing"),
            serde_json::json!({}),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::encode_path_segment;

    #[test]
    fn unreserved_chars_are_untouched() {
        assert_eq!(encode_path_segment("aZ09-_.~"), "aZ09-_.~");
    }

    #[test]
    fn unicode_emoji_is_percent_encoded() {
        // 👍 = U+1F44D = F0 9F 91 8D en UTF-8.
        assert_eq!(encode_path_segment("👍"), "%F0%9F%91%8D");
    }

    #[test]
    fn path_and_query_injection_is_neutralised() {
        // Tentative d'évasion de segment via `/`, `..`, `?`, `#`, `:` et un octet NUL.
        let evil = "a/@me/../x?q=1#frag:200\0";
        let enc = encode_path_segment(evil);
        // Aucun méta-caractère de chemin/requête ne survit en clair.
        for c in ['/', '?', '#', ':', '@'] {
            assert!(!enc.contains(c), "le caractère {c:?} aurait dû être encodé");
        }
        assert_eq!(enc, "a%2F%40me%2F..%2Fx%3Fq%3D1%23frag%3A200%00");
    }
}
