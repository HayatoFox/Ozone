//! Registre de présence en mémoire (mode tout-en-un) : suit le nombre de connexions Gateway
//! par utilisateur et son statut désiré. Cf. docs/features/13-notifications.md (statut/présence).
//!
//! Le statut **effectif** vu par autrui est `offline` si l'utilisateur n'a aucune connexion
//! active **ou** s'il s'est mis en `invisible`.

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Clone, Debug)]
struct Entry {
    connections: u32,
    /// Statut désiré : `online` | `idle` | `dnd` | `invisible`.
    status: String,
    custom_status: Option<String>,
}

#[derive(Default)]
pub struct Registry {
    inner: Mutex<HashMap<i64, Entry>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Statut valide ?
    pub fn valid_status(s: &str) -> bool {
        matches!(s, "online" | "idle" | "dnd" | "invisible")
    }

    /// Enregistre une connexion. Renvoie `true` si l'utilisateur **vient** de passer en ligne.
    pub fn connect(&self, uid: i64) -> bool {
        let mut m = self.inner.lock().unwrap();
        let e = m.entry(uid).or_insert_with(|| Entry {
            connections: 0,
            status: "online".into(),
            custom_status: None,
        });
        e.connections += 1;
        e.connections == 1
    }

    /// Retire une connexion. Renvoie `true` si l'utilisateur **passe** hors ligne.
    pub fn disconnect(&self, uid: i64) -> bool {
        let mut m = self.inner.lock().unwrap();
        if let Some(e) = m.get_mut(&uid) {
            e.connections = e.connections.saturating_sub(1);
            if e.connections == 0 {
                m.remove(&uid);
                return true;
            }
        }
        false
    }

    /// Définit le statut désiré (crée l'entrée si besoin). N'altère pas le compteur de connexions.
    pub fn set_status(&self, uid: i64, status: &str, custom_status: Option<String>) {
        let mut m = self.inner.lock().unwrap();
        let e = m.entry(uid).or_insert_with(|| Entry {
            connections: 0,
            status: "online".into(),
            custom_status: None,
        });
        e.status = status.to_string();
        e.custom_status = custom_status;
    }

    /// Statut **effectif** visible par autrui : `offline` si hors ligne ou invisible.
    pub fn effective(&self, uid: i64) -> (String, Option<String>) {
        let m = self.inner.lock().unwrap();
        match m.get(&uid) {
            Some(e) if e.connections > 0 && e.status != "invisible" => {
                (e.status.clone(), e.custom_status.clone())
            }
            _ => ("offline".to_string(), None),
        }
    }
}
