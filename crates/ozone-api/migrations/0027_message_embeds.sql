-- Embeds riches sur les messages (JSON Vec<MessageEmbed>, NULL = aucun).
-- Surtout utiles pour les webhooks (CI, monitoring) et les messages avec EMBED_LINKS.
ALTER TABLE messages ADD COLUMN embeds TEXT;
