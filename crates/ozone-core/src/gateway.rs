//! Client Gateway temps réel : connexion WebSocket, handshake `HELLO`/`IDENTIFY`/`READY`,
//! heartbeat automatique, et flux des événements `DISPATCH` (`MESSAGE_CREATE`, `PRESENCE_UPDATE`…).
//! Cf. `docs/05-gateway-temps-reel.md`. Côté client : `tokio-tungstenite` + `rustls`.

use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use ozone_proto::gateway::{opcode, GatewayFrame};
use serde_json::json;
use std::time::Duration;
use tokio::sync::mpsc;
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

/// Connexion Gateway active : payload `READY` + flux des événements dispatchés.
pub struct GatewayConnection {
    pub ready: serde_json::Value,
    events: mpsc::UnboundedReceiver<GatewayFrame>,
}

impl GatewayConnection {
    /// Prochain événement temps réel (`None` si la connexion est fermée).
    pub async fn next_event(&mut self) -> Option<GatewayFrame> {
        self.events.recv().await
    }
}

/// Se connecte à la Gateway de l'instance, s'authentifie, et démarre heartbeat + réception.
pub async fn connect(api_base: &str, token: &str) -> Result<GatewayConnection> {
    let url = ws_url(api_base);
    let (ws, _resp) = tokio_tungstenite::connect_async(&url).await?;
    let (mut write, mut read) = ws.split();

    // 1) HELLO (intervalle de heartbeat).
    let hello = next_frame(&mut read)
        .await?
        .ok_or_else(|| anyhow!("connexion fermée avant HELLO"))?;
    let heartbeat_ms = hello
        .d
        .as_ref()
        .and_then(|d| d.get("heartbeat_interval"))
        .and_then(|v| v.as_u64())
        .unwrap_or(30_000);

    // 2) IDENTIFY.
    let identify = GatewayFrame::with_data(opcode::IDENTIFY, json!({ "token": token }));
    write
        .send(Message::Text(serde_json::to_string(&identify)?.into()))
        .await?;

    // 3) Attend READY (ou rejet de session).
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

    // 4) Tâche : heartbeat périodique + transfert des événements dispatchés.
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
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

    Ok(GatewayConnection { ready, events: rx })
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
