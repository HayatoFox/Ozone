# 01 — Architecture globale

## 1. Vue d'ensemble

Ozone suit une architecture **client léger natif ↔ services serveur spécialisés**, avec trois canaux de communication distincts :

1. **REST/HTTPS** — actions ponctuelles (CRUD, auth, upload). Sans état, idempotent.
2. **Gateway WebSocket (WSS)** — flux d'événements temps réel (messages, presence, frappe, mises à jour). Stateful, *resumable*.
3. **Média (UDP + WebRTC/SRTP)** — voix, vidéo, partage d'écran, via un **SFU** (Selective Forwarding Unit).

Cette séparation est exactement celle de Discord, et elle est essentielle : on ne fait jamais transiter du média par l'API, ni des événements temps réel par des requêtes REST.

> **Portée = une instance.** Tout le schéma ci-dessous décrit **une instance** Ozone (un déploiement auto-hébergé, isolé, avec ses propres comptes/guildes). Le client est **multi-instances** : il maintient un jeu de connexions (REST/Gateway/SFU) **par instance**, avec une identité/session séparée, et bascule de l'une à l'autre via un *switcher*. Avant toute connexion, le client appelle `GET /instance` pour le branding et la politique d'accès, puis franchit le **gate** (mot de passe d'instance facultatif). Détail : [features/00-instances](features/00-instances.md).

```
┌──────────────────────────── CLIENT OZONE (natif) ────────────────────────────┐
│                                                                               │
│  ┌──────────────────────────┐        ┌──────────────────────────────────┐    │
│  │      Couche UI (GPUI)     │◄──────►│       ozone-core (Rust)          │    │
│  │  - Vues & composants      │ state  │  - Store / réducteurs            │    │
│  │  - Rendu GPU, virtualisé  │ events │  - Client REST (reqwest)         │    │
│  │  - Thèmes, markdown, emoji │       │  - Client Gateway (WS)           │    │
│  └──────────────────────────┘        │  - Cache local (SQLite + WAL)    │    │
│                                       │  - Moteur voix (capture/Opus/RTC)│    │
│                                       │  - Crypto (E2EE, keystore)       │    │
│                                       └───────┬─────────┬─────────┬──────┘    │
└───────────────────────────────────────────────│─────────│─────────│──────────┘
                                          REST   │  WSS    │   UDP   │
                                                 ▼         ▼         ▼
┌──────────────────────────────────────── SERVEUR OZONE ────────────────────────┐
│                                                                                │
│  ┌────────────┐   ┌─────────────────┐   ┌──────────────┐   ┌───────────────┐   │
│  │ API REST   │   │  Gateway WS      │   │  Voice SFU   │   │  Workers       │  │
│  │ (axum)     │   │  (tokio-tungst.) │   │  (str0m)     │   │  (jobs async)  │  │
│  │ auth,CRUD, │   │  sessions, sub,  │   │  signaling,  │   │  push, emails, │  │
│  │ upload     │   │  resume, presence│   │  SRTP, mix   │   │  thumbnails,   │  │
│  └─────┬──────┘   └────────┬─────────┘   └──────┬───────┘   │  search index  │  │
│        │                   │                    │           └───────┬────────┘  │
│        └──────────┬────────┴──────────┬─────────┴───────────────────┘           │
│                   ▼                   ▼                                          │
│            ┌────────────┐      ┌────────────┐                                    │
│            │   NATS     │      │   Redis    │  (presence, rate-limit, sessions,  │
│            │ (bus event)│      │  pub/sub)  │   cache chaud, fan-out gateway)    │
│            └────────────┘      └────────────┘                                    │
│                                                                                 │
│   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐    │
│   │ PostgreSQL   │  │  ScyllaDB    │  │  S3 / MinIO  │  │  Meilisearch     │    │
│   │ (relationnel)│  │  (messages)  │  │  (médias)    │  │  (recherche)     │    │
│   └──────────────┘  └──────────────┘  └──────────────┘  └──────────────────┘    │
└────────────────────────────────────────────────────────────────────────────────┘
```

## 2. Architecture du client

Le client est divisé en **deux crates** pour garantir la réutilisabilité multiplateforme :

### `ozone-core` (logique, sans UI)
| Module | Rôle |
|---|---|
| `net::rest` | Client HTTP typé (reqwest), retries, rate-limit côté client, idempotence. |
| `net::gateway` | Connexion WebSocket, heartbeat, identify, **resume**, décompression zstd, dispatch des events. |
| `store` | Source de vérité locale : entités normalisées (guildes, salons, messages, membres, presence). Mises à jour **optimistes**. |
| `cache` | Persistance SQLite (mode WAL) : messages récents, métadonnées, file d'envoi hors-ligne. |
| `voice` | Capture/restitution audio (cpal), encodage **Opus**, jitter buffer, pipeline WebRTC, partage d'écran (capture GPU). |
| `crypto` | Keystore OS (Keychain/DPAPI/Secret Service), E2EE vocal (MLS), vérification d'identité. |
| `media` | Décodage images/vidéos, génération de miniatures, cache disque LRU. |
| `markdown` | Parseur markdown Discord-compatible → AST de rendu. |
| `instances` | Registre des **instances** connues ; sondage `GET /instance` ; **gate** mot de passe ; **session/jeton par instance** (keystore) ; switcher ; isolation stricte des identités. |

`ozone-core` expose une **API de commandes** (envoyer un message, rejoindre un vocal…) et un **flux d'événements** (nouveau message, presence changée…). L'UI ne fait que : envoyer des commandes, s'abonner aux events, dessiner l'état.

### Couche UI (par plateforme)
- **Desktop** : **GPUI** (Rust). Composants, fenêtres, rendu GPU, virtualisation des listes, thèmes.
- **Mobile** : `ozone-core` exposé via **UniFFI** → UI **SwiftUI** (iOS) / **Jetpack Compose** (Android).

> Avantage clé : un bug réseau/état se corrige **une fois** dans `ozone-core` pour toutes les plateformes. Seul le dessin diffère.

### Boucle de données côté client (unidirectionnelle)
```
  Action utilisateur ─► Commande core ─► (optimiste) maj Store ─► UI redessine
                                   │
                                   └─► REST/Gateway ─► réponse/event serveur
                                                   └─► réconciliation Store ─► UI
```
Le rendu optimiste donne la sensation « instantanée » : le message apparaît avant l'ACK serveur, puis est réconcilié (ou marqué en erreur).

## 3. Architecture du serveur

Quatre services déployables indépendamment (scalables séparément) :

### a) API REST (`ozone-api`)
- Framework **axum** (tokio, hyper). Stateless → scale horizontal trivial derrière un load-balancer.
- Responsabilités : authentification, CRUD (guildes, salons, rôles, membres, messages non temps-réel), upload (présigné S3), rate-limiting, validation des permissions.
- Émet des événements sur **NATS** après chaque mutation (ex. `message.create`) pour que la Gateway les diffuse.

### b) Gateway temps réel (`ozone-gateway`)
- Maintient les connexions **WebSocket** persistantes (cible : 50–100k par nœud).
- À l'`IDENTIFY`, charge l'état initial (`READY`), abonne la session aux guildes de l'utilisateur via **Redis pub/sub** / NATS.
- Diffuse les événements (fan-out), gère **heartbeat**, **resume** (buffer d'événements par session avec numéros de séquence), presence.
- Détail du protocole : [05 — Gateway temps réel](05-gateway-temps-reel.md).

### c) Voice SFU (`ozone-sfu`)
- Signaling WebSocket dédié + transport **UDP/SRTP**. Reçoit chaque flux montant une fois, le **redistribue** aux autres participants (pas de mixage côté serveur pour l'audio → faible CPU, faible latence).
- Gère simulcast vidéo (plusieurs résolutions), sélection de couche, RTX/NACK, congestion (GCC).
- Tunnel **E2EE DAVE** : le SFU route des paquets déjà chiffrés de bout en bout sans voir le clair.
- Détail : [06 — Infrastructure vocale](06-infrastructure-vocale.md).

### d) Workers (`ozone-workers`)
- Tâches asynchrones consommées depuis NATS : envoi de push notifications, emails, génération de miniatures/transcodage, indexation recherche, exécution AutoMod, expiration d'invitations/timeouts, nettoyage.

## 4. Magasins de données

| Magasin | Contenu | Pourquoi ce choix |
|---|---|---|
| **PostgreSQL** | users, guildes, salons, rôles, permissions, membres, relations, invitations, webhooks, settings | ACID, requêtes relationnelles complexes (permissions), maturité. |
| **ScyllaDB** | messages (partition = `channel_id`, clustering = `message_id` décroissant) | Débit d'écriture énorme, lecture par plage efficace, time-series. *MVP : Postgres, migration ensuite.* |
| **Redis** | presence, sessions gateway, rate-limit, cache chaud, locks, pub/sub fan-out | Latence sub-ms, structures adaptées. |
| **NATS** | bus d'événements inter-services (JetStream pour durabilité) | Découplage, fan-out, back-pressure. |
| **S3 / MinIO** | pièces jointes, avatars, bannières, emojis, stickers, sons, enregistrements | Stockage objet bon marché, URLs présignées, CDN devant. |
| **Meilisearch / Elastic** | index de recherche messages & membres | Recherche full-text + filtres rapides. |

Le détail des schémas : [03 — Modèle de données](03-modele-de-donnees.md).

## 5. Identifiants : Snowflakes

Comme Discord, chaque entité a un **ID Snowflake 64 bits** : `timestamp(42 bits) | worker(5) | process(5) | incrément(12)`. Avantages : triables chronologiquement, générables sans coordination centrale, encodent l'instant de création. Voir [03 — Modèle de données](03-modele-de-donnees.md#snowflakes).

## 6. Flux clés (séquences)

### Connexion + état initial
```
Client ──POST /auth/login──► API ──► vérifie, émet JWT + refresh
Client ──WSS connect──► Gateway ──► HELLO(heartbeat_interval)
Client ──IDENTIFY(token,intents)──► Gateway ──► charge guildes (Postgres/Redis)
                                            └─► READY(user, guildes, salons, settings)
Gateway abonne la session aux topics NATS/Redis des guildes de l'utilisateur.
```

### Envoi d'un message
```
Client (rendu optimiste local) ──POST /channels/{id}/messages──► API
API: vérifie permissions, persiste (Scylla), publie message.create sur NATS
Gateway (tous les abonnés du salon) ──MESSAGE_CREATE──► autres clients
API ──► répond au client émetteur (réconciliation de l'optimiste)
Worker: indexe le message (Meilisearch), exécute AutoMod, push notifications.
```

### Rejoindre un salon vocal
```
Client ──Gateway: VOICE_STATE_UPDATE(channel_id)──► serveur
Serveur ──► VOICE_SERVER_UPDATE(endpoint SFU, token) + VOICE_STATE_UPDATE(session_id)
Client ──WSS──► SFU: IDENTIFY(session_id, token) ──► READY(ssrc, ip, port, modes)
Client ──UDP IP discovery──► SFU ──► SELECT_PROTOCOL/SESSION_DESCRIPTION (clés SRTP)
Négociation DAVE (MLS) entre participants ──► flux média E2EE relayés par le SFU.
```

## 7. Topologie de déploiement

> **Un déploiement = une instance.** Du « tout-en-un » (un seul binaire ou un `docker-compose`, SQLite + disque local, idéal instance familiale) jusqu'au cluster complet ci-dessous, selon la charge. Plusieurs instances indépendantes peuvent coexister sur des domaines différents sans rien partager.

- **Edge** : load-balancer L4/L7 (HAProxy/Envoy), terminaison TLS (ACME/Let's Encrypt pour le self-host), CDN devant S3.
- **API** : N réplicas stateless (autoscaling CPU).
- **Gateway** : M réplicas, *sticky* par session, dimensionnés par nombre de connexions.
- **SFU** : pool de nœuds proches des utilisateurs (régions), sélection par latence ; **scalent par bande passante**.
- **Données** : Postgres (primaire + réplicas lecture), cluster Scylla (3+ nœuds), Redis (cluster/sentinel), NATS (cluster JetStream), MinIO (érasure coding).
- **Observabilité** : OpenTelemetry (traces), Prometheus (métriques), Loki (logs), Grafana.

Tout est conteneurisable (Docker) et orchestrable (Kubernetes / Nomad). Un `docker-compose` « tout-en-un » servira au dev local (voir [roadmap](09-roadmap.md)).

Suite : **[02 — Stack technique](02-stack-technique.md)**.
