-- Persistance multi-appareils du chiffrement E2EE des MP (Option A : escrow chiffré + auth ZK).
-- La clé privée DM, emballée (AES-GCM) par une KEK dérivée du mot de passe CÔTÉ CLIENT (le serveur
-- ne voit jamais le mot de passe ni la KEK). Permet de récupérer ses MP sur tout appareil après login.
ALTER TABLE users ADD COLUMN dm_priv_wrapped TEXT;
-- Schéma d'authentification : 1 = legacy (le serveur recevait le mot de passe brut), 2 = zero-knowledge
-- (le client envoie un `authSecret` dérivé ; le serveur n'en stocke qu'un hash Argon2). Migration v1→v2
-- au prochain login (cf. endpoint upgrade-encryption).
ALTER TABLE users ADD COLUMN pw_scheme INTEGER NOT NULL DEFAULT 1;
-- Sel KDF par utilisateur (aléatoire, base64, PUBLIC). Restitué avant login (endpoint prelogin) pour que
-- le client dérive `authSecret`/KEK quel que soit l'identifiant tapé (e-mail OU pseudo). NULL en v1.
ALTER TABLE users ADD COLUMN kdf_salt TEXT;
