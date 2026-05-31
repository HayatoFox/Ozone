//! Cache local **SQLite** du [`Store`] client : démarrage hors-ligne et historique persistant,
//! avec **rétention bornée** (anti-croissance disque/mémoire, cf. SECURITY §30/R8).
//!
//! Les DTO sont stockés en **blobs JSON** : le cache est un *miroir* local, pas l'autorité
//! (le serveur l'est). Avantage : robuste à l'évolution des DTO — aucune migration par champ,
//! et toute ligne devenue illisible est simplement **ignorée** à l'hydratation (sans panique).
//!
//! Même `sqlx`/SQLite (sans TLS) que le serveur ⇒ pas de second binding natif côté client.

use crate::store::{id_field, Store};
use anyhow::Result;
use ozone_proto::dto::{Channel, Guild, Message, PresenceView};
use ozone_proto::gateway::GatewayFrame;
use ozone_proto::Snowflake;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

/// Plafond de messages **persistés** par salon, appliqué après chaque insertion live.
pub const DEFAULT_DISK_CAP: i64 = 2000;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS cache_guilds    (id INTEGER PRIMARY KEY, data TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS cache_channels  (id INTEGER PRIMARY KEY, data TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS cache_messages  (id INTEGER PRIMARY KEY, channel_id INTEGER NOT NULL, created_at INTEGER NOT NULL, data TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS idx_cache_msgs_chan ON cache_messages(channel_id, created_at, id);
CREATE TABLE IF NOT EXISTS cache_presences (user_id INTEGER PRIMARY KEY, data TEXT NOT NULL);
";

/// Cache local d'**une** instance (un fichier par instance, comme le `Store`).
pub struct Cache {
    pool: SqlitePool,
    /// Plafond disque par salon, appliqué automatiquement après les insertions live.
    disk_cap: i64,
}

impl Cache {
    /// Ouvre (ou crée) le cache au chemin donné et garantit le schéma.
    pub async fn open(path: &str) -> Result<Self> {
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(opts)
            .await?;
        sqlx::raw_sql(SCHEMA).execute(&pool).await?;
        Ok(Self {
            pool,
            disk_cap: DEFAULT_DISK_CAP,
        })
    }

    /// Change le plafond disque par salon (`0` = illimité).
    pub fn set_disk_cap(&mut self, cap: i64) {
        self.disk_cap = cap;
    }

    // ─────────────── Écriture (upsert) ───────────────

    pub async fn save_guild(&self, g: &Guild) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO cache_guilds(id, data) VALUES (?, ?)")
            .bind(g.id.as_i64())
            .bind(serde_json::to_string(g)?)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn save_channel(&self, c: &Channel) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO cache_channels(id, data) VALUES (?, ?)")
            .bind(c.id.as_i64())
            .bind(serde_json::to_string(c)?)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn save_message(&self, m: &Message) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO cache_messages(id, channel_id, created_at, data) VALUES (?, ?, ?, ?)",
        )
        .bind(m.id.as_i64())
        .bind(m.channel_id.as_i64())
        .bind(m.created_at as i64)
        .bind(serde_json::to_string(m)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn save_presence(&self, p: &PresenceView) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO cache_presences(user_id, data) VALUES (?, ?)")
            .bind(p.user_id.as_i64())
            .bind(serde_json::to_string(p)?)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ─────────────── Suppression ───────────────

    pub async fn delete_message(&self, id: Snowflake) -> Result<bool> {
        let r = sqlx::query("DELETE FROM cache_messages WHERE id = ?")
            .bind(id.as_i64())
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected() > 0)
    }

    /// Supprime un salon **et tous ses messages** persistés. Renvoie `true` si quoi que ce
    /// soit a été supprimé (la ligne salon **ou** au moins un message — le cache peut détenir
    /// des messages d'un salon dont la ligne n'a jamais été persistée).
    pub async fn delete_channel(&self, id: Snowflake) -> Result<bool> {
        let msgs = sqlx::query("DELETE FROM cache_messages WHERE channel_id = ?")
            .bind(id.as_i64())
            .execute(&self.pool)
            .await?;
        let chan = sqlx::query("DELETE FROM cache_channels WHERE id = ?")
            .bind(id.as_i64())
            .execute(&self.pool)
            .await?;
        Ok(msgs.rows_affected() + chan.rows_affected() > 0)
    }

    pub async fn delete_guild(&self, id: Snowflake) -> Result<bool> {
        let r = sqlx::query("DELETE FROM cache_guilds WHERE id = ?")
            .bind(id.as_i64())
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected() > 0)
    }

    // ─────────────── Instantanés REST ───────────────

    pub async fn save_guilds(&self, guilds: &[Guild]) -> Result<()> {
        for g in guilds {
            self.save_guild(g).await?;
        }
        Ok(())
    }

    pub async fn save_channels(&self, channels: &[Channel]) -> Result<()> {
        for c in channels {
            self.save_channel(c).await?;
        }
        Ok(())
    }

    /// Remplace l'historique persisté d'un salon par `msgs` (puis applique le plafond disque).
    pub async fn replace_channel_messages(
        &self,
        channel_id: Snowflake,
        msgs: &[Message],
    ) -> Result<()> {
        sqlx::query("DELETE FROM cache_messages WHERE channel_id = ?")
            .bind(channel_id.as_i64())
            .execute(&self.pool)
            .await?;
        for m in msgs {
            self.save_message(m).await?;
        }
        self.prune_channel_messages(channel_id, self.disk_cap)
            .await?;
        Ok(())
    }

    // ─────────────── Rétention (clôt R8 côté disque) ───────────────

    /// Conserve les `keep` messages **les plus récents** d'un salon, supprime les plus anciens.
    /// `keep <= 0` ⇒ aucune purge (illimité). Renvoie le nombre de lignes supprimées.
    pub async fn prune_channel_messages(&self, channel_id: Snowflake, keep: i64) -> Result<u64> {
        if keep <= 0 {
            return Ok(0);
        }
        // Sous-requête : les `keep` plus récents à GARDER ; on supprime le reste du même salon.
        let r = sqlx::query(
            "DELETE FROM cache_messages \
             WHERE channel_id = ?1 AND id NOT IN ( \
                 SELECT id FROM cache_messages WHERE channel_id = ?1 \
                 ORDER BY created_at DESC, id DESC LIMIT ?2 \
             )",
        )
        .bind(channel_id.as_i64())
        .bind(keep)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected())
    }

    // ─────────────── Application des événements Gateway (jumeau disque de `Store::apply`) ───────────────

    /// Persiste l'effet d'un événement Gateway. Renvoie `true` si une écriture a eu lieu.
    pub async fn apply(&self, frame: &GatewayFrame) -> Result<bool> {
        let Some(t) = frame.t.as_deref() else {
            return Ok(false);
        };
        let Some(d) = frame.d.as_ref() else {
            return Ok(false);
        };
        match t {
            "MESSAGE_CREATE" | "MESSAGE_UPDATE" => {
                match serde_json::from_value::<Message>(d.clone()) {
                    Ok(m) => {
                        let cid = m.channel_id;
                        self.save_message(&m).await?;
                        if t == "MESSAGE_CREATE" {
                            self.prune_channel_messages(cid, self.disk_cap).await?;
                        }
                        Ok(true)
                    }
                    Err(_) => Ok(false),
                }
            }
            "MESSAGE_DELETE" => match id_field(d, "id") {
                Some(mid) => self.delete_message(Snowflake::from_i64(mid)).await,
                None => Ok(false),
            },
            "CHANNEL_CREATE" | "CHANNEL_UPDATE" | "THREAD_CREATE" => {
                match serde_json::from_value::<Channel>(d.clone()) {
                    Ok(c) => {
                        self.save_channel(&c).await?;
                        Ok(true)
                    }
                    Err(_) => Ok(false),
                }
            }
            "CHANNEL_DELETE" => match id_field(d, "id") {
                Some(cid) => self.delete_channel(Snowflake::from_i64(cid)).await,
                None => Ok(false),
            },
            "GUILD_CREATE" | "GUILD_UPDATE" => match serde_json::from_value::<Guild>(d.clone()) {
                Ok(g) => {
                    self.save_guild(&g).await?;
                    Ok(true)
                }
                Err(_) => Ok(false),
            },
            "GUILD_DELETE" => match id_field(d, "id") {
                Some(gid) => self.delete_guild(Snowflake::from_i64(gid)).await,
                None => Ok(false),
            },
            "PRESENCE_UPDATE" => match serde_json::from_value::<PresenceView>(d.clone()) {
                Ok(p) => {
                    self.save_presence(&p).await?;
                    Ok(true)
                }
                Err(_) => Ok(false),
            },
            _ => Ok(false),
        }
    }

    // ─────────────── Hydratation du `Store` ───────────────

    /// Remplit un [`Store`] depuis le cache. Pour chaque salon, charge au plus
    /// `msgs_per_channel` messages **les plus récents** (rendus dans l'ordre chronologique).
    /// Les lignes JSON illisibles (DTO obsolète) sont ignorées sans erreur.
    pub async fn load_into(&self, store: &mut Store, msgs_per_channel: i64) -> Result<()> {
        for row in sqlx::query("SELECT data FROM cache_guilds")
            .fetch_all(&self.pool)
            .await?
        {
            if let Ok(g) = serde_json::from_str::<Guild>(row.get::<String, _>("data").as_str()) {
                store.guilds.insert(g.id.as_i64(), g);
            }
        }
        for row in sqlx::query("SELECT data FROM cache_channels")
            .fetch_all(&self.pool)
            .await?
        {
            if let Ok(c) = serde_json::from_str::<Channel>(row.get::<String, _>("data").as_str()) {
                store.channels.insert(c.id.as_i64(), c);
            }
        }
        for row in sqlx::query("SELECT data FROM cache_presences")
            .fetch_all(&self.pool)
            .await?
        {
            if let Ok(p) =
                serde_json::from_str::<PresenceView>(row.get::<String, _>("data").as_str())
            {
                store.presences.insert(p.user_id.as_i64(), p);
            }
        }
        let channel_ids: Vec<i64> =
            sqlx::query_scalar("SELECT DISTINCT channel_id FROM cache_messages")
                .fetch_all(&self.pool)
                .await?;
        for cid in channel_ids {
            let rows = sqlx::query(
                "SELECT data FROM cache_messages WHERE channel_id = ?1 \
                 ORDER BY created_at DESC, id DESC LIMIT ?2",
            )
            .bind(cid)
            .bind(msgs_per_channel.max(0))
            .fetch_all(&self.pool)
            .await?;
            let mut msgs: Vec<Message> = rows
                .iter()
                .filter_map(|r| serde_json::from_str(r.get::<String, _>("data").as_str()).ok())
                .collect();
            msgs.reverse(); // récupérés du plus récent au plus ancien ⇒ on rétablit l'ordre d'arrivée
            store.set_messages(Snowflake::from_i64(cid), msgs);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn temp_db(tag: &str) -> String {
        let unique = format!(
            "{}_{:?}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            std::thread::current().id()
        );
        std::env::temp_dir()
            .join(format!("ozone-cache-{tag}-{unique}.db"))
            .to_string_lossy()
            .to_string()
    }

    fn dispatch(t: &str, d: Value) -> GatewayFrame {
        GatewayFrame::dispatch(t, d, 1)
    }

    fn msg(id: u64, cid: u64, content: &str) -> Value {
        json!({
            "id": id.to_string(), "channel_id": cid.to_string(),
            "author": { "id": "1", "username": "u", "display_name": null, "avatar_id": null },
            "content": content, "type": 0, "created_at": id, "edited_at": null, "pinned": false
        })
    }

    #[tokio::test]
    async fn persists_and_hydrates_across_reopen() {
        let path = temp_db("rt");
        let guild: Guild = serde_json::from_value(json!({
            "id": "1", "name": "G", "owner_id": "1", "icon_id": null,
            "description": null, "discoverable": false
        }))
        .unwrap();
        let chan: Channel = serde_json::from_value(json!({
            "id": "10", "guild_id": "1", "type": 0, "name": "c", "topic": null,
            "position": 0, "parent_id": null, "nsfw": false, "rate_limit_per_user": 0
        }))
        .unwrap();

        {
            let cache = Cache::open(&path).await.expect("open");
            cache.save_guild(&guild).await.unwrap();
            cache.save_channel(&chan).await.unwrap();
            // Trois messages live + une présence.
            for i in 1..=3u64 {
                assert!(cache
                    .apply(&dispatch("MESSAGE_CREATE", msg(i, 10, &format!("m{i}"))))
                    .await
                    .unwrap());
            }
            assert!(cache
                .apply(&dispatch(
                    "PRESENCE_UPDATE",
                    json!({ "user_id": "7", "status": "online", "custom_status": null })
                ))
                .await
                .unwrap());
        }

        // Réouverture (fichier) → l'état doit revenir.
        let cache = Cache::open(&path).await.expect("reopen");
        let mut store = Store::new();
        cache.load_into(&mut store, 100).await.expect("hydrate");

        assert!(store.guilds.contains_key(&1));
        assert!(store.channels.contains_key(&10));
        assert_eq!(store.status_of(Snowflake::from_i64(7)), "online");
        let msgs = store.messages_of(Snowflake::from_i64(10));
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].content, "m1"); // ordre chronologique rétabli
        assert_eq!(msgs[2].content, "m3");

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn retention_keeps_newest() {
        let path = temp_db("ret");
        let cache = Cache::open(&path).await.expect("open");
        for i in 1..=10u64 {
            cache
                .save_message(&serde_json::from_value(msg(i, 5, "x")).unwrap())
                .await
                .unwrap();
        }
        let removed = cache
            .prune_channel_messages(Snowflake::from_i64(5), 3)
            .await
            .unwrap();
        assert_eq!(removed, 7);

        let mut store = Store::with_message_cap(0);
        cache.load_into(&mut store, 100).await.unwrap();
        let msgs = store.messages_of(Snowflake::from_i64(5));
        assert_eq!(msgs.len(), 3);
        // Les 3 plus récents (ids 8,9,10) survivent, dans l'ordre.
        assert_eq!(msgs[0].id.as_i64(), 8);
        assert_eq!(msgs[2].id.as_i64(), 10);

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn channel_delete_drops_messages() {
        let path = temp_db("del");
        let cache = Cache::open(&path).await.expect("open");
        cache
            .save_message(&serde_json::from_value(msg(1, 42, "a")).unwrap())
            .await
            .unwrap();
        cache
            .save_message(&serde_json::from_value(msg(2, 42, "b")).unwrap())
            .await
            .unwrap();
        assert!(cache
            .apply(&dispatch("CHANNEL_DELETE", json!({ "id": "42" })))
            .await
            .unwrap());

        let mut store = Store::new();
        cache.load_into(&mut store, 100).await.unwrap();
        assert!(store.messages_of(Snowflake::from_i64(42)).is_empty());

        let _ = std::fs::remove_file(&path);
    }
}
