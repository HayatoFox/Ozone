# Fonctionnalités — Serveurs (guildes)

Réf. : [03-modele-de-donnees](../03-modele-de-donnees.md#guild-serveur) · [10-roles-permissions](10-roles-permissions.md).

## Création & gestion de base
- [ ] **Créer un serveur** : depuis zéro ou via **modèle** (template thématique : gaming, communauté, école, amis…).
- [ ] Nom, **icône** (image/GIF si « boosté »), bannière, splash d'invitation.
- [ ] Rejoindre via **invitation** ou **découverte**.
- [ ] **Quitter** / **supprimer** (propriétaire) / **transférer la propriété**.
- [ ] Réorganisation drag & drop des salons et catégories (positions).
- [ ] **Limite de membres** très élevée (jusqu'à 25 M, comme Discord).

## Invitations
- [ ] Générer un lien d'invitation avec : **expiration** (30 min → jamais), **nombre d'usages max**, **membre temporaire** (kické à la déconnexion), salon de destination.
- [ ] **Lien vanity** personnalisé (`ozone.gg/macommu`) pour serveurs éligibles.
- [ ] Liste/révocation des invitations actives (qui a invité, usages).
- [ ] Aperçu d'invitation (nom, icône, nb de membres en ligne/total) avant de rejoindre.
- [ ] Invitations vers un **événement** ou un **stream** (Go Live) spécifiques.

## Paramètres du serveur (Server Settings)
- [ ] **Aperçu** : nom, icône, bannière, salon système (messages de bienvenue/boost), salon AFK + timeout, langue.
- [ ] **Rôles** (voir [10](10-roles-permissions.md)).
- [ ] **Emojis / Stickers / Soundboard** (voir [12](12-expressions.md)).
- [ ] **Modération** : niveau de vérification, filtre de contenu explicite, **AutoMod** (voir [11](11-moderation-securite.md)).
- [ ] **Audit log** (voir [11](11-moderation-securite.md)).
- [ ] **Bans** (liste, recherche, déban, bulk-ban).
- [ ] **Intégrations** : bots, webhooks, salons suivis.
- [ ] **Widget** du serveur (embed externe, lien d'invitation widget).
- [ ] **Modèles de serveur** : créer un template depuis le serveur, le synchroniser, le partager.
- [ ] **URL vanity**, **découverte**, **écran de bienvenue**, **onboarding** (voir [19](19-decouverte-onboarding.md)).
- [ ] **Server Tag (clan)** : tag court affiché à côté des pseudos des membres, badge.
- [ ] **Statistiques / insights** (activité, rétention, croissance) pour serveurs communautaires.
- [ ] **Salons d'alertes de sécurité**, **community updates channel**.

## Serveurs communautaires
- [ ] Activer le mode **Communauté** : exige un **salon de règles** et un **salon de mises à jour communautaires**, vérification email obligatoire des membres.
- [ ] Déverrouille : découverte, écran de bienvenue, onboarding, insights, salons d'annonces, statistiques.

## Boosts (mécanique de paliers, sans facturation réelle)
- [ ] Paliers de boost (0–3) débloquant des **perks** : qualité audio supérieure, plus d'emojis/stickers, upload plus gros, icône animée, splash, **bannière de serveur**, vanity URL.
- [ ] Affichage des boosters, badge de boost, messages système de boost.
- [ ] Chez Ozone : les perks sont **configurables par l'admin** (pas vendus) — voir [périmètre](../00-vision-et-perimetre.md#5-périmètre--ce-quon-exclut-le-superflu).

## Profil par serveur
- [ ] **Pseudo**, **avatar**, **bannière**, **bio**, **pronoms** spécifiques au serveur.
- [ ] Couleur d'affichage dérivée du rôle le plus haut « hoisté » coloré.

## Definition of Done
- Un propriétaire crée un serveur depuis un template, configure catégories/salons/rôles, active le mode Communauté avec onboarding et écran de bienvenue, génère des invitations à durée limitée, définit un Server Tag, et consulte l'audit log de toutes ces actions.
