# Revue de sécurité — S1 (permissions/rôles/membres) · S2 (messages) · S3 (salons)

> Document vivant : une vérification cyber est effectuée à la fin de **chaque** couche serveur.

**Périmètre** : code serveur `ozone-api` des couches S1 et S2.
**Méthode** : revue statique du code (chaque endpoint : authentification, autorisation, requêtes) + **tests d'intrusion automatisés** (`crates/ozone-api/tests/security.rs`, 11 scénarios adverses) rejouables en CI.
**Résultat** : 4 failles trouvées et **corrigées**, 4 risques résiduels **documentés** (planifiés). 16 tests au vert.

## 1. Failles trouvées et corrigées

| # | Sévérité | Domaine | Description | Correctif |
|---|---|---|---|---|
| **F1** | Moyenne | DoS | `bulk-delete` exécutait une boucle de `DELETE` non bornée (un client pouvait envoyer des milliers d'IDs). | Plafond **100** messages par appel (`400` au-delà). |
| **F2** | Faible | DoS / stockage | L'emoji de réaction (segment d'URL) n'était pas borné → stockage arbitraire. | Validation **1–64 caractères**. |
| **F3** | Faible | Fuite de données | Le message cité (`referenced_message`) était récupéré par `id` seul, sans contrôle de salon (fuite potentielle inter-salons). | Récupération **scopée au salon** du message. |
| **F4** | Faible | Intégrité / escalade | Une surcharge de salon pouvait viser une cible inexistante et tenter d'accorder `ADMINISTRATOR`. | **Validation de la cible** (rôle/membre de la guilde) + **retrait d'`ADMINISTRATOR`** des surcharges. |

## 2. Points vérifiés — **non vulnérables** (couverts par des tests)

| Vecteur testé | Test | Résultat |
|---|---|---|
| **IDOR** — non-membre lit/écrit/énumère une guilde | `non_member_is_denied_everything` | bloqué (4xx) |
| **IDOR** — éditer/supprimer le message d'autrui | `cannot_edit_or_delete_others_messages` | `403` |
| Réponse pointant un message d'un autre salon | `reply_across_channels_is_blocked` | `400` |
| **Escalade** — créer un rôle avec des perms qu'on n'a pas | `role_permissions_are_clamped_to_grantor` | perms clampées |
| **Hiérarchie** — s'attribuer un rôle au-dessus du sien | `cannot_assign_role_above_own_highest` | `403` |
| **Isolation inter-guildes** — attribuer un rôle d'une autre guilde | `roles_are_isolated_between_guilds` | `404` |
| Expulser le propriétaire | `cannot_kick_owner` | `403` |
| **JWT** — signature altérée / vide / `alg:none` / format invalide | `forged_or_tampered_tokens_are_rejected` | `401` |
| **Injection SQL** — contenu et emoji avec métacaractères | `sql_injection_is_neutralized` | stocké littéralement, table intacte |
| **DoS** — bulk-delete > 100, contenu > 4000 | `dos_limits_are_enforced` | `400` |
| Surcharge `ADMINISTRATOR` / cible invalide | `overwrites_strip_admin_and_validate_target` | retiré / `404` |

### Défenses structurelles confirmées
- **Requêtes paramétrées** partout (`sqlx::bind`) ; les rares `format!` n'injectent que des constantes (`MSG_SELECT`) ou des **entiers issus de la base** (liste d'IDs des réactions) — jamais d'entrée utilisateur.
- **JWT HS256 maison** : vérification HMAC à temps constant, contrôle du `kind` (un jeton *gate* ne vaut pas un *access*) et de l'`exp` ; l'en-tête n'est pas fait confiance (pas de confusion d'algorithme / `alg:none`).
- **Autorité serveur** : chaque mutation recalcule les permissions effectives ; un non-membre obtient `0` permission (le rôle `@everyone` ne s'applique qu'aux membres).
- **Anti-mass-assignment** : les DTOs n'exposent pas les champs sensibles (`position`, `managed`, `pinned`, `author` sont fixés côté serveur).
- **Limite de corps** : la limite par défaut d'axum (~2 Mio sur `Json`) borne les payloads.

## 3. Risques résiduels — **acceptés / planifiés**

| # | Sévérité | Description | Plan |
|---|---|---|---|
| **R1** | Moyenne | **Pas de rate-limiting** (login, création d'invitations, envoi de messages, gate). Brute-force et spam possibles. | Couche de rate-limit Redis (token bucket) — cf. [docs/04-api-rest.md](04-api-rest.md#2-rate-limiting) ; à implémenter avant exposition publique. |
| **R2** | Faible | **Énumération de comptes** : `register` distingue « pseudo/e-mail déjà utilisé ». | Message générique + vérification par e-mail (slice comptes/anti-abus). |
| **R3** | Faible | **Politique de mot de passe** minimale (≥ 8). | zxcvbn + liste HIBP (slice comptes). |
| **R4** | ~~Info~~ **Résolu (S6b)** | ~~`join_invite` ne vérifie pas un **bannissement**~~. | Contrôle de ban ajouté dans `join_invite` (cf. §10). |
| **R5** | Faible | **Course sur le quota d'invitation d'instance** : la vérification `uses < max_uses` et la consommation `uses + 1` ne sont pas atomiques (deux statements séparés). Deux inscriptions concurrentes avec la même invitation à usage unique pourraient dépasser le quota de 1. | Consommation atomique via `UPDATE … WHERE uses < max_uses` à l'intérieur d'une transaction `BEGIN IMMEDIATE` (passe de durcissement transactionnel de `register`). Risque réel faible : scénario de bootstrap self-host, très faible concurrence sur la rédemption. |
| **R6** | Moyenne | **Exécution de webhook non authentifiée et sans rate-limit** (S7) : un détenteur du jeton peut poster sans session ni quota → spam/abus. | Couche de rate-limit (cf. R1) avec **quota dédié par webhook** ; option de désactivation/rotation rapide du jeton. À traiter avant exposition publique. |
| **R7** | Élevée (plan média, **avant exposition**) | **SFU sans authz** (S17) : `POST /sfu/rooms/:room/peers` n'exige/vérifie **pas** le jeton vocal émis par l'API → n'importe qui pourrait rejoindre un salon média en connaissant l'identifiant. | Le SFU **doit vérifier le jeton `VOICE_SERVER_UPDATE`** (secret partagé `OZONE_VOICE_SECRET`) et que le `room` correspond, avant d'admettre un pair. Bloquant **avant tout déploiement du média** ; sans impact tant que le SFU n'est pas exposé. Cf. `crates/ozone-sfu/README.md`. |

## 4. Comment rejouer

```sh
cargo test -p ozone-api --test security      # intrusion S1/S2
cargo test -p ozone-api --test security_s3   # intrusion S3
cargo test -p ozone-api --test security_s7   # intrusion S7 (webhooks, recherche, événements)
cargo test -p ozone-api --test security_s8   # intrusion S8 (lecture, mentions, notifications)
cargo test -p ozone-api --test realtime      # S9 — émission/portée des événements Gateway
cargo test -p ozone-api --test security_s10  # intrusion S10 (profils & réglages)
cargo test -p ozone-api --test security_s11  # intrusion S11 (présence)
cargo test -p ozone-api --test realtime_social # S12 — portée des événements relations/MP
cargo test -p ozone-api --test security_s13  # intrusion S13 (gestion de guilde)
cargo test -p ozone-api --test invites       # S14 — aperçu & révocation d'invitations
cargo test -p ozone-api --test leave_guild   # S15 — quitter une guilde
cargo test -p ozone-api --test security_s16  # intrusion S16 (signalisation vocale)
cargo test -p ozone-api                       # suite complète (99 tests)
```

La CI ([.github/workflows/ci.yml](../.github/workflows/ci.yml)) exécute ces tests sur Ubuntu **et** AlmaLinux 9 à chaque push.

## 5. S3 — Salons avancés (catégories, slowmode, NSFW)

Aucune faille exploitable post-hoc : les mutations réutilisent `require_channel_perm` / `require_guild_perm`. **Durcissements proactifs** appliqués : validation que la catégorie parente appartient à la guilde (anti-IDOR inter-guildes), réordonnancement borné (≤ 500) et restreint aux salons de la guilde, **slowmode** dont le contournement est strictement lié aux permissions (`MANAGE_MESSAGES` / `MANAGE_CHANNELS` / `BYPASS_SLOWMODE`), bornes sur `rate_limit_per_user` (0–21600), topic (≤ 1024), nom (≤ 100) et liste blanche de types de salon.

| Vecteur testé | Test (`security_s3.rs`) | Résultat |
|---|---|---|
| Membre sans `MANAGE_CHANNELS` modifie/supprime/crée/réordonne | `member_without_manage_channels_cannot_modify` | `403` |
| Non-membre lit un salon | idem | `403` |
| Parenter / déplacer / réordonner vers une **autre guilde** | `cross_guild_parenting_and_reorder_blocked` | `404` |
| Contourner le **slowmode** (membre simple vs `MANAGE_MESSAGES`) | `slowmode_gate_is_permission_based` | bloqué / autorisé selon la permission |

## 6. S4 — Relations (amis / blocages / notes)

Module implémenté par un **sous-agent**, puis **intégré et audité de façon adverse par le mainteneur**. Une faille non couverte par les tests générés a été trouvée à la revue et corrigée :

| # | Sévérité | Faille | Correctif |
|---|---|---|---|
| **F5** | Faible | `PUT /users/@me/relationships/:id` (par identifiant) ne vérifiait ni l'auto-relation ni l'existence de la cible → création possible de `(moi,moi,incoming)` ou de lignes vers un utilisateur fantôme. | `friend_request` durci : rejet de l'auto-relation (`400`) + vérification d'existence de la cible (`404`). Test ajouté. |

Protections confirmées (`relationships.rs` + `security_s4.rs`) : relations strictement **scopées à `@me`** (aucune route ne touche les relations d'autrui → pas d'IDOR), demande à un bloqueur → `403`, à soi-même → `400`, pseudo inconnu → `404`, note bornée (≤ 256), **isolation** (un tiers ne voit aucune relation). Requêtes paramétrées.

> Note d'ingénierie : le harnais de test a aussi été durci (nom de base SQLite unique par **thread**) pour éliminer une **course** entre tests concurrents (horodatage ns identique sur l'horloge Windows) — stabilité CI, sans impact sécurité.

## 7. S4b — Messages privés & groupes

Cœur (intégration de l'accès MP dans `require_channel_perm`) par le mainteneur ; tests générés par des **sous-agents** en parallèle, puis intégrés et audités. Un bug **fonctionnel** (contrainte `NOT NULL channels.name` violée à l'ouverture d'un MP sans nom) a été trouvé à l'intégration et corrigé — sans impact sécurité. Revue adverse du mainteneur : **aucune faille exploitable** ; un test de durcissement a été ajouté (IDOR sur `GET /channels/:id`, immuabilité d'un MP 1:1).

| Vecteur testé | Test (`security_s4b.rs`) | Résultat |
|---|---|---|
| Non-destinataire lit / envoie dans un MP | `non_recipient_cannot_read_or_send` | `403` |
| Non-membre ajoute au groupe | `non_recipient_cannot_add_to_group` | `403` |
| Membre non-propriétaire retire un autre / se retire soi-même | `non_owner_cannot_remove_other_member` | `403` / `200` |
| Utilisateur **bloqué** envoie en MP 1:1 | `blocked_user_cannot_send_in_dm` | `403` (lecture seule) |
| Isolation de la liste des MP | `dm_listing_isolation` | liste vide |
| IDOR `GET /channels/:id` + ajout sur un MP 1:1 | `dm_channel_idor_and_one_to_one_is_immutable` | `403` / `400` |

Protections structurelles : accès MP/groupe réservé aux **destinataires** (intégré dans `require_channel_perm` → un non-destinataire obtient `0` permission), retrait d'un tiers **réservé au propriétaire**, blocage = **lecture seule** en 1:1 (inactif en groupe, parité Discord assumée), taille de groupe bornée (10), transfert automatique de propriété au départ du propriétaire.

## 8. S5 — Routage Gateway (souscription pub/sub)

> Rappel terminologique : « abonnement / souscription » désigne ici le **routage pub/sub** des événements temps réel, **pas** une offre payante. Aucune fonctionnalité payante dans le projet.

| # | Sévérité | Faille | Correctif |
|---|---|---|---|
| **F6** | **Moyenne (confidentialité)** | Le gateway minimal **diffusait chaque événement à toutes les connexions authentifiées** → un utilisateur recevait les messages de guildes/MP où il n'était pas. | Chaque événement porte une **portée** (`EventScope`) ; `should_deliver` ne pousse l'événement qu'aux ayants droit (membre de la guilde, droit de **voir** le salon, destinataire du MP, ou utilisateur ciblé). |

Protections confirmées (`gateway.rs` + `security_s5.rs`) :

| Vecteur testé | Test | Résultat |
|---|---|---|
| Membre / destinataire reçoit ses événements | `routing_delivers_to_authorized_users` | livré |
| Non-membre reçoit un événement de guilde / salon / MP | `routing_denies_unauthorized_users` | **non livré** |
| Membre **sans VIEW** reçoit un événement d'un salon privé | idem | **non livré** (le propriétaire, oui) |
| Événement à portée `User` reçu par un autre | idem | **non livré** |

Le filtrage réutilise exactement `channel_permissions` (même logique d'autorisation que l'API REST) → cohérence entre ce qu'on peut lire et ce qu'on reçoit en temps réel. *Note d'échelle : en mode tout-en-un, le filtrage fait une vérification par événement/connexion ; à grande échelle, on passe aux topics Redis/NATS par guilde (cf. [05-gateway-temps-reel](05-gateway-temps-reel.md)).*

## 9. S6a — Expressions (emojis / stickers / soundboard)

Trois modules CRUD **écrits par des sous-agents en parallèle** (+ tests fonctionnels), **intégrés et audités par le mainteneur** (qui a écrit les tests de sécurité). Intégration propre ; seule correction : un lint clippy (`manual_range_contains`) dans le code généré. **Aucune faille exploitable.**

| Vecteur testé | Test (`security_s6.rs`) | Résultat |
|---|---|---|
| Membre sans permission crée une expression | `non_privileged_cannot_create_expressions` | `403` (emoji/sticker/son) |
| Non-membre liste / crée | idem | `403` |
| `CREATE_GUILD_EXPRESSIONS` gère **seulement les siennes** | `create_permission_limits_to_own_expressions` | tiers `403`, auteur/propriétaire `200` |
| Isolation inter-guildes (gérer une expression d'une autre guilde) | `cross_guild_isolation_and_validation` | `404` |
| Nom invalide / asset vide | idem | `400` |

Gardes : `require_expression_create` (CREATE **ou** MANAGE) et `require_expression_manage` (MANAGE **ou** auteur), `fetch` scopé à la guilde (pas d'IDOR inter-guildes), validation des noms/assets.

## 10. S6b — Modération (bans, timeout, audit log)

Cœur couplé par le mainteneur (enforcement dans l'envoi de message et la jonction). **Aucune faille exploitable.**

| Vecteur testé | Test (`security_s6b.rs`) | Résultat |
|---|---|---|
| Membre sans `BAN_MEMBERS` bannit | `moderation_requires_permissions` | `403` |
| Membre sans `MODERATE_MEMBERS` met en sourdine | idem | `403` |
| Membre sans `VIEW_AUDIT_LOG` lit l'audit | idem | `403` |
| Bannir le **propriétaire** | `cannot_ban_owner_or_higher_role` | `403` |
| Bannir un **rôle supérieur** (hiérarchie) | idem | `403` ; membre sous soi → `200` |

Enforcement vérifié (`moderation.rs`) : un **banni** ne peut pas rejoindre (contrôle dans `join_invite`) ni écrire (plus membre) ; un membre **en timeout** ne peut pas envoyer de message (contrôle `communication_disabled_until` dans `create_message`) ; toutes les actions (ban/unban/kick/timeout) sont tracées dans le **journal d'audit**.

## 11. S6c — Administration d'instance (rôles, suspension, invitations d'instance)

Tableau de bord du self-hoster : config d'instance, **invitations d'instance** (politique `invite`), **rôles d'instance** (owner/admin/moderator/user) et **suspension** de comptes. Cœur écrit et audité par le mainteneur. **Aucune faille exploitable** ; un risque résiduel de faible sévérité identifié (R5, course sur le quota d'invitation).

| Vecteur testé | Test (`security_s6c.rs`) | Résultat |
|---|---|---|
| Non-admin accède à `/instance/admin/*` (config, invitations, comptes) | `admin_endpoints_require_admin` | `403` |
| Admin (non-propriétaire) gère les rôles | `only_owner_manages_roles_and_owner_is_protected` | `403` (réservé au propriétaire) |
| Suspendre le **propriétaire** | idem | `403` |
| Modifier le rôle du **propriétaire** (même par lui-même) | idem | `403` (rôle immuable) |
| Promouvoir le rôle **`owner`** | idem | `400` (non attribuable) |

Défenses (`routes_instance_admin.rs` + `routes_auth.rs`) :
- **Étagement des gardes** : lecture/invitations/suspension → `require_instance_admin` ; attribution de rôle → `require_instance_owner` (strict).
- **Propriétaire protégé** : ni suspension, ni modification de rôle (vérif `instance_role(target) == "owner"`), et `owner` n'est pas une valeur attribuable (liste blanche `admin | moderator | user`).
- **Bootstrap sûr** : seul le **tout premier** compte contourne la politique d'inscription (`is_first` ⇔ table `users` vide) et devient `owner` ; tout compte ultérieur est soumis à la politique (`open` / `invite` / `closed`).
- **Inscription sur invitation** : l'invitation d'instance est validée (existence, non-expiration, `uses < max_uses`) **avant** création puis consommée après — cf. **R5** pour la limite de concurrence sur le quota.
- **Suspension effective** : `login` lit la colonne `suspended` et refuse la connexion (`403`). **F7 (corrigée à la revue)** : `refresh` ne vérifiait pas la suspension — un compte suspendu pouvait régénérer indéfiniment des jetons d'accès via la rotation du refresh token, contournant totalement la suspension. Corrigé sur deux niveaux : (1) la suspension **révoque immédiatement** toutes les sessions du compte (`DELETE FROM sessions`), et (2) `refresh` rejette tout renouvellement pour un compte suspendu (et purge ses sessions). Le jeton d'accès déjà émis reste valide jusqu'à sa **TTL de 10 min** (fenêtre acceptée). Régression couverte par `suspension_revokes_token_renewal`.

## 12. S7 — Webhooks, recherche (FTS5), événements programmés

Cœur **webhooks** + **recherche** écrit et audité par le mainteneur (la recherche touche aux entrailles de la messagerie et au filtrage par permissions — points sensibles) ; module **événements** écrit par un **sous-agent** puis audité de façon adverse par le mainteneur. **Aucune faille exploitable.**

| Vecteur testé | Test (`security_s7.rs`) | Résultat |
|---|---|---|
| Membre sans `MANAGE_WEBHOOKS` crée/liste/modifie/supprime/régénère un webhook | `webhook_authorization` | `403` |
| Exécution d'un webhook avec **mauvais jeton** ou **id inconnu** | idem | `401` (réponse uniforme) |
| Création d'un webhook en **message privé** | idem | `403` |
| **Fuite inter-salons** en recherche : membre cherchant dans un salon où VIEW est refusé | `search_respects_permissions` | `total = 0` (propriétaire : `1`) |
| Recherche de guilde par un **non-membre** | idem | `403` |
| Recherche par salon sans `VIEW_CHANNEL` | idem | `403` |
| Membre sans `CREATE_EVENTS` crée un événement | `event_authorization_and_isolation` | `403` |
| Non-créateur sans `MANAGE_EVENTS` modifie/supprime l'événement d'autrui | idem | `403` |
| **Isolation inter-guildes** d'un événement (accès/gestion via une autre guilde) | idem | `404` |

Défenses :
- **Webhooks** (`routes_webhooks.rs`) : gestion gardée par `require_channel_perm(MANAGE_WEBHOOKS)` (honore les surcharges de salon) ; déplacement de salon revalidé sur la **cible** + **même guilde** (anti-IDOR inter-guildes) ; jeton secret de 256 bits (`URL_SAFE`), **jamais listé** (présent seulement à la création/régénération), régénérable (invalide l'ancien) ; exécution **non authentifiée par session** mais gardée par le jeton, avec réponse `401` **uniforme** (pas d'oracle d'existence), validation du contenu (1–4000) et du nom de remplacement (réservés `clyde`/`discord` rejetés). Les messages de webhook réutilisent l'insertion centralisée (déclencheurs FTS + `MESSAGE_CREATE` routé par portée).
- **Recherche** (`routes_messages.rs`) : `require_guild_member` (guilde) / `require_channel_perm` (salon, gère les MP) ; les résultats sont **strictement restreints aux salons que l'utilisateur peut lire** (`viewable_channel_ids` recalcule `VIEW_CHANNEL | READ_MESSAGE_HISTORY` par salon) ; le `channel_id` demandé est **intersecté** avec l'ensemble autorisé (pas de contournement). **Injection** : seuls des entiers validés (`parse_i64`) sont interpolés ; le texte utilisateur passe par une requête FTS5 **paramétrée** où chaque terme est mis entre guillemets (aucun opérateur FTS injectable).
- **Événements** (`routes_events.rs`, sous-agent) : `require_guild_member` (lecture/RSVP), `require_event_create` (création), `require_event_manage` (gestion : `MANAGE_EVENTS` ou créateur+`CREATE_EVENTS`) ; **toutes** les requêtes scopées `WHERE id = ? AND guild_id = ?` (pas d'IDOR inter-guildes) ; RSVP n'agit que sur la ligne de l'appelant ; validations (nom 1–100, type 1–3, salon ∈ guilde, lieu requis pour l'externe, fin > début, statut 1–4). Tout est paramétré.

**Risques résiduels notés** (faibles/négligeables, non bloquants) :
- **R6** (Moyen) : l'exécution de webhook est **non authentifiée** (jeton uniquement) et **sans rate-limit** → vecteur de spam/abus. À couvrir par la couche de rate-limit (cf. R1), avec un quota dédié par webhook.
- Comparaison de jeton de webhook non à temps constant (négligeable : secret de 256 bits, attaque temporelle réseau irréaliste).
- Oracle d'existence d'événement (`404` vs `403`) pour un non-membre sondant des identifiants (négligeable : Snowflakes 64 bits ; info interne à la guilde).
- `viewable_channel_ids` calcule les permissions salon par salon (N requêtes) — **performance**, pas sécurité ; à optimiser pour les grosses guildes.

## 13. S8 — Marqueurs de lecture & notifications

État de lecture (`last_read_id` + compteur de mentions), boîte de mentions, réglages de notification (niveau + mute). Écrit et audité par le mainteneur (couplage sensible à la messagerie et aux permissions). **Aucune faille exploitable.** Notifications natives OS / push : hors périmètre serveur (client/workers).

| Vecteur testé | Test (`security_s8.rs`) | Résultat |
|---|---|---|
| `ack` sur un salon non visible | `ack_requires_channel_view` | `403` |
| **Mention fantôme** : mentionner un utilisateur qui ne peut pas voir le salon | `mention_to_unauthorized_user_is_ignored` | aucun compteur, boîte vide |
| Régler les notifs d'une guilde dont on n'est pas membre / d'un salon non visible | `notification_settings_authorization` | `403` |
| Boîte de mentions après **perte d'accès** au salon | `inbox_drops_channels_after_access_loss` | mention masquée dynamiquement |

Défenses (`routes_notifications.rs` + `routes_messages.rs`) :
- **Isolation par utilisateur** : tous les états de lecture / réglages sont clés sur `user_id = @me` (lecture **et** écriture) — aucun accès ni écriture croisés possibles.
- **Gardes de portée** : `ack`/réglage de salon → `require_channel_perm(VIEW_CHANNEL)` ; `ack` de guilde / réglage de guilde → `require_guild_member` ; `ack` de guilde ne touche que les salons réellement visibles.
- **Anti-mention fantôme** : `process_mentions` n'enregistre une mention (et n'incrémente le compteur) que si la cible peut **voir** le salon (membre+`VIEW` en guilde, destinataire en MP), exclut l'auto-mention, et déduplique (compteur exact).
- **Boîte de mentions filtrée dynamiquement** : `mentions_inbox` recalcule `VIEW_CHANNEL | READ_MESSAGE_HISTORY` par salon **au moment de la lecture** et ignore les messages supprimés — une perte d'accès masque immédiatement les mentions passées.
- **Parseur de mentions** sûr (indices sur octets ASCII uniquement → pas de panique sur frontière de caractère) ; aucune interpolation de chaîne utilisateur en SQL.
- Note négligeable : `ack` ne vérifie pas que `message_id` appartient bien au salon (effet limité au seul badge non-lu de l'appelant ; pas de fuite ni d'impact tiers).

## 14. S9 — Émission d'événements Gateway (temps réel)

Diffusion des mutations de guilde (membres : join/kick/ban/timeout/rôles ; rôles : CRUD ; surcharges de salon ; événements programmés : CRUD) sur le bus Gateway via `AppState::publish`. **Aucune faille exploitable.** Le point sensible est la **confidentialité du routage** : un événement ne doit jamais atteindre quelqu'un qui n'y a pas droit.

| Vecteur testé | Test (`realtime.rs`) | Résultat |
|---|---|---|
| Les mutations émettent bien les événements attendus | `guild_mutations_emit_scoped_events` | `GUILD_MEMBER_ADD/REMOVE`, `GUILD_ROLE_CREATE`, `CHANNEL_CREATE`, `GUILD_BAN_ADD` présents |
| Portée correcte (rôle → guilde, salon → salon) | idem | `EventScope::Guild` / `EventScope::Channel` |
| **Confidentialité** : un non-membre reçoit-il les événements de la guilde ? | idem | `should_deliver` → **false** |

Défenses :
- **Séparation publication / livraison** : `publish` ne fait que déposer l'événement avec sa **portée** ; la décision « qui le reçoit » reste centralisée dans `gateway::should_deliver` (déjà audité en S5), recalculée par connexion. Aucun handler ne pousse directement vers une socket.
- **Portées prudentes** : mutations de membres/rôles/événements → `EventScope::Guild` (membres only) ; création/màj de salon → `EventScope::Channel` (réévalue `VIEW_CHANNEL`, donc respecte les salons privés) ; **suppression** de salon et **changement de surcharge** → `EventScope::Guild` (le calcul par salon n'est plus fiable une fois l'accès retiré/le salon supprimé — on notifie tous les membres, charge à eux de réconcilier ; aucune donnée sensible dans le payload, seulement des identifiants).
- **Payloads minimaux** pour les événements de gestion (identifiants seulement) — pas de fuite de contenu via un événement à portée large.
- `publish` est *fire-and-forget* (l'absence d'abonné n'est pas une erreur) : aucune mutation ne peut échouer à cause de la Gateway.

## 15. S10 — Profils & réglages

Édition de son profil (nom affiché, avatar, bio, pronoms, bannière, couleur d'accent), profil public, blob de réglages client privé. Écrit et audité par le mainteneur. **Aucune faille exploitable.**

| Vecteur testé | Test (`security_s10.rs`) | Résultat |
|---|---|---|
| Édition de profil / lecture des réglages sans jeton | `profile_edit_requires_auth` | `401` |
| **Fuite d'e-mail** via le profil public | `public_profile_never_leaks_email` | absent (champ inexistant + chaîne introuvable) |
| Profil d'un utilisateur inexistant | idem | `404` |
| **Isolation des réglages** entre utilisateurs | `settings_are_isolated_per_user` | chacun ne voit que les siens |

Défenses (`routes_users.rs`) :
- **Édition limitée à soi** : `PATCH /users/@me` n'écrit que `WHERE id = <session>` ; aucune route ne permet d'éditer le profil d'autrui.
- **E-mail jamais exposé** : le DTO `UserProfile` ne contient pas de champ e-mail (seul `GET /users/@me` — son propre compte — renvoie l'e-mail).
- **Réglages strictement par utilisateur** (lecture et écriture clés sur la session), **objet JSON obligatoire**, **taille plafonnée** (64 Ko).
- **Validations** : bio ≤ 190, pronoms ≤ 40, nom affiché ≤ 32, avatar/bannière ≤ 256, couleur ≤ `0xFFFFFF` ; longueurs comptées en caractères Unicode ; tout est paramétré (aucune injection).
- Choix de conception noté : le profil est **public au sein de l'instance** (tout compte authentifié peut le consulter) — cohérent avec le modèle de confiance mono-instance ; restriction « guilde commune / ami » possible ultérieurement.

## 16. S11 — Présence & statut

Registre de présence en mémoire (connexions actives suivies par le cycle de vie Gateway), statut désiré (`online`/`idle`/`dnd`/`invisible`) + statut personnalisé, diffusion `PRESENCE_UPDATE`. Écrit et audité par le mainteneur. **Aucune faille exploitable.**

| Vecteur testé | Test (`security_s11.rs`) | Résultat |
|---|---|---|
| Définir sa présence sans jeton / statut invalide / statut perso trop long | `presence_requires_auth_and_valid_status` | `401` / `400` / `400` |
| Lire les présences d'une guilde en **non-membre** | `presences_member_only_and_invisible_hidden` | `403` |
| Membre **invisible** visible par les autres ? | idem | masqué (absent des présences) |

Défenses (`presence.rs`, `routes_presence.rs`, `gateway.rs`) :
- **Confidentialité du routage** : `PRESENCE_UPDATE` est diffusé en portée `Guild` (membres des guildes partagées), filtré par `should_deliver` ; `GET /guilds/:id/presences` exige `require_guild_member`.
- **Invisible = hors ligne pour autrui** : le statut **effectif** renvoyé/diffusé est `offline` si l'utilisateur est invisible ou non connecté, et le **statut personnalisé est supprimé** dans ce cas (aucune fuite).
- **Édition limitée à soi** : `set_status` est clé sur la session ; aucun moyen de modifier la présence d'un tiers. Statut validé (liste blanche), statut perso ≤ 128.
- **Cycle de vie robuste** : toutes les sorties de la boucle Gateway (fermeture, erreur, échec d'envoi) passent par un nettoyage unique qui décrémente le compteur et repasse l'utilisateur hors ligne à la dernière déconnexion (comptage multi-sessions).
- Limite connue (non-sécurité) : le statut désiré est en mémoire (perdu à la déconnexion complète / au redémarrage) — persistance en base possible ultérieurement.

## 17. S12 — Événements temps réel des relations & MP

Complète la couche temps réel (S9) : demandes/suppressions d'amis et nouveaux MP poussés en direct, en portées `User`/`Dm`. Écrit et audité par le mainteneur. **Aucune faille exploitable.**

| Vecteur testé | Test (`realtime_social.rs`) | Résultat |
|---|---|---|
| Demande d'ami → cible notifiée en portée `User(cible)` | `friend_request_and_dm_are_scoped` | `RELATIONSHIP_ADD` (type `incoming`) |
| Confidentialité : seul l'intéressé reçoit l'événement `User` | idem | `should_deliver` vrai pour la cible, **faux** pour un tiers (et l'émetteur) |
| Nouveau MP → portée `Dm`, destinataires uniquement | idem | `CHANNEL_CREATE`, `should_deliver` faux pour un tiers |

Défenses :
- **Portée individuelle/MP** : `RELATIONSHIP_ADD`/`RELATIONSHIP_REMOVE` en `EventScope::User` (chaque partie ses propres sessions) ; `CHANNEL_CREATE`/`CHANNEL_RECIPIENT_*` en `EventScope::Dm` (destinataires actuels). Routage filtré par `should_deliver`.
- **Le blocage ne fuite pas** : bloquer un utilisateur ne lui envoie **aucun** événement (seules les sessions de l'auteur sont synchronisées) — conforme à Discord (on ne révèle pas un blocage).
- **Retrait de groupe** : l'utilisateur retiré est notifié via `User(cible)` (il n'est plus destinataire, donc hors portée `Dm`), sans fuite aux ex-membres au-delà du groupe.
- Payloads minimaux (identifiants + type de relation).

## 18. S13 — Gestion de guilde (récupération / renommage / suppression)

`GET`/`PATCH`/`DELETE /guilds/:id` + événements `GUILD_CREATE`/`UPDATE`/`DELETE`. Écrit et audité par le mainteneur. **Aucune faille exploitable.**

| Vecteur testé | Test (`security_s13.rs`) | Résultat |
|---|---|---|
| Lecture de guilde par un non-membre | `guild_management_authorization` | `403` |
| Édition par un membre sans `MANAGE_GUILD` | idem | `403` |
| Suppression par un membre (non-propriétaire) | idem | `403` |
| **`MANAGE_GUILD` peut renommer mais PAS supprimer** | `manage_guild_can_rename_but_not_delete` | renommage `200`, suppression `403` ; propriétaire `200` |

Défenses (`routes_chat.rs`) :
- **Gardes étagées** : lecture → `require_guild_member` ; édition → `require_guild_perm(MANAGE_GUILD)` ; **suppression → propriétaire strict** (`owner == session`, `MANAGE_GUILD` ne suffit pas).
- **Suppression atomique** : la cascade (réactions, mentions, états de lecture, messages [déclencheurs FTS], surcharges, réglages de notif, webhooks, intéressés/événements, salons, expressions, rôles, attributions, invitations, bannissements, audit, membres, guilde) s'exécute dans **une transaction** — tout ou rien ; chaque `DELETE` est paramétré (aucune interpolation de valeur utilisateur).
- **Diffusion** : `GUILD_CREATE` en portée `User` (créateur), `GUILD_UPDATE` en portée `Guild` (membres), `GUILD_DELETE` adressé à chaque **ancien** membre en portée `User` (la guilde n'existe plus → la portée `Guild` ne livrerait plus rien).
- Validation du nom (1–100), icône effaçable (chaîne vide → `NULL`).

## 19. S14 — Aperçu & révocation d'invitations

`GET /invites/:code` (aperçu sans rejoindre) et `DELETE /invites/:code` (révocation). Écrit et audité par le mainteneur. **Aucune faille exploitable.**

| Vecteur testé | Test (`invites.rs`) | Résultat |
|---|---|---|
| L'aperçu fait-il rejoindre la guilde ? | `preview_does_not_join` | non (lecture seule ; l'utilisateur reste non-membre) |
| Révoquer l'invitation **d'autrui** sans `MANAGE_GUILD` | `revoke_authorization` | `403` |
| Révoquer **sa propre** invitation | idem | `200` (créateur autorisé) |
| Rejoindre via une invitation révoquée | idem | `404` |

Défenses (`routes_guild.rs`) : l'aperçu est en **lecture seule** (n'insère aucun membre) et n'expose que des informations d'invitation (nom de guilde, nombre de membres, créateur) — précisément ce qu'une invitation est censée révéler ; la révocation exige d'être **le créateur de l'invitation** ou de détenir `MANAGE_GUILD` ; expiration vérifiée à l'aperçu. Requêtes paramétrées.

## 20. S15 — Quitter une guilde

`DELETE /guilds/:id/members/@me`. Écrit et audité par le mainteneur. **Aucune faille exploitable.** Test `leave_guild::member_can_leave_owner_cannot` : un membre quitte (`200`, n'est plus membre ensuite), le **propriétaire ne peut pas** quitter (`403` — il doit supprimer la guilde), un non-membre obtient `404`. Émet `GUILD_MEMBER_REMOVE` (portée guilde). N'agit que sur **sa propre** adhésion (route `@me`, jamais celle d'un tiers — le retrait d'autrui passe par `kick_member`, gardé par `KICK_MEMBERS` + hiérarchie). Note routage : `/members/@me` (statique) et `/members/:user_id` (paramètre) coexistent sans conflit (priorité au segment statique).

## 21. S16 — Signalisation vocale

États vocaux + rejoindre/déplacer/quitter, mute/deaf (soi + modération), `VOICE_STATE_UPDATE`/`VOICE_SERVER_UPDATE`, régions. **Le transport média (SFU/SRTP/ICE) et l'E2EE (DAVE/MLS) restent un sous-projet média séparé** (non implémenté ici). Écrit et audité par le mainteneur. **Aucune faille exploitable.**

| Vecteur testé | Test (`security_s16.rs`) | Résultat |
|---|---|---|
| Rejoindre/lister en **non-membre** | `join_requires_membership_and_connect` | `403` |
| Rejoindre sans `CONNECT` (refusé sur le salon) | idem | `403` |
| Mute serveur sans `MUTE_MEMBERS` | `moderation_requires_perms_and_protects_owner` | `403` (puis `200` une fois la permission accordée) |
| Modérer le **propriétaire** en vocal | idem | `403` |

Défenses (`routes_voice.rs`) :
- **Jonction gardée** : `require_guild_member` + salon de type vocal/stage **de la guilde** + `require_channel_perm(VIEW_CHANNEL | CONNECT)` (respecte les overrides de salon, ex. salon vocal privé).
- **Le jeton média ne fuite pas** : `VOICE_SERVER_UPDATE` (qui porte le **jeton** signé + l'endpoint) est diffusé **uniquement à l'intéressé** (`EventScope::User`) ; les autres ne reçoivent que `VOICE_STATE_UPDATE` (portée guilde, sans jeton).
- **Actions sur soi** : mise à jour d'indicateurs / départ ne touchent que sa propre ligne (`user_id = session`).
- **Modération étagée** : `MUTE_MEMBERS` / `DEAFEN_MEMBERS` / `MOVE_MEMBERS` selon l'action ; cible obligatoirement connectée au vocal de la guilde ; **propriétaire protégé** ; déplacement borné aux salons vocaux de la même guilde (anti-IDOR inter-guildes).
- **Nettoyage** : la fermeture de la session Gateway libère l'état vocal (pas d'état fantôme). Jeton vocal = JWT signé `kind="voice"`, TTL 1 h, à vérifier par le futur SFU. Requêtes paramétrées.

## 22. S17 — Nœud média SFU (fondation WebRTC)

Crate **séparée** `ozone-sfu` (binaire média) : pile WebRTC (`webrtc-rs`), cœur SFU (salles, pairs, offre/réponse, relais de pistes RTP), signalisation HTTP. **Le média n'est pas encore exposé.** Audit de fondation par le mainteneur.

- **Isolation crypto** : `ring`/`rustls` (tirés par WebRTC) sont **confinés à `ozone-sfu`** ; `ozone-api` reste sans `ring` (vérifié : seul `ozone-sfu` compile `ring`). Le risque d'une CVE `ring` est cantonné au binaire média, optionnel et séparable.
- **Validation d'entrée** : l'offre SDP est parsée/validée par la pile WebRTC ; une offre invalide → `400`. Pas d'interpolation de chaîne.
- **R7 (noté, bloquant avant exposition)** : le SFU **ne vérifie pas encore le jeton vocal** de l'API → le plan média est **non authentifié** (cf. tableau §3 et `ozone-sfu/README.md`). Sans impact tant que le SFU n'est pas déployé/exposé ; **à brancher impérativement** (vérification du jeton `VOICE_SERVER_UPDATE` + correspondance du salon) avant toute mise en service.
- Test unitaire : construction du SFU (MediaEngine + intercepteurs) et registre de salles ; le chemin média complet (RTP/ICE/DTLS) est un test **E2E manuel** (deux clients WebRTC réels).

> Rappel : `VOICE_SERVER_UPDATE` (qui porte le jeton) est déjà diffusé en **portée `User`** par l'API (S16) — le jeton ne fuite pas aux autres. Reste à ce que le SFU le **consomme et le vérifie**.

---
*Document vivant — revue effectuée pour S1 → S17 ; à reconduire à chaque couche. À compléter par : **authz du plan média (R7, bloquant avant exposition)**, renégociation WS (mesh N-à-N), E2EE DAVE/MLS et son audit, rate-limiting (R1/R6), RESUME Gateway + persistance du statut, fuzzing du parseur gateway, tests de charge, et consommation transactionnelle des invitations (R5).*
