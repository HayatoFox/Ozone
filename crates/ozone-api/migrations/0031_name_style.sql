-- Style de pseudonyme (façon Discord Nitro) : police + effet + couleur(s), appliqué PARTOUT où le nom
-- de l'utilisateur s'affiche. Stocké en JSON (`{"font":u8,"effect":"...","color":u32,"color2":u32}`)
-- pour voyager en un seul champ sur l'objet User (auteur de message, membre, destinataire de MP, profil).
ALTER TABLE users ADD COLUMN name_style TEXT;
