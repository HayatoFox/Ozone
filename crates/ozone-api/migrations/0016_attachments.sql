-- S22 : pièces jointes (fichiers/images). Stockage fichier local (un fichier par id).
-- Cf. docs/features/04-messagerie.md. Le passage à un stockage objet (S3/MinIO) = mode full-stack.

CREATE TABLE attachments (
    id           INTEGER PRIMARY KEY,
    channel_id   INTEGER NOT NULL,
    uploader_id  INTEGER NOT NULL,
    message_id   INTEGER,                 -- NULL = téléversée mais pas encore attachée à un message
    filename     TEXT    NOT NULL,
    content_type TEXT    NOT NULL,
    size         INTEGER NOT NULL,
    created_at   INTEGER NOT NULL
);
CREATE INDEX idx_attachments_msg ON attachments (message_id);
