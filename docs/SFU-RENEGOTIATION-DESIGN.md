# SFU — Renégociation WebRTC (vocal « flawless », plus de reload de flux)

## Problème
Signalisation **HTTP one-shot** (`POST /sfu/rooms/:room/peers` → réponse SDP unique), **aucune renégociation**.
Conséquence : tout changement de piste relance toute la `RTCPeerConnection` :
- activer/désactiver sa caméra ou le partage d'écran → `rejoinMedia` (teardown + reconnexion) ;
- un autre membre rejoint / publie → `scheduleVoiceResync` → reconnexion.
→ « rechargement » visible du flux.

## Solution : canal de signalisation WebSocket persistant + renégociation (negotiation parfaite)

### Transport
- L'**offre/réponse initiale reste en HTTP** (`join`) — inchangée, faible risque.
- Après `join`, le client ouvre un **WebSocket** `GET /sfu/rooms/:room/peers/:peer_id/signal?token=…`
  (authentifié par le même jeton vocal ; on vérifie que le `uid` du jeton **possède** le `peer_id`).
- Messages JSON bidirectionnels :
  - `{ "t":"offer", "sdp":…, "tracks":{id→kind} }` — émetteur ajoute/retire des pistes (manifeste mis à jour) ;
  - `{ "t":"answer", "sdp":… }` — réponse à une offre ;
  - `{ "t":"unpublish", "id":"<localTrackId>" }` — l'émetteur retire explicitement une piste (cam/écran off).

### Qui offre quoi
- **Publier** une nouvelle piste (cam/écran ON) ⇒ **le client offre** (seul un émetteur peut introduire une piste sortante). Le SFU répond.
- **Recevoir** une nouvelle piste d'autrui (push) ⇒ **le SFU offre** ; le client répond.
- Les deux sens existent ⇒ collisions possibles ⇒ **negotiation parfaite** : le **client est « poli »** (rollback + ré-offre), le **SFU « impoli »** (ignore l'offre client en cas de collision ; le client ré-essaiera).

### SFU : un acteur par pair (sérialisation)
Chaque `Peer` possède une tâche-acteur (mpsc) qui traite **séquentiellement** : `WsConnected`, `WsClosed`,
`ClientMsg`, `Renegotiate`. Cela évite tout `create_offer`/`set_remote_description` concurrent.
- `Renegotiate` (déclenché quand le SFU ajoute/retire une piste relayée vers ce pair) : si WS connecté et
  `signaling_state == Stable` et pas d'offre en vol → crée une offre (attend l'ICE, non-trickle) → envoie ;
  sinon marque `pending` (ré-offre après la réponse).
- `ClientMsg offer` : si `Stable` → `set_remote` + maj manifeste + `create_answer` → envoie ; sinon (collision,
  SFU impoli) → ignore (le client poli ré-offrira).
- `ClientMsg answer` : `set_remote` ; `making_offer=false` ; si `pending` → ré-offre.
- `ClientMsg unpublish` : retire la piste relayée chez les autres pairs + `Renegotiate` chacun.

### Relais & push
- `on_track` (nouvel arrivant qui publie) → `publish_track` ajoute la piste aux PC des autres pairs **et**
  envoie `Renegotiate` à chacun → ils reçoivent la piste **en direct** (plus de reconnexion).
- Bookkeeping : chaque pair garde `relayed: { published_track_id → RTCRtpSender }` pour pouvoir
  `remove_track` proprement à l'`unpublish`.

### Client : negotiation parfaite + repli
- WS persistant ; `onnegotiationneeded` → offre (poli) ; réception d'offre → rollback si collision puis réponse.
- `setCamera(on)` / `setScreen(stream|null)` : `addTrack`/`removeTrack` (ou `replaceTrack`) sur la PC existante,
  maj du manifeste, puis renégociation — **plus de teardown**.
- **Repli sûr** : toute erreur/timeout de renégociation (ou WS indisponible) ⇒ on retombe sur `rejoinMedia`
  (reconnexion complète, comportement actuel). Le vocal ne peut donc pas régresser.

### Sécurité (revue à faire — slice SFU, `ring` autorisé)
- WS authentifié par le **jeton vocal** (même `authorize()` que l'HTTP) ; vérifie que le `uid` possède le `peer_id`.
- `tracks`/`kind` toujours filtrés par **liste blanche** (`sanitize_kind`) ; l'`uid` reste **imposé par le serveur**.
- `unpublish` ne peut retirer **que** les pistes du pair demandeur (propriété vérifiée).
- Aucune chaîne client n'atteint le SDP relayé (inchangé vs §85-86).
