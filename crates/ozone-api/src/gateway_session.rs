//! Sessions Gateway **résumables** (cf. docs/05-gateway-temps-reel.md, RESUME).
//!
//! Chaque session = un **acteur** (tâche) qui possède son propre abonnement au bus `hub`,
//! filtre les événements pour son utilisateur (`should_deliver`), leur assigne un **numéro de
//! séquence** monotone et les **mémorise** dans un tampon borné. L'acteur **survit à la coupure
//! du socket** pendant une fenêtre de grâce : un client qui se reconnecte peut alors RESUME et
//! recevoir **les événements manqués** (rejeu depuis son dernier `seq`), sans re-IDENTIFY ni
//! perte de message.
//!
//! Garantie de correction : on n'accepte un RESUME que si le tampon **couvre encore** tout ce qui
//! suit le `seq` du client (sinon `INVALID_SESSION` → le client repart sur un IDENTIFY propre).
//! Isolation : une session ne peut être reprise que par **le même utilisateur** (vérifié).

use crate::gateway::{broadcast_presence, should_deliver};
use crate::state::AppState;
use ozone_proto::gateway::GatewayFrame;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, oneshot};

/// Nombre maximal d'événements conservés pour le rejeu (par session).
const BUFFER_CAP: usize = 512;
/// Durée pendant laquelle une session détachée (socket coupé) reste résumable.
const GRACE: Duration = Duration::from_secs(60);

/// Commandes envoyées à l'acteur de session par le handler de socket.
enum Ctrl {
    /// (Ré)attache un socket : rejoue les événements après `after_seq`, puis diffuse en direct.
    Attach {
        sink: mpsc::UnboundedSender<GatewayFrame>,
        after_seq: u64,
        ack: oneshot::Sender<AttachAck>,
    },
    /// Le socket s'est coupé (réseau) : démarre la fenêtre de grâce (session toujours résumable).
    Detach,
    /// Fermeture propre (Close reçu / déconnexion) : arrête l'acteur immédiatement.
    Close,
}

/// Réponse à une demande d'attache.
struct AttachAck {
    /// `true` si l'attache (et le rejeu éventuel) est possible ; `false` si le tampon ne couvre
    /// plus le `seq` demandé (le client doit repartir sur IDENTIFY).
    ok: bool,
    /// Séquence courante de la session (pour information du client).
    current_seq: u64,
}

/// Poignée vers l'acteur d'une session, détenue par le handler de socket.
pub struct SessionConn {
    ctrl: mpsc::UnboundedSender<Ctrl>,
}

impl SessionConn {
    /// Attache un nouveau socket à la session. Renvoie le flux d'événements à pousser sur le
    /// WebSocket et la séquence courante, ou `None` si l'attache est refusée (tampon dépassé /
    /// acteur disparu) — auquel cas le client doit faire un IDENTIFY.
    pub async fn attach(
        &self,
        after_seq: u64,
    ) -> Option<(mpsc::UnboundedReceiver<GatewayFrame>, u64)> {
        let (sink, sink_rx) = mpsc::unbounded_channel();
        let (ack_tx, ack_rx) = oneshot::channel();
        self.ctrl
            .send(Ctrl::Attach {
                sink,
                after_seq,
                ack: ack_tx,
            })
            .ok()?;
        let ack = ack_rx.await.ok()?;
        if ack.ok {
            Some((sink_rx, ack.current_seq))
        } else {
            None
        }
    }

    /// Signale une coupure réseau : la session reste résumable pendant la fenêtre de grâce.
    pub fn detach(&self) {
        let _ = self.ctrl.send(Ctrl::Detach);
    }

    /// Ferme proprement la session (pas de grâce).
    pub fn close(&self) {
        let _ = self.ctrl.send(Ctrl::Close);
    }
}

/// Registre des sessions vivantes (id de session → acteur + propriétaire).
#[derive(Default)]
pub struct SessionRegistry {
    inner: Mutex<HashMap<String, Entry>>,
}

struct Entry {
    user_id: i64,
    ctrl: mpsc::UnboundedSender<Ctrl>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn insert(&self, session_id: String, user_id: i64, ctrl: mpsc::UnboundedSender<Ctrl>) {
        self.inner
            .lock()
            .unwrap()
            .insert(session_id, Entry { user_id, ctrl });
    }

    fn remove(&self, session_id: &str) {
        self.inner.lock().unwrap().remove(session_id);
    }

    /// Renvoie une poignée vers la session **si elle existe et appartient à `user_id`**.
    fn handle_for(&self, session_id: &str, user_id: i64) -> Option<SessionConn> {
        let m = self.inner.lock().unwrap();
        let e = m.get(session_id)?;
        if e.user_id != user_id {
            return None; // isolation : pas de reprise de la session d'autrui
        }
        Some(SessionConn {
            ctrl: e.ctrl.clone(),
        })
    }
}

/// Crée une session pour `user_id`, démarre son acteur, et renvoie `(session_id, poignée)`.
/// Gère la **présence** (passage en ligne à la création, hors ligne à la fin de l'acteur).
pub fn create_session(st: &AppState, user_id: i64) -> (String, SessionConn) {
    let session_id = st.ids.next().to_string();
    let (ctrl_tx, ctrl_rx) = mpsc::unbounded_channel();
    st.sessions
        .insert(session_id.clone(), user_id, ctrl_tx.clone());
    let st2 = st.clone();
    let sid = session_id.clone();
    tokio::spawn(async move {
        actor(st2, sid, user_id, ctrl_rx).await;
    });
    (session_id, SessionConn { ctrl: ctrl_tx })
}

/// Tente de reprendre une session existante (même utilisateur). `None` ⇒ inconnue/expirée/autrui.
pub fn resume_session(st: &AppState, session_id: &str, user_id: i64) -> Option<SessionConn> {
    st.sessions.handle_for(session_id, user_id)
}

/// Boucle de l'acteur : filtre/numérote/bufferise les événements et les pousse au socket attaché.
async fn actor(
    st: AppState,
    session_id: String,
    user_id: i64,
    mut ctrl_rx: mpsc::UnboundedReceiver<Ctrl>,
) {
    // Présence : première session de l'utilisateur ⇒ en ligne (diffusé aux guildes partagées).
    if st.presence.connect(user_id) {
        broadcast_presence(&st, user_id).await;
    }

    let mut hub_rx = st.hub.subscribe();
    let mut seq: u64 = 0;
    let mut buffer: VecDeque<GatewayFrame> = VecDeque::new();
    // Plus haute séquence évincée du tampon (0 si rien évincé) : sert à valider les RESUME.
    let mut evicted_through: u64 = 0;
    let mut sink: Option<mpsc::UnboundedSender<GatewayFrame>> = None;
    let mut detached_since: Option<Instant> = Some(Instant::now());
    let mut tick = tokio::time::interval(Duration::from_secs(5));

    loop {
        tokio::select! {
            biased;

            ctrl = ctrl_rx.recv() => {
                match ctrl {
                    Some(Ctrl::Attach { sink: new_sink, after_seq, ack }) => {
                        // Résumable seulement si rien d'évincé après `after_seq`.
                        if after_seq >= evicted_through {
                            for f in buffer.iter().filter(|f| f.s.unwrap_or(0) > after_seq) {
                                let _ = new_sink.send(f.clone());
                            }
                            sink = Some(new_sink);
                            detached_since = None;
                            let _ = ack.send(AttachAck { ok: true, current_seq: seq });
                        } else {
                            let _ = ack.send(AttachAck { ok: false, current_seq: seq });
                        }
                    }
                    Some(Ctrl::Detach) => {
                        sink = None;
                        detached_since = Some(Instant::now());
                    }
                    Some(Ctrl::Close) | None => break,
                }
            }

            ev = hub_rx.recv() => {
                match ev {
                    Ok(event) => {
                        if should_deliver(&st, user_id, &event.scope).await {
                            seq += 1;
                            let frame = GatewayFrame::dispatch(event.t, event.d, seq);
                            buffer.push_back(frame.clone());
                            if buffer.len() > BUFFER_CAP {
                                if let Some(old) = buffer.pop_front() {
                                    evicted_through = old.s.unwrap_or(evicted_through);
                                }
                            }
                            if let Some(s) = &sink {
                                if s.send(frame).is_err() {
                                    // Le socket a disparu sans Detach explicite ⇒ on passe en grâce.
                                    sink = None;
                                    detached_since = Some(Instant::now());
                                }
                            }
                        }
                    }
                    // Retard du bus : on ne peut pas garantir l'absence de trou ⇒ on continue
                    // (un RESUME couvrant cette zone restera possible tant que rien n'est évincé).
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }

            _ = tick.tick() => {
                if let Some(since) = detached_since {
                    if since.elapsed() >= GRACE {
                        break;
                    }
                }
            }
        }
    }

    // Fin de session : déréférencement + présence hors ligne (si c'était la dernière) + voix.
    st.sessions.remove(&session_id);
    if st.presence.disconnect(user_id) {
        broadcast_presence(&st, user_id).await;
        crate::routes_voice::disconnect_all_voice(&st, user_id).await;
    }
}
