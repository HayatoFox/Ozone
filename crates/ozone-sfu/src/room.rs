//! Cœur SFU : salles, pairs WebRTC, relais de pistes RTP.
//!
//! Modèle : à la connexion (`join`), le nouveau pair **reçoit** les pistes déjà publiées dans
//! la salle (ajoutées avant la réponse SDP) et **publie** ses propres pistes (`on_track`), qui
//! sont relayées aux autres pairs. La **renégociation poussée** (pour que les pairs déjà connectés
//! reçoivent un nouveau venu) se fait via la signalisation WS — étape suivante (cf. README SFU).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::{APIBuilder, API};
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::{TrackLocal, TrackLocalWriter};
use webrtc::track::track_remote::TrackRemote;

/// Un pair connecté : sa `RTCPeerConnection` et les pistes locales qu'il **publie** vers la salle.
struct Peer {
    pc: Arc<RTCPeerConnection>,
    published: Mutex<Vec<Arc<TrackLocalStaticRTP>>>,
}

#[derive(Default)]
struct Room {
    peers: HashMap<String, Arc<Peer>>,
}

/// Nœud SFU : une `API` WebRTC partagée + l'ensemble des salles.
pub struct Sfu {
    api: API,
    rooms: Mutex<HashMap<String, Room>>,
    next_id: AtomicU64,
}

impl Sfu {
    /// Construit le SFU (codecs par défaut : Opus, VP8/VP9/H264 ; intercepteurs RTCP/NACK).
    pub fn new() -> anyhow::Result<Arc<Self>> {
        let mut media = MediaEngine::default();
        media
            .register_default_codecs()
            .map_err(|e| anyhow::anyhow!("codecs: {e}"))?;
        let mut registry = webrtc::interceptor::registry::Registry::new();
        registry = register_default_interceptors(registry, &mut media)
            .map_err(|e| anyhow::anyhow!("interceptors: {e}"))?;
        let api = APIBuilder::new()
            .with_media_engine(media)
            .with_interceptor_registry(registry)
            .build();
        Ok(Arc::new(Self {
            api,
            rooms: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }))
    }

    fn config() -> RTCConfiguration {
        RTCConfiguration {
            ice_servers: vec![RTCIceServer {
                urls: vec!["stun:stun.l.google.com:19302".to_owned()],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    /// Nombre de salles actives (diagnostic / tests).
    pub async fn room_count(&self) -> usize {
        self.rooms.lock().await.len()
    }

    /// Nombre de pairs dans une salle (diagnostic / tests).
    pub async fn peer_count(&self, room_id: &str) -> usize {
        self.rooms
            .lock()
            .await
            .get(room_id)
            .map(|r| r.peers.len())
            .unwrap_or(0)
    }

    /// Connecte un pair : applique l'offre, relaie les pistes, renvoie `(peer_id, réponse SDP)`.
    pub async fn join(
        self: &Arc<Self>,
        room_id: &str,
        offer_sdp: String,
    ) -> anyhow::Result<(String, String)> {
        let pc = Arc::new(self.api.new_peer_connection(Self::config()).await?);
        let peer_id = format!("p{}", self.next_id.fetch_add(1, Ordering::Relaxed));

        // 1) Le nouveau venu reçoit les pistes déjà publiées dans la salle.
        {
            let rooms = self.rooms.lock().await;
            if let Some(room) = rooms.get(room_id) {
                for other in room.peers.values() {
                    for t in other.published.lock().await.iter() {
                        pc.add_track(t.clone() as Arc<dyn TrackLocal + Send + Sync>)
                            .await?;
                    }
                }
            }
        }

        // 2) Les pistes entrantes du nouveau venu sont relayées aux autres pairs.
        let sfu = self.clone();
        let room_key = room_id.to_string();
        let owner = peer_id.clone();
        pc.on_track(Box::new(
            move |track: Arc<TrackRemote>, _receiver, _transceiver| {
                let sfu = sfu.clone();
                let room_key = room_key.clone();
                let owner = owner.clone();
                Box::pin(async move {
                    let local = Arc::new(TrackLocalStaticRTP::new(
                        track.codec().capability,
                        format!("{owner}-{}", track.id()),
                        owner.clone(),
                    ));
                    sfu.publish_track(&room_key, &owner, local.clone()).await;
                    // Recopie les paquets RTP entrants vers la piste locale relayée.
                    tokio::spawn(async move {
                        while let Ok((pkt, _)) = track.read_rtp().await {
                            if local.write_rtp(&pkt).await.is_err() {
                                break;
                            }
                        }
                    });
                })
            },
        ));

        // 3) Offre → réponse (attente de la fin du rassemblement ICE : pas de trickle).
        let offer = RTCSessionDescription::offer(offer_sdp)?;
        pc.set_remote_description(offer).await?;
        let answer = pc.create_answer(None).await?;
        let mut gather = pc.gathering_complete_promise().await;
        pc.set_local_description(answer).await?;
        let _ = gather.recv().await;
        let local_desc = pc
            .local_description()
            .await
            .ok_or_else(|| anyhow::anyhow!("description locale absente"))?;

        // 4) Enregistre le pair dans la salle.
        {
            let mut rooms = self.rooms.lock().await;
            rooms.entry(room_id.to_string()).or_default().peers.insert(
                peer_id.clone(),
                Arc::new(Peer {
                    pc: pc.clone(),
                    published: Mutex::new(Vec::new()),
                }),
            );
        }
        Ok((peer_id, local_desc.sdp))
    }

    /// Enregistre une piste publiée par `owner` et l'ajoute aux `RTCPeerConnection` des autres pairs.
    async fn publish_track(&self, room_id: &str, owner: &str, track: Arc<TrackLocalStaticRTP>) {
        let rooms = self.rooms.lock().await;
        let Some(room) = rooms.get(room_id) else {
            return;
        };
        if let Some(p) = room.peers.get(owner) {
            p.published.lock().await.push(track.clone());
        }
        for (id, other) in room.peers.iter() {
            if id != owner {
                let _ = other
                    .pc
                    .add_track(track.clone() as Arc<dyn TrackLocal + Send + Sync>)
                    .await;
            }
        }
    }

    /// Déconnecte un pair (ferme sa `RTCPeerConnection`). Supprime la salle si elle devient vide.
    pub async fn leave(&self, room_id: &str, peer_id: &str) {
        let mut rooms = self.rooms.lock().await;
        if let Some(room) = rooms.get_mut(room_id) {
            if let Some(p) = room.peers.remove(peer_id) {
                let _ = p.pc.close().await;
            }
            if room.peers.is_empty() {
                rooms.remove(room_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sfu_builds_and_tracks_rooms() {
        // Valide la construction de l'API WebRTC (MediaEngine + intercepteurs) et le registre de salles.
        let sfu = Sfu::new().expect("construction SFU");
        assert_eq!(sfu.room_count().await, 0);
        assert_eq!(sfu.peer_count("absente").await, 0);

        // `leave` sur une salle inexistante est sans effet (pas de panique).
        sfu.leave("absente", "p1").await;
        assert_eq!(sfu.room_count().await, 0);
    }
}
