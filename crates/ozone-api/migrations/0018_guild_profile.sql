-- Profil de serveur enrichi : bannière (couleur de dégradé OU image), jeux joués, profil privé.
ALTER TABLE guilds ADD COLUMN banner_color INTEGER;
ALTER TABLE guilds ADD COLUMN banner_id TEXT;
ALTER TABLE guilds ADD COLUMN games TEXT;            -- tableau JSON de clés de jeux ["cs2", ...]
ALTER TABLE guilds ADD COLUMN private_profile INTEGER NOT NULL DEFAULT 0;
