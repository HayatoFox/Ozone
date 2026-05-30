-- Rôles & permissions, surcharges de salon, invitations (cf. docs/features/10-roles-permissions.md).

CREATE TABLE roles (
    id          INTEGER PRIMARY KEY,
    guild_id    INTEGER NOT NULL,
    name        TEXT    NOT NULL,
    color       INTEGER NOT NULL DEFAULT 0,
    hoist       INTEGER NOT NULL DEFAULT 0,
    position    INTEGER NOT NULL DEFAULT 0,
    permissions INTEGER NOT NULL DEFAULT 0,   -- bitfield u64 (motif binaire stocké en i64)
    mentionable INTEGER NOT NULL DEFAULT 0,
    managed     INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL
);
CREATE INDEX idx_roles_guild ON roles (guild_id);

-- Rôles attribués à un membre (le rôle @everyone, id = guild_id, est implicite).
CREATE TABLE member_roles (
    guild_id INTEGER NOT NULL,
    user_id  INTEGER NOT NULL,
    role_id  INTEGER NOT NULL,
    PRIMARY KEY (guild_id, user_id, role_id)
);

-- Surcharges de permission par salon (target_type : 0 = rôle, 1 = membre).
CREATE TABLE channel_overwrites (
    channel_id  INTEGER NOT NULL,
    target_id   INTEGER NOT NULL,
    target_type INTEGER NOT NULL,
    allow       INTEGER NOT NULL DEFAULT 0,
    deny        INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (channel_id, target_id)
);

-- Invitations de guilde.
CREATE TABLE invites (
    code       TEXT    PRIMARY KEY,
    guild_id   INTEGER NOT NULL,
    channel_id INTEGER,
    inviter_id INTEGER NOT NULL,
    uses       INTEGER NOT NULL DEFAULT 0,
    max_uses   INTEGER NOT NULL DEFAULT 0,
    max_age    INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    expires_at INTEGER
);
CREATE INDEX idx_invites_guild ON invites (guild_id);
