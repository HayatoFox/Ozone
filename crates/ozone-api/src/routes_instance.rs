//! Routes d'instance : métadonnées publiques, santé, gate (mot de passe d'instance).
//! Cf. docs/features/00-instances.md, docs/04-api-rest.md.

use crate::crypto;
use crate::error::{AppError, AppResult};
use crate::extract::ClientIp;
use crate::ratelimit;
use crate::state::AppState;
use axum::extract::State;
use axum::Json;
use ozone_proto::dto::{AccessGate, GateRequest, GateResponse, InstanceInfo};
use serde_json::{json, Value};

/// `GET /instance` — métadonnées publiques (sans authentification).
pub async fn get_instance(State(st): State<AppState>) -> Json<InstanceInfo> {
    let inst = &st.instance;
    Json(InstanceInfo {
        instance_id: inst.instance_id,
        name: inst.name.clone(),
        description: inst.description.clone(),
        accent_color: None,
        version: inst.version.clone(),
        registration_policy: inst.registration_policy,
        access_gate: AccessGate {
            required: inst.gate_enabled,
        },
    })
}

/// `GET /instance/health`
pub async fn health(State(st): State<AppState>) -> Json<Value> {
    Json(json!({
        "status": "ok",
        "instance_id": st.instance.instance_id.to_string(),
        "version": st.instance.version,
    }))
}

/// `POST /instance/gate` — vérifie le mot de passe d'instance → jeton de gate court.
pub async fn gate(
    State(st): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(req): Json<GateRequest>,
) -> AppResult<Json<GateResponse>> {
    st.rate
        .check(ratelimit::GATE, &ip)
        .map_err(AppError::rate_limited)?;
    let Some(hash) = st.instance.gate_hash.as_deref() else {
        return Err(AppError::bad_request(
            "cette instance n'exige pas de mot de passe d'instance",
        ));
    };
    if !crypto::verify_password(&req.password, hash) {
        return Err(AppError::unauthorized("mot de passe d'instance incorrect"));
    }
    let token = crypto::jwt_encode(&st.jwt_secret, "gate", "gate", 600);
    Ok(Json(GateResponse { gate_token: token }))
}
