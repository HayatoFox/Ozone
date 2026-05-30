# 08 — Performance & optimisation

La raison d'être d'Ozone. Tout ce qui suit est un **budget** mesuré en CI, pas un vœu.

## 1. Budgets cibles (client)

| Métrique | Cible | Discord-Electron (réf.) |
|---|---|---|
| Démarrage à froid → interactif | **< 500 ms** | 2–5 s |
| RAM au repos (1 serveur) | **< 120 Mo** | 300–500 Mo |
| RAM usage lourd (gros serveurs) | **< 300 Mo** | 600 Mo–1,5 Go |
| Frame time (scroll, animations) | **< 8 ms** (120 fps) | 16–33 ms, jank |
| Latence saisie → écho écran | **< 16 ms** | variable |
| Taille binaire | **15–60 Mo** | 150–300 Mo |
| Conso CPU au repos (idle, 1 vocal) | **quasi 0 %** hors voix | notable |
| Latence bouche-à-oreille (vocal) | **< 150 ms** | ~ |

## 2. Rendu UI

- **GPU direct** (Metal/DX12/Vulkan via GPUI/wgpu) : pas de DOM, pas de reflow, pas de CSS engine.
- **Virtualisation** de toutes les listes longues (messages, membres, serveurs, fils) : seuls les éléments visibles (+ marge) sont matérialisés. Hauteurs variables gérées par mesure paresseuse + cache.
- **Rendu incrémental** : on ne redessine que les zones invalidées (dirty regions), pas toute la fenêtre.
- **Texte** : shaping mis en cache (clé = contenu+style+largeur), atlas de glyphes GPU, emoji couleur en atlas, fallback de polices pré-résolu.
- **Animations** sur le GPU (transitions, anneau de parole, typing) découplées de la logique, désactivables (mouvement réduit).
- **Images/vidéos** : décodage hors du thread UI, miniatures progressives, cache disque LRU borné, downscale GPU, formats modernes (AVIF/WebP).
- **Scrolling** : défilement à inertie natif, ancrage de position (anti-saut quand de nouveaux messages arrivent au-dessus), préchargement directionnel.

## 3. Mémoire

- **Modèle de données normalisé** (pas de duplication d'entités), références par id.
- **Fenêtrage de l'historique** : on garde en RAM une fenêtre de messages par salon ; le reste est en SQLite (pagination à la demande), éviction LRU.
- **Cache disque** (SQLite WAL) pour reprise instantanée (l'état s'affiche avant la fin de la sync gateway).
- Pas de fuite : arènes/pools pour objets temporaires, profils mémoire en CI (détection de régression).
- **Lazy guild subscriptions** : on n'hydrate membres/presence que des salons visibles (gros serveurs).

## 4. Réseau & temps réel

- **Compression zstd** du flux gateway, **ETF**-like binaire optionnel (plus compact que JSON).
- **Resume** systématique (pas de full-reload sur micro-coupure) — voir [05](05-gateway-temps-reel.md#4-resume-reprise-sans-perte--critique).
- **Coalescing** : regroupement des petits événements (typing, presence) en fenêtres.
- **Rendu optimiste** : actions affichées immédiatement, réconciliées ensuite.
- **Préfetch** intelligent : messages autour de la position, avatars/emojis du viewport.
- **Backoff** exponentiel + jitter sur reconnexion, file d'envoi hors-ligne.

## 5. Démarrage rapide

- Affichage de l'**état caché (SQLite)** instantané, puis réconciliation via `READY`/`GUILD_CREATE`.
- Chargement **paresseux** : seul le serveur/salon actif est pleinement hydraté au lancement.
- Binaire optimisé : `lto=fat`, `codegen-units=1`, `panic=abort` en release, strip des symboles, **PGO** (profile-guided optimization) en cible.
- Pas de runtime lourd à initialiser (vs Chromium).

## 6. Voix/vidéo (perf)

- SFU **sans transcodage audio** (relai pur) → CPU serveur minimal, latence faible.
- **Simulcast/SVC** : le client faible reçoit une couche légère ; pas de surcharge.
- Encodage/décodage vidéo **accéléré matériellement** (NVENC/QuickSync/VideoToolbox/VA-API).
- Capture d'écran **zéro-copie GPU** quand la plateforme le permet.
- Suppression de bruit ML **sur l'appareil**, optimisée (SIMD), désactivable sur machines faibles.

## 7. Serveur (perf & scalabilité)

- **API stateless** → scale horizontal linéaire.
- **Gateway** : tâche tokio légère par connexion, structures partagées sans verrou (sharding interne), back-pressure bornée. Cible 50–100k conn/nœud.
- **Messages** sur Scylla (écriture massive, lecture par plage O(log)).
- **Redis** pour le chaud (presence, fan-out) ; **NATS** pour le découplage.
- **CDN** devant S3 pour les médias (l'app ne sert jamais d'octets média).
- Caches multi-niveaux (per-process LRU → Redis → DB), invalidation par événement.

## 8. Méthodologie & garde-fous

- **Budgets en CI** : tests de perf qui **échouent** si une métrique régresse (frame time, RAM, démarrage).
- **Profiling continu** : Tracy/`puffin` (client), `tokio-console`/flamegraphs (serveur).
- **Bancs de charge** : k6/Gatling sur API & Gateway, simulateur de N clients vocaux sur le SFU.
- **Télémétrie opt-in** (perf anonymisée) pour repérer les régressions terrain.
- Règle d'or : **mesurer avant d'optimiser**, mais concevoir avec les budgets en tête dès le départ.

Suite : **[09 — Roadmap](09-roadmap.md)**.
