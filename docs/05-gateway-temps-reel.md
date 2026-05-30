# 05 — Gateway temps réel

La **Gateway** est une connexion **WebSocket** persistante qui pousse les événements en temps réel (messages, presence, frappe, mises à jour de toutes les entités). C'est le composant le plus sensible en concurrence : objectif **50–100k connexions par nœud**, *resume* sans perte.

## 1. Protocole

- **Transport** : WSS. Encodage **JSON** (par défaut) ou binaire **ETF**-like (plus compact). Compression **zstd-stream** (ou zlib-stream) optionnelle, négociée à la connexion : `?v=1&encoding=json&compress=zstd-stream`.
- **Trames** : `{ "op": <int>, "d": <data>, "s": <seq?>, "t": <event?> }`
  - `op` = opcode, `d` = payload, `s` = numéro de séquence (sur les dispatchs), `t` = nom d'événement (sur `op:0`).

### Opcodes
| op | Nom | Sens | Rôle |
|---|---|---|---|
| 0 | **DISPATCH** | ⬇ | événement (`t` + `s`) |
| 1 | **HEARTBEAT** | ⬆⬇ | battement (envoie le dernier `s`) |
| 2 | **IDENTIFY** | ⬆ | authentification + intents |
| 3 | **PRESENCE_UPDATE** | ⬆ | mettre à jour son statut/activité |
| 4 | **VOICE_STATE_UPDATE** | ⬆ | rejoindre/quitter/modifier un vocal |
| 6 | **RESUME** | ⬆ | reprendre une session interrompue |
| 7 | **RECONNECT** | ⬇ | le serveur demande une reconnexion |
| 8 | **REQUEST_GUILD_MEMBERS** | ⬆ | charger des membres à la demande |
| 9 | **INVALID_SESSION** | ⬇ | session invalide (resumable ou non) |
| 10 | **HELLO** | ⬇ | `heartbeat_interval` à la connexion |
| 11 | **HEARTBEAT_ACK** | ⬇ | acquittement de battement |
| 13 | REQUEST_SOUNDBOARD / autres requêtes ciblées | ⬆ | extensions |

## 2. Cycle de vie d'une connexion

```
1. Client ouvre WSS ──► Serveur: HELLO { heartbeat_interval }
2. Client: IDENTIFY { token, intents, properties (os, device), compress, presence, large_threshold }
3. Client démarre les HEARTBEAT toutes heartbeat_interval ms (avec jitter), attend HEARTBEAT_ACK
4. Serveur: DISPATCH t=READY { user, session_id, resume_gateway_url, guilds (non chargées), settings, relationships, private_channels, read_states }
5. Pour chaque guilde: DISPATCH t=GUILD_CREATE { … état complet: salons, rôles, membres (jusqu'au seuil), presences, voice_states, emojis, … }
6. Flux d'événements en continu (MESSAGE_CREATE, TYPING_START, PRESENCE_UPDATE, …)
```

- **READY** envoie les guildes « non disponibles », puis un `GUILD_CREATE` par guilde hydrate l'état → démarrage perçu rapide.
- `large_threshold` : au-delà de N membres, la liste n'est pas envoyée en entier (lazy via `REQUEST_GUILD_MEMBERS` / lazy guild subscriptions par fenêtre de salon visible).

## 3. Heartbeat & détection de coupure

- Le serveur envoie `heartbeat_interval` dans HELLO (ex. 41 250 ms).
- Le client envoie `HEARTBEAT { d: dernier_s }`, le serveur répond `HEARTBEAT_ACK`.
- Pas d'ACK reçu → le client considère la connexion morte, ferme et **RESUME**.
- Le serveur ferme les connexions sans heartbeat (zombie) pour libérer les ressources.

## 4. Resume (reprise sans perte) — critique

À la déconnexion, le client se reconnecte sur `resume_gateway_url` et envoie :
```
RESUME { token, session_id, seq: dernier_s_reçu }
```
- Le serveur **rejoue** les événements manqués depuis `seq` (buffer par session, en Redis/mémoire), puis `DISPATCH t=RESUMED`.
- Si la session a expiré → `INVALID_SESSION { resumable: false }` → le client refait `IDENTIFY` à froid.
- **Buffer de resume** : chaque session conserve une fenêtre glissante (ex. dernières 5 min / N événements) avec leurs `s`. C'est ce qui évite de tout recharger à chaque micro-coupure réseau (essentiel en mobilité).

## 5. Intents (réduction de volume)

À l'`IDENTIFY`, le client/bot déclare les **intents** (familles d'événements souhaitées) → le serveur ne pousse que celles-ci. Exemples : `GUILDS`, `GUILD_MEMBERS`, `GUILD_MESSAGES`, `MESSAGE_CONTENT`, `GUILD_PRESENCES`, `GUILD_VOICE_STATES`, `DIRECT_MESSAGES`, `GUILD_MODERATION`, `AUTO_MODERATION`. Le client Ozone officiel demande tout ; les bots tiers restreignent (perf + vie privée). Certains intents sont « privilégiés » (presence, membres, contenu) et requièrent une autorisation.

## 6. Catalogue d'événements (DISPATCH `t`)

### Cycle / session
`READY`, `RESUMED`, `RECONNECT`.

### Guildes
`GUILD_CREATE`, `GUILD_UPDATE`, `GUILD_DELETE`, `GUILD_ROLE_CREATE/UPDATE/DELETE`, `GUILD_EMOJIS_UPDATE`, `GUILD_STICKERS_UPDATE`, `GUILD_SOUNDBOARD_SOUNDS_UPDATE`, `GUILD_INTEGRATIONS_UPDATE`, `GUILD_AUDIT_LOG_ENTRY_CREATE`.

### Membres / bans
`GUILD_MEMBER_ADD/UPDATE/REMOVE`, `GUILD_MEMBERS_CHUNK` (réponse à REQUEST_GUILD_MEMBERS), `GUILD_BAN_ADD/REMOVE`.

### Salons / threads
`CHANNEL_CREATE/UPDATE/DELETE`, `CHANNEL_PINS_UPDATE`, `THREAD_CREATE/UPDATE/DELETE`, `THREAD_LIST_SYNC`, `THREAD_MEMBER_UPDATE`, `THREAD_MEMBERS_UPDATE`.

### Messages
`MESSAGE_CREATE/UPDATE/DELETE`, `MESSAGE_DELETE_BULK`, `MESSAGE_REACTION_ADD/REMOVE/REMOVE_ALL/REMOVE_EMOJI`, `MESSAGE_POLL_VOTE_ADD/REMOVE`.

### Présence / frappe / vocal
`PRESENCE_UPDATE`, `TYPING_START`, `VOICE_STATE_UPDATE`, `VOICE_SERVER_UPDATE`, `VOICE_CHANNEL_EFFECT_SEND` (soundboard/effets), `USER_UPDATE`.

### Relations / MP
`RELATIONSHIP_ADD/REMOVE`, `CHANNEL_RECIPIENT_ADD/REMOVE`, `DM_CHANNEL_CREATE`.

### Modération / events / invites
`AUTO_MODERATION_RULE_CREATE/UPDATE/DELETE`, `AUTO_MODERATION_ACTION_EXECUTION`, `GUILD_SCHEDULED_EVENT_CREATE/UPDATE/DELETE`, `GUILD_SCHEDULED_EVENT_USER_ADD/REMOVE`, `INVITE_CREATE/DELETE`, `WEBHOOKS_UPDATE`.

### Apps
`INTERACTION_CREATE` (slash commands, boutons, menus, modales).

## 7. Fan-out & abonnements (côté serveur)

- À l'`IDENTIFY`, le nœud Gateway s'abonne, via **Redis pub/sub** / **NATS**, aux topics des guildes de l'utilisateur (`guild.{id}.events`) et à son topic personnel (`user.{id}`).
- Quand l'API publie `message.create` sur NATS, tous les nœuds Gateway abonnés au salon poussent `MESSAGE_CREATE` aux sessions concernées (en respectant intents & permissions de lecture).
- **Lazy guild subscriptions** : pour les gros serveurs, le client n'« écoute » la liste de membres/presence que des salons réellement affichés (fenêtre visible), réduisant drastiquement le trafic.

## 8. Presence

- `PRESENCE_UPDATE` (op 3) du client → statut (`online/idle/dnd/invisible`) + activités (custom status, « joue à », « écoute »…).
- Le serveur agrège la présence multi-appareils (desktop/mobile/web) en Redis et diffuse aux amis + co-membres des guildes (selon intent `GUILD_PRESENCES`).
- Idle automatique après inactivité ; `invisible` = apparaît hors-ligne tout en restant connecté.

## 9. Sharding (montée en charge des très gros comptes/bots)

- Pour un bot présent sur des milliers de guildes, la connexion est **shardée** : `shard = (guild_id >> 22) % num_shards`. Chaque shard est une connexion Gateway gérant un sous-ensemble de guildes.
- Le client utilisateur normal n'a pas besoin de sharding (une seule connexion). Le mécanisme existe pour les apps/bots à grande échelle, comme chez Discord.

## 10. Sécurité & robustesse

- Authentification du token à l'`IDENTIFY` (révocable) ; déconnexion immédiate si le token est invalidé (changement de mdp, logout).
- **Back-pressure** : si un client lent ne consomme pas, buffer borné puis déconnexion (évite l'OOM serveur).
- **Rate-limit** des opcodes entrants (ex. PRESENCE_UPDATE, VOICE_STATE_UPDATE) par session.
- Filtrage des événements selon les **permissions de lecture** au moment de l'envoi (un membre sans accès à un salon ne reçoit pas ses messages).

Suite : **[06 — Infrastructure vocale](06-infrastructure-vocale.md)**.
