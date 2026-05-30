# 00 — Vision & périmètre

## 1. Objectif

Construire **Ozone**, un clone natif fidèle de Discord, couvrant le client et le serveur, avec :

- **Parité fonctionnelle réelle** sur le périmètre « utile » de Discord ;
- **Des performances très supérieures** au client officiel Electron (RAM, CPU, démarrage, fluidité) ;
- **Une base auto-hébergeable** (on contrôle le serveur de bout en bout).

Le mot d'ordre : *tout ce que fait Discord d'utile, en mieux optimisé, sans le superflu.*

## 1.5. Modèle self-host & multi-instances (structurant)

Ozone est conçu pour l'**auto-hébergement**. L'unité de déploiement est l'**instance**.

- **Instance** = un déploiement serveur Ozone **complet et isolé**, avec **ses propres** comptes, amis, MP, guildes, fichiers. Analogie : un *homeserver* Matrix, une instance Mastodon/Revolt.
- **Guilde** (UI : « serveur ») = une **communauté** créée **dans** une instance, rejointe par **invitation**. C'est le « serveur » au sens Discord pour l'utilisateur.
- **Compte** = identité **propre à une instance** (le même e-mail peut avoir un compte différent sur deux instances).

> ⚠️ « Serveur » est ambigu. Doc technique : **instance** (backend) vs **guilde** (communauté). UI : « instance Ozone » vs « serveur » (= guilde).

**Conséquences sur le produit :**
- Le **premier écran** du client est la **connexion à une instance** (adresse + **mot de passe d'instance facultatif**), avant tout compte. Détail : [features/00-instances](features/00-instances.md).
- Le client est **multi-instances** : il mémorise plusieurs instances, avec une **identité/session séparée** par instance, et un *switcher*.
- **Isolation totale, pas de fédération** (par choix) : les instances ne communiquent pas entre elles. Une fédération optionnelle reste une extension future possible, hors périmètre.
- Chaque instance a ses **rôles d'instance** (propriétaire/admin/modérateur), distincts des rôles de guilde.

## 2. Pourquoi natif (et pas Electron / Tauri)

| Critère | Electron (Discord actuel) | Tauri (WebView système) | **Ozone (natif GPU)** |
|---|---|---|---|
| Rendu | Chromium (HTML/CSS) | WebView OS | GPU direct (wgpu/Metal/DX12/Vulkan) |
| RAM au repos | 300–700 Mo | 150–300 Mo | **40–120 Mo** (cible) |
| Démarrage à froid | 2–5 s | 1–3 s | **< 500 ms** (cible) |
| Scroll listes longues | jank fréquent | variable | **120 fps constant** (virtualisation) |
| Contrôle du pipeline | faible | moyen | **total** |
| Taille binaire | 150–300 Mo | 10–40 Mo | **15–60 Mo** |

> Tauri reste du rendu **web** (HTML/CSS dans une WebView) : il allège l'empreinte mais ne donne pas le contrôle GPU ni la fluidité d'un moteur de rendu dédié. Ozone vise un rendu **retenu/immédiat GPU** pour maîtriser chaque frame.

## 3. Plateformes cibles

| Plateforme | Priorité | Approche |
|---|---|---|
| Windows 10/11 | P0 | Rust + GPUI (DX12) |
| macOS | P0 | Rust + GPUI (Metal) |
| Linux (X11/Wayland) | P1 | Rust + GPUI/Iced (Vulkan) |
| iOS | P2 | `ozone-core` (Rust/FFI) + SwiftUI |
| Android | P2 | `ozone-core` (Rust/FFI) + Jetpack Compose |
| Web | P3 (optionnel) | Cœur compilé WASM + canvas/WebGPU |

Desktop d'abord : c'est là que la promesse « natif & fluide » a le plus de valeur et que Discord-Electron est le plus faible.

> **Côté serveur**, `ozone-api` (et les futurs services) ciblent **Linux — AlmaLinux/RHEL en référence** (auto-hébergement), ainsi que Debian/Ubuntu, Windows et macOS. Code Rust portable, sans dépendance à `ring`/OpenSSL. Détail : [10-deploiement](10-deploiement.md).

## 4. Périmètre — CE QU'ON INCLUT

### Messagerie & texte
- Markdown complet (gras, italique, souligné, barré, code inline/bloc avec coloration, citations, spoilers, titres `#/##/###`, listes, sous-texte `-#`, liens masqués, horodatages `<t:>`, mentions).
- Réactions (normales + **super-réactions/burst**), réponses, transfert de message, épingles (cap 250), édition/suppression, accusés de frappe (typing), indicateurs non-lus.
- Pièces jointes (images, vidéos, audio, fichiers) avec aperçus, texte alternatif, spoiler.
- Embeds riches, **sondages**, **messages vocaux**, stickers, emojis custom, GIF (Tenor-like).
- Fils de discussion (threads publics/privés), salons **forum** et **média** (tags, tri, mise en page).
- Commandes slash, composants interactifs (boutons, menus, modales).
- Recherche avancée (filtres `from:`, `mentions:`, `has:`, `before/after/during:`, `in:`, `pinned:`).

### Serveurs (guildes)
- Création/jonction, **invitations** (expiration, usages max, membres temporaires), liens vanity.
- Catégories et tous les **types de salons** (texte, vocal, annonces, stage, forum, média, catégorie).
- **Rôles & permissions** granulaires (matrice complète, hiérarchie, overrides par salon/catégorie, sync).
- Modèles de serveur, **onboarding**, écran de bienvenue, **découverte**, serveurs communautaires.
- **Événements programmés**, salons d'annonces (suivi/crosspost).
- **Boosts** (paliers & perks, en tant que mécanique de features — sans la facturation réelle).
- **Server Tags / clans**, profils par serveur (pseudo, avatar, bannière, pronoms spécifiques au serveur).
- Audit log, bans/kicks/**timeouts**, niveaux de vérification, filtre de contenu.

### Vocal / vidéo / partage
- Salons vocaux persistants, appels vidéo (jusqu'à 25), **partage d'écran / Go Live** (jusqu'à 4K60).
- **Suppression de bruit (type Krisp)**, annulation d'écho, gain auto, détection d'activité vocale / push-to-talk.
- Volume par utilisateur, **priorité au micro**, **soundboard**, **statut de salon vocal**.
- **Salons Stage** (audience, intervenants, demande de parole, modérateurs).
- Chat texte intégré aux salons vocaux, mute/deafen serveur, régions, bitrate, qualité vidéo.
- **Chiffrement E2EE** vocal/vidéo (protocole type **DAVE**, échange de clés **MLS**).

### Identité & social
- Profil global + **profils par serveur**, avatar (animé), bannière, bio, **statut personnalisé**, pronoms.
- Décorations d'avatar, **nameplates**, effets de profil, badges, thèmes de profil.
- Presence riche (joue à / écoute…), serveurs & amis en commun.
- Amis (en ligne / tous / en attente / bloqués), **demandes d'ami**, blocage, **notes**, surnoms d'amis.
- Messages privés, **groupes de MP** (jusqu'à 10), appels MP.

### Paramètres & personnalisation
- Compte (pseudo, email, tél, mot de passe, **2FA**, sessions/appareils, suppression de compte).
- Confidentialité & sécurité, applications autorisées, connexions.
- Voix & vidéo (périphériques, modes d'entrée, keybinds, avancé).
- Apparence (**Sombre / Clair / Minuit / Sync OS**, densité Cosy/Compact, taille de police, zoom, couleurs d'accent).
- **Accessibilité** (mouvement réduit, autoplay GIF/stickers, couleurs de rôle, saturation, daltonisme, TTS, navigation clavier).
- **Notifications** (par serveur/salon/catégorie, sons granulaires, overrides, mise en sourdine planifiée).
- **Keybinds** personnalisables, langue, **mode streamer**, **overlay en jeu**, options avancées (mode dev, accélération matérielle).

### Modération & sûreté
- **AutoMod** (filtres de mots-clés, anti-spam, anti-mention-spam, liens nuisibles, profils suspects).
- Audit log complet, timeouts, bans avec purge de messages, niveaux de vérification, raid protection, signalements.

### Extensibilité
- **Webhooks**, **bots/apps** (slash commands, composants, modales, rich presence), OAuth2, app directory, intégrations.

## 5. Périmètre — CE QU'ON EXCLUT (le « superflu »)

Conformément à la demande (« pas de la merde »), on **n'implémente pas** (au moins pas avant la parité fonctionnelle) :

- ❌ Activités embarquées / mini-jeux in-app (Watch Together, jeux Discord, etc.).
- ❌ Détection d'activité de jeu / Rich Presence *de jeux tiers* automatique (l'API rich presence reste possible pour les bots, mais pas l'intégration jeux propriétaire).
- ❌ Monétisation cosmétique non essentielle : boutique, **Orbs**, cadeaux payants, achats de nameplates/decorations en argent réel (les objets existent comme **features**, mais débloqués sans paiement).
- ❌ Abonnements payants réels (Nitro) — les *capacités* Nitro (upload plus gros, emojis animés partout, HD stream…) sont configurables par l'admin, pas vendues.
- ❌ Family Center, intégrations marketing (quêtes, sponsors), publicités.
- ❌ Clyde / fonctions IA gadget.

> Ces exclusions concernent le **produit**. L'architecture reste extensible : un module « activités » ou « boutique » pourrait être ajouté plus tard sans refonte.

## 6. Critères de succès (definition of done global)

1. Un utilisateur peut créer un compte, un serveur, des salons texte/vocaux, inviter des amis, discuter en markdown riche, partager des fichiers, et passer en vocal/vidéo + partage d'écran **chiffré**.
2. La gestion des **rôles & permissions** reproduit fidèlement la matrice Discord (overrides, hiérarchie).
3. Le client tient **120 fps** au scroll d'un salon de 100 000 messages, démarre **< 500 ms**, et reste **< 150 Mo** de RAM en usage courant.
4. Le serveur encaisse **N×10⁴ connexions WS** par nœud gateway et supporte le **resume** sans perte d'événements.
5. Le vocal a une latence bouche-à-oreille **< 150 ms** en conditions normales, avec suppression de bruit active.

## 7. Hypothèses & risques

- **GPUI** est jeune hors macOS : un repli **Iced** est prévu (voir [stack](02-stack-technique.md)). Risque maîtrisé par l'isolation UI/cœur.
- Le **SFU média** + **DAVE/MLS** est la partie la plus complexe : prototypage tôt (phase 3 de la [roadmap](09-roadmap.md)).
- **ScyllaDB** demande de l'expertise ops : démarrer en Postgres pur pour le MVP, migrer les messages vers Scylla quand le volume l'exige.

Voir la suite : **[01 — Architecture globale](01-architecture.md)**.
