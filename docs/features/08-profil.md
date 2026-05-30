# Fonctionnalités — Profil & personnalisation

Réf. : [03-modele-de-donnees](../03-modele-de-donnees.md#user-compte-global) · [16-apparence-themes](16-apparence-themes.md).

## Profil global
- [ ] **Avatar** (image ou **GIF animé**), **bannière**, **couleur d'accent**.
- [ ] **Nom affiché** + **pseudo unique**, **pronoms**, **bio** « À propos » (markdown limité).
- [ ] **Badges** (staff, early adopter, développeur d'app vérifiée, boosteur, etc.).
- [ ] Liens / connexions affichables (optionnel).
- [ ] Aperçu du profil tel que vu par les autres.

## Profils par serveur
- [ ] **Avatar**, **bannière**, **bio**, **pseudo**, **pronoms** spécifiques à chaque serveur.
- [ ] Couleur d'affichage = couleur du rôle coloré le plus haut.
- [ ] Sélecteur « profil global vs profil de ce serveur » dans l'éditeur.

## Cosmétiques (débloqués sans paiement chez Ozone)
- [ ] **Décorations d'avatar** (cadre animé autour de l'avatar).
- [ ] **Nameplates** (bandeau décoratif derrière le nom dans les listes).
- [ ] **Effets de profil** (animation sur la carte de profil).
- [ ] **Thème de profil** (couleurs personnalisées primaire/secondaire de la carte).

## Statut & présence
- [ ] **Statut** : En ligne / Inactif / Ne pas déranger / Invisible.
- [ ] **Statut personnalisé** : emoji + texte + **expiration** (30 min, 1 h, aujourd'hui, custom, jamais).
- [ ] **Presence riche** : « Joue à… », « Écoute… », « Regarde… », « En vocal dans… » (l'API rich presence reste dispo pour les apps ; pas d'auto-détection de jeux propriétaire — voir [périmètre](../00-vision-et-perimetre.md)).
- [ ] Présence multi-appareils (desktop/mobile/web), idle auto.

## Carte de profil (popout)
- [ ] Affiche avatar/bannière/bio/pronoms/badges, **rôles** (sur un serveur), **membre depuis**, **serveurs & amis en commun**.
- [ ] Actions rapides : message, appel, ajouter en ami, bloquer, voir le profil complet, **note** privée, gérer les rôles (si permission), expulser/bannir/timeout (si permission).
- [ ] **Statut d'activité** et bouton « rejoindre » si applicable.

## Definition of Done
- Un utilisateur configure un avatar animé + bannière + bio + pronoms globaux, puis un profil distinct pour un serveur précis, applique une décoration d'avatar et un nameplate, et définit un statut personnalisé avec expiration.
