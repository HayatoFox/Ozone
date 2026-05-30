-- Salons avancés : NSFW et slowmode (cf. docs/features/03-salons.md).

ALTER TABLE channels ADD COLUMN nsfw INTEGER NOT NULL DEFAULT 0;
ALTER TABLE channels ADD COLUMN rate_limit_per_user INTEGER NOT NULL DEFAULT 0;
