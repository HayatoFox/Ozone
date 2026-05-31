//! Cœur du nœud média **SFU** (Selective Forwarding Unit) d'Ozone.
//!
//! Le SFU reçoit **un** flux montant par participant et le **relaie** aux autres (sans mixage
//! ni transcodage audio) → faible latence, faible CPU, compatible E2EE. Cf.
//! `docs/06-infrastructure-vocale.md`. Processus séparé de l'API (la pile WebRTC introduit
//! `ring`/`rustls`, confinés à ce binaire ; l'API REST/Gateway reste sans `ring`).

pub mod room;
