-- Paramètres de salon vocal et textuel (fidélité Discord, audio poussé plus loin).
-- Vocal : débit audio (bps — on autorise jusqu'à 512 kbps, bien au-delà des 96 kbps de Discord),
-- limite d'utilisateurs (0 = illimité), région imposée (NULL = automatique), qualité vidéo
-- (1 = auto, 2 = 720p). Texte : durée de masquage des fils inactifs (minutes).
ALTER TABLE channels ADD COLUMN bitrate INTEGER NOT NULL DEFAULT 64000;
ALTER TABLE channels ADD COLUMN user_limit INTEGER NOT NULL DEFAULT 0;
ALTER TABLE channels ADD COLUMN rtc_region TEXT;
ALTER TABLE channels ADD COLUMN video_quality_mode INTEGER NOT NULL DEFAULT 1;
ALTER TABLE channels ADD COLUMN default_auto_archive INTEGER NOT NULL DEFAULT 4320;
