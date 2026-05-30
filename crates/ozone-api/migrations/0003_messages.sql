-- Messages avancés : réponses, épingles, réactions (cf. docs/features/04-messagerie.md).

ALTER TABLE messages ADD COLUMN reference_id INTEGER;
ALTER TABLE messages ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;

CREATE TABLE reactions (
    message_id INTEGER NOT NULL,
    emoji      TEXT    NOT NULL,
    user_id    INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (message_id, emoji, user_id)
);
CREATE INDEX idx_reactions_msg ON reactions (message_id);

CREATE INDEX idx_messages_pinned ON messages (channel_id, pinned);
