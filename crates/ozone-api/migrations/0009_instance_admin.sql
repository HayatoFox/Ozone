-- Administration de l'instance : invitations d'instance, suspension de comptes.
-- (instance_roles existe déjà — migration 0001.) Cf. docs/features/00-instances.md §6-7.

CREATE TABLE instance_invites (
    code       TEXT    PRIMARY KEY,
    created_by INTEGER NOT NULL,
    max_uses   INTEGER NOT NULL DEFAULT 0,
    uses       INTEGER NOT NULL DEFAULT 0,
    expires_at INTEGER,
    created_at INTEGER NOT NULL
);

-- Compte suspendu : connexion refusée jusqu'à réactivation.
ALTER TABLE users ADD COLUMN suspended INTEGER NOT NULL DEFAULT 0;
