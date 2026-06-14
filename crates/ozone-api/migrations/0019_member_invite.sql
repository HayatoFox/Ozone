-- Méthode d'adhésion : code d'invitation utilisé par le membre pour rejoindre (si connu).
ALTER TABLE guild_members ADD COLUMN invite_code TEXT;
