-- Schéma initial Ozone (mode tout-en-un SQLite). Cf. docs/03-modele-de-donnees.md.

-- Configuration de l'instance (singleton, id = 1).
CREATE TABLE instance_config (
    id                  INTEGER PRIMARY KEY CHECK (id = 1),
    instance_id         INTEGER NOT NULL,
    name                TEXT    NOT NULL,
    description         TEXT,
    accent_color        INTEGER,
    registration_policy TEXT    NOT NULL DEFAULT 'open',
    access_gate_hash    TEXT,                 -- mot de passe d'instance (Argon2id), NULL = pas de gate
    jwt_secret          TEXT    NOT NULL,
    public_key          TEXT    NOT NULL,
    version             TEXT    NOT NULL,
    created_at          INTEGER NOT NULL
);

-- Comptes (propres à l'instance).
CREATE TABLE users (
    id            INTEGER PRIMARY KEY,
    username      TEXT    NOT NULL UNIQUE,
    display_name  TEXT,
    email         TEXT    NOT NULL UNIQUE,
    password_hash TEXT    NOT NULL,
    avatar_id     TEXT,
    created_at    INTEGER NOT NULL
);

-- Rôles au niveau instance (owner/admin/moderator/user).
CREATE TABLE instance_roles (
    user_id INTEGER PRIMARY KEY,
    role    TEXT    NOT NULL DEFAULT 'user'
);

-- Sessions / refresh tokens (hachés).
CREATE TABLE sessions (
    id           INTEGER PRIMARY KEY,
    user_id      INTEGER NOT NULL,
    refresh_hash TEXT    NOT NULL UNIQUE,
    device       TEXT,
    created_at   INTEGER NOT NULL,
    expires_at   INTEGER NOT NULL
);
CREATE INDEX idx_sessions_user ON sessions (user_id);

-- Guildes (les « serveurs » communautaires au sein de l'instance).
CREATE TABLE guilds (
    id         INTEGER PRIMARY KEY,
    name       TEXT    NOT NULL,
    owner_id   INTEGER NOT NULL,
    icon_id    TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE guild_members (
    guild_id  INTEGER NOT NULL,
    user_id   INTEGER NOT NULL,
    nick      TEXT,
    joined_at INTEGER NOT NULL,
    PRIMARY KEY (guild_id, user_id)
);

CREATE TABLE channels (
    id         INTEGER PRIMARY KEY,
    guild_id   INTEGER,
    type       INTEGER NOT NULL DEFAULT 0,
    name       TEXT    NOT NULL,
    topic      TEXT,
    position   INTEGER NOT NULL DEFAULT 0,
    parent_id  INTEGER,
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_channels_guild ON channels (guild_id);

CREATE TABLE messages (
    id         INTEGER PRIMARY KEY,
    channel_id INTEGER NOT NULL,
    author_id  INTEGER NOT NULL,
    content    TEXT    NOT NULL,
    type       INTEGER NOT NULL DEFAULT 0,
    nonce      TEXT,
    created_at INTEGER NOT NULL,
    edited_at  INTEGER
);
CREATE INDEX idx_messages_channel ON messages (channel_id, id DESC);
