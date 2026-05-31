-- S25 : suppression de compte (anonymisation). On conserve la ligne `users` pour que les
-- messages publiés restent attribués à un « utilisateur supprimé » (la jointure messages↔users
-- reste valide), mais toutes les données personnelles sont effacées. Cf. docs/features/01.

ALTER TABLE users ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0;
