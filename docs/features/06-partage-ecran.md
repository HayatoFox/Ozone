# Fonctionnalités — Partage d'écran & Go Live

Réf. technique : [06-infrastructure-vocale](../06-infrastructure-vocale.md#5-vidéo--partage-décran).

## Démarrer un partage
- [ ] Partager : **écran entier**, **fenêtre d'application**, ou **onglet/zone** spécifique.
- [ ] Sélecteur de source avec aperçus (multi-écrans gérés).
- [ ] **Go Live** : streamer une application dans un salon vocal de serveur (spectateurs rejoignent).
- [ ] Partage en **MP / groupe MP** et en **salon vocal de serveur**.
- [ ] Inclure / exclure l'**audio de l'application** partagée (loopback), + micro.

## Qualité & performance
- [ ] Résolutions : 720p / 1080p / 1440p / **4K**, fréquences **15 / 30 / 60 fps**.
- [ ] Limites selon palier de boost / config admin (chez Ozone : configurable, pas vendu).
- [ ] **Simulcast / SVC** : plusieurs couches publiées, le SFU sert la meilleure adaptée à chaque spectateur.
- [ ] Codec **AV1/VP9** préféré (efficacité), fallback H.264 ; accélération matérielle (NVENC/QuickSync/VideoToolbox).
- [ ] Contrôle de débit/congestion (GCC/TWCC), keyframes à la demande, faible latence.
- [ ] Indicateur de performance (fps réel, débit, spectateurs, perte).

## Capture native par plateforme
- [ ] **Windows** : Desktop Duplication API / Windows.Graphics.Capture (capture GPU efficace).
- [ ] **macOS** : ScreenCaptureKit (autorisations système gérées).
- [ ] **Linux** : PipeWire (Wayland) + portails xdg-desktop-portal ; X11 fallback.
- [ ] Capture du curseur (option), exclusion de fenêtres (anti-leak), région personnalisée.

## Côté spectateur
- [ ] Rejoindre un stream, plein écran, **contrôle de la qualité reçue** (auto/720p/1080p/source).
- [ ] Réactions, chat pendant le visionnage, liste des spectateurs.
- [ ] Épingler le stream, basculer entre plusieurs streams du salon.
- [ ] **Multistream** : regarder plusieurs partages simultanément (selon capacités).

## Sécurité & confidentialité
- [ ] **E2EE (DAVE)** sur le flux de partage comme pour la vidéo.
- [ ] Avertissement avant de partager des fenêtres sensibles, masquage des notifications pendant le partage.
- [ ] Intégration **mode streamer** (masque infos perso, désactive notifs/sons) — voir [20](20-overlay-streamer.md).

## Definition of Done
- Un utilisateur lance un Go Live d'une app en 1080p60 avec audio d'application, 4 spectateurs rejoignent et choisissent leur qualité de réception, le tout chiffré E2EE et avec le mode streamer masquant ses infos perso.
