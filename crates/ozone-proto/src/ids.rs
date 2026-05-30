//! Identifiants Snowflake (64 bits, triables chronologiquement) — cf. `docs/03-modele-de-donnees.md`.

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Epoch Ozone : 2025-01-01T00:00:00Z en millisecondes.
pub const OZONE_EPOCH_MS: u64 = 1_735_689_600_000;

/// Identifiant 64 bits. Sérialisé en **chaîne** dans le JSON (précision JS).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Snowflake(pub u64);

impl Snowflake {
    pub const fn new(v: u64) -> Self {
        Snowflake(v)
    }
    pub const fn get(self) -> u64 {
        self.0
    }
    pub const fn as_i64(self) -> i64 {
        self.0 as i64
    }
    pub const fn from_i64(v: i64) -> Self {
        Snowflake(v as u64)
    }
    /// Instant de création encodé dans l'identifiant (ms depuis l'epoch Unix).
    pub const fn timestamp_ms(self) -> u64 {
        (self.0 >> 22) + OZONE_EPOCH_MS
    }
}

impl fmt::Display for Snowflake {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl fmt::Debug for Snowflake {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Snowflake({})", self.0)
    }
}
impl FromStr for Snowflake {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Snowflake(s.parse()?))
    }
}

impl Serialize for Snowflake {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0.to_string())
    }
}
impl<'de> Deserialize<'de> for Snowflake {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl de::Visitor<'_> for V {
            type Value = Snowflake;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("un snowflake (chaîne ou entier non signé)")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Snowflake, E> {
                v.parse().map(Snowflake).map_err(de::Error::custom)
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Snowflake, E> {
                Ok(Snowflake(v))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Snowflake, E> {
                Ok(Snowflake(v as u64))
            }
        }
        d.deserialize_any(V)
    }
}

/// Générateur de snowflakes thread-safe (timestamp | worker | séquence).
pub struct SnowflakeGenerator {
    worker: u64,
    state: Mutex<(u64, u64)>, // (dernier_ms, séquence)
}

impl SnowflakeGenerator {
    pub fn new(worker: u16) -> Self {
        SnowflakeGenerator {
            worker: (worker as u64) & 0x3FF, // 10 bits (worker + process)
            state: Mutex::new((0, 0)),
        }
    }

    pub fn next(&self) -> Snowflake {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(OZONE_EPOCH_MS);
        let mut g = self.state.lock().unwrap();
        let (last_ms, last_seq) = *g;
        let (ms, seq) = if now <= last_ms {
            // même milliseconde (ou horloge en recul) → incrémente la séquence
            (last_ms, last_seq + 1)
        } else {
            (now, 0)
        };
        *g = (ms, seq);
        let ts = ms.saturating_sub(OZONE_EPOCH_MS);
        Snowflake((ts << 22) | (self.worker << 12) | (seq & 0xFFF))
    }
}

impl Default for SnowflakeGenerator {
    fn default() -> Self {
        Self::new(1)
    }
}
