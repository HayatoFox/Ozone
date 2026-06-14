-- Stickers dans les messages : référence facultative vers un sticker de la guilde du salon.
ALTER TABLE messages ADD COLUMN sticker_id INTEGER;
