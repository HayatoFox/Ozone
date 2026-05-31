//! Store normalisé côté client : conserve guildes, salons, messages (par salon) et présences,
//! et **applique les événements Gateway** pour rester à jour en temps réel. Indépendant de l'UI.

use ozone_proto::dto::{Channel, Guild, Message, PresenceView};
use ozone_proto::gateway::GatewayFrame;
use ozone_proto::Snowflake;
use serde_json::Value;
use std::collections::HashMap;

pub(crate) fn id_field(d: &Value, key: &str) -> Option<i64> {
    d.get(key)?.as_str()?.parse::<i64>().ok()
}

/// Plafond par défaut de messages conservés **en mémoire** par salon (anti-OOM, cf. SECURITY §30/R8).
pub const DEFAULT_MESSAGE_CAP: usize = 1000;

/// État client d'**une** instance (multi-instances ⇒ un `Store` par instance).
#[derive(Default)]
pub struct Store {
    pub guilds: HashMap<i64, Guild>,
    pub channels: HashMap<i64, Channel>,
    /// Messages par `channel_id` (ordre d'arrivée).
    pub messages: HashMap<i64, Vec<Message>>,
    /// Statut effectif par utilisateur (`online`/`idle`/`dnd`/`offline`).
    pub presences: HashMap<i64, PresenceView>,
    /// Plafond de messages gardés **en mémoire** par salon (`0` = illimité). Borne la
    /// croissance mémoire face à un flot d'événements ou un serveur compromis (cf. R8).
    pub max_messages_per_channel: usize,
}

impl Store {
    /// Store avec le plafond mémoire par défaut ([`DEFAULT_MESSAGE_CAP`]).
    pub fn new() -> Self {
        Self {
            max_messages_per_channel: DEFAULT_MESSAGE_CAP,
            ..Default::default()
        }
    }

    /// Store avec un plafond mémoire explicite par salon (`0` = illimité).
    pub fn with_message_cap(cap: usize) -> Self {
        Self {
            max_messages_per_channel: cap,
            ..Default::default()
        }
    }

    /// Tronque le salon au plafond en mémoire en supprimant les messages **les plus anciens**.
    fn trim_channel(&mut self, channel_id: i64) {
        let cap = self.max_messages_per_channel;
        if cap == 0 {
            return;
        }
        if let Some(list) = self.messages.get_mut(&channel_id) {
            if list.len() > cap {
                let excess = list.len() - cap;
                list.drain(0..excess);
            }
        }
    }

    // ─────────────── Ingestion REST ───────────────

    pub fn set_guilds(&mut self, guilds: Vec<Guild>) {
        for g in guilds {
            self.guilds.insert(g.id.as_i64(), g);
        }
    }

    pub fn set_channels(&mut self, channels: Vec<Channel>) {
        for c in channels {
            self.channels.insert(c.id.as_i64(), c);
        }
    }

    pub fn set_messages(&mut self, channel_id: Snowflake, mut msgs: Vec<Message>) {
        let cap = self.max_messages_per_channel;
        if cap != 0 && msgs.len() > cap {
            let excess = msgs.len() - cap;
            msgs.drain(0..excess); // garde les plus récents (fin de liste)
        }
        self.messages.insert(channel_id.as_i64(), msgs);
    }

    /// Messages connus d'un salon (vide si inconnu).
    pub fn messages_of(&self, channel_id: Snowflake) -> &[Message] {
        self.messages
            .get(&channel_id.as_i64())
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Statut effectif d'un utilisateur (`offline` si inconnu).
    pub fn status_of(&self, user_id: Snowflake) -> &str {
        self.presences
            .get(&user_id.as_i64())
            .map(|p| p.status.as_str())
            .unwrap_or("offline")
    }

    // ─────────────── Application des événements Gateway ───────────────

    /// Met à jour l'état d'après un événement Gateway. Renvoie `true` si l'état a changé.
    pub fn apply(&mut self, frame: &GatewayFrame) -> bool {
        let Some(t) = frame.t.as_deref() else {
            return false;
        };
        let Some(d) = frame.d.as_ref() else {
            return false;
        };
        match t {
            "MESSAGE_CREATE" => match serde_json::from_value::<Message>(d.clone()) {
                Ok(m) => {
                    let cid = m.channel_id.as_i64();
                    self.messages.entry(cid).or_default().push(m);
                    self.trim_channel(cid);
                    true
                }
                Err(_) => false,
            },
            "MESSAGE_UPDATE" => match serde_json::from_value::<Message>(d.clone()) {
                Ok(m) => {
                    if let Some(list) = self.messages.get_mut(&m.channel_id.as_i64()) {
                        if let Some(slot) = list.iter_mut().find(|x| x.id == m.id) {
                            *slot = m;
                            return true;
                        }
                    }
                    false
                }
                Err(_) => false,
            },
            "MESSAGE_DELETE" => {
                let (Some(cid), Some(mid)) = (id_field(d, "channel_id"), id_field(d, "id")) else {
                    return false;
                };
                if let Some(list) = self.messages.get_mut(&cid) {
                    let before = list.len();
                    list.retain(|m| m.id.as_i64() != mid);
                    return list.len() != before;
                }
                false
            }
            "CHANNEL_CREATE" | "CHANNEL_UPDATE" | "THREAD_CREATE" => {
                match serde_json::from_value::<Channel>(d.clone()) {
                    Ok(c) => {
                        self.channels.insert(c.id.as_i64(), c);
                        true
                    }
                    Err(_) => false,
                }
            }
            "CHANNEL_DELETE" => match id_field(d, "id") {
                Some(cid) => {
                    self.messages.remove(&cid);
                    self.channels.remove(&cid).is_some()
                }
                None => false,
            },
            "GUILD_CREATE" | "GUILD_UPDATE" => match serde_json::from_value::<Guild>(d.clone()) {
                Ok(g) => {
                    self.guilds.insert(g.id.as_i64(), g);
                    true
                }
                Err(_) => false,
            },
            "GUILD_DELETE" => match id_field(d, "id") {
                Some(gid) => self.guilds.remove(&gid).is_some(),
                None => false,
            },
            "PRESENCE_UPDATE" => match serde_json::from_value::<PresenceView>(d.clone()) {
                Ok(p) => {
                    self.presences.insert(p.user_id.as_i64(), p);
                    true
                }
                Err(_) => false,
            },
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ozone_proto::gateway::{opcode, GatewayFrame};
    use serde_json::json;

    fn dispatch(t: &str, d: Value) -> GatewayFrame {
        GatewayFrame::dispatch(t, d, 1)
    }

    #[test]
    fn applies_message_lifecycle() {
        let mut s = Store::new();
        let cid = Snowflake::from_i64(42);
        let msg = json!({
            "id": "100", "channel_id": "42",
            "author": { "id": "7", "username": "alice", "display_name": null, "avatar_id": null },
            "content": "salut", "type": 0, "created_at": 1, "edited_at": null, "pinned": false
        });
        assert!(s.apply(&dispatch("MESSAGE_CREATE", msg)));
        assert_eq!(s.messages_of(cid).len(), 1);
        assert_eq!(s.messages_of(cid)[0].content, "salut");

        // Édition.
        let edited = json!({
            "id": "100", "channel_id": "42",
            "author": { "id": "7", "username": "alice", "display_name": null, "avatar_id": null },
            "content": "salut (édité)", "type": 0, "created_at": 1, "edited_at": 2, "pinned": false
        });
        assert!(s.apply(&dispatch("MESSAGE_UPDATE", edited)));
        assert_eq!(s.messages_of(cid)[0].content, "salut (édité)");

        // Suppression.
        assert!(s.apply(&dispatch(
            "MESSAGE_DELETE",
            json!({ "id": "100", "channel_id": "42" })
        )));
        assert_eq!(s.messages_of(cid).len(), 0);
    }

    #[test]
    fn applies_presence_and_channel_delete() {
        let mut s = Store::new();
        assert!(s.apply(&dispatch(
            "PRESENCE_UPDATE",
            json!({ "user_id": "7", "status": "dnd", "custom_status": null })
        )));
        assert_eq!(s.status_of(Snowflake::from_i64(7)), "dnd");
        assert_eq!(s.status_of(Snowflake::from_i64(8)), "offline");

        // Salon créé puis supprimé.
        let ch = json!({
            "id": "5", "guild_id": "1", "type": 0, "name": "général", "topic": null,
            "position": 0, "parent_id": null, "nsfw": false, "rate_limit_per_user": 0
        });
        assert!(s.apply(&dispatch("CHANNEL_CREATE", ch)));
        assert!(s.channels.contains_key(&5));
        assert!(s.apply(&dispatch(
            "CHANNEL_DELETE",
            json!({ "id": "5", "guild_id": "1" })
        )));
        assert!(!s.channels.contains_key(&5));
    }

    #[test]
    fn ignores_non_dispatch_or_unknown() {
        let mut s = Store::new();
        assert!(!s.apply(&GatewayFrame::new(opcode::HEARTBEAT_ACK)));
        assert!(!s.apply(&dispatch("TYPING_START", json!({ "channel_id": "1" }))));
    }

    #[test]
    fn caps_messages_in_memory() {
        // Plafond de 3 : au-delà, les plus anciens sont évincés (anti-OOM, R8).
        let mut s = Store::with_message_cap(3);
        let cid = Snowflake::from_i64(9);
        for i in 1..=6u64 {
            let m = json!({
                "id": i.to_string(), "channel_id": "9",
                "author": { "id": "1", "username": "u", "display_name": null, "avatar_id": null },
                "content": format!("m{i}"), "type": 0, "created_at": i, "edited_at": null, "pinned": false
            });
            assert!(s.apply(&dispatch("MESSAGE_CREATE", m)));
        }
        let kept = s.messages_of(cid);
        assert_eq!(kept.len(), 3, "plafonné à 3");
        // Les 3 plus récents (m4,m5,m6) sont conservés, dans l'ordre.
        assert_eq!(kept[0].content, "m4");
        assert_eq!(kept[2].content, "m6");

        // set_messages respecte aussi le plafond (garde les plus récents).
        let many: Vec<_> = (1..=5u64)
            .map(|i| {
                serde_json::from_value(json!({
                    "id": i.to_string(), "channel_id": "9",
                    "author": { "id": "1", "username": "u", "display_name": null, "avatar_id": null },
                    "content": format!("x{i}"), "type": 0, "created_at": i, "edited_at": null, "pinned": false
                }))
                .unwrap()
            })
            .collect();
        s.set_messages(cid, many);
        let kept = s.messages_of(cid);
        assert_eq!(kept.len(), 3);
        assert_eq!(kept[0].content, "x3");
    }
}
