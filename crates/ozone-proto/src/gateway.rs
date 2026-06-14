//! Protocole de la Gateway temps réel (cf. `docs/05-gateway-temps-reel.md`).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Opcodes de la Gateway.
pub mod opcode {
    pub const DISPATCH: u8 = 0;
    pub const HEARTBEAT: u8 = 1;
    pub const IDENTIFY: u8 = 2;
    pub const PRESENCE_UPDATE: u8 = 3;
    pub const VOICE_STATE_UPDATE: u8 = 4;
    /// Uplink : le client signale qu'il parle / se tait (indicateur vocal temps réel).
    pub const VOICE_SPEAKING: u8 = 5;
    pub const RESUME: u8 = 6;
    pub const RECONNECT: u8 = 7;
    pub const INVALID_SESSION: u8 = 9;
    pub const HELLO: u8 = 10;
    pub const HEARTBEAT_ACK: u8 = 11;
}

/// Trame générique de la Gateway : `{ op, d?, s?, t? }`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GatewayFrame {
    pub op: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub d: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub t: Option<String>,
}

impl GatewayFrame {
    pub fn new(op: u8) -> Self {
        GatewayFrame {
            op,
            d: None,
            s: None,
            t: None,
        }
    }
    pub fn with_data(op: u8, d: Value) -> Self {
        GatewayFrame {
            op,
            d: Some(d),
            s: None,
            t: None,
        }
    }
    pub fn dispatch(t: impl Into<String>, d: Value, s: u64) -> Self {
        GatewayFrame {
            op: opcode::DISPATCH,
            d: Some(d),
            s: Some(s),
            t: Some(t.into()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Hello {
    pub heartbeat_interval: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Identify {
    pub token: String,
}
