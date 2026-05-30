-- Expressions de guilde : emojis, stickers, sons de soundboard (cf. docs/features/12-expressions.md).
-- Les assets binaires (image/son) sont référencés par un identifiant ; le pipeline de stockage
-- objet (S3/MinIO) viendra plus tard — ici on gère les métadonnées et les permissions.

CREATE TABLE emojis (
    id         INTEGER PRIMARY KEY,
    guild_id   INTEGER NOT NULL,
    name       TEXT    NOT NULL,
    animated   INTEGER NOT NULL DEFAULT 0,
    image_id   TEXT    NOT NULL,
    created_by INTEGER NOT NULL,
    available  INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_emojis_guild ON emojis (guild_id);

CREATE TABLE stickers (
    id          INTEGER PRIMARY KEY,
    guild_id    INTEGER NOT NULL,
    name        TEXT    NOT NULL,
    description TEXT,
    tags        TEXT,
    format_type INTEGER NOT NULL DEFAULT 1,  -- 1 png · 2 apng · 3 lottie · 4 gif
    asset_id    TEXT    NOT NULL,
    created_by  INTEGER NOT NULL,
    available   INTEGER NOT NULL DEFAULT 1,
    created_at  INTEGER NOT NULL
);
CREATE INDEX idx_stickers_guild ON stickers (guild_id);

CREATE TABLE soundboard_sounds (
    id         INTEGER PRIMARY KEY,
    guild_id   INTEGER NOT NULL,
    name       TEXT    NOT NULL,
    sound_id   TEXT    NOT NULL,
    volume     REAL    NOT NULL DEFAULT 1.0,
    emoji      TEXT,
    created_by INTEGER NOT NULL,
    available  INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_sounds_guild ON soundboard_sounds (guild_id);
