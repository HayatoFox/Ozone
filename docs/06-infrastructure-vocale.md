# 06 — Infrastructure vocale (voix, vidéo, partage d'écran)

Le pilier « temps réel média ». Architecture **SFU** (Selective Forwarding Unit) : le serveur **relaie** les flux sans les mixer ni les transcoder côté audio → faible latence, faible CPU, et compatibilité **E2EE**.

## 1. Pourquoi un SFU (et pas un MCU ni du P2P)

| Modèle | Principe | Verdict |
|---|---|---|
| **P2P mesh** | chaque pair envoie à chaque pair | ❌ explose au-delà de 3–4 participants (bande passante montante) |
| **MCU** | serveur mixe tout en un flux | ❌ CPU serveur énorme, casse l'E2EE, latence |
| **SFU** | serveur reçoit 1 flux montant/pair, le redistribue | ✅ **choix Discord & Ozone** : scalable, faible latence, E2EE-compatible |

## 2. Établissement d'un appel (signaling)

Le signaling initial passe par la **Gateway** principale, puis bascule sur une connexion vocale dédiée :

```
1. Client ──Gateway: VOICE_STATE_UPDATE { guild_id, channel_id, self_mute, self_deaf, self_video } ──►
2. Serveur ──► VOICE_STATE_UPDATE { session_id, … }  (état partagé aux autres)
            └─► VOICE_SERVER_UPDATE { token, endpoint (nœud SFU), guild_id }
3. Client ──WSS──► SFU (endpoint) : Voice-IDENTIFY { server_id, user_id, session_id, token }
4. SFU ──► Voice-READY { ssrc, ip, port, modes de chiffrement supportés }
5. Client ──UDP "IP discovery"──► découvre son IP/port publics (NAT)
6. Client ──► Voice-SELECT_PROTOCOL { protocol:"udp", address, port, mode:"aead_aes256_gcm_rtpsize" }
7. SFU ──► Voice-SESSION_DESCRIPTION { mode, secret_key (clés SRTP) }
8. Média RTP/SRTP commence (UDP). SPEAKING signale qui parle.
9. (Si E2EE) négociation DAVE/MLS entre participants → surchiffrement de bout en bout.
```

### Opcodes du canal vocal (WS dédié)
| op | Nom | Rôle |
|---|---|---|
| 0 | IDENTIFY | authentifier la session vocale |
| 1 | SELECT_PROTOCOL | choisir transport + mode de chiffrement |
| 2 | READY | ssrc, ip, port, modes |
| 3 | HEARTBEAT | battement |
| 4 | SESSION_DESCRIPTION | clés SRTP, mode |
| 5 | SPEAKING | indicateur de parole (+ ssrc, priorité) |
| 6 | HEARTBEAT_ACK | |
| 7 | RESUME | reprise vocale |
| 8 | HELLO | intervalle de heartbeat |
| 9 | RESUMED | |
| 11 | CLIENTS_CONNECT | participants présents |
| 13 | CLIENT_DISCONNECT | départ d'un participant |
| 12 | VIDEO / streams | description des flux vidéo (ssrc, résolution, simulcast) |
| 21–31 | **DAVE** | transition de protocole E2EE, échanges MLS (commits, welcome, proposals) |

## 3. Transport média

- **UDP** + **RTP** (paquets média) + **RTCP** (qualité, NACK, REMB/TWCC pour congestion).
- **IP discovery** pour traverser le NAT ; **ICE** complet côté WebRTC (STUN/TURN) pour la version compatible navigateur.
- **SRTP** (chiffrement *hop* client↔SFU) : modes `aead_aes256_gcm_rtpsize` (préféré) ou `aead_xchacha20_poly1305_rtpsize`.
- Jitter buffer adaptatif côté réception ; PLC (concealment) sur perte de paquets.

## 4. Audio

| Aspect | Choix |
|---|---|
| Codec | **Opus** 48 kHz, mono/stéréo, **FEC** (correction d'erreur), **DTX** (silence → 0 trafic) |
| Trame | 20 ms (configurable 10/40/60) |
| Bitrate | adaptatif 8–510 kbps selon réseau & qualité du salon (bitrate de salon configurable) |
| Traitement capture | **AEC** (annulation d'écho), **NS** (suppression de bruit), **AGC** (gain auto) via `webrtc-audio-processing` |
| Suppression de bruit avancée | modèle **type Krisp** (ML, sur l'appareil, aucune donnée envoyée) — voir [05-vocal-video](features/05-vocal-video.md) |
| Détection d'activité | **VAD** (voice activity) ou **push-to-talk** (keybind) |
| Volume par utilisateur | atténuation côté réception, par participant |
| Priorité au micro | abaisse le volume des autres quand un « priority speaker » parle |
| Atténuation | baisse auto du son des apps quand quelqu'un parle (option) |

L'audio n'est **jamais mixé** par le SFU : chaque client reçoit N flux Opus et mixe localement → permet le volume par personne et l'E2EE.

## 5. Vidéo & partage d'écran

| Aspect | Choix |
|---|---|
| Codecs | **AV1 (SVC)** préféré → **VP9** → **VP8** → **H.264** (fallback compat) |
| Caméra | jusqu'à 25 flux vidéo simultanés par salon (grille) |
| Partage d'écran / **Go Live** | capture écran/fenêtre/application ; jusqu'à **4K @ 60 fps** (selon perms/qualité) |
| Simulcast / **SVC** | l'émetteur publie plusieurs couches (résolution/débit) ; le SFU **sélectionne** la couche à relayer selon le réseau/abonnement du récepteur |
| Capture (natif) | Windows: **Desktop Duplication API** ; macOS: **ScreenCaptureKit** ; Linux: **PipeWire** (Wayland) / X11 |
| Audio d'application | capture de l'audio de l'app partagée (loopback) |
| Congestion | **GCC** (Google Congestion Control) / TWCC : ajuste débit/couche pour éviter la gigue |
| Régulation | NACK + RTX (retransmission) + keyframes à la demande (PLI/FIR) |

Le **partage d'écran** est traité dans [06-partage-ecran](features/06-partage-ecran.md) (UX, sélection de source, qualité, spectateurs).

## 6. Salons Stage (conférences/audience)

- Rôles : **modérateurs/intervenants** (peuvent parler) vs **audience** (écoute).
- L'audience demande la parole (`REQUEST_TO_SPEAK`) ; un modérateur invite à intervenir (déplace en « speaker »).
- Sujet du stage, audience potentiellement large (le SFU ne relaie l'upload que des intervenants → scalable à des milliers d'auditeurs).
- Détails UX : [05-vocal-video](features/05-vocal-video.md).

## 7. Chiffrement de bout en bout — **DAVE / MLS**

Discord chiffre désormais la voix/vidéo **de bout en bout** (les serveurs relaient sans déchiffrer le contenu média). Ozone reprend ce modèle.

- **Deux couches** :
  1. **Transport** : SRTP client↔SFU (protège le saut réseau).
  2. **E2EE** : **DAVE** — surchiffrement du payload média (audio/vidéo) avec une clé de groupe que le SFU **ne possède pas**.
- **Échange de clés** : **MLS** (Messaging Layer Security, RFC 9420) via `openmls`. MLS gère un groupe dynamique : ajout/retrait de participants avec **rotation de clé** efficace (forward secrecy + post-compromise security), même à grande échelle.
  - Chaque participant a un *KeyPackage* ; rejoindre = *Add proposal* + *Commit* ; quitter = *Remove* → nouvelle *epoch* de clé.
  - Le SFU relaie les messages MLS (op 21–31) mais ne peut pas déchiffrer le média.
- **Frame encryption** : chaque trame média est chiffrée (type SFrame/AES-GCM) avec la clé de l'epoch courante ; les en-têtes RTP nécessaires au routage restent en clair pour le SFU.
- **Vérification d'identité** : codes de vérification (empreintes) affichables pour confirmer l'absence de MITM, comme dans les apps E2EE.
- **Repli** : si un participant ne supporte pas DAVE, le salon peut basculer en transport-only (signalé clairement dans l'UI).

## 8. Sélection de région & qualité

- `GET /voice/regions` ; choix automatique par latence (ping vers les nœuds SFU) ou manuel par salon.
- Indicateurs réseau temps réel (ping, perte de paquets, gigue) exposés dans l'UI (debug overlay vocal).
- Qualité vidéo configurable par salon (`video_quality_mode`: auto/full) et bitrate vocal par salon.

## 9. Robustesse

- **Resume vocal** (op 7) après micro-coupure sans renégocier toutes les clés.
- Reconnexion transparente sur changement de réseau (Wi-Fi ↔ data) avec re-ICE.
- Bascule de nœud SFU si le nœud tombe (le serveur réémet `VOICE_SERVER_UPDATE`).
- Limitation : nombre de flux vidéo relayés borné par les capacités du récepteur (le SFU coupe les couches HD aux clients faibles).

Suite : **[07 — Sécurité & chiffrement](07-securite-chiffrement.md)**.
