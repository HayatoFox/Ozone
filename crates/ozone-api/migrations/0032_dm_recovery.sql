-- Code de récupération E2EE (mot de passe oublié). Second « coffre » de la clé privée DM, emballé par
-- une KEK dérivée d'un CODE DE RÉCUPÉRATION aléatoire (montré une fois, conservé hors-ligne par
-- l'utilisateur). Permet de retrouver l'accès au compte ET aux MP sans que le serveur puisse lire.
ALTER TABLE users ADD COLUMN recovery_hash TEXT;     -- Argon2(recoveryAuthSecret), comme un mot de passe
ALTER TABLE users ADD COLUMN recovery_wrapped TEXT;  -- AES-GCM(recoveryKEK, clé privée JWK)
