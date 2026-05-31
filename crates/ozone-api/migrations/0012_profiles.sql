-- S10 : profils enrichis + réglages client. Cf. docs/features/08-profil.md, 15-parametres-utilisateur.md.

-- Champs de profil public.
ALTER TABLE users ADD COLUMN bio          TEXT;
ALTER TABLE users ADD COLUMN pronouns     TEXT;
ALTER TABLE users ADD COLUMN banner_id    TEXT;
ALTER TABLE users ADD COLUMN accent_color INTEGER;

-- Réglages client (privés) : blob JSON géré par le client (thème, locale, affichage…).
CREATE TABLE user_settings (
    user_id INTEGER PRIMARY KEY,
    data    TEXT NOT NULL DEFAULT '{}'
);
