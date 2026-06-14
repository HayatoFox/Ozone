-- Cycle de vie des fils (threads, type 11/12) : archivage + verrouillage.
-- archived : masqué de la liste active (réactivé en écrivant, sauf si verrouillé).
-- locked   : seuls les modérateurs (MANAGE_THREADS via MANAGE_CHANNELS) peuvent écrire/désarchiver.
ALTER TABLE channels ADD COLUMN archived INTEGER NOT NULL DEFAULT 0;
ALTER TABLE channels ADD COLUMN locked INTEGER NOT NULL DEFAULT 0;
ALTER TABLE channels ADD COLUMN archived_at INTEGER;

-- Abonnés d'un fil (pour notifications + onglet « Mes fils »).
CREATE TABLE thread_members (
    channel_id INTEGER NOT NULL,
    user_id    INTEGER NOT NULL,
    joined_at  INTEGER NOT NULL,
    PRIMARY KEY (channel_id, user_id)
);
