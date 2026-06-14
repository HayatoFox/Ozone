-- Style de couleur de rôle : couleur secondaire (dégradé/vague) + type de style.
-- color_style ∈ { 'solid', 'gradient', 'neon', 'wave' } (validé côté serveur).
ALTER TABLE roles ADD COLUMN secondary_color INTEGER;
ALTER TABLE roles ADD COLUMN color_style TEXT NOT NULL DEFAULT 'solid';
