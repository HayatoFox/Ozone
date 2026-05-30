# Fonctionnalités — Overlay & mode streamer

Réf. : [05-vocal-video](05-vocal-video.md) · [06-partage-ecran](06-partage-ecran.md) · [15-parametres-utilisateur](15-parametres-utilisateur.md).

## Overlay (par-dessus les applications)
- [ ] **Overlay vocal** : affiche les participants qui parlent, mute/deafen, par-dessus une app en plein écran.
- [ ] **Notifications** in-overlay (messages, mentions), chat rapide.
- [ ] **Raccourci** d'activation/verrouillage, déplacement/redimensionnement des widgets, opacité, position, taille des avatars.
- [ ] Mode « verrouillé » pour interagir (clics) vs « passif » (clic-through).
- [ ] Activation par application (liste blanche), désactivation auto si non supporté.
- [ ] Implémentation native : injection/compositing par plateforme (DXGI/Vulkan layer sur Windows, surfaces transparentes ailleurs) — overlay **performant**, pas de WebView.

## Mode streamer
- [ ] **Activation auto** quand un logiciel de streaming (OBS, etc.) est détecté, ou manuelle.
- [ ] **Masquer les informations personnelles** : email, numéros, codes d'invitation, identifiants sensibles.
- [ ] **Désactiver les notifications** (et leurs aperçus) pendant le stream.
- [ ] **Désactiver les sons** (notifications) pendant le stream.
- [ ] Masquer les liens d'invitation et empêcher les fuites accidentelles.
- [ ] Réglages indépendants (on peut garder les sons mais cacher les infos, etc.).

## Definition of Done
- Pendant une partie en plein écran, l'utilisateur voit l'overlay vocal de son salon, reçoit ses mentions en surimpression, puis lance OBS : le mode streamer s'active automatiquement, masque son email/invitations et coupe les notifications.
