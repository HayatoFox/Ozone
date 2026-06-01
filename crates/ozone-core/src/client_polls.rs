//! Bindings `ApiClient` — **sondages**. Un sondage est porté par un message du salon : création,
//! consultation des résultats, et (re)définition de ses votes (mono ou multi-sélection).
//! Cf. routes `routes_polls`. Suit le patron de `client_guild`.

use crate::proto::dto::{CastVote, CreatePoll, Poll};
use crate::proto::Snowflake;
use crate::ApiClient;
use anyhow::Result;

impl ApiClient {
    /// `POST /channels/:channel_id/polls` — crée un sondage.
    ///
    /// Le serveur insère un **message porteur** dans le salon puis renvoie le [`Poll`] : son champ
    /// `message_id` est l'identifiant à utiliser pour [`ApiClient::get_poll`] et
    /// [`ApiClient::cast_vote`].
    pub async fn create_poll(&self, channel_id: Snowflake, poll: &CreatePoll) -> Result<Poll> {
        self.post(&format!("/channels/{channel_id}/polls"), poll)
            .await
    }

    /// `GET /channels/:channel_id/polls/:message_id` — résultats d'un sondage (décomptes + le vote
    /// de l'utilisateur courant).
    pub async fn get_poll(&self, channel_id: Snowflake, message_id: Snowflake) -> Result<Poll> {
        self.get(&format!("/channels/{channel_id}/polls/{message_id}"))
            .await
    }

    /// `PUT /channels/:channel_id/polls/:message_id/votes` — (re)définit ses votes (remplace les
    /// votes précédents). Renvoie le [`Poll`] à jour.
    pub async fn cast_vote(
        &self,
        channel_id: Snowflake,
        message_id: Snowflake,
        vote: &CastVote,
    ) -> Result<Poll> {
        self.put(
            &format!("/channels/{channel_id}/polls/{message_id}/votes"),
            vote,
        )
        .await
    }
}
