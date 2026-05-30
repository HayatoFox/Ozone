-- Relations (amis / blocages) et notes privées (cf. docs/features/09-amis-relations.md).

CREATE TABLE relationships (
    user_id   INTEGER NOT NULL,
    target_id INTEGER NOT NULL,
    type      TEXT    NOT NULL,   -- friend | incoming | outgoing | blocked
    since     INTEGER NOT NULL,
    PRIMARY KEY (user_id, target_id)
);
CREATE INDEX idx_rel_user ON relationships (user_id);

CREATE TABLE user_notes (
    user_id   INTEGER NOT NULL,
    target_id INTEGER NOT NULL,
    note      TEXT    NOT NULL,
    PRIMARY KEY (user_id, target_id)
);
