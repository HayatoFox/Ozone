//! Primitives cryptographiques : mots de passe (Argon2id), JWT HS256 (pur Rust, sans `ring`),
//! jetons aléatoires et hachage de jetons.

use argon2::password_hash::rand_core::{OsRng, RngCore};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

// ───────────────────────────── Mots de passe ─────────────────────────────

pub fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

// ────────────────────────────── Jetons ──────────────────────────────

pub fn random_token() -> String {
    let mut b = [0u8; 32];
    OsRng.fill_bytes(&mut b);
    B64.encode(b)
}

pub fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    let out = h.finalize();
    let mut hex = String::with_capacity(out.len() * 2);
    for byte in out {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

// ─────────────────────────── JWT HS256 (maison) ───────────────────────────

#[derive(Serialize, Deserialize)]
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

pub fn jwt_encode(secret: &[u8], sub: &str, kind: &str, ttl_secs: u64) -> String {
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

pub fn jwt_verify(secret: &[u8], token: &str, expected_kind: &str) -> Option<Claims> {
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
