# 09 — Roadmap

Ordre de construction recommandé : on livre de la **valeur testable à chaque phase**, en posant d'abord les fondations qui débloquent le reste. Chaque phase a une *Definition of Done* vérifiable.

## Vue d'ensemble

```
Phase 0  Conception ........................ ✅ (ce dépôt)
Phase 1  Fondations + MVP texte temps réel
Phase 2  Serveurs, salons, rôles & messagerie riche
Phase 3  Vocal, vidéo, partage d'écran, E2EE
Phase 4  Social, profils, notifications, recherche, expressions
Phase 5  Modération, découverte, onboarding, événements, apps/bots
Phase 6  Polish, accessibilité, overlay, mobile, perf finale
```

Dépendances : `1 → 2 → {3, 4}` (3 et 4 parallélisables après 2) `→ 5 → 6`.

---

## Phase 1 — Fondations & MVP texte ⏳

**But :** après s'être connectés à une **instance** auto-hébergée, deux utilisateurs discutent en temps réel dans un salon, avec un client natif fluide.

- [ ] **Monorepo Cargo** : `ozone-proto` (types partagés), `ozone-core`, `ozone-ui`, `ozone-api`, `ozone-gateway`. CI (fmt, clippy, tests, build Win/macOS/Linux).
- [ ] **`docker-compose` dev** : Postgres, Redis, NATS, MinIO.
- [ ] **Bootstrap d'instance** : assistant de configuration (nom, branding, **compte propriétaire**, politique d'inscription, **mot de passe d'instance facultatif**) + endpoints `GET /instance` & `POST /instance/gate`. ([00-instances](features/00-instances.md))
- [ ] **Écran de connexion à une instance** (le *tout premier* écran) : saisie d'adresse, sondage du branding, **gate** mot de passe, login/register, **instances enregistrées** + session **par instance** (keystore).
- [ ] **Auth** : register/login, Argon2id, JWT + refresh, sessions. ([01](features/01-comptes-authentification.md))
- [ ] **API REST** socle : users, guildes, salons, messages (CRUD minimal). ([04](04-api-rest.md))
- [ ] **Gateway** minimal : HELLO/IDENTIFY/HEARTBEAT/READY/RESUME + `MESSAGE_CREATE/UPDATE/DELETE`, `TYPING_START`, fan-out NATS/Redis. ([05](05-gateway-temps-reel.md))
- [ ] **Client GPUI** : fenêtre, login, barre de serveurs, liste de salons, **liste de messages virtualisée**, composer, markdown de base, rendu optimiste.
- [ ] **Cache SQLite** + reprise instantanée.

**DoD :** chat temps réel fluide entre 2 clients, resume après coupure réseau, démarrage perçu < 1 s.

---

## Phase 2 — Serveurs, salons, rôles & messagerie riche

**But :** un serveur communautaire pleinement utilisable en **texte**.

- [ ] **Serveurs** : création (from scratch + templates), paramètres, invitations (expiration/usages/temporaire), membres, profils par serveur. ([02](features/02-serveurs.md))
- [ ] **Salons** : catégories, texte, annonces (crosspost/follow), **threads**, **forum/média** (tags/tri). ([03](features/03-salons.md))
- [ ] **Rôles & permissions** complets : bitfield, hiérarchie, **overrides** salon/catégorie, sync, éditeur UI. ([10](features/10-roles-permissions.md))
- [ ] **Messagerie riche** : markdown complet, **pièces jointes** (upload S3 présigné), embeds/unfurl, réactions (+ burst), réponses, **transfert**, épingles (250), édition/suppression, **sondages**, **messages vocaux**. ([04](features/04-messagerie.md))
- [ ] Modération de base : kick/ban/timeout, audit log socle.

**DoD :** créer un serveur structuré (catégories/salons/rôles), inviter, discuter en markdown riche avec fichiers/réactions/threads, permissions respectées côté serveur.

---

## Phase 3 — Vocal, vidéo & partage d'écran (parallélisable avec Phase 4)

**But :** communication média temps réel **chiffrée de bout en bout**.

- [ ] **SFU** (`ozone-sfu`, str0m) : signaling vocal, UDP/SRTP, IP discovery, heartbeat/resume. ([06](06-infrastructure-vocale.md))
- [ ] **Audio** : capture/restitution (cpal), **Opus** (FEC/DTX), VAD/PTT, AEC/AGC, **suppression de bruit ML**, volume par utilisateur, priorité micro.
- [ ] **Vidéo** : caméra (jusqu'à 25), **AV1/VP9/H.264**, simulcast/SVC, accélération matérielle.
- [ ] **Partage d'écran / Go Live** : capture native par OS, qualités jusqu'à 4K60, audio d'app, spectateurs. ([06-partage](features/06-partage-ecran.md))
- [ ] **E2EE DAVE/MLS** (`openmls`), codes de vérification.
- [ ] **Salons Stage** : intervenants/audience, demande de parole.
- [ ] **Soundboard**, statut de salon vocal, chat texte intégré.

**DoD :** 5 personnes en vocal/vidéo/partage chiffré E2EE, < 150 ms de latence, suppression de bruit active, modération vocale fonctionnelle.

---

## Phase 4 — Social, notifications, recherche, expressions (parallélisable avec Phase 3)

**But :** parité fonctionnelle **sociale** et de confort.

- [ ] **Amis & relations** : demandes, blocage, notes, surnoms, présence. ([09](features/09-amis-relations.md))
- [ ] **MP & groupes MP** (jusqu'à 10) + appels MP. ([07](features/07-messages-prives.md))
- [ ] **Profils complets** : global + par serveur, avatar animé, bannière, bio, **statut perso**, cosmétiques (décorations, nameplates, effets). ([08](features/08-profil.md))
- [ ] **Presence riche** multi-appareils.
- [ ] **Notifications** complètes : niveaux par serveur/catégorie/salon, mute planifié, sons granulaires, natives OS, inbox de mentions. ([13](features/13-notifications.md))
- [ ] **Recherche** (Meilisearch) : filtres `from/mentions/has/before/in/pinned`, quick switcher Ctrl+K. ([14](features/14-recherche.md))
- [ ] **Expressions** : emojis (animés, restreints), stickers (Lottie), GIF (Tenor-like), favoris. ([12](features/12-expressions.md))

**DoD :** ajouter des amis, MP/groupes avec appels, profils riches, notifications fines, recherche avancée, expressions complètes.

---

## Phase 5 — Modération avancée, découverte, événements, apps

**But :** serveurs communautaires complets + écosystème extensible.

- [ ] **AutoMod** complet (mots-clés/regex/presets, anti-spam/mention, liens, profils), actions, exemptions. ([11](features/11-moderation-securite.md))
- [ ] **Audit log** exhaustif, raid protection, niveaux de vérification, filtres de contenu, signalements.
- [ ] **Tableau de bord d'instance** : branding, politique d'inscription, **invitations d'instance**, **bans d'instance**, rôles d'instance, audit & limites/quotas. ([00-instances](features/00-instances.md#6-administration-de-linstance-tableau-de-bord-du-self-hoster))
- [ ] **Découverte / onboarding / welcome screen / Server Tags / boosts** (mécanique). ([19](features/19-decouverte-onboarding.md), [02](features/02-serveurs.md))
- [ ] **Événements programmés** (récurrence, RSVP, stage). ([18](features/18-evenements.md))
- [ ] **Apps & bots** : OAuth2, intents, **slash commands**, composants, modales, **webhooks**, rich presence API, app directory. ([17](features/17-webhooks-bots-integrations.md))
- [ ] **Sharding** Gateway pour gros bots.

**DoD :** un serveur communautaire complet (onboarding, découverte, AutoMod, événements) avec bots à slash commands et webhooks opérationnels.

---

## Phase 6 — Polish, accessibilité, overlay, mobile, perf finale

**But :** atteindre les **budgets de performance** et étendre les plateformes.

- [ ] **Thèmes** custom (tokens, hot-reload, import/export), densité, **accessibilité complète** (AccessKit, daltonisme, mouvement réduit, lecteurs d'écran). ([16](features/16-apparence-themes.md))
- [ ] **Overlay** natif en jeu + **mode streamer**. ([20](features/20-overlay-streamer.md))
- [ ] **Keybinds** personnalisables, i18n complète, mode développeur.
- [ ] **Mobile** : `ozone-core` via UniFFI → **SwiftUI** (iOS) + **Jetpack Compose** (Android).
- [ ] **Perf finale** : budgets en CI, PGO, virtualisation poussée, **migration messages → ScyllaDB**.
- [ ] **Distribution** : auto-update, binaires signés (notarization/Authenticode), MSI/dmg/AppImage/Flatpak.
- [ ] **Instance tout-en-un** : mode **mono-binaire** (SQLite + disque local, zéro dépendance) pour petites instances ; **switcher multi-instances** peaufiné (notifications agrégées, identités séparées).

**DoD :** tous les budgets de [perf](08-performance.md) atteints et **gardés en CI** ; clients desktop **et** mobile ; prêt pour la production auto-hébergée.

---

## Pistes de parallélisation par équipe

| Équipe | Phases concernées |
|---|---|
| **Backend API/Gateway** | 1 → 2 → 5 |
| **Realtime/Voix (SFU/DAVE)** | 3 (après socle gateway de 1) |
| **Client UI (GPUI)** | 1 → 2 → 4 → 6 |
| **Cœur partagé (`ozone-core`)** | transverse, démarre en 1 |
| **Infra/Ops** | docker-compose (1) → k8s + observabilité (5/6) |
| **Mobile** | démarre en 4–6 (dès que `ozone-core` est stable) |

## Risques & jalons de dérisquage

- **GPUI hors macOS** : valider un prototype Windows/Linux en début de Phase 1 ; sinon basculer **Iced** (impact UI seulement).
- **DAVE/MLS** : prototyper l'E2EE à 3 participants **avant** de finaliser le SFU (Phase 3, tôt).
- **Scalabilité Gateway** : test de charge 50k connexions dès la fin de Phase 2.
- **Scylla** : rester Postgres jusqu'à ce que le volume de messages l'impose (fin Phase 6).

---

*Ce document clôt le plan de conception. Le code commence en Phase 1.*
