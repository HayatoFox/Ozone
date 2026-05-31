-- S20 : annuaire de découverte des guildes publiques. Cf. docs/features/19-decouverte-onboarding.md.

-- Description publique + opt-in à l'annuaire de découverte.
ALTER TABLE guilds ADD COLUMN description  TEXT;
ALTER TABLE guilds ADD COLUMN discoverable INTEGER NOT NULL DEFAULT 0;
