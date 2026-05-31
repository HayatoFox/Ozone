//! Client Gateway temps réel : connexion WebSocket, handshake `HELLO`/`IDENTIFY`/`READY`,
//! **RESUME** (reprise sans perte après coupure), heartbeat automatique, et flux des événements
//! `DISPATCH` (`MESSAGE_CREATE`, `PRESENCE_UPDATE`…). Cf. `docs/05-gateway-temps-reel.md`.
//! Côté client : `tokio-tungstenite` + `rustls`.

use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use ozone_proto::gateway::{opcode, GatewayFrame};
use serde_json::json;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

/// Convertit une base d'API (`http(s)://hôte`) en URL WebSocket de la Gateway.
fn ws_url(api_base: &str) -> String {
    let b = api_base.trim_end_matches('/');
    let b = if let Some(rest) = b.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = b.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        format!("wss://{b}")
    };
    format!("{b}/gateway")
}

/// Connexion Gateway active : payload `READY`/`RESUMED` + flux des événements dispatchés.
pub struct GatewayConnection {
    /// Payload du handshake (`READY` pour un IDENTIFY, `{session_id}` pour un RESUMED).
    pub ready: serde_json::Value,
    session_id: Option<String>,
    /// Dernière séquence **consommée** par l'appelant (avance dans `next_event`, pas à la
    /// réception) : un RESUME reprend exactement après le dernier événement réellement traité.
    last_seq: u64,
    events: mpsc::UnboundedReceiver<GatewayFrame>,
    task: JoinHandle<()>,
}

impl GatewayConnection {
    /// Prochain événement temps réel (`None` si la connexion est fermée). Met à jour `last_seq`.
    pub async fn next_event(&mut self) -> Option<GatewayFrame> {
        let frame = self.events.recv().await?;
        if let Some(s) = frame.s {
            if s > self.last_seq {
                self.last_seq = s;
            }
        }
        Some(frame)
    }

    /// Identifiant de session (présent après `READY`) — nécessaire pour un RESUME.
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Dernière séquence d'événement **consommée** (à fournir lors d'un RESUME pour le rejeu).
    pub fn last_seq(&self) -> u64 {
        self.last_seq
    }

    /// Coupe brutalement le socket (simulation/gestion d'une perte réseau). La **session reste
    /// résumable** côté serveur pendant sa fenêtre de grâce : appeler ensuite un RESUME.
    pub fn abort(&self) {
        self.task.abort();
    }
}

impl Drop for GatewayConnection {
    fn drop(&mut self) {
        // Évite de laisser tourner la tâche de pompage si la connexion est abandonnée.
        self.task.abort();
    }
}

/// Résultat d'une tentative de RESUME.
pub enum Resumed {
    /// Reprise acceptée : les événements manqués sont rejoués sur le flux.
    Ok(GatewayConnection),
    /// Session refusée (inconnue/expirée/tampon dépassé) : refaire un `connect` (IDENTIFY) complet.
    Invalid,
}

/// Se connecte à la Gateway, s'authentifie (`IDENTIFY`), attend `READY`, puis démarre le pompage.
pub async fn connect(api_base: &str, token: &str) -> Result<GatewayConnection> {
    let url = ws_url(api_base);
    let (ws, _resp) = tokio_tungstenite::connect_async(&url).await?;
    let (mut write, mut read) = ws.split();

    let heartbeat_ms = read_hello(&mut read).await?;

    let identify = GatewayFrame::with_data(opcode::IDENTIFY, json!({ "token": token }));
    write
        .send(Message::Text(serde_json::to_string(&identify)?.into()))
        .await?;

    // Attend READY (ou rejet de session).
    let ready = loop {
        let f = next_frame(&mut read)
            .await?
            .ok_or_else(|| anyhow!("connexion fermée avant READY"))?;
        if f.op == opcode::INVALID_SESSION {
            return Err(anyhow!("session invalide (jeton refusé par la Gateway)"));
        }
        if f.op == opcode::DISPATCH && f.t.as_deref() == Some("READY") {
            break f.d.unwrap_or_default();
        }
    };
    let session_id = ready
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let (events, task) = spawn_pump(write, read, heartbeat_ms);
    Ok(GatewayConnection {
        ready,
        session_id,
        last_seq: 0,
        events,
        task,
    })
}

/// Tente une reprise (`RESUME`) d'une session précédente à partir de son `session_id` et du
/// dernier `seq` consommé. En cas de refus serveur, renvoie [`Resumed::Invalid`] (faire un `connect`).
pub async fn connect_resume(
    api_base: &str,
    token: &str,
    session_id: &str,
    seq: u64,
) -> Result<Resumed> {
    let url = ws_url(api_base);
    let (ws, _resp) = tokio_tungstenite::connect_async(&url).await?;
    let (mut write, mut read) = ws.split();

    let heartbeat_ms = read_hello(&mut read).await?;

    let resume = GatewayFrame::with_data(
        opcode::RESUME,
        json!({ "token": token, "session_id": session_id, "seq": seq }),
    );
    write
        .send(Message::Text(serde_json::to_string(&resume)?.into()))
        .await?;

    // Attend RESUMED (succès) ou INVALID_SESSION (refus).
    loop {
        let f = next_frame(&mut read)
            .await?
            .ok_or_else(|| anyhow!("connexion fermée avant RESUMED"))?;
        if f.op == opcode::INVALID_SESSION {
            return Ok(Resumed::Invalid);
        }
        if f.op == opcode::DISPATCH && f.t.as_deref() == Some("RESUMED") {
            let ready = f.d.unwrap_or_default();
            let (events, task) = spawn_pump(write, read, heartbeat_ms);
            return Ok(Resumed::Ok(GatewayConnection {
                ready,
                session_id: Some(session_id.to_string()),
                last_seq: seq,
                events,
                task,
            }));
        }
    }
}

/// Lit le `HELLO` initial et renvoie l'intervalle de heartbeat (ms).
async fn read_hello<R>(read: &mut R) -> Result<u64>
where
    R: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let hello = next_frame(read)
        .await?
        .ok_or_else(|| anyhow!("connexion fermée avant HELLO"))?;
    Ok(hello
        .d
        .as_ref()
        .and_then(|d| d.get("heartbeat_interval"))
        .and_then(|v| v.as_u64())
        .unwrap_or(30_000))
}

/// Démarre la tâche de pompage : heartbeat périodique + transfert des `DISPATCH`.
/// Renvoie le récepteur d'événements et le handle de la tâche.
fn spawn_pump<W, R>(
    mut write: W,
    mut read: R,
    heartbeat_ms: u64,
) -> (mpsc::UnboundedReceiver<GatewayFrame>, JoinHandle<()>)
where
    W: futures_util::Sink<Message> + Unpin + Send + 'static,
    R: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin
        + Send
        + 'static,
{
    let (tx, rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        let mut hb = tokio::time::interval(Duration::from_millis(heartbeat_ms.max(1000)));
        loop {
            tokio::select! {
                _ = hb.tick() => {
                    let frame = GatewayFrame::new(opcode::HEARTBEAT);
                    let Ok(txt) = serde_json::to_string(&frame) else { continue };
                    if write.send(Message::Text(txt.into())).await.is_err() {
                        break;
                    }
                }
                msg = read.next() => match msg {
                    Some(Ok(Message::Text(txt))) => {
                        if let Ok(frame) = serde_json::from_str::<GatewayFrame>(txt.as_str()) {
                            if frame.op == opcode::DISPATCH && tx.send(frame).is_err() {
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                },
            }
        }
    });
    (rx, task)
}

/// Lit la prochaine trame texte (ignore ping/pong/binaire).
async fn next_frame<S>(read: &mut S) -> Result<Option<GatewayFrame>>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    while let Some(msg) = read.next().await {
        match msg? {
            Message::Text(txt) => return Ok(Some(serde_json::from_str(txt.as_str())?)),
            Message::Close(_) => return Ok(None),
            _ => continue,
        }
    }
    Ok(None)
}
