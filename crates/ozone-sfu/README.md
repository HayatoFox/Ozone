# ozone-sfu — nœud média (SFU WebRTC)

Processus **séparé** de `ozone-api`. Relaie les flux RTP audio/vidéo entre participants d'un
salon vocal (Selective Forwarding Unit : un flux montant par pair, redistribué aux autres, sans
mixage ni transcodage → faible latence, faible CPU, compatible E2EE). Cf.
[`docs/06-infrastructure-vocale.md`](../../docs/06-infrastructure-vocale.md).

> **Isolation cryptographique** : la pile WebRTC (`webrtc-rs`) introduit `ring`/`rustls`.
> Ces dépendances sont **confinées à cette crate** ; `ozone-api` (REST + Gateway) reste **sans
> `ring`** (build AlmaLinux inchangé).

## Flux de signalisation

```
1. API ──Gateway: VOICE_SERVER_UPDATE { token, endpoint, guild_id, channel_id, session_id } ──► client
2. Client ──POST {endpoint}/sfu/rooms/:room/peers { sdp: <offre> } ──► SFU
3. SFU  ──► { peer_id, sdp: <réponse> }     (ICE non-trickle : la réponse attend la fin du gathering)
4. Média RTP/SRTP (UDP) entre client et SFU ; le SFU relaie aux autres pairs du salon.
5. Départ : DELETE {endpoint}/sfu/rooms/:room/peers/:peer_id
```

## État (S17 — fondation)

- ✅ Pile WebRTC validée (compile : `ring`, `webrtc-srtp/ice/sctp/...`).
- ✅ `API` WebRTC partagée (MediaEngine : Opus + VP8/VP9/H264 ; intercepteurs RTCP/NACK).
- ✅ Registre de salles/pairs, `join` (offre→réponse), `leave`.
- ✅ Relais de pistes RTP : un nouveau venu **reçoit les pistes déjà publiées** ; ses pistes
  entrantes sont recopiées vers les autres pairs.
- ✅ Signalisation HTTP (`POST`/`DELETE`), endpoint `/health`.

## Authentification (fait — S18)

Le SFU **vérifie le jeton vocal** émis par l'API (`VOICE_SERVER_UPDATE`) avant toute opération :
signature HS256 (`ozone_proto::token`, secret partagé `OZONE_VOICE_SECRET`), `kind = "voice"`,
expiration, et **correspondance du salon** (`:room` == `channel_id` du jeton). Le départ exige le
jeton **et** la propriété du pair. **Fail-closed** : sans `OZONE_VOICE_SECRET`, toute connexion est
refusée (`503`). Cf. `tests/auth.rs`.

## À faire (prochaines étapes média)

1. **Renégociation poussée (WS)** : pour que les pairs **déjà connectés** reçoivent un nouveau
   venu (mesh N-à-N complet), une signalisation WebSocket persistante (offres serveur→client).
2. **E2EE DAVE/MLS** : surchiffrement du média (le SFU relaie sans déchiffrer).
3. **TURN/STUN** dédiés, sélection de couche (simulcast/SVC), bitrate adaptatif, resume vocal.

## Lancer

```sh
OZONE_SFU_BIND=127.0.0.1:8081 cargo run -p ozone-sfu
```

Test média de bout en bout : nécessite deux vrais clients WebRTC (navigateur) — test E2E manuel,
hors suite unitaire (qui couvre la construction du SFU et le registre de salles).
