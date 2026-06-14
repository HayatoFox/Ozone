-- Réglages de guilde additionnels.
-- default_message_notifications : 0 = tous les messages, 1 = mentions seulement (fallback du
--   niveau « hériter » d'un membre qui n'a pas réglé son propre niveau).
-- afk_channel_id / afk_timeout : salon vocal AFK + délai d'inactivité (secondes).
-- vanity_code : code d'invitation permanent et personnalisé (référencé dans le profil de serveur).
ALTER TABLE guilds ADD COLUMN default_message_notifications INTEGER NOT NULL DEFAULT 0;
ALTER TABLE guilds ADD COLUMN afk_channel_id INTEGER;
ALTER TABLE guilds ADD COLUMN afk_timeout INTEGER NOT NULL DEFAULT 300;
ALTER TABLE guilds ADD COLUMN vanity_code TEXT;
