-- S7 : webhooks entrants, événements programmés, recherche plein-texte (FTS5).
-- Cf. docs/features/17-webhooks-bots-integrations.md, 18-evenements.md, 14-recherche.md.

-- ───────────────────────────── Webhooks ─────────────────────────────
CREATE TABLE webhooks (
    id         INTEGER PRIMARY KEY,
    channel_id INTEGER NOT NULL,
    guild_id   INTEGER NOT NULL,
    name       TEXT    NOT NULL,
    avatar_id  TEXT,
    token      TEXT    NOT NULL UNIQUE,
    created_by INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_webhooks_channel ON webhooks (channel_id);
CREATE INDEX idx_webhooks_guild ON webhooks (guild_id);

-- Attribution d'un message à un webhook (nom/avatar de remplacement par exécution).
ALTER TABLE messages ADD COLUMN webhook_id    INTEGER;
ALTER TABLE messages ADD COLUMN author_name   TEXT;
ALTER TABLE messages ADD COLUMN author_avatar TEXT;

-- ───────────────────────────── Événements programmés ─────────────────────────────
CREATE TABLE scheduled_events (
    id              INTEGER PRIMARY KEY,
    guild_id        INTEGER NOT NULL,
    channel_id      INTEGER,                    -- salon vocal/stage ; NULL si externe
    creator_id      INTEGER NOT NULL,
    name            TEXT    NOT NULL,
    description     TEXT,
    cover_id        TEXT,
    entity_type     INTEGER NOT NULL,           -- 1 = stage, 2 = vocal, 3 = externe
    location        TEXT,                        -- lieu/URL libre (externe)
    scheduled_start INTEGER NOT NULL,
    scheduled_end   INTEGER,
    status          INTEGER NOT NULL DEFAULT 1, -- 1 programmé, 2 actif, 3 terminé, 4 annulé
    created_at      INTEGER NOT NULL
);
CREATE INDEX idx_events_guild ON scheduled_events (guild_id, scheduled_start);

CREATE TABLE event_interested (
    event_id   INTEGER NOT NULL,
    user_id    INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (event_id, user_id)
);

-- ───────────────────────────── Recherche plein-texte (FTS5) ─────────────────────────────
-- Table à contenu externe : l'index ne stocke pas le texte, il pointe vers messages(id).
CREATE VIRTUAL TABLE messages_fts USING fts5(content, content='messages', content_rowid='id');

-- Backfill des messages déjà présents.
INSERT INTO messages_fts (rowid, content) SELECT id, content FROM messages;

-- Synchronisation automatique (aucun couplage applicatif nécessaire).
CREATE TRIGGER messages_ai AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts (rowid, content) VALUES (new.id, new.content);
END;
CREATE TRIGGER messages_ad AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts (messages_fts, rowid, content) VALUES ('delete', old.id, old.content);
END;
CREATE TRIGGER messages_au AFTER UPDATE ON messages BEGIN
    INSERT INTO messages_fts (messages_fts, rowid, content) VALUES ('delete', old.id, old.content);
    INSERT INTO messages_fts (rowid, content) VALUES (new.id, new.content);
END;
