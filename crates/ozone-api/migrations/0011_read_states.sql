-- S8 : marqueurs de lecture, réglages de notification, boîte de mentions.
-- Cf. docs/features/13-notifications.md. (Notifications natives OS / push = côté client/workers.)

-- État de lecture par (utilisateur, salon) : dernier message lu + compteur de mentions non lues.
CREATE TABLE read_states (
    user_id       INTEGER NOT NULL,
    channel_id    INTEGER NOT NULL,
    last_read_id  INTEGER NOT NULL DEFAULT 0,
    mention_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, channel_id)
);

-- Réglages de notification par portée.
-- scope_type : 0 = guilde, 1 = salon. level : 0 = tous, 1 = @mentions, 2 = rien, 3 = hériter (salon).
-- muted_until : NULL = non mute ; valeur epoch ms (sentinelle lointaine = « jusqu'à réactivation »).
CREATE TABLE notification_settings (
    user_id     INTEGER NOT NULL,
    scope_type  INTEGER NOT NULL,
    scope_id    INTEGER NOT NULL,
    level       INTEGER NOT NULL DEFAULT 0,
    muted_until INTEGER,
    PRIMARY KEY (user_id, scope_type, scope_id)
);

-- Mentions reçues (source du compteur + de la boîte de réception « mentions »).
CREATE TABLE mentions (
    user_id    INTEGER NOT NULL,
    message_id INTEGER NOT NULL,
    channel_id INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, message_id)
);
CREATE INDEX idx_mentions_user ON mentions (user_id, message_id DESC);
