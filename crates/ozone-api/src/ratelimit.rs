//! Limitation de débit (rate-limiting) en mémoire — **token bucket** par clé.
//!
//! Mono-process (mode tout-en-un) : pas de dépendance externe (ni Redis ni `ring`).
//! Chaque `(classe, clé)` possède un seau de jetons qui se recharge linéairement dans le temps ;
//! une requête consomme 1 jeton, sinon elle est refusée avec un délai `Retry-After`.
//!
//! Calibrage : les **bursts** sont généreux (usage normal et tests d'intégration transparents)
//! mais bornent un attaquant (brute-force login, spam de messages/webhooks). Désactivable via
//! `OZONE_RATE_LIMIT=0` (utile en bench/CI).

use crate::db::now_ms;
use std::collections::HashMap;
use std::sync::Mutex;

/// Paramètres d'une classe de limite : capacité (burst) et recharge par seconde.
#[derive(Clone, Copy)]
pub struct RateClass {
    /// Préfixe de bucket (évite les collisions de clés entre classes).
    pub name: &'static str,
    /// Nombre de jetons au maximum (rafale tolérée).
    pub capacity: f64,
    /// Jetons rechargés par seconde (débit soutenu).
    pub refill_per_sec: f64,
}

// ───────────────────────── Politiques ─────────────────────────
// Bornes pensées pour : usage humain normal OK, scripts d'abus coupés.

/// Inscription (par IP) : ~1 / 2 s soutenu, rafale 20.
pub const REGISTER: RateClass = RateClass { name: "register", capacity: 20.0, refill_per_sec: 0.5 };
/// Connexion (par IP) : anti brute-force ; rafale 20, 1 / s soutenu.
pub const LOGIN: RateClass = RateClass { name: "login", capacity: 20.0, refill_per_sec: 1.0 };
/// Porte d'accès d'instance (par IP) : anti brute-force du mot de passe d'instance.
pub const GATE: RateClass = RateClass { name: "gate", capacity: 10.0, refill_per_sec: 0.5 };
/// Envoi de message (par utilisateur) : rafale 30, 5 / s soutenu (le slowmode reste par-salon).
pub const MESSAGE: RateClass = RateClass { name: "msg", capacity: 30.0, refill_per_sec: 5.0 };
/// Exécution de webhook (par webhook) : R6 — rafale 20, 2 / s soutenu.
pub const WEBHOOK: RateClass = RateClass { name: "wh", capacity: 20.0, refill_per_sec: 2.0 };
/// Création d'invitation (par utilisateur) : rafale 15, 1 / s.
pub const INVITE: RateClass = RateClass { name: "invite", capacity: 15.0, refill_per_sec: 1.0 };

struct Bucket {
    tokens: f64,
    last_ms: i64,
    // Paramètres de la CLASSE propre du bucket (la table est partagée entre classes ; la purge
    // doit évaluer chaque seau avec SES paramètres, pas ceux de l'appel courant).
    capacity: f64,
    refill_per_sec: f64,
}

/// Au-delà de ce nombre d'entrées, on purge les seaux « pleins » (inactifs) pour borner la mémoire.
const MAX_BUCKETS: usize = 50_000;

pub struct RateLimiter {
    buckets: Mutex<HashMap<String, Bucket>>,
    enabled: bool,
}

impl RateLimiter {
    pub fn new(enabled: bool) -> Self {
        RateLimiter {
            buckets: Mutex::new(HashMap::new()),
            enabled,
        }
    }

    /// Tente de consommer un jeton pour `(class, key)`.
    /// `Ok(())` autorisé ; `Err(secs)` refusé avec un délai d'attente conseillé (`Retry-After`).
    pub fn check(&self, class: RateClass, key: &str) -> Result<(), u64> {
        if !self.enabled {
            return Ok(());
        }
        let now = now_ms();
        let mut map = self.buckets.lock().unwrap_or_else(|e| e.into_inner());

        // Purge opportuniste : si la table explose, on retire les seaux rechargés à plein
        // (aucune perte de protection — un seau plein équivaut à « jamais vu »). Chaque seau est
        // évalué avec SES propres paramètres (b.capacity/b.refill_per_sec), pas ceux de l'appel.
        if map.len() > MAX_BUCKETS {
            map.retain(|_, b| {
                let elapsed = (now - b.last_ms).max(0) as f64 / 1000.0;
                (b.tokens + elapsed * b.refill_per_sec) < b.capacity
            });
        }

        let full_key = format!("{}:{}", class.name, key);
        let b = map.entry(full_key).or_insert(Bucket {
            tokens: class.capacity,
            last_ms: now,
            capacity: class.capacity,
            refill_per_sec: class.refill_per_sec,
        });
        // Recharge proportionnelle au temps écoulé, plafonnée à la capacité.
        let elapsed = (now - b.last_ms).max(0) as f64 / 1000.0;
        b.tokens = (b.tokens + elapsed * class.refill_per_sec).min(class.capacity);
        b.last_ms = now;

        if b.tokens >= 1.0 {
            b.tokens -= 1.0;
            Ok(())
        } else {
            // Temps nécessaire pour récupérer 1 jeton (arrondi au supérieur, min 1 s).
            let need = (1.0 - b.tokens) / class.refill_per_sec;
            Err((need.ceil() as u64).max(1))
        }
    }
}
