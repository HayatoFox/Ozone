-- S21 : sondages (un sondage est porté par un message). Cf. docs/features/04-messagerie.md.

CREATE TABLE polls (
    message_id  INTEGER PRIMARY KEY,
    channel_id  INTEGER NOT NULL,
    question    TEXT    NOT NULL,
    multiselect INTEGER NOT NULL DEFAULT 0,
    expires_at  INTEGER,                 -- NULL = sans expiration
    created_at  INTEGER NOT NULL
);

CREATE TABLE poll_answers (
    message_id INTEGER NOT NULL,
    answer_id  INTEGER NOT NULL,         -- 1..N
    text       TEXT    NOT NULL,
    PRIMARY KEY (message_id, answer_id)
);

CREATE TABLE poll_votes (
    message_id INTEGER NOT NULL,
    answer_id  INTEGER NOT NULL,
    user_id    INTEGER NOT NULL,
    PRIMARY KEY (message_id, answer_id, user_id)
);
CREATE INDEX idx_poll_votes_msg ON poll_votes (message_id);
