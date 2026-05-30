# 02 — Stack technique

Ce document justifie chaque choix, donne les alternatives sérieuses et les compromis. Règle générale : **Rust partout où c'est possible** (un seul langage du client au serveur au SFU = partage de code, perfs, sûreté mémoire).

---

## 1. UI client (desktop) — le choix critique

La promesse « natif & fluide » se joue ici. Exigences : rendu **GPU**, listes virtualisées à 120 fps, markdown/emoji/médias riches, thèmes dynamiques, menus contextuels, drag & drop, multi-fenêtres, overlay.

### Comparatif

| Framework | Langage | Rendu | Maturité desktop | Fluidité | Verdict |
|---|---|---|---|---|---|
| **GPUI** | Rust | GPU (Metal/DX/Vulkan) | macOS mûr, Win/Linux en progrès | ★★★★★ (moteur de Zed) | **Choix principal** |
| **Iced** | Rust | GPU (wgpu) | Cross-platform stable | ★★★★ | **Repli sûr** |
| Slint | Rust/DSL | GPU/software | Bonne | ★★★★ | Alternative (licence GPL/commerciale) |
| Dioxus (Blitz) | Rust | GPU natif (jeune) | Émergent | ★★★ | À surveiller |
| egui | Rust | GPU (immédiat) | Bonne | ★★★ | UI « outil », moins « produit poli » |
| **Flutter** | Dart | GPU (Impeller) | Excellente, + mobile | ★★★★★ | **Alternative non-Rust** (voir §1.2) |
| Qt/QML | C++ | GPU | Très mûr | ★★★★ | Lourd, ergonomie datée |
| Tauri | Rust+Web | **WebView** | — | ★★★ | ❌ rejeté (rendu web) |

### 1.1 Recommandation : `ozone-core` (Rust) + **GPUI**

- **GPUI** est le moteur d'interface de **Zed**, l'éditeur réputé pour sa fluidité extrême. Immédiat + retenu hybride, GPU-natif, pensé pour des UI denses et rapides — exactement notre cas (listes de messages, membres, serveurs).
- 100 % Rust → partage total avec `ozone-core`, le serveur et le SFU.
- **Compromis** : écosystème jeune hors macOS, documentation publique limitée, API mouvante. On l'**isole derrière notre couche de composants** pour pouvoir basculer.

### 1.2 Repli / alternative décisionnelle

- **Iced** (Rust, wgpu) si GPUI bloque sur Windows/Linux : architecture Elm claire, multiplateforme éprouvée, bonne fluidité. Migration peu coûteuse car la logique est dans `ozone-core`.
- **Flutter** si l'équipe privilégie la **vélocité UI + mobile immédiat** : Impeller donne un rendu très fluide réellement natif (pas WebView), un seul codebase desktop+mobile, écosystème énorme. Compromis : Dart pour l'UI (le cœur resterait en Rust via FFI), et on s'éloigne du « tout-Rust ».

> **Décision par défaut retenue pour le plan : Rust + GPUI, repli Iced.** C'est un point de décision majeur ; il n'impacte quasiment pas les docs fonctionnelles (95 % indépendantes de l'UI). À rediscuter avant la phase 1.

### 1.3 Briques transverses UI
- **Texte** : shaping via `cosmtext`/`swash` + `fontdb` (ligatures, emoji couleur, RTL, fallback de polices).
- **Images/vidéo** : décodage `image`, `resvg` (SVG), vidéo via `ffmpeg`/`gstreamer` ou pipeline `wgpu` custom.
- **Accessibilité** : `accesskit` (exposé aux lecteurs d'écran natifs) — compatible GPUI/Iced.
- **Fenêtres/entrées** : `winit` (sous Iced) ou la couche fenêtres de GPUI.

---

## 2. Cœur client partagé

| Besoin | Crate / techno |
|---|---|
| HTTP | `reqwest` + `tower` (retry, timeout, rate-limit) |
| WebSocket | `tokio-tungstenite` |
| Sérialisation | `serde` (JSON) + **ETF**-like binaire optionnel ; `zstd` pour compression gateway |
| Cache local | `rusqlite`/`sqlx` (SQLite, mode WAL) |
| État | store normalisé maison (entités + index) ; signaux réactifs |
| Audio | `cpal` (capture/restitution), `opus`, `webrtc-audio-processing` (AEC/NS/AGC) |
| WebRTC | `webrtc-rs` ou `str0m` (sans-IO, idéal contrôle fin) |
| Crypto | `ring`/`rustls`, `openmls` (MLS pour DAVE) |
| Keystore | `keyring` (Keychain / DPAPI / Secret Service) |
| FFI mobile | `uniffi` (génère bindings Swift/Kotlin) |

---

## 3. Serveur — API & Gateway

| Besoin | Choix | Alternative |
|---|---|---|
| Runtime | **tokio** | — |
| HTTP/API | **axum** (hyper, tower) | actix-web |
| WebSocket gateway | `tokio-tungstenite` + tâches par connexion | — |
| Validation/permissions | logique maison (bitfields, voir [10](features/10-roles-permissions.md)) | — |
| ORM/DB | **sqlx** (async, requêtes vérifiées à la compilation) | SeaORM, diesel |
| Auth | JWT court (access) + refresh token rotatif, **Argon2id** pour mots de passe | Paseto |
| Migrations | `sqlx migrate` / `refinery` | — |

> **Note « authenticité Discord » :** la vraie Gateway de Discord est en **Elixir/Erlang** (BEAM) pour sa concurrence massive et sa tolérance aux pannes. Si l'on veut coller au modèle d'origine pour le seul service Gateway, **Elixir + Phoenix Channels** est une option défendable. Le plan retient **Rust** pour l'uniformité, mais Elixir reste un choix valable et documenté ici.

> **Portabilité serveur :** stack Rust **sans `ring`/OpenSSL** (JWT HS256 via `hmac`/`sha2`, mots de passe via `argon2` pur Rust) → build et exécution sur **Linux AlmaLinux/RHEL** (cible self-host de référence), Debian/Ubuntu, Windows et macOS. Seule brique C : **SQLite embarqué** (`libsqlite3-sys` *bundled*) → un compilateur C est requis au *build*, aucune dépendance système au *runtime*. Déploiement : [10-deploiement](10-deploiement.md).

---

## 4. SFU vocal / vidéo

| Besoin | Choix | Alternative |
|---|---|---|
| Transport/ICE/DTLS | **str0m** (sans-IO, déterministe) | `webrtc-rs` |
| Codec audio | **Opus** (48 kHz, FEC, DTX) | — |
| Codec vidéo | **AV1** (SVC) → **VP9** → **VP8** → H.264 (fallback) | — |
| Sécurité transport | **SRTP** (AES-GCM) | — |
| E2EE | **DAVE**-like via **MLS** (`openmls`) | Insertable streams / SFrame |
| Mixage | aucun pour l'audio (SFU pur) ; sélection de couche vidéo (simulcast/SVC) | MCU (rejeté : CPU) |

Alternative « clé en main » : **mediasoup** (C++/Node) ou **LiveKit** (Go, open-source) comme SFU si l'on veut accélérer. Compromis : on quitte le tout-Rust et on dépend d'un projet externe. Recommandation : **str0m** pour le contrôle, **LiveKit** comme accélérateur de prototypage si besoin.

---

## 5. Bases de données & infra

| Composant | Choix | Rôle | Alternative |
|---|---|---|---|
| Relationnel | **PostgreSQL 16** | users, guildes, salons, rôles, settings | — |
| Messages | **ScyllaDB** | stockage des messages à fort débit | Cassandra ; Postgres partitionné (MVP) |
| Cache/presence | **Redis 7** (cluster) | presence, sessions, rate-limit, fan-out | Dragonfly, KeyDB |
| Bus d'événements | **NATS JetStream** | événements inter-services durables | Kafka, Redis Streams |
| Stockage objets | **MinIO** (S3) | médias | S3, Garage |
| Recherche | **Meilisearch** | recherche messages/membres | Elasticsearch, Typesense |
| CDN | Cache HTTP devant S3 | livraison médias | Cloudflare, bunny |

---

## 6. Build, CI/CD, qualité

- **Cargo workspaces** : `ozone-core`, `ozone-ui`, `ozone-api`, `ozone-gateway`, `ozone-sfu`, `ozone-workers`, `ozone-proto` (types partagés client/serveur).
- **Types partagés** : un crate `ozone-proto` définit toutes les structures d'API/Gateway → **impossible de désynchroniser** client et serveur (même source de vérité Rust).
- **CI** : GitHub Actions — `cargo fmt`, `clippy`, tests, build multiplateforme (matrix Win/macOS/Linux), `cargo-deny` (licences/vulns).
- **Tests** : unitaires (logique permissions, markdown, store), intégration (API+DB éphémère via testcontainers), end-to-end (client headless ↔ serveur), charge (gateway/SFU avec k6/Gatling).
- **Releases** : binaires signés (codesign/notarization macOS, Authenticode Windows), auto-update (`omaha`-like / Sparkle-like maison).
- **Packaging** : MSI/MSIX (Windows), `.dmg`/`.app` (macOS), AppImage/Flatpak/deb (Linux).

---

## 7. Récapitulatif des décisions (ADR condensés)

| # | Décision | Statut | Raison principale |
|---|---|---|---|
| 1 | Natif GPU, **pas** Electron/Tauri | ✅ ferme | Exigence produit (perf/fluidité) |
| 2 | UI desktop : **GPUI**, repli **Iced** | 🟡 à confirmer | Fluidité max, tout-Rust |
| 3 | Cœur **Rust** partagé multiplateforme | ✅ ferme | Réutilisation, perfs, sûreté |
| 4 | Serveur **Rust/axum** (Gateway Rust, alt. Elixir) | 🟡 par défaut Rust | Uniformité ; Elixir = option |
| 5 | SFU **str0m** (alt. LiveKit) | 🟡 par défaut str0m | Contrôle E2EE/perf |
| 6 | Postgres + **Scylla** (messages) | ✅ avec MVP Postgres | Débit messages |
| 7 | **NATS** comme bus d'événements | ✅ | Découplage/fan-out |
| 8 | E2EE vocal **DAVE/MLS** | ✅ ferme | Parité + vie privée |

Légende : ✅ ferme · 🟡 par défaut, point de décision ouvert.

Suite : **[03 — Modèle de données](03-modele-de-donnees.md)**.
