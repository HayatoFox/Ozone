-- Règles d'auto-modération par guilde (mots filtrés, anti-spam de mentions).
-- trigger_type : 'keyword' (mots interdits) | 'mention_spam' (trop de mentions dans un message).
-- action       : 'block' (refuse le message) | 'alert' (laisse passer mais notifie le salon d'alerte).
-- keywords     : JSON Vec<String> (mots, insensibles à la casse) pour 'keyword'.
-- mention_limit: seuil de mentions pour 'mention_spam'.
-- exempt_roles : JSON Vec<Snowflake> (rôles non soumis à la règle).
CREATE TABLE automod_rules (
    id              INTEGER PRIMARY KEY,
    guild_id        INTEGER NOT NULL,
    name            TEXT    NOT NULL,
    trigger_type    TEXT    NOT NULL,
    keywords        TEXT    NOT NULL DEFAULT '[]',
    mention_limit   INTEGER NOT NULL DEFAULT 5,
    action          TEXT    NOT NULL DEFAULT 'block',
    alert_channel_id INTEGER,
    exempt_roles    TEXT    NOT NULL DEFAULT '[]',
    enabled         INTEGER NOT NULL DEFAULT 1,
    created_at      INTEGER NOT NULL
);
CREATE INDEX idx_automod_guild ON automod_rules (guild_id);
