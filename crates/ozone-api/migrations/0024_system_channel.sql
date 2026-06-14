-- Salon système de la guilde : reçoit les messages système (arrivées de membres, type 7).
-- NULL = désactivé. Pas de FK stricte (SQLite ALTER) : la validité est vérifiée à l'écriture
-- et le service tolère un salon supprimé (message simplement non émis).
ALTER TABLE guilds ADD COLUMN system_channel_id INTEGER;
