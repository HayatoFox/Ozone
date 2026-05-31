//! JWT HS256 (pur Rust, **sans `ring`**) — partagé entre l'API (`ozone-api`) et le nœud média
//! (`ozone-sfu`), qui doit vérifier les jetons vocaux émis par l'API.

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub iat: u64,
    pub exp: u64,
    pub kind: String,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Émet un JWT HS256 signé avec `secret`.
pub fn encode(secret: &[u8], sub: &str, kind: &str, ttl_secs: u64) -> String {
    let header = B64.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let iat = now_secs();
    let claims = Claims {
        sub: sub.to_string(),
        iat,
        exp: iat + ttl_secs,
        kind: kind.to_string(),
    };
    let payload = B64.encode(serde_json::to_vec(&claims).unwrap_or_default());
    let signing_input = format!("{header}.{payload}");
    let sig = hs256(secret, signing_input.as_bytes());
    format!("{signing_input}.{sig}")
}

/// Vérifie un JWT HS256 (signature, `kind` attendu, expiration). `None` si invalide.
pub fn verify(secret: &[u8], token: &str, expected_kind: &str) -> Option<Claims> {
    let mut parts = token.split('.');
    let h = parts.next()?;
    let p = parts.next()?;
    let s = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let signing_input = format!("{h}.{p}");
    let expected = hs256(secret, signing_input.as_bytes());
    if !constant_eq(expected.as_bytes(), s.as_bytes()) {
        return None;
    }
    let claims: Claims = serde_json::from_slice(&B64.decode(p).ok()?).ok()?;
    if claims.kind != expected_kind || claims.exp < now_secs() {
        return None;
    }
    Some(claims)
}

fn hs256(secret: &[u8], data: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).expect("clé HMAC de longueur valide");
    mac.update(data);
    B64.encode(mac.finalize().into_bytes())
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut r = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        r |= x ^ y;
    }
    r == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_tamper() {
        let secret = b"un-secret-de-test-suffisant";
        let t = encode(secret, "42.99", "voice", 60);
        let c = verify(secret, &t, "voice").expect("jeton valide");
        assert_eq!(c.sub, "42.99");
        // Mauvais kind.
        assert!(verify(secret, &t, "access").is_none());
        // Mauvais secret.
        assert!(verify(b"autre-secret", &t, "voice").is_none());
        // Jeton altéré.
        let tampered = format!("{t}x");
        assert!(verify(secret, &tampered, "voice").is_none());
    }
}
