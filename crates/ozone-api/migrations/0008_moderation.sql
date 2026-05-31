-- Modération : bannissements, mises en sourdine (timeout), journal d'audit.
-- Cf. docs/features/11-moderation-securite.md.

CREATE TABLE guild_bans (
    guild_id     INTEGER NOT NULL,
    user_id      INTEGER NOT NULL,
    reason       TEXT,
    moderator_id INTEGER NOT NULL,
    created_at   INTEGER NOT NULL,
    PRIMARY KEY (guild_id, user_id)
);

-- Timeout : instant (ms epoch) jusqu'auquel le membre ne peut ni écrire ni parler. NULL = aucun.
ALTER TABLE guild_members ADD COLUMN communication_disabled_until INTEGER;

CREATE TABLE audit_log (
    id          INTEGER PRIMARY KEY,
    guild_id    INTEGER NOT NULL,
    user_id     INTEGER NOT NULL,   -- acteur (modérateur)
    target_id   INTEGER,            -- cible
    action_type TEXT    NOT NULL,   -- member_kick | member_ban | member_unban | member_timeout | ...
    reason      TEXT,
    changes     TEXT,               -- JSON optionnel (avant/après)
    created_at  INTEGER NOT NULL
);
CREATE INDEX idx_audit_guild ON audit_log (guild_id, id DESC);
