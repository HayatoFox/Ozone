//! Primitives cryptographiques : mots de passe (Argon2id), JWT HS256 (pur Rust, sans `ring`),
//! jetons aléatoires et hachage de jetons.

use argon2::password_hash::rand_core::{OsRng, RngCore};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use sha2::{Digest, Sha256};

// JWT HS256 : implémentation partagée dans `ozone_proto::token` (réutilisée par le SFU).
pub use ozone_proto::token::Claims;

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
    use std::fmt::Write as _;
    let mut hex = String::with_capacity(out.len() * 2);
    for byte in out {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

// ─────────────────────────── JWT HS256 (délégué à ozone_proto::token) ───────────────────────────

/// Émet un JWT HS256 (cf. `ozone_proto::token`).
pub fn jwt_encode(secret: &[u8], sub: &str, kind: &str, ttl_secs: u64) -> String {
    ozone_proto::token::encode(secret, sub, kind, ttl_secs)
}

/// Vérifie un JWT HS256 (signature, `kind`, expiration).
pub fn jwt_verify(secret: &[u8], token: &str, expected_kind: &str) -> Option<Claims> {
    ozone_proto::token::verify(secret, token, expected_kind)
}
