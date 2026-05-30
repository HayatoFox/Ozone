# Fonctionnalités — Paramètres utilisateur

Vue d'ensemble de **tous** les panneaux de réglages (chaque sous-section renvoie au détail concerné).

## Instances (niveau client)
→ [00-instances](00-instances.md) : ajouter/oublier une instance, **switcher** entre instances, gérer la session **par instance**, vérifier l'identité d'instance (empreinte). Réglages **non synchronisés** entre instances (chaque instance est isolée).

## Mon compte
> Réglages **propres à l'instance active** (chaque instance a son propre compte).

→ [01-comptes-authentification](01-comptes-authentification.md) : pseudo, email, tél, mot de passe, **2FA**, suppression.

## Profils
→ [08-profil](08-profil.md) : profil global + **profils par serveur**, avatar/bannière/bio/pronoms, cosmétiques.

## Confidentialité & sécurité
- [ ] Qui peut m'envoyer un **MP**, qui peut m'**ajouter en ami**.
- [ ] **Filtrage des MP** (médias explicites/liens), filtres de contenu.
- [ ] Données & vie privée : utilisation des données, **export RGPD**, suppression.
- [ ] Bloquer les messages des non-amis dans les serveurs (option).

## Applications autorisées & connexions
- [ ] Liste des **apps OAuth2** autorisées, scopes, révocation.
- [ ] Connexions de comptes externes (optionnel/désactivable).

## Appareils & sessions
- [ ] Liste des sessions actives (appareil, lieu, dernière activité), **déconnexion à distance**.

## Voix & Vidéo
→ [05-vocal-video](05-vocal-video.md#paramètres-vocaux-avancés-paramètres--voix--vidéo) : périphériques, mode d'entrée, PTT, suppression de bruit, AEC/AGC, codec, QoS, atténuation.

## Apparence
→ [16-apparence-themes](16-apparence-themes.md) : thèmes (Sombre/Clair/Minuit/Sync), densité, taille de police, zoom, couleurs d'accent.

## Accessibilité
→ [16-apparence-themes](16-apparence-themes.md#accessibilité) : mouvement réduit, autoplay GIF/stickers, couleurs de rôle, saturation, daltonisme, TTS, navigation clavier, taille du chat.

## Notifications
→ [13-notifications](13-notifications.md) : activer le bureau, badge, flash, sons granulaires, TTS, push.

## Keybinds (raccourcis)
- [ ] Raccourcis **personnalisables** pour : push-to-talk, mute, deafen, naviguer salons/serveurs, marquer comme lu, overlay, toggle caméra/partage, soundboard, etc.
- [ ] Détection de conflits, profils de raccourcis, raccourcis globaux (hors focus app).

## Langue
- [ ] Sélection de la **langue** de l'interface (i18n complète), formats date/heure/nombre localisés.

## Mode streamer
→ [20-overlay-streamer](20-overlay-streamer.md) : activation auto, masquage d'infos perso, désactivation notifs/sons.

## Overlay (en jeu / bureau)
→ [20-overlay-streamer](20-overlay-streamer.md) : overlay vocal, notifications, raccourci, opacité, position.

## Avancé
- [ ] **Mode développeur** (copier les IDs), **accélération matérielle**, logs de debug, données expérimentales.
- [ ] Préférences réseau (proxy), cache (vider), dossier de téléchargement.
- [ ] **Mises à jour** (canal stable/beta), redémarrage pour appliquer.

## Synchronisation
- [ ] Réglages **synchronisés** entre appareils (stockés serveur, `user_settings` JSONB versionné), avec résolution de conflits.
- [ ] Réglages **locaux par appareil** (périphériques audio, accélération matérielle) non synchronisés.

## Definition of Done
- Un utilisateur parcourt tous les panneaux, personnalise un raccourci push-to-talk, change la langue, active le mode développeur, et retrouve ses réglages de thème/notifications synchronisés sur un second appareil.
