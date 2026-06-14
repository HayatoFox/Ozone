//! Petit client HTTP **clair** vers le nœud média SFU, sans aucune dépendance externe (TcpStream +
//! HTTP/1.1 minimal). Sert UNIQUEMENT à l'éviction de modération : quand un modérateur déconnecte
//! ou déplace un membre du vocal, l'API demande au SFU de couper réellement son flux média (sinon
//! le SFU continuerait de relayer son micro/sa caméra). Best-effort : un échec réseau ne doit pas
//! faire échouer l'action de modération côté API (l'état DB/Gateway reste la source de vérité).
//!
//! Pas de TLS (l'API et le SFU sont co-localisés / derrière le même reverse-proxy) → conserve la
//! posture **zéro-`ring`** de l'API.

use crate::state::AppState;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

/// Évince (déconnecte du média) tous les pairs de `uid` dans le salon `channel_id`. Best-effort,
/// non bloquant pour l'appelant (lancé en tâche détachée). `uid`/`channel_id` en décimal.
pub fn evict_voice_peer(st: &AppState, channel_id: i64, uid: i64) {
    let base = st.sfu_url.as_ref().clone();
    let secret = st.voice_secret.as_ref().clone();
    tokio::spawn(async move {
        // Jeton d'éviction : kind="evict", sub="<uid>.<room>", TTL court. Seul un détenteur du
        // secret partagé (donc l'API) peut le forger → le SFU l'exige sur la route /evict.
        let sub = format!("{uid}.{channel_id}");
        let token = ozone_proto::token::encode(&secret, &sub, "evict", 30);
        let body = serde_json::json!({ "token": token }).to_string();
        let path = format!("/sfu/rooms/{channel_id}/evict");
        if let Err(e) = post_json(&base, &path, &body).await {
            tracing::warn!("éviction SFU échouée (média non coupé) : {e}");
        }
    });
}

/// Extrait `host:port` d'une base `http://host:port` (ignore le schéma ; défaut port 80).
fn host_port(base: &str) -> (String, u16) {
    let no_scheme = base
        .strip_prefix("http://")
        .or_else(|| base.strip_prefix("https://"))
        .unwrap_or(base);
    let authority = no_scheme.split('/').next().unwrap_or(no_scheme);
    match authority.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse().unwrap_or(80)),
        None => (authority.to_string(), 80),
    }
}

/// POST JSON minimal en HTTP/1.1 clair, avec timeout global. Lit assez de réponse pour la ligne
/// de statut (on ne parse pas le corps).
async fn post_json(base: &str, path: &str, body: &str) -> std::io::Result<()> {
    let (host, port) = host_port(base);
    let fut = async {
        let mut stream = TcpStream::connect((host.as_str(), port)).await?;
        let req = format!(
            "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\n\
             Content-Length: {len}\r\nConnection: close\r\n\r\n{body}",
            len = body.len(),
        );
        stream.write_all(req.as_bytes()).await?;
        stream.flush().await?;
        let mut buf = [0u8; 512];
        let _ = stream.read(&mut buf).await?; // on draine la réponse (statut), sans la parser
        Ok::<(), std::io::Error>(())
    };
    timeout(Duration::from_secs(3), fut)
        .await
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "délai SFU dépassé"))?
}
