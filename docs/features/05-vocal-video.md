# Fonctionnalités — Vocal & vidéo

Réf. technique : [06-infrastructure-vocale](../06-infrastructure-vocale.md).

## Salon vocal — base
- [ ] Rejoindre/quitter, indicateur « connecté à », déplacement entre salons.
- [ ] Liste des participants : qui parle (anneau vert), mute/deaf, vidéo/stream actifs, plateforme.
- [ ] **Chat texte intégré** au salon vocal.
- [ ] Sonnerie d'entrée/sortie (sons configurables), notification de connexion d'un ami.

## Micro & audio
- [ ] **Modes d'entrée** : détection d'activité vocale (**VAD**) avec sensibilité réglable, ou **push-to-talk** (keybind + délai de relâche).
- [ ] **Mute / deafen** (soi-même), **mute/deafen serveur** (modération).
- [ ] **Suppression de bruit type Krisp** (ML sur l'appareil, aucune donnée envoyée).
- [ ] **Annulation d'écho** (AEC), **gain automatique** (AGC), suppression de bruit standard.
- [ ] **Volume par utilisateur** (atténuation individuelle), **mute individuel** local.
- [ ] **Priorité au micro** (priority speaker : atténue les autres).
- [ ] **Atténuation** : baisse auto du volume des autres apps quand quelqu'un parle.
- [ ] Sélection périphériques **entrée/sortie**, volumes, test du micro, indicateur de niveau.
- [ ] Bitrate adaptatif, FEC/DTX, jitter buffer, indicateur de qualité réseau (ping, perte, gigue).

## Vidéo (caméra)
- [ ] Activer/couper la caméra, **sélection du périphérique**, aperçu local.
- [ ] Jusqu'à **25 flux vidéo** simultanés, grille adaptative, épinglage d'un participant (focus).
- [ ] **Arrière-plan flou / virtuel** (option), miroir, qualité auto/HD.
- [ ] Plein écran, vue « film » vs grille, masquage des participants sans caméra.

## Partage d'écran
→ Voir [06-partage-ecran](06-partage-ecran.md) (sélection de source, qualité, Go Live, spectateurs).

## Salons Stage
- [ ] Démarrer/terminer un stage avec **sujet**.
- [ ] Rôles **intervenant** / **modérateur** / **audience**.
- [ ] **Demande de parole** (lever la main), invitation à intervenir, retour en audience.
- [ ] Indicateur « en direct », nombre d'auditeurs, association à un **événement programmé**.
- [ ] Enregistrement/clips (option, hors périmètre MVP).

## Effets & ambiance
- [ ] **Soundboard** : jouer des sons courts dans le vocal (sons serveur + custom), volume, cooldown, permission.
- [ ] **Effets vocaux** / voice channel effects (réactions animées dans le vocal).
- [ ] Statut de salon vocal (texte d'activité).

## Contrôles de modération vocale
- [ ] Mute/deafen serveur, **déplacer** un membre vers un autre salon, **déconnecter** du vocal.
- [ ] Limite d'utilisateurs, salon vocal privé (overrides `CONNECT`).
- [ ] `MOVE_MEMBERS`, `MUTE_MEMBERS`, `DEAFEN_MEMBERS`, `PRIORITY_SPEAKER` (voir [10](10-roles-permissions.md)).

## Chiffrement
- [ ] **E2EE par défaut (DAVE/MLS)** sur voix/vidéo/partage — voir [06-infra](../06-infrastructure-vocale.md#7-chiffrement-de-bout-en-bout--davemls).
- [ ] Indicateur de chiffrement + **codes de vérification** d'identité.

## Paramètres vocaux avancés (Paramètres → Voix & Vidéo)
- [ ] Périphériques, volumes, mode d'entrée, sensibilité, push-to-talk + raccourci.
- [ ] Suppression de bruit (off / standard / Krisp), AEC, AGC.
- [ ] QoS haute priorité des paquets, atténuation, codec vidéo (OpenH264/AV1), accélération matérielle.
- [ ] Réinitialiser les paramètres vocaux, overlay de debug réseau.

## Definition of Done
- Cinq utilisateurs rejoignent un vocal chiffré E2EE, parlent en push-to-talk avec suppression de bruit Krisp, ajustent le volume par personne, activent caméra et soundboard, un modérateur en mute/déplace un, le tout < 150 ms de latence.
