-- Messages privés (1:1) et groupes de MP (cf. docs/features/07-messages-prives.md).
-- Les salons de MP réutilisent la table `channels` avec guild_id NULL, type 1 (DM) ou 3 (groupe).

ALTER TABLE channels ADD COLUMN owner_id INTEGER;  -- propriétaire d'un groupe de MP

CREATE TABLE dm_recipients (
    channel_id INTEGER NOT NULL,
    user_id    INTEGER NOT NULL,
    PRIMARY KEY (channel_id, user_id)
);
CREATE INDEX idx_dm_recipients_user ON dm_recipients (user_id);
