-- Chiffrement de bout en bout des messages privés (1:1).
-- La clé PUBLIQUE de chiffrement DM de chaque utilisateur (P-256 ECDH, SPKI base64). La clé privée
-- ne quitte JAMAIS le client → l'administrateur de l'instance (accès BDD/SSH) ne peut pas lire les MP.
ALTER TABLE users ADD COLUMN dm_public_key TEXT;
-- Texte chiffré (base64 « iv|ciphertext » AES-GCM). Quand présent, `content` reste vide et le
-- serveur ne voit qu'un blob opaque. Le client déchiffre via ECDH avec l'autre participant.
ALTER TABLE messages ADD COLUMN cipher TEXT;
