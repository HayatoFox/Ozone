# Fonctionnalités — Notifications

Réf. : [05-gateway](../05-gateway-temps-reel.md) · [15-parametres-utilisateur](15-parametres-utilisateur.md).

## Niveaux de notification
- [ ] Par **serveur** : Tous les messages / **Seulement @mentions** / Rien.
- [ ] Par **catégorie** et par **salon** : override du réglage serveur (hérite par défaut).
- [ ] Suppression sélective : `@everyone`/`@here`, **mentions de rôle**, mises en avant (highlights), événements.
- [ ] **Override push mobile** distinct du desktop.

## Mise en sourdine (mute)
- [ ] Mute **serveur / catégorie / salon / fil / MP / groupe**.
- [ ] **Durée** : 15 min, 1 h, 3 h, 8 h, 24 h, jusqu'à réactivation.
- [ ] Un élément mute n'allume pas le badge non-lu (mais garde le compteur de mentions optionnel).

## Notifications bureau & système
- [ ] **Notifications natives OS** (Windows/macOS/Linux) avec aperçu, actions rapides (répondre, marquer lu).
- [ ] **Badge** d'icône (compte de mentions/non-lus), **flash de la barre des tâches**.
- [ ] **Sons** de notification granulaires (message, mention, appel entrant, join/leave vocal, soundboard, deafen/mute, PTT…), chacun activable/désactivable.
- [ ] **TTS** des notifications (option), aperçu du contenu masquable.
- [ ] Regroupement, anti-spam (ne pas notifier deux fois la même chose), respect du « Ne pas déranger ».

## Push (mobile)
- [ ] Push via APNs (iOS) / FCM (Android) déclenchées par les **workers**.
- [ ] Réglages indépendants, mute synchronisé entre appareils.
- [ ] Notifications d'appel (CallKit / ConnectionService).

## Marqueurs de lecture
- [ ] **Read states** par salon (`last_read_id`, compteur de mentions), synchronisés multi-appareils.
- [ ] Badges non-lus, séparateur « nouveaux messages », « marquer le serveur/salon comme lu ».
- [ ] **Mentions inbox** : vue récapitulative de toutes ses mentions récentes (sauter au message).

## Ne pas déranger & focus
- [ ] Statut **DND** supprime les notifications (sauf exceptions configurables).
- [ ] **Mode streamer** désactive notifications/sons pendant le partage (voir [20](20-overlay-streamer.md)).
- [ ] Suppression des notifications pendant un partage d'écran.

## Definition of Done
- Un utilisateur règle un serveur sur « @mentions seulement », mute une catégorie pour 8 h, désactive le son de join vocal, reçoit une notif native OS pour une mention directe (avec réponse rapide), et retrouve toutes ses mentions dans l'inbox dédiée.
