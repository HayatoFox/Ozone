//! Bindings `ApiClient` — **recherche de messages** (plein-texte FTS5, par salon ou par guilde).
//! Cf. routes `routes_messages` (`search_guild`, `search_channel`). Suit le patron de `client_guild`.

use crate::proto::dto::SearchResponse;
use crate::proto::Snowflake;
use crate::ApiClient;
use anyhow::Result;

/// Encode une valeur pour l'insérer **sans danger** dans une chaîne de requête (`?clé=valeur`).
///
/// Il n'existe pas de crate d'URL-encoding dans `ozone-core` : on applique un percent-encoding
/// minimal façon RFC 3986 (« unreserved »). Tout octet hors `A-Z a-z 0-9 - _ . ~` est échappé en
/// `%XX`. Cela neutralise notamment l'espace, `&`, `=`, `?`, `#` et `+`, qui sinon casseraient la
/// valeur (un terme de recherche ne peut donc pas s'évader pour injecter un autre paramètre).
fn percent_encode(value: &str) -> String {
    // 3 octets de sortie au maximum par octet d'entrée (`%XX`).
    let mut out = String::with_capacity(value.len() * 3);
    for &byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                out.push('%');
                out.push(HEX[(byte >> 4) as usize] as char);
                out.push(HEX[(byte & 0x0f) as usize] as char);
            }
        }
    }
    out
}

impl ApiClient {
    /// `GET /guilds/:guild_id/messages/search?q=...` — recherche plein-texte sur **toute la
    /// guilde**, restreinte aux salons que l'utilisateur peut lire. Le terme `query` est
    /// percent-encodé dans la chaîne de requête.
    pub async fn search_guild(&self, guild_id: Snowflake, query: &str) -> Result<SearchResponse> {
        self.get(&format!(
            "/guilds/{guild_id}/messages/search?q={}",
            percent_encode(query)
        ))
        .await
    }

    /// `GET /channels/:channel_id/messages/search?q=...` — recherche plein-texte dans **un seul
    /// salon** (de guilde ou MP). Le terme `query` est percent-encodé dans la chaîne de requête.
    pub async fn search_channel(
        &self,
        channel_id: Snowflake,
        query: &str,
    ) -> Result<SearchResponse> {
        self.get(&format!(
            "/channels/{channel_id}/messages/search?q={}",
            percent_encode(query)
        ))
        .await
    }
}
