# Ozone

> Un clone natif, complet et fidèle (« 1 pour 1 ») de Discord — client **et** serveur — écrit pour être **rapide, fluide et léger**, sans Electron ni WebView.

Ozone vise la parité fonctionnelle avec Discord sur tout ce qui compte vraiment (messagerie, serveurs, vocal/vidéo, partage d'écran, profils, rôles & permissions, modération, paramètres, thèmes, notifications, recherche…), en bannissant le superflu (mini-jeux, gadgets marketing, monétisation cosmétique non essentielle).

Ozone est **auto-hébergé et multi-instances** : chaque **instance** est un déploiement serveur complet et **isolé** (ses propres comptes, amis, MP, guildes). Le client démarre par un **écran de connexion à une instance** (adresse + mot de passe d'instance facultatif). Voir **[Instances & self-hosting](docs/features/00-instances.md)**.

> **Terminologie** — **Instance** = le backend auto-hébergé entier (analogie : un *homeserver* Matrix). **Guilde** = la communauté qu'on rejoint par invitation *à l'intérieur* d'une instance (c'est le « serveur » au sens Discord côté utilisateur). « Serveur » étant ambigu, la doc dit **instance** (backend) et **guilde** (communauté).

L'objectif technique central : **une application réellement native, GPU-accélérée, au démarrage instantané et au défilement à 120+ fps**, là où le client officiel (Electron) consomme beaucoup de RAM et de CPU.

---

## Principes directeurs

1. **Natif avant tout.** Aucune WebView, aucun runtime web embarqué. Rendu GPU, binaire compilé, empreinte mémoire minimale.
2. **Fluidité non négociable.** Budget de 8 ms par frame (120 fps). Listes virtualisées, rendu incrémental, zéro jank au scroll.
3. **Cœur partagé, UI fine par plateforme.** Toute la logique (réseau, état, cache, voix, crypto) vit dans un cœur Rust réutilisable ; chaque plateforme n'a qu'une couche d'affichage mince.
4. **Parité fonctionnelle réelle.** On reproduit le comportement exact de Discord (permissions, types de salons, markdown, presence, resume gateway…), pas une approximation.
5. **Temps réel d'abord.** Architecture événementielle (gateway WebSocket + SFU vocal), cohérence optimiste côté client.
6. **Sécurité & vie privée par défaut.** Chiffrement de bout en bout du vocal/vidéo (protocole type DAVE/MLS), 2FA, sessions auditables.
7. **Self-host & multi-instances.** Chaque instance est souveraine et isolée ; le client gère plusieurs instances (identités séparées) avec un *switcher*. Pas de fédération inter-instances (par choix).

## Stack en un coup d'œil

| Couche | Choix | Pourquoi |
|---|---|---|
| **UI client (desktop)** | **React + TypeScript**, empaqueté **Tauri** (`.exe`) | Client de référence : itération rapide, écosystème web mûr, WebRTC natif |
| **Mobile (phase ultérieure)** | Cœur Rust via FFI + SwiftUI / Jetpack Compose | Réutilise 80 % du code, UI 100 % native |
| **API & Gateway** | **Rust** (axum + tokio) | Latence faible, millions de connexions WS |
| **SFU vocal/vidéo** | **Rust** (str0m / webrtc-rs) | Routage média performant, Opus + AV1/VP9 |
| **Base relationnelle** | **PostgreSQL** | Users, guildes, salons, rôles, permissions |
| **Base messages** | **ScyllaDB** (Cassandra) | Écriture massive, time-series, partitionnement par salon |
| **Cache / presence / pub-sub** | **Redis** (+ NATS) | Presence, rate-limit, fan-out gateway |
| **Stockage objets** | **S3 / MinIO** | Pièces jointes, avatars, emojis, enregistrements |
| **Recherche** | **Meilisearch / Elastic** | Recherche de messages avec filtres |

> Le détail des choix, des alternatives et des compromis est dans [docs/02-stack-technique.md](docs/02-stack-technique.md).

> **Cibles serveur** : Linux **AlmaLinux / RHEL** (référence), Rocky, Debian/Ubuntu, Windows, macOS. Rust portable, **sans `ring`/OpenSSL**. Déploiement (systemd, Docker AlmaLinux, musl statique) : [docs/10-deploiement.md](docs/10-deploiement.md).

## Architecture en 30 secondes

```
                          ┌─────────────────────────────────────────────┐
                          │                CLIENTS OZONE                 │
                          │  Desktop (Rust+GPUI) · Mobile (Rust core +   │
                          │  SwiftUI/Compose)                            │
                          └───────┬───────────────┬───────────────┬──────┘
                   REST (HTTPS)   │   Gateway (WSS)│   Voix (UDP/  │
                                  │                │   WebRTC+DAVE)│
            ┌─────────────────────▼───┐  ┌─────────▼────────┐  ┌──▼───────────────┐
            │   API REST (Rust/axum)  │  │ Gateway temps    │  │  SFU média       │
            │   actions, CRUD, auth   │  │ réel (Rust)      │  │  (Rust, SRTP)    │
            └───────┬─────────────────┘  └───┬──────────────┘  └──┬───────────────┘
                    │                        │                    │
        ┌───────────┼──────────────┬─────────┴─────────┬──────────┘
        ▼           ▼              ▼                   ▼
   ┌─────────┐ ┌──────────┐  ┌──────────┐        ┌──────────┐
   │Postgres │ │ ScyllaDB │  │  Redis   │        │  NATS    │  ◄─ bus d'événements
   │(relat.) │ │(messages)│  │(presence)│        │(fan-out) │
   └─────────┘ └──────────┘  └──────────┘        └──────────┘
        │
        ▼
   ┌─────────────┐   ┌──────────────┐
   │ S3 / MinIO  │   │ Meilisearch  │
   │ (médias)    │   │ (recherche)  │
   └─────────────┘   └──────────────┘
```

Détails : [docs/01-architecture.md](docs/01-architecture.md).

---

## Index de la documentation

### Fondations
- **[00 — Vision & périmètre](docs/00-vision-et-perimetre.md)** — objectifs, ce qu'on inclut / exclut, principes natif & perf.
- **[01 — Architecture globale](docs/01-architecture.md)** — composants, flux de données, déploiement.
- **[02 — Stack technique](docs/02-stack-technique.md)** — choix natifs détaillés, alternatives, compromis.
- **[03 — Modèle de données](docs/03-modele-de-donnees.md)** — entités, snowflakes, schémas Postgres/Scylla.
- **[04 — API REST](docs/04-api-rest.md)** — conventions, ressources, rate limiting, idempotence.
- **[05 — Gateway temps réel](docs/05-gateway-temps-reel.md)** — protocole WS, opcodes, events, presence, resume, sharding.
- **[06 — Infrastructure vocale](docs/06-infrastructure-vocale.md)** — signaling, SFU, codecs, partage d'écran, chiffrement DAVE/MLS.
- **[07 — Sécurité & chiffrement](docs/07-securite-chiffrement.md)** — authN/Z, E2EE, secrets, conformité.
- **[08 — Performance & optimisation](docs/08-performance.md)** — budgets, virtualisation, GPU, mémoire, démarrage.
- **[10 — Déploiement (Linux / AlmaLinux)](docs/10-deploiement.md)** — build natif, systemd, Docker, musl statique, TLS.

### Fonctionnalités (le détail « le plus complet possible »)
- **[Instances & self-hosting](docs/features/00-instances.md)** — *point d'entrée* : connexion à une instance, mot de passe d'instance, multi-instances, admin d'instance.
- **[Comptes & authentification](docs/features/01-comptes-authentification.md)**
- **[Serveurs (guildes)](docs/features/02-serveurs.md)**
- **[Salons (tous types)](docs/features/03-salons.md)**
- **[Messagerie texte](docs/features/04-messagerie.md)**
- **[Vocal & vidéo](docs/features/05-vocal-video.md)**
- **[Partage d'écran & Go Live](docs/features/06-partage-ecran.md)**
- **[Messages privés & groupes](docs/features/07-messages-prives.md)**
- **[Profil & personnalisation](docs/features/08-profil.md)**
- **[Amis & relations](docs/features/09-amis-relations.md)**
- **[Rôles & permissions](docs/features/10-roles-permissions.md)**
- **[Modération & sécurité](docs/features/11-moderation-securite.md)**
- **[Emojis, stickers & soundboard](docs/features/12-expressions.md)**
- **[Notifications](docs/features/13-notifications.md)**
- **[Recherche](docs/features/14-recherche.md)**
- **[Paramètres utilisateur](docs/features/15-parametres-utilisateur.md)**
- **[Apparence & thèmes](docs/features/16-apparence-themes.md)**
- **[Webhooks, bots & intégrations](docs/features/17-webhooks-bots-integrations.md)**
- **[Événements programmés](docs/features/18-evenements.md)**
- **[Découverte & onboarding](docs/features/19-decouverte-onboarding.md)**
- **[Overlay & mode streamer](docs/features/20-overlay-streamer.md)**

### Planification
- **[Roadmap par phases](docs/09-roadmap.md)** — du MVP au clone complet, jalons et dépendances.

---

## Statut

📐 **Phase 0 — Conception.** Ce dépôt contient actuellement le **plan complet**. Aucune ligne de code produit encore. La [roadmap](docs/09-roadmap.md) décrit l'ordre de construction recommandé.

## Nom de code

**Ozone** — une couche fine, transparente et protectrice. Léger là où Discord est lourd.
