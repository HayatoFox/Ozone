//! Cœur SFU : salles, pairs WebRTC, relais de pistes RTP, **renégociation** (negotiation parfaite).
//!
//! Modèle : à la connexion (`join`, HTTP), le nouveau pair **reçoit** les pistes déjà publiées et
//! **publie** les siennes (`on_track`), relayées aux autres. Ensuite, un **canal WebSocket** par
//! pair permet la **renégociation poussée** : quand une piste est ajoutée/retirée, le SFU émet une
//! nouvelle offre (sens descendant) ou répond à l'offre du client (sens montant). Chaque pair a une
//! **tâche-acteur** qui sérialise toute sa signalisation (aucun `create_offer`/`set_remote` concurrent).
//! Le SFU est « impoli » (ignore l'offre client en cas de collision) ; le client est « poli ».

use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::{APIBuilder, API};
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::signaling_state::RTCSignalingState;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtcp::payload_feedbacks::full_intra_request::FullIntraRequest;
use webrtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use webrtc::rtp_transceiver::rtp_sender::RTCRtpSender;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::{TrackLocal, TrackLocalWriter};
use webrtc::track::track_remote::TrackRemote;

/// Demande une **keyframe** (PLI) au publieur d'une piste vidéo identifiée par `ssrc`. Sans cela,
/// une vidéo dont la keyframe est perdue (perte réseau) OU un nouvel abonné restent FIGÉS jusqu'à
/// la prochaine keyframe spontanée (parfois jamais). On force donc le publieur à en émettre une.
async fn request_keyframe(pc: &Arc<RTCPeerConnection>, ssrc: u32) {
    let pli = PictureLossIndication {
        sender_ssrc: 0,
        media_ssrc: ssrc,
    };
    let _ = pc.write_rtcp(&[Box::new(pli)]).await;
}

/// Relaie en continu les demandes de keyframe (PLI/FIR) d'un ABONNÉ vers le PUBLIEUR de la piste.
/// Quand le décodeur d'un spectateur perd le fil (paquets manquants), il émet un PLI ; le SFU le
/// transmet au publieur qui régénère une keyframe → la vidéo se « répare » au lieu de figer.
fn spawn_keyframe_forwarder(
    sender: Arc<RTCRtpSender>,
    publisher_pc: Arc<RTCPeerConnection>,
    ssrc: u32,
) {
    tokio::spawn(async move {
        loop {
            match sender.read_rtcp().await {
                Ok((packets, _)) => {
                    let wants_kf = packets.iter().any(|p| {
                        let a = p.as_any();
                        a.downcast_ref::<PictureLossIndication>().is_some()
                            || a.downcast_ref::<FullIntraRequest>().is_some()
                    });
                    if wants_kf {
                        request_keyframe(&publisher_pc, ssrc).await;
                    }
                }
                Err(_) => break,
            }
        }
    });
}

/// Filtre la nature de piste demandée par le client par **liste blanche** : seules trois valeurs
/// connues sont admises (sinon repli par défaut selon le média). Garantit qu'aucune chaîne
/// arbitraire issue du client ne se retrouve dans le `stream_id` (donc dans le SDP relayé).
fn sanitize_kind(requested: Option<&String>, is_video: bool) -> &'static str {
    match requested.map(String::as_str) {
        Some("screen") => "screen",
        Some("cam") => "cam",
        Some("mic") => "mic",
        // Son de la source partagée (audio du partage d'écran), distinct du micro côté récepteur.
        Some("screen_audio") => "screen_audio",
        _ if is_video => "cam",
        _ => "mic",
    }
}

/// Commandes envoyées à la tâche-acteur d'un pair (toute sa signalisation passe par là).
pub enum PeerCmd {
    /// Un WebSocket de signalisation s'est connecté ; voici par où lui envoyer des messages.
    WsConnected(mpsc::UnboundedSender<String>),
    /// Le WebSocket s'est fermé (le pair reste, la renégociation est différée).
    WsClosed,
    /// Message JSON brut reçu du client (`offer` / `answer` / `unpublish`).
    ClientMsg(String),
    /// Le SFU a modifié les pistes relayées vers ce pair → (ré)émettre une offre.
    Renegotiate,
}

/// Un pair connecté : sa `RTCPeerConnection`, son propriétaire (uid issu du jeton vérifié), les
/// pistes qu'il **publie**, les `senders` des pistes qu'il **reçoit** (pour pouvoir les retirer),
/// le manifeste `id_de_piste → nature` (mutable, mis à jour aux renégociations) et le canal acteur.
struct Peer {
    owner: String,
    pc: Arc<RTCPeerConnection>,
    published: Mutex<Vec<Arc<TrackLocalStaticRTP>>>,
    relayed: Mutex<HashMap<String, Arc<RTCRtpSender>>>,
    // SSRC distant des pistes VIDÉO publiées par ce pair (local_track_id → ssrc), pour adresser
    // les demandes de keyframe (PLI) au bon flux.
    pub_video_ssrc: Mutex<HashMap<String, u32>>,
    cmd_tx: mpsc::UnboundedSender<PeerCmd>,
    // « Pierres tombales » : `local_track_id` (id navigateur) dont l'`unpublish` est arrivé AVANT que
    // la piste correspondante ne soit captée par `on_track`. Sans cela, une activation/désactivation
    // rapide de la caméra (offre montante retardée par l'ICE, unpublish synchrone qui passe devant)
    // relaierait une piste déjà retirée, jamais nettoyée → tuile figée chez les autres.
    tombstones: Mutex<HashSet<String>>,
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

    // Connexion ultra-rapide : **pas de STUN**. Les candidats hôtes (localhost / LAN) suffisent et
    // sont disponibles quasi instantanément → le rassemblement ICE se termine sans aller-retour
    // externe (join ≤1 s). Compromis assumé : pas de traversée NAT inter-réseaux (rétablir STUN/TURN
    // pour un déploiement hors LAN).
    fn config() -> RTCConfiguration {
        RTCConfiguration::default()
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

    /// Connecte un pair (identité `uid` déjà authentifiée par le jeton vocal) : applique l'offre,
    /// relaie les pistes, renvoie `(peer_id, réponse SDP)`. Démarre la tâche-acteur de signalisation.
    pub async fn join(
        self: &Arc<Self>,
        room_id: &str,
        uid: &str,
        offer_sdp: String,
        manifest: HashMap<String, String>,
    ) -> anyhow::Result<(String, String)> {
        let pc = Arc::new(self.api.new_peer_connection(Self::config()).await?);
        let peer_id = format!("p{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let manifest = Arc::new(Mutex::new(manifest));

        // 1) Le nouveau venu reçoit les pistes déjà publiées dans la salle (incluses dans la réponse).
        // On MÉMORISE les senders (track_id → sender) pour pouvoir les retirer ensuite : sinon une
        // piste reçue au join ne serait jamais retirable (départ/cam-off du publieur → tuile figée).
        let mut seeded_relayed: HashMap<String, Arc<RTCRtpSender>> = HashMap::new();
        {
            let rooms = self.rooms.lock().await;
            if let Some(room) = rooms.get(room_id) {
                for other in room.peers.values() {
                    let vssrc = other.pub_video_ssrc.lock().await;
                    for t in other.published.lock().await.iter() {
                        let tid = TrackLocal::id(t.as_ref()).to_string();
                        if let Ok(sender) = pc
                            .add_track(t.clone() as Arc<dyn TrackLocal + Send + Sync>)
                            .await
                        {
                            // Vidéo : route les PLI de ce nouvel abonné vers le publieur → il reçoit
                            // une keyframe dès sa connexion (sinon tuile noire/figée à l'arrivée).
                            if let Some(&ssrc) = vssrc.get(&tid) {
                                spawn_keyframe_forwarder(sender.clone(), other.pc.clone(), ssrc);
                            }
                            seeded_relayed.insert(tid, sender);
                        }
                    }
                }
            }
        }

        // 2) Pistes entrantes du nouveau venu → relayées aux autres. `stream_id` = « <uid>.<kind> ».
        let sfu = self.clone();
        let room_key = room_id.to_string();
        let peer_key = peer_id.clone();
        let uid_tag = uid.to_string();
        let manifest_for_track = manifest.clone();
        pc.on_track(Box::new(
            move |track: Arc<TrackRemote>, _receiver, _transceiver| {
                let sfu = sfu.clone();
                let room_key = room_key.clone();
                let peer_key = peer_key.clone();
                let uid_tag = uid_tag.clone();
                let manifest = manifest_for_track.clone();
                Box::pin(async move {
                    let mime = track.codec().capability.mime_type.to_lowercase();
                    let is_video = mime.starts_with("video/");
                    let kind = {
                        let m = manifest.lock().await;
                        sanitize_kind(m.get(&track.id()), is_video)
                    };
                    let local = Arc::new(TrackLocalStaticRTP::new(
                        track.codec().capability,
                        format!("{uid_tag}.{kind}.{}", track.id()),
                        format!("{uid_tag}.{kind}"),
                    ));
                    // SSRC du flux vidéo entrant : sert à router les demandes de keyframe (PLI).
                    let video_ssrc = if is_video { Some(track.ssrc()) } else { None };
                    sfu.publish_track(&room_key, &peer_key, local.clone(), video_ssrc)
                        .await;
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

        // 2bis) Mort brutale (onglet fermé sans `leave`, perte réseau, ICE qui tombe) : on purge le
        // pair pour ne pas laisser de pair fantôme ni de pistes relayées fantômes chez les autres.
        {
            let sfu = self.clone();
            let room_key = room_id.to_string();
            let peer_key = peer_id.clone();
            pc.on_peer_connection_state_change(Box::new(move |state: RTCPeerConnectionState| {
                let sfu = sfu.clone();
                let room_key = room_key.clone();
                let peer_key = peer_key.clone();
                Box::pin(async move {
                    if matches!(
                        state,
                        RTCPeerConnectionState::Failed
                            | RTCPeerConnectionState::Disconnected
                            | RTCPeerConnectionState::Closed
                    ) {
                        // Détaché (`spawn`) : ce handler peut être exécuté INLINE par `pc.close()`,
                        // lui-même appelé depuis purge_peer ; un `await` direct rentrerait dans
                        // purge_peer sur la même tâche. Le spawn casse cette réentrance.
                        tokio::spawn(async move {
                            sfu.purge_peer(&room_key, &peer_key).await;
                        });
                    }
                })
            }));
        }

        // 3) Tâche-acteur de signalisation (sérialise offer/answer/renégociation).
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<PeerCmd>();

        // 4) Enregistre le pair AVANT la négociation (pour que `on_track`/`publish_track` le trouvent).
        {
            let mut rooms = self.rooms.lock().await;
            rooms.entry(room_id.to_string()).or_default().peers.insert(
                peer_id.clone(),
                Arc::new(Peer {
                    owner: uid.to_string(),
                    pc: pc.clone(),
                    published: Mutex::new(Vec::new()),
                    // Pistes reçues au join : déjà ajoutées à la PC, indexées pour pouvoir les retirer.
                    relayed: Mutex::new(seeded_relayed),
                    pub_video_ssrc: Mutex::new(HashMap::new()),
                    cmd_tx,
                    tombstones: Mutex::new(HashSet::new()),
                }),
            );
        }

        tokio::spawn(peer_actor(
            self.clone(),
            room_id.to_string(),
            peer_id.clone(),
            pc.clone(),
            manifest.clone(),
            cmd_rx,
        ));

        // 5) Offre → réponse (attente de la fin du rassemblement ICE : pas de trickle).
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
        Ok((peer_id, local_desc.sdp))
    }

    /// Enregistre une piste publiée par `owner` et l'ajoute aux PC des autres pairs (+ renégociation).
    /// `video_ssrc` : présent pour une piste vidéo (sert au routage des demandes de keyframe).
    async fn publish_track(
        &self,
        room_id: &str,
        owner: &str,
        track: Arc<TrackLocalStaticRTP>,
        video_ssrc: Option<u32>,
    ) {
        let rooms = self.rooms.lock().await;
        let Some(room) = rooms.get(room_id) else {
            return;
        };
        let track_id = TrackLocal::id(track.as_ref()).to_string();
        // Tombstone : un `unpublish` est arrivé AVANT cette piste (course cam-off rapide). On NE la
        // relaie PAS et on consomme la pierre tombale — sinon une piste déjà retirée côté client
        // resterait relayée chez les autres pour toujours.
        if let Some(p) = room.peers.get(owner) {
            let mut tombs = p.tombstones.lock().await;
            let matched = tombs.iter().find(|ts| track_id.ends_with(&format!(".{ts}"))).cloned();
            if let Some(ts) = matched {
                tombs.remove(&ts);
                return;
            }
        }
        let owner_pc = room.peers.get(owner).map(|p| p.pc.clone());
        if let Some(p) = room.peers.get(owner) {
            p.published.lock().await.push(track.clone());
            if let Some(ssrc) = video_ssrc {
                p.pub_video_ssrc.lock().await.insert(track_id.clone(), ssrc);
            }
        }
        for (id, other) in room.peers.iter() {
            if id != owner {
                if let Ok(sender) = other
                    .pc
                    .add_track(track.clone() as Arc<dyn TrackLocal + Send + Sync>)
                    .await
                {
                    // Vidéo : relaie les demandes de keyframe de cet abonné vers le publieur
                    // (récupération de freeze + premier rendu d'un nouvel abonné).
                    if let (Some(ssrc), Some(pub_pc)) = (video_ssrc, owner_pc.clone()) {
                        spawn_keyframe_forwarder(sender.clone(), pub_pc, ssrc);
                    }
                    other.relayed.lock().await.insert(track_id.clone(), sender);
                    let _ = other.cmd_tx.send(PeerCmd::Renegotiate);
                }
            }
        }
    }

    /// Retire une piste publiée par `owner_peer` (cam/écran off) : la retire des PC des autres pairs
    /// (+ renégociation) et de la liste publiée du propriétaire. `local_track_id` = id de la piste
    /// **côté navigateur** ; on ne retire QUE les pistes du pair demandeur.
    async fn unpublish(&self, room_id: &str, owner_peer: &str, local_track_id: &str) {
        let suffix = format!(".{local_track_id}");
        let rooms = self.rooms.lock().await;
        let Some(room) = rooms.get(room_id) else {
            return;
        };
        // Identifie les pistes du propriétaire à retirer.
        let mut to_remove: Vec<String> = Vec::new();
        if let Some(p) = room.peers.get(owner_peer) {
            let mut pubs = p.published.lock().await;
            pubs.retain(|t| {
                let tid = TrackLocal::id(t.as_ref()).to_string();
                if tid.ends_with(&suffix) {
                    to_remove.push(tid);
                    false
                } else {
                    true
                }
            });
            // Oublie les SSRC vidéo des pistes retirées (plus de keyframe à router pour elles).
            let mut vssrc = p.pub_video_ssrc.lock().await;
            for tid in &to_remove {
                vssrc.remove(tid);
            }
        }
        if to_remove.is_empty() {
            // La piste n'est pas (encore) publiée : l'`unpublish` a devancé le `on_track`
            // correspondant. On pose une pierre tombale ; `publish_track` la verra et refusera de
            // relayer la piste tardive. (Best-effort : si la piste n'arrive jamais, le tombstone
            // est anodin et disparaît avec le pair.)
            if let Some(p) = room.peers.get(owner_peer) {
                p.tombstones.lock().await.insert(local_track_id.to_string());
            }
            return;
        }
        for (id, other) in room.peers.iter() {
            if id == owner_peer {
                continue;
            }
            let mut changed = false;
            let mut relayed = other.relayed.lock().await;
            for tid in &to_remove {
                if let Some(sender) = relayed.remove(tid) {
                    let _ = other.pc.remove_track(&sender).await;
                    changed = true;
                }
            }
            drop(relayed);
            if changed {
                let _ = other.cmd_tx.send(PeerCmd::Renegotiate);
            }
        }
    }

    /// Renvoie le canal acteur d'un pair **si `requester_uid` en est le propriétaire** (pour le WS).
    pub async fn signal_handle(
        &self,
        room_id: &str,
        peer_id: &str,
        requester_uid: &str,
    ) -> Option<mpsc::UnboundedSender<PeerCmd>> {
        let rooms = self.rooms.lock().await;
        let room = rooms.get(room_id)?;
        let peer = room.peers.get(peer_id)?;
        if peer.owner != requester_uid {
            return None;
        }
        Some(peer.cmd_tx.clone())
    }

    /// Déconnecte un pair **uniquement si `requester_uid` en est le propriétaire**. Renvoie `true`
    /// si retiré. Ferme la `RTCPeerConnection`, retire ses pistes chez les autres et supprime la
    /// salle si elle devient vide.
    pub async fn leave(&self, room_id: &str, peer_id: &str, requester_uid: &str) -> bool {
        {
            let rooms = self.rooms.lock().await;
            let owned = rooms
                .get(room_id)
                .and_then(|r| r.peers.get(peer_id))
                .map(|p| p.owner == requester_uid)
                .unwrap_or(false);
            if !owned {
                return false;
            }
        }
        self.purge_peer(room_id, peer_id).await
    }

    /// Évince un pair par **uid** sans condition de propriété (déconnexion de modération initiée par
    /// l'API). Retire TOUS les pairs de cet uid dans la salle. Renvoie le nombre de pairs retirés.
    pub async fn evict_uid(&self, room_id: &str, uid: &str) -> usize {
        let targets: Vec<String> = {
            let rooms = self.rooms.lock().await;
            match rooms.get(room_id) {
                Some(room) => room
                    .peers
                    .iter()
                    .filter(|(_, p)| p.owner == uid)
                    .map(|(id, _)| id.clone())
                    .collect(),
                None => Vec::new(),
            }
        };
        let mut n = 0;
        for pid in targets {
            if self.purge_peer(room_id, &pid).await {
                n += 1;
            }
        }
        n
    }

    /// Retire un pair de la salle : ferme sa PC, RETIRE ses pistes relayées chez les autres pairs
    /// (+ Renegotiate) pour ne pas leur laisser de tuile/flux fantôme, et supprime la salle si vide.
    /// Cœur commun au départ propre (`leave`), à l'éviction de modération (`evict_uid`) et à la
    /// mort brutale (handler d'état de connexion). Renvoie `true` si un pair a été retiré.
    async fn purge_peer(&self, room_id: &str, peer_id: &str) -> bool {
        // ── Phase 1 : sous le verrou, on RETIRE le pair et on COLLECTE les références à manipuler.
        // On ne fait AUCUN `.await` long (remove_track/close/send) sous le verrou : `pc.close()`
        // exécute INLINE le handler on_peer_connection_state_change qui re-appelle purge_peer →
        // 2e lock du Mutex non-réentrant → deadlock total du SFU. On libère donc le verrou avant.
        let (gone, others, room_now_empty) = {
            let mut rooms = self.rooms.lock().await;
            let Some(room) = rooms.get_mut(room_id) else {
                return false;
            };
            let Some(gone) = room.peers.remove(peer_id) else {
                return false;
            };
            let others: Vec<Arc<Peer>> = room.peers.values().cloned().collect();
            let empty = room.peers.is_empty();
            if empty {
                rooms.remove(room_id);
            }
            (gone, others, empty)
        }; // ← verrou `rooms` relâché ici
        let _ = room_now_empty; // (la room a déjà été retirée si vide)

        // ── Phase 2 : hors verrou, on effectue les opérations asynchrones.
        // Ids des pistes que le partant publiait (pour les retirer chez les autres).
        let track_ids: Vec<String> = {
            let pubs = gone.published.lock().await;
            pubs.iter()
                .map(|t| TrackLocal::id(t.as_ref()).to_string())
                .collect()
        };
        for other in &others {
            let mut changed = false;
            let mut relayed = other.relayed.lock().await;
            for tid in &track_ids {
                if let Some(sender) = relayed.remove(tid) {
                    let _ = other.pc.remove_track(&sender).await;
                    changed = true;
                }
            }
            drop(relayed);
            if changed {
                let _ = other.cmd_tx.send(PeerCmd::Renegotiate);
            }
        }
        // close() peut réentrer dans le handler d'état → purge_peer, mais le pair est déjà retiré
        // de la map (early-return None) ET le verrou est libre : la réentrance est inoffensive.
        let _ = gone.pc.close().await;
        true
    }
}

/// Tâche-acteur d'un pair : sérialise offer/answer/renégociation. SFU « impoli ».
async fn peer_actor(
    sfu: Arc<Sfu>,
    room_id: String,
    peer_id: String,
    pc: Arc<RTCPeerConnection>,
    manifest: Arc<Mutex<HashMap<String, String>>>,
    mut rx: mpsc::UnboundedReceiver<PeerCmd>,
) {
    let mut out: Option<mpsc::UnboundedSender<String>> = None;
    let mut making_offer = false;
    let mut pending = false;

    while let Some(cmd) = rx.recv().await {
        match cmd {
            PeerCmd::WsConnected(tx) => {
                out = Some(tx);
                if pending && !making_offer {
                    pending = false;
                    making_offer = send_offer(&pc, out.as_ref()).await;
                }
            }
            PeerCmd::WsClosed => {
                out = None;
            }
            PeerCmd::Renegotiate => {
                if out.is_none() || making_offer || pc.signaling_state() != RTCSignalingState::Stable
                {
                    pending = true;
                } else {
                    making_offer = send_offer(&pc, out.as_ref()).await;
                }
            }
            PeerCmd::ClientMsg(text) => {
                let Ok(v) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                match v.get("t").and_then(Value::as_str) {
                    Some("offer") => {
                        // SFU impoli : en cas de collision (pas Stable), on ignore — le client poli ré-offrira.
                        if pc.signaling_state() != RTCSignalingState::Stable {
                            continue;
                        }
                        if let Some(tracks) = v.get("tracks").and_then(Value::as_object) {
                            let mut m = manifest.lock().await;
                            for (k, val) in tracks {
                                if let Some(s) = val.as_str() {
                                    m.insert(k.clone(), s.to_string());
                                }
                            }
                        }
                        let Some(sdp) = v.get("sdp").and_then(Value::as_str) else {
                            continue;
                        };
                        let Ok(desc) = RTCSessionDescription::offer(sdp.to_string()) else {
                            continue;
                        };
                        if pc.set_remote_description(desc).await.is_err() {
                            continue;
                        }
                        if let Ok(answer) = pc.create_answer(None).await {
                            let mut gather = pc.gathering_complete_promise().await;
                            if pc.set_local_description(answer).await.is_ok() {
                                let _ = gather.recv().await;
                                if let (Some(ld), Some(tx)) =
                                    (pc.local_description().await, out.as_ref())
                                {
                                    let _ = tx.send(json!({"t":"answer","sdp": ld.sdp}).to_string());
                                }
                            }
                        }
                    }
                    Some("answer") => {
                        if let Some(sdp) = v.get("sdp").and_then(Value::as_str) {
                            if let Ok(desc) = RTCSessionDescription::answer(sdp.to_string()) {
                                let _ = pc.set_remote_description(desc).await;
                            }
                        }
                        making_offer = false;
                        if pending {
                            pending = false;
                            making_offer = send_offer(&pc, out.as_ref()).await;
                        }
                    }
                    Some("unpublish") => {
                        if let Some(id) = v.get("id").and_then(Value::as_str) {
                            sfu.unpublish(&room_id, &peer_id, id).await;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Crée une offre, attend l'ICE (non-trickle) et l'envoie au client. Renvoie `true` si émise.
async fn send_offer(
    pc: &Arc<RTCPeerConnection>,
    out: Option<&mpsc::UnboundedSender<String>>,
) -> bool {
    let Some(tx) = out else {
        return false;
    };
    let Ok(offer) = pc.create_offer(None).await else {
        return false;
    };
    let mut gather = pc.gathering_complete_promise().await;
    if pc.set_local_description(offer).await.is_err() {
        return false;
    }
    let _ = gather.recv().await;
    if let Some(ld) = pc.local_description().await {
        return tx
            .send(json!({"t":"offer","sdp": ld.sdp}).to_string())
            .is_ok();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_kind_whitelists_only_known_natures() {
        assert_eq!(sanitize_kind(Some(&"mic".to_string()), false), "mic");
        assert_eq!(sanitize_kind(Some(&"cam".to_string()), true), "cam");
        assert_eq!(sanitize_kind(Some(&"screen".to_string()), true), "screen");
        let injection = "cam\r\na=evil:1".to_string();
        assert_eq!(sanitize_kind(Some(&injection), true), "cam");
        assert_eq!(sanitize_kind(Some(&injection), false), "mic");
        assert_eq!(sanitize_kind(Some(&"../../etc".to_string()), false), "mic");
        assert_eq!(sanitize_kind(Some(&String::new()), true), "cam");
        assert_eq!(sanitize_kind(None, true), "cam");
        assert_eq!(sanitize_kind(None, false), "mic");
        let _: &'static str = sanitize_kind(Some(&"screen".to_string()), true);
    }

    #[tokio::test]
    async fn sfu_builds_and_tracks_rooms() {
        let sfu = Sfu::new().expect("construction SFU");
        assert_eq!(sfu.room_count().await, 0);
        assert_eq!(sfu.peer_count("absente").await, 0);
        assert!(!sfu.leave("absente", "p1", "u1").await);
        assert!(sfu.signal_handle("absente", "p1", "u1").await.is_none());
        assert_eq!(sfu.room_count().await, 0);
    }
}
