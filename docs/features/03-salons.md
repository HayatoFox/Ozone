# Fonctionnalités — Salons (tous types)

Réf. : [03-modele-de-donnees](../03-modele-de-donnees.md#channel) · [10-roles-permissions](10-roles-permissions.md).

## Types de salons (parité Discord)
| Type | Description | Implémenté |
|---|---|---|
| **Texte** (0) | conversations écrites | [ ] |
| **Vocal** (2) | voix/vidéo + chat texte intégré | [ ] |
| **Catégorie** (4) | regroupe/ordonne les salons, permissions héritées | [ ] |
| **Annonces** (5) | publiable/suivable par d'autres serveurs (crosspost) | [ ] |
| **Stage** (13) | conférences audience/intervenants | [ ] |
| **Forum** (15) | salon de fils (posts) avec tags & tri | [ ] |
| **Média** (16) | comme forum, orienté galerie média | [ ] |
| **Thread public** (11) | fil temporaire dans texte/forum | [ ] |
| **Thread privé** (12) | fil sur invitation | [ ] |
| **Thread d'annonce** (10) | fil sur salon d'annonces | [ ] |
| **Répertoire** (14) | hub (listing de serveurs) | [ ] |

## Gestion des salons
- [ ] Créer/renommer/supprimer/déplacer, **catégories** (drag & drop, collapse).
- [ ] **Sujet/description** (topic), **slowmode** (rate limit par utilisateur), **NSFW** (gate d'âge).
- [ ] **Permissions par salon** (overrides) et **synchronisation** avec la catégorie parente (voir [10](10-roles-permissions.md)).
- [ ] Position/ordre, salon par défaut, salon system, salon AFK.
- [ ] Notifications par salon (voir [13](13-notifications.md)).
- [ ] Liens directs vers un salon/message (deep links).

## Salons texte
- [ ] Historique paginé, **épingles** (cap 250), recherche dans le salon.
- [ ] **Indicateur de frappe**, séparateurs « nouveaux messages », marque-page de lecture.
- [ ] Webhooks entrants, salon suivi (annonces).

## Salons vocaux
- [ ] Rejoindre/quitter, voix + **vidéo** + **partage d'écran** (voir [05](05-vocal-video.md)/[06](06-partage-ecran.md)).
- [ ] **Chat texte intégré** au salon vocal.
- [ ] **Bitrate** configurable, **limite d'utilisateurs**, **région**, **qualité vidéo**.
- [ ] **Statut de salon vocal** (texte court décrivant l'activité).
- [ ] Liste des participants (mute/deaf, qui parle, vidéo/stream actifs).

## Salons d'annonces
- [ ] Publier un message → **crosspost** vers les serveurs qui **suivent** le salon.
- [ ] S'abonner/suivre un salon d'annonces depuis un autre serveur.

## Salons Forum & Média
- [ ] Créer des **posts** (= threads) avec titre, contenu initial, **tags** appliqués.
- [ ] **Tags disponibles** (gérés par modérateur), **require_tag**, **emoji de réaction par défaut**.
- [ ] **Tri** (activité récente / date de création), **mise en page** (liste / galerie pour média).
- [ ] Slowmode de création de posts, post épinglé.

## Fils de discussion (threads)
- [ ] Créer un **thread public** (depuis un message ou à vide) / **privé** (sur invitation).
- [ ] **Auto-archivage** (1 h / 24 h / 3 j / 7 j), verrouillage, invitabilité.
- [ ] Rejoindre/quitter, liste des fils actifs/archivés, membres du fil.
- [ ] Notifications de fils, compteurs (messages, membres).
- [ ] Slowmode par fil.

## Salons Stage
- [ ] Démarrer un stage (sujet), rôles **intervenant**/**audience**, **demande de parole**.
- [ ] Modérateurs invitent/retirent des intervenants, déplacent en audience.
- [ ] Indicateur « en direct », nombre d'auditeurs, lien d'événement associé.

## Definition of Done
- Un serveur expose une catégorie « Général » synchronisée contenant un salon texte (avec slowmode et épingles), un salon vocal (bitrate custom + chat intégré + partage d'écran), un salon d'annonces suivi par un autre serveur, un forum à tags, et un stage fonctionnel avec demande de parole.
