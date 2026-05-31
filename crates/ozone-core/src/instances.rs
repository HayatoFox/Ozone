//! Registre **multi-instances** côté client : liste des instances connues + instance courante.
//!
//! Un client Ozone parle à plusieurs **instances** auto-hébergées, chacune avec sa propre session.
//! La persistance est volontairement **sans jetons** : seules les métadonnées **non secrètes**
//! (adresse, nom, identifiant d'instance) sont sérialisées ; les jetons d'accès/refresh restent
//! **en mémoire** (pas de secret au repos en clair — cf. SECURITY §32/§34).

use crate::InstanceRef;
use ozone_proto::Snowflake;
use serde::{Deserialize, Serialize};

/// Entrée **persistée** d'une instance — métadonnées non secrètes uniquement (jamais de jeton).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedInstance {
    pub address: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<Snowflake>,
}

/// Registre des instances connues (chacune sa session ; une seule « courante » à la fois).
#[derive(Default)]
pub struct InstanceRegistry {
    instances: Vec<InstanceRef>,
    current: Option<usize>,
}

impl InstanceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ajoute une instance (**dédupliquée par adresse normalisée**) et la rend courante.
    /// Si l'adresse existe déjà, fusionne les métadonnées/jetons fournis (sans en effacer) et
    /// renvoie l'index existant.
    pub fn add(&mut self, inst: InstanceRef) -> usize {
        let key = inst.api_base();
        let idx = if let Some(i) = self.instances.iter().position(|x| x.api_base() == key) {
            let slot = &mut self.instances[i];
            if inst.access_token.is_some() {
                slot.access_token = inst.access_token;
            }
            if inst.refresh_token.is_some() {
                slot.refresh_token = inst.refresh_token;
            }
            if inst.display_name.is_some() {
                slot.display_name = inst.display_name;
            }
            if inst.instance_id.is_some() {
                slot.instance_id = inst.instance_id;
            }
            i
        } else {
            self.instances.push(inst);
            self.instances.len() - 1
        };
        self.current = Some(idx);
        idx
    }

    /// Retire l'instance d'index `idx` et réajuste la sélection courante.
    pub fn remove(&mut self, idx: usize) {
        if idx >= self.instances.len() {
            return;
        }
        self.instances.remove(idx);
        self.current = match self.current {
            _ if self.instances.is_empty() => None,
            Some(c) if c == idx => Some(idx.min(self.instances.len() - 1)),
            Some(c) if c > idx => Some(c - 1),
            other => other,
        };
    }

    /// Sélectionne l'instance courante. Renvoie `false` si l'index est hors limites.
    pub fn select(&mut self, idx: usize) -> bool {
        if idx < self.instances.len() {
            self.current = Some(idx);
            true
        } else {
            false
        }
    }

    pub fn current_index(&self) -> Option<usize> {
        self.current
    }

    pub fn current(&self) -> Option<&InstanceRef> {
        self.current.and_then(|i| self.instances.get(i))
    }

    pub fn current_mut(&mut self) -> Option<&mut InstanceRef> {
        self.current.and_then(|i| self.instances.get_mut(i))
    }

    pub fn list(&self) -> &[InstanceRef] {
        &self.instances
    }

    pub fn len(&self) -> usize {
        self.instances.len()
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// Vue **persistable** : métadonnées non secrètes (jamais les jetons).
    pub fn to_persisted(&self) -> Vec<PersistedInstance> {
        self.instances
            .iter()
            .map(|i| PersistedInstance {
                address: i.address.clone(),
                display_name: i.display_name.clone(),
                instance_id: i.instance_id,
            })
            .collect()
    }

    /// Reconstruit un registre depuis des entrées persistées (sans jetons ⇒ **non authentifié** :
    /// l'utilisateur se reconnecte par instance).
    pub fn from_persisted(items: Vec<PersistedInstance>) -> Self {
        let instances: Vec<InstanceRef> = items
            .into_iter()
            .map(|p| InstanceRef {
                address: p.address,
                instance_id: p.instance_id,
                display_name: p.display_name,
                access_token: None,
                refresh_token: None,
            })
            .collect();
        let current = (!instances.is_empty()).then_some(0);
        Self { instances, current }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inst(addr: &str) -> InstanceRef {
        InstanceRef::new(addr)
    }

    #[test]
    fn add_dedups_by_address_and_selects_latest() {
        let mut r = InstanceRegistry::new();
        assert_eq!(r.add(inst("https://a.fr")), 0);
        assert_eq!(r.add(inst("https://b.fr")), 1);
        assert_eq!(r.len(), 2);
        assert_eq!(r.current_index(), Some(1));
        // Ré-ajout de la même adresse → index existant, pas de doublon.
        assert_eq!(r.add(inst("https://a.fr")), 0);
        assert_eq!(r.len(), 2);
        assert_eq!(r.current_index(), Some(0));
    }

    #[test]
    fn readd_preserves_existing_tokens() {
        let mut r = InstanceRegistry::new();
        let mut authed = inst("https://a.fr");
        authed.access_token = Some("tok".into());
        r.add(authed);
        // Ré-ajout sans jeton (p. ex. depuis le registre persisté) ne doit pas effacer le jeton.
        r.add(inst("https://a.fr"));
        assert_eq!(r.current().unwrap().access_token.as_deref(), Some("tok"));
    }

    #[test]
    fn remove_adjusts_selection() {
        let mut r = InstanceRegistry::new();
        r.add(inst("https://a.fr"));
        r.add(inst("https://b.fr"));
        r.add(inst("https://c.fr"));
        assert!(r.select(2));
        r.remove(0); // a retiré ; courant (c, ex-index 2) → index 1
        assert_eq!(r.len(), 2);
        assert_eq!(r.current().unwrap().address, "https://c.fr");
    }

    #[test]
    fn persist_excludes_tokens_and_roundtrips() {
        let mut r = InstanceRegistry::new();
        let mut authed = inst("https://a.fr");
        authed.access_token = Some("SECRET_ACCESS".into());
        authed.refresh_token = Some("SECRET_REFRESH".into());
        authed.display_name = Some("Atelier".into());
        r.add(authed);

        let persisted = r.to_persisted();
        let json = serde_json::to_string(&persisted).unwrap();
        // Garantie de sécurité : aucun jeton n'atteint la forme persistée.
        assert!(!json.contains("SECRET"));
        assert!(json.contains("https://a.fr"));

        let back = InstanceRegistry::from_persisted(persisted);
        assert_eq!(back.len(), 1);
        assert_eq!(
            back.current().unwrap().display_name.as_deref(),
            Some("Atelier")
        );
        assert!(back.current().unwrap().access_token.is_none()); // non authentifié au rechargement
    }
}
