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
| **R7** | ~~Élevée~~ **Résolu (S18)** | ~~SFU sans authz~~ : le SFU vérifie désormais le jeton vocal (signature HS256 + `kind` + salon) et la propriété du pair au départ, **fail-closed** sans `OZONE_VOICE_SECRET`. | Cf. §23 + `crates/ozone-sfu/tests/auth.rs`. |

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
cargo test -p ozone-sfu --test auth          # S18 — authz du plan média (SFU)
cargo test -p ozone-api --test discovery     # S20 — découverte de guildes publiques
cargo test -p ozone-api --test polls         # S21 — sondages
cargo test -p ozone-api --test attachments   # S22 — pièces jointes
cargo test -p ozone-api --test threads       # S23 — fils + héritage de permissions
cargo test -p ozone-api --test account       # S24 — mot de passe & e-mail
cargo test -p ozone-api --test account_delete # S25 — suppression de compte
cargo test -p ozone-api                       # API : suite complète (112 tests)
cargo test --workspace                        # API + SFU + proto (119 tests)
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

> Rappel : `VOICE_SERVER_UPDATE` (qui porte le jeton) est diffusé en **portée `User`** par l'API (S16) — le jeton ne fuite pas aux autres.

## 23. S18 — Authentification du plan média (résout R7)

Le SFU **vérifie désormais le jeton vocal** avant toute opération. Audité par le mainteneur. **R7 résolu.**

| Vecteur testé | Test (`ozone-sfu/tests/auth.rs`) | Résultat |
|---|---|---|
| SFU sans secret configuré | `fail_closed_without_secret` | `503` (**fail-closed**) |
| Jeton absent/altéré/signé avec un autre secret | `rejects_invalid_token` | `401` |
| Jeton valide mais **salon ≠ celui du jeton** | `rejects_wrong_room` | `403` |
| Jeton valide + salon correct | `valid_token_passes_auth_then_sfu_rejects_bad_sdp` | auth franchie (SDP invalide → `400`) |
| Départ : jeton requis + **propriété** du pair | `leave_requires_token_and_ownership` | `503`/`401`/`403` |

Conception :
- **Primitives JWT partagées** : `ozone_proto::token` (HS256 pur Rust, **sans `ring`**) factorise l'émission (API) et la vérification (SFU) — une seule implémentation, testée (`round_trip_and_tamper`).
- **Jeton vocal** : `sub = "<user_id>.<channel_id>"`, `kind = "voice"`, TTL 1 h, signé avec un **secret partagé** (`OZONE_VOICE_SECRET`, repli sur le secret JWT de l'instance côté API). Le SFU recharge ce secret depuis l'environnement.
- **Contrôle du salon** : le SFU exige que le `:room` de l'URL == `channel_id` du jeton (anti-rejeu inter-salons).
- **Départ authentifié** : les `peer_id` étant séquentiels (devinables), `DELETE` exige le jeton **et** que l'`user_id` corresponde au **propriétaire** du pair (anti-déconnexion d'autrui).
- **Fail-closed** : sans `OZONE_VOICE_SECRET`, le SFU refuse toute connexion (`503`) — pas de mode « ouvert » accidentel.
- **Isolation crypto maintenue** : `ozone-proto` n'introduit que `hmac`/`sha2`/`base64` (pas de `ring`) ; `ring` reste confiné à `ozone-sfu` (via WebRTC).

## 24. S20 — Annuaire de découverte (guildes publiques)

`GET /discovery/guilds` (liste opt-in) + `POST /discovery/guilds/:id/join` (adhésion directe). Champs `description`/`discoverable` ajoutés aux guildes. Écrit et audité par le mainteneur. **Aucune faille exploitable.** (S19 — déploiement du SFU — sans surface applicative nouvelle.)

| Vecteur testé | Test (`discovery.rs`) | Résultat |
|---|---|---|
| Guilde non publique listée / révélée | `private_guilds_and_bans` | non listée ; adhésion → `404` (existence non révélée) |
| Inscrire une guilde à la découverte sans `MANAGE_GUILD` | idem | `403` |
| Adhérer sans jeton | idem | `401` |
| **Banni** rejoignant une guilde publique | idem | `403` |
| Listing + adhésion directe (opt-in) | `listing_and_direct_join` | `200`, membre ensuite |

Défenses (`routes_discovery.rs`) : l'annuaire n'expose **que** les guildes `discoverable = 1` (opt-in explicite, basculé via `MANAGE_GUILD`) ; l'adhésion vérifie la découvrabilité (sinon `404`, sans révéler l'existence d'une guilde privée) **et** le bannissement ; recherche `LIKE` **paramétrée** (pas d'injection) ; `GUILD_MEMBER_ADD` émis en portée guilde uniquement à l'insertion effective.

## 25. S21 — Sondages

`POST /channels/:id/polls`, `GET …/polls/:mid`, `PUT …/polls/:mid/votes`. Un sondage est porté par un message du salon. Écrit et audité par le mainteneur. **Aucune faille exploitable.**

| Vecteur testé | Test (`polls.rs`) | Résultat |
|---|---|---|
| Créer / lire / voter en non-membre | `poll_requires_channel_access` | `403` |
| Voter une réponse inexistante | `multiselect_and_validation` | `400` |
| Mono-réponse : voter pour plusieurs | `create_vote_results` | `400` |
| Décomptes corrects + changement de vote | idem | recomptés |

Défenses (`routes_polls.rs`) : création gardée par `SEND_MESSAGES`, lecture/vote par `VIEW_CHANNEL` (+ `READ_MESSAGE_HISTORY` en lecture) ; le sondage est **scopé au salon** (`message_id` **et** `channel_id` — pas de vote inter-salons) ; les `answer_ids` sont **validés** contre les réponses du sondage, dédupliqués, et bornés à 1 hors multi-sélection ; vote refusé après expiration ; le vote **remplace** les votes existants de l'utilisateur (pas de bourrage) ; validations de création (question ≤ 300, 1–10 réponses ≤ 55). Tout est paramétré.

## 26. S22 — Pièces jointes

Téléversement (`POST /channels/:id/attachments`, multipart) + téléchargement (`GET /attachments/:id/:filename`) + liaison aux messages. Stockage fichier local (un fichier par identifiant). Écrit et audité par le mainteneur.

| Vecteur testé | Test (`attachments.rs`) | Résultat |
|---|---|---|
| Téléverser en non-membre | `attachment_permissions_and_ownership` | `403` (`ATTACH_FILES` requis) |
| Télécharger sans accès au salon / sans jeton | idem | `403` / `401` |
| Attacher la pièce jointe **d'autrui** | idem | non liée (silencieux) |
| Cycle téléverser → attacher (msg vide ok) → télécharger | `upload_attach_and_download` | contenu + type corrects |

Défenses (`routes_attachments.rs`) :
- **Téléversement** gardé par `VIEW_CHANNEL | SEND_MESSAGES | ATTACH_FILES` ; taille **plafonnée à 25 Mo** (limite de corps appliquée **uniquement** à cette route, pas globalement).
- **Pas de traversée de chemin** : le fichier est nommé par **identifiant** sur disque ; le nom fourni n'entre jamais dans un chemin (juste affiché, assaini pour `Content-Disposition`).
- **Liaison sûre** : à l'envoi d'un message, on ne lie que les pièces **du même auteur, du même salon, encore en attente** (`UPDATE … WHERE uploader_id = ? AND channel_id = ? AND message_id IS NULL`) — pas de vol de pièce jointe.
- **Téléchargement non public** : gardé par `VIEW_CHANNEL` du salon de la pièce jointe (auth requise) — pas d'URL anonyme. *(Amélioration future : URLs signées/CDN.)*
- **F8 (trouvée et corrigée à la revue)** : servir un contenu téléversé avec un `Content-Type` **contrôlé par l'attaquant** en `inline` permettait un **XSS stocké** (HTML/JS téléversé exécuté dans l'origine de l'instance). Corrigé : `X-Content-Type-Options: nosniff`, `Content-Security-Policy: default-src 'none'; sandbox`, et `Content-Disposition: attachment` (téléchargement forcé) pour tout type **hors** médias sûrs (`image/`/`audio/`/`video/`/`text/plain`).

## 27. S23 — Fils (threads)

`POST`/`GET /channels/:id/threads`. Un fil est un salon (type 11) sous un salon texte/annonces ; ses messages réutilisent les routes de messagerie existantes. Le point sensible — **l'héritage des permissions** — a nécessité une modification du **cœur des permissions**, vérifiée non-régressive (les 106 tests préexistants passent toujours). Audité par le mainteneur. **Aucune faille exploitable.**

| Vecteur testé | Test (`threads.rs`) | Résultat |
|---|---|---|
| Fil **public** : un membre peut y poster | `thread_inherits_parent_permissions` | `200` |
| Fil sous un salon **privé** : non-visible/non-écrivable par un non-autorisé | idem | lecture `403`, écriture `403` |
| Lister les fils d'un salon non visible | idem | `403` |
| Fil sous un salon vocal | `create_post_list_thread` | `400` |

Défenses :
- **Héritage des surcharges** (`permissions::channel_permissions`) : pour un fil (type 11/12), les permissions effectives sont calculées contre **les surcharges du salon parent** — un fil sous un salon privé reste privé partout (REST **et** routage Gateway, qui utilisent la même fonction). Changement **rétro-compatible** : pour un salon normal, le comportement est inchangé (vérifié par toute la suite préexistante).
- **Création** gardée par `VIEW_CHANNEL | CREATE_PUBLIC_THREADS` sur le **parent**, restreinte aux salons texte/annonces d'une guilde ; nom validé (1–100). Liste filtrée par visibilité effective de chaque fil.

## 28. S24 — Gestion de compte (mot de passe & e-mail)

`PATCH /users/@me/password` et `PATCH /users/@me/email`. Écrit et audité par le mainteneur. **Aucune faille exploitable.**

| Vecteur testé | Test (`account.rs`) | Résultat |
|---|---|---|
| Changer le mot de passe avec un **mauvais** mot de passe actuel | `change_password_revokes_sessions` | `401` |
| Nouveau mot de passe trop court | idem | `400` |
| Après changement : **sessions révoquées** (ancien refresh) | idem | `401` ; ancien mot de passe refusé à la connexion |
| Changer l'e-mail sans jeton / mauvais mot de passe / e-mail déjà pris | `change_email_flow` | `401` / `401` / `409` |

Défenses (`routes_auth.rs`) :
- **Ré-authentification obligatoire** : les deux opérations exigent le **mot de passe actuel** (vérifié Argon2id) — un jeton volé ne suffit pas à détourner le compte.
- **Révocation des sessions** au changement de mot de passe : `DELETE FROM sessions WHERE user_id = ?` invalide tous les refresh tokens (reconnexion requise) — coupe un attaquant éventuel.
- **E-mail** : validé (`@`, ≤ 254), **unicité** vérifiée (hors soi) → `409` sinon. Nouveau mot de passe ≥ 8. Requêtes paramétrées.

## 29. S25 — Suppression de compte (anonymisation)

`DELETE /users/@me`. Écrit et audité par le mainteneur. **Aucune faille exploitable.**

| Vecteur testé | Test (`account_delete.rs`) | Résultat |
|---|---|---|
| Supprimer avec un mauvais mot de passe | `delete_account_anonymizes_and_keeps_messages` | `401` |
| Après suppression : connexion impossible | idem | `401` |
| Messages conservés mais **anonymisés** (auteur « deleted_… ») | idem | oui ; plus membre des guildes |
| Supprimer en **possédant une guilde** | `cannot_delete_while_owning_guild` | `400` |

Défenses (`routes_auth.rs`) :
- **Ré-authentification** par mot de passe ; refus si l'utilisateur **possède encore une guilde** (anti-orphelinage — il doit la supprimer/transférer).
- **Anonymisation transactionnelle** : suppression de toutes les données personnelles (sessions, relations, notes, états de lecture, réglages, rôles d'instance, états vocaux, mentions, destinataires MP, rôles de membre, adhésions, votes, réactions, pièces jointes en attente) ; la ligne `users` est **conservée** mais vidée (pseudo/e-mail → `deleted_<id>`, champs de profil → NULL, mot de passe rendu inutilisable) pour que **les messages restent attribués** à un « utilisateur supprimé » (jointure intacte).
- **Connexion bloquée** : `login` rejette un compte `deleted` (et le hash inutilisable échoue de toute façon). Cohérent avec la fenêtre de jeton d'accès de 10 min (sessions révoquées).

## 30. S26–S28 — Cœur client (`ozone-core` : `ApiClient`, Gateway, Store)

Première frontière de confiance **côté client**. Contrairement au serveur, `ring` y est **accepté** (`reqwest`/`rustls`, future pile WebRTC) — la contrainte « zéro `ring` » ne vise que `ozone-api`. Écrit et audité par le mainteneur. **Aucune faille exploitable.**

| Vecteur examiné | Couche | Posture |
|---|---|---|
| Identifiants/jetons en clair sur le réseau | `ApiClient` / Gateway | `InstanceRef::api_base()` **force `https://`** si aucun schéma fourni ; la Gateway dérive `wss://` de `https://` (`ws_url`). Pas de transport en clair par défaut. |
| Trame Gateway malformée → panique/DoS | `store::apply` | `serde_json::from_value` en échec ⇒ **`false`** (jamais de `unwrap`/panique) ; `id_field` parse via `.ok()`. Trames hors-`DISPATCH`/inconnues ignorées. |
| Ré-dérivation des permissions côté client | `store` | **Volontairement absente** : le store fait confiance au filtrage **serveur** (`should_deliver`) — une connexion ne reçoit que les événements auxquels elle a droit. Refaire le calcul de droits côté client serait à la fois faux (le serveur est l'autorité) et une surface de contournement. |
| XSS via contenu de message | `store` | Aucun rendu dans cette couche : le contenu est stocké en **chaînes inertes**. La défense XSS relève de l'UI (à venir) ; côté serveur, les téléversements sont déjà `nosniff` + CSP (cf. §26/F8). |
| `unsafe` / SQL / FS / exécution | `store` | Aucun : machine à états purement en mémoire, sans `unsafe`, sans I/O. |

**Point suivi (R8, faible) — croissance mémoire non bornée côté client.** `apply(MESSAGE_CREATE)` empile sans limite dans `messages` (de même `guilds`/`channels`/`presences` ne sont jamais purgés). Un serveur compromis ou un flot d'événements pourrait faire croître la mémoire du client (DoS **côté client**). Acceptable sous le modèle de confiance actuel (l'instance est le serveur **choisi/auto-hébergé** par l'utilisateur) et **renvoyé à la couche cache SQLite** (prochaine tranche), qui portera la politique d'éviction/rétention. Aucune donnée d'autrui n'est exposée par ce point.

| Risque | Sévérité | État |
|---|---|---|
| R8 — croissance mémoire non bornée du `Store` client | Faible | **Résolu (S29)** — plafond mémoire + rétention disque |

## 31. S29 — Cache local SQLite du client (`ozone-core::cache`)

Persistance locale du `Store` (démarrage hors-ligne, historique). **Même `sqlx`/SQLite sans TLS** que le serveur ⇒ pas de second binding natif ; invariant « zéro `ring` » du serveur **inchangé** (vérifié : `cargo tree -p ozone-api -i ring` → aucun paquet ; `ring` reste confiné au client et au SFU). Écrit et audité par le mainteneur. **Aucune faille exploitable.**

| Vecteur examiné | Posture |
|---|---|
| Injection SQL | Toutes les requêtes sont **paramétrées** (`.bind`) ; le seul SQL littéral est le `SCHEMA` constant (aucune entrée utilisateur concaténée). |
| DTO obsolète / blob corrompu → panique | Hydratation via `serde_json::from_str(...).ok()` : toute ligne **illisible est ignorée** (jamais de `unwrap`/panique) — même posture *fail-safe* que `Store::apply`. |
| Contenu hostile stocké | Blobs **JSON inertes** ; aucune exécution, pas de rendu (XSS relève de l'UI). |
| Traversée de chemin | Le chemin du cache est **choisi par l'application** (fichier local de l'utilisateur), pas par un attaquant distant ; les identifiants ne touchent jamais un chemin de fichier. |
| Ré-dérivation de droits | Aucune : le cache **miroir** se fie au filtrage serveur, comme le `Store`. |

**R8 — résolu.** Double borne contre la croissance non maîtrisée (DoS côté client / disque) :
- **Mémoire** (`Store`) : plafond par salon (`DEFAULT_MESSAGE_CAP = 1000`, configurable `with_message_cap`) appliqué sur `MESSAGE_CREATE` **et** `set_messages` (éviction des plus anciens).
- **Disque** (`Cache`) : plafond par salon (`DEFAULT_DISK_CAP = 2000`, configurable `set_disk_cap`) appliqué après chaque `MESSAGE_CREATE` persisté et à `replace_channel_messages`, plus `prune_channel_messages` explicite. Tests : rétention conserve bien les **plus récents**, suppression de salon purge les messages, round-trip persiste/réhydrate à la réouverture.

## 32. S30 — Orchestrateur de session client (`ozone-core::session`)

Colle de haut niveau au-dessus de couches **déjà auditées** (`ApiClient`/Gateway §30, `Store` §30, `Cache` §31). N'introduit **aucune** nouvelle surface réseau/SQL ni logique de droits. Écrit et audité par le mainteneur. **Aucune faille exploitable.**

| Vecteur examiné | Posture |
|---|---|
| Jetons au repos | Les jetons (accès/refresh) restent **en mémoire** (`Session` + `InstanceRef`) ; le **cache SQLite ne stocke pas de jeton** (uniquement guildes/salons/messages/présences). Le stockage chiffré au repos (trousseau OS) relèvera du **registre d'instances**/UI — exposé via `access_token()`/`refresh_token()` pour cette future couche. |
| Boucle d'événements résiliente | `poll_event` applique au `Store` puis persiste au cache en **best-effort** (erreur de cache ignorée) : un incident d'écriture ne peut **pas** bloquer/planter la boucle UI. |
| Connexion sans authentification | `connect_gateway` échoue **proprement** (`Err`, pas de panique) sans jeton ; `poll_event` sans Gateway renvoie `None` (testé). |
| Ré-dérivation de droits | Aucune : la session se fie au filtrage **serveur**, comme `Store`/`Cache`. |
| Réconciliation hors-ligne→ligne | `hydrate_from_cache` (instantané local) puis `bootstrap` (REST) puis flux Gateway : aucune donnée d'autrui exposée (le bootstrap n'obtient que ce que l'API autorise pour le jeton). |

*Suivi (faible, futur) : stockage chiffré des jetons au repos côté registre d'instances ; rotation proactive via `refresh_session` avant expiration.*

## 33. S31 — RESUME Gateway (reprise sans perte)

Reconnexion après coupure **sans re-IDENTIFY ni perte d'événement** : chaque session est un **acteur** (`gateway_session.rs`) qui possède son abonnement au bus, filtre via `should_deliver`, numérote et **bufferise** les événements, et **survit à la coupure du socket** pendant une fenêtre de grâce. Refonte du cœur Gateway — **non-régressive** (les 112 tests `ozone-api` passent toujours). Écrit et audité par le mainteneur. **Aucune faille exploitable.**

| Vecteur testé / examiné | Test | Résultat / défense |
|---|---|---|
| Reprise par **un autre utilisateur** (vol de session ⇒ fuite d'événements) | `resume_rejects_another_users_session` | `INVALID_SESSION` : `resume_session` vérifie `meta.user_id == uid`. **La sécurité ne repose pas sur le secret du `session_id`** (snowflake) mais sur ce contrôle d'appartenance. |
| RESUME d'une **session inconnue/expirée** | `resume_unknown_session_is_refused` | `INVALID_SESSION` ⇒ le client repart sur un IDENTIFY propre. |
| RESUME sans/avec mauvais **jeton** | (même chemin `verify_token`) | `INVALID_SESSION` : RESUME exige un jeton d'accès valide, comme IDENTIFY. |
| **Perte silencieuse** d'un événement manqué | `session_resume_replays_events_missed_during_outage` | L'acteur bufferise pendant la coupure ; le rejeu renvoie tout `s > seq`. Si le tampon a évincé un événement nécessaire (`after_seq < evicted_through`) ⇒ refus (`INVALID_SESSION`), **jamais de trou masqué**. |
| Événement reçu mais **non traité** au moment de la coupure | (conception client) | `last_seq` n'avance qu'à la **consommation** (`next_event`), pas à la réception ⇒ un RESUME reprend exactement après le dernier événement réellement appliqué. |
| Croissance mémoire (DoS) | — | Tampon **borné** `BUFFER_CAP = 512`/session (éviction des plus anciens). |
| Fuite via permissions changées | — | `should_deliver` est évalué **à la livraison** : le rejeu ne renvoie que des trames déjà autorisées au moment du buffer ; les événements postérieurs à une perte de droit sont filtrés normalement (livraison continue). Pas de nouvelle fuite. |

**Présence durcie au passage** : la présence est désormais liée au **cycle de vie de l'acteur** (et non du socket brut) ⇒ une micro-coupure suivie d'un RESUME **ne fait plus clignoter** le statut en/hors ligne ; le passage hors ligne + nettoyage vocal n'a lieu qu'à l'expiration de la grâce (60 s).

**Reconnexion auto** (`Session::poll_event_resilient`) : pure orchestration de primitives **déjà auditées** (`connect`/`connect_resume`/`refresh_session`) — aucune nouvelle surface. Boucle de reconnexion **bornée** (`MAX_ATTEMPTS = 5`) avec back-off, pour ne pas marteler un serveur injoignable. Testé (coupure → auto-reprise → livraison de l'événement manqué).

| Risque | Sévérité | État |
|---|---|---|
| R9 — accumulation d'acteurs de session pendant la grâce (connexions/déconnexions rapides) | Faible | Suivi → plafond de sessions par utilisateur + rate-limit IDENTIFY (futur) |

## 34. S32 — UI native Iced (fondation client)

Premier écran (connexion à une instance) + vues guildes/salons/messages, en architecture **Elm**, branchées sur `ozone_core::ApiClient` (déjà audité §30). Écrit et audité par le mainteneur. **Aucune faille exploitable.** *(Validation visuelle/interactions hors portée headless : se fait en exécutant le binaire.)*

| Vecteur examiné | Posture |
|---|---|
| Injection via contenu de message (équivalent XSS) | **N/A par construction** : le rendu est du **texte brut** (widget `text`, glyphes GPU) — aucun markup/HTML/script n'est interprété. Un message hostile s'affiche **inerte**. |
| Transport en clair | L'adresse saisie passe par `InstanceRef::api_base` ⇒ **HTTPS forcé** sauf `http://` explicite. Le défaut `http://127.0.0.1:8080` est une **commodité de dev** (à durcir pour une distribution : défaut vide/HTTPS). |
| Jetons / mot de passe en mémoire | Jeton d'accès en mémoire (`ApiClient`), **non persisté** par l'UI ; le **mot de passe est effacé** après connexion réussie. *(Futurs durcissements : type `zeroize`, stockage chiffré au repos via le registre d'instances.)* |
| Surface réseau / SQL / FS / `unsafe` | Aucune nouvelle : les `Task` réutilisent `ApiClient` ; pas de SQL/FS/`unsafe` dans l'UI. |
| Fuite d'erreurs | La barre de statut affiche les erreurs de l'API pour **la session de l'utilisateur lui-même** — pas de fuite tierce. |

Tests : 5 unitaires du réducteur `update` (validation du formulaire, transitions d'écran, sélections guilde/salon, retour à l'écran de connexion sur échec) — exécutables **sans fenêtre** (les `Task` async ne tournent pas hors runtime Iced).

## 35. S33 — Multi-instances + porte d'accès (cœur client)

`ApiClient` gagne le **gate d'instance** (jeton de gate joint à l'inscription/connexion) et `ozone-core` un **registre multi-instances**. Aucune nouvelle surface **serveur** (les routes `/instance/gate` + champ `gate_token` existaient déjà, audités). Écrit et audité par le mainteneur. **Aucune faille exploitable.**

| Vecteur testé / examiné | Test | Résultat / défense |
|---|---|---|
| **Jetons au repos** (registre persisté) | `persist_excludes_tokens_and_roundtrips` | `to_persisted` ne sérialise **que** l'adresse/nom/id ; le JSON **ne contient aucun jeton** (assertion explicite). Au rechargement, les instances sont **non authentifiées** (reconnexion requise) — conforme à la posture « pas de secret en clair au repos » (§32/§34). |
| Porte d'instance contournée | `gate_required_blocks_until_password_passed` | Sans jeton de gate, inscription/connexion **refusées** ; mauvais mot de passe d'instance ⇒ refus ; le bon mot de passe ⇒ jeton court (600 s) ⇒ accès. La vérification reste **côté serveur** (`check_gate`). |
| Mot de passe d'instance en transit | (conception) | Envoyé à `/instance/gate` via `api_base` ⇒ **HTTPS forcé** sauf `http://` explicite (dev). |
| Doublons d'instances | `add_dedups_by_address_and_selects_latest` | Déduplication par `api_base` normalisée ; un ré-ajout sans jeton **n'efface pas** les jetons en mémoire (`readd_preserves_existing_tokens`). |

*Rappel d'intégration UI : lors du câblage de la persistance, n'écrire sur disque que `to_persisted()` (jamais `InstanceRef` complet).*

## 36. S34 — UI multi-instances + porte d'accès

Restructuration de l'UI (rail d'instances, flux d'ajout avec gate) au-dessus du cœur **déjà audité** (§30, §35). Écrit et audité par le mainteneur. **Aucune faille exploitable.** *(Validation visuelle = exécuter le binaire.)*

| Vecteur examiné | Test | Posture |
|---|---|---|
| Isolation inter-instances (jeton/données) | `stale_guilds_result_is_ignored_after_switch` | Chaque `Instance` a **son propre `ApiClient`** (et donc son jeton) ; chaque `Task` clone l'`ApiClient` de l'instance ciblée. Les résultats async sont **étiquetés par index** et **ignorés s'ils ne concernent plus l'instance courante** ⇒ pas de fuite de données d'une instance sous une autre. |
| Secrets en mémoire | `authenticated_marks_instance_and_opens_main` | Mot de passe **et** mot de passe d'instance **effacés** après authentification réussie. |
| Gate contournée | `validates_auth_form_with_optional_gate` | Si l'instance est gardée, le formulaire **exige** le mot de passe d'instance ; le gate est de toute façon vérifié **côté serveur** (§35). |
| Jetons au repos | (conception) | Les instances vivent **en mémoire** ; aucune persistance de jeton (la persistance éventuelle n'écrira que `to_persisted()`, sans jeton — §35). |
| Exécution de contenu | (conception) | Rendu **texte brut** (cf. §34) — aucune exécution. |

## 37. S35 — Temps réel dans l'UI (abonnement Gateway)

Un abonnement Iced ouvre la Gateway de l'instance courante et pousse ses événements dans le fil. Réutilise des couches **déjà auditées** (Gateway/RESUME §31/§33). Écrit et audité par le mainteneur. **Aucune faille exploitable.** *(Validation visuelle = exécuter le binaire.)*

| Vecteur examiné | Test | Posture |
|---|---|---|
| Portée d'application des events | `gateway_events_update_current_channel_feed` | `apply_gateway` n'applique qu'au **salon couramment ouvert** et **déduplique par id** (l'écho d'un envoi optimiste ne crée pas de doublon ; `MESSAGE_UPDATE` met à jour en place ; `MESSAGE_DELETE` retire). Les events d'un autre salon sont ignorés. |
| Filtrage des droits | (serveur) | L'UI **fait confiance au filtrage serveur** (`should_deliver`) : elle ne reçoit que les events autorisés ; aucune ré-dérivation de droits côté client (cf. §30). |
| Jeton dans l'abonnement | (conception) | Le jeton (de l'utilisateur lui-même) est passé **en mémoire** au flux ; l'`id` d'abonnement `(adresse, jeton)` est interne à Iced (identité d'abonnement), non journalisé. Re-clé à chaque (ré)auth ⇒ cycle de vie propre. |
| Reconnexion | (réutilise §31/§33) | Le flux reconnecte via **RESUME** (rejeu sans perte) puis repli en connexion complète ; isolation de session garantie côté serveur (§33). |
| Trame malformée / exécution | (conception) | `serde_json::from_value` en échec ⇒ ignoré (pas de panique) ; rendu **texte brut** (pas d'exécution, cf. §34). |

## 38. S36 — Thèmes & palettes (UI)

État **purement local** : `ThemeChoice` (palettes de marque « Ozone » + thèmes intégrés) + bascule. **Aucune surface de sécurité** — pas de réseau, d'I/O, de secret ni de désérialisation ; seules des couleurs/constantes. **Aucune faille exploitable.** Tests : cycle des thèmes (réducteur + module), libellés non vides, `to_theme()` sans panique.

## 39. S37 — Complétion des bindings client (toutes les routes **non vocales**)

`ApiClient` couvre désormais **toute** la surface REST non-vocale du serveur : ~70 méthodes réparties en 15 modules `client_*` (relations, MP, rôles & overwrites, membres & modération, invitations, actions sur messages, recherche, marqueurs de lecture/notifications, présence/profil/compte, expressions emoji/sticker/soundboard, webhooks, événements programmés, sondages, découverte, administration d'instance) — en plus du CRUD guildes/salons/fils. Implémenté **en parallèle par sous-agents** sur le patron `client_guild`, **spine + revue cyber par le mainteneur**. **Aucune faille exploitable.**

**Posture commune (vérifiée module par module) :**
- **Pass-through authentifié, autorité serveur.** Chaque méthode traverse `auth()` (bearer attaché) puis `send_json`/`send_unit` (tout statut non-2xx ⇒ `Err` — jamais de succès silencieux). **Aucune ré-dérivation de droits côté client** : l'autorisation reste **entièrement** côté serveur (permissions de guilde, `require_instance_admin`/`require_instance_owner`, bans, gate de salon…). Les endpoints privilégiés (admin d'instance, modération) ne sont que relayés ; un appelant non-admin reçoit **403** (testé).
- **Anti-injection de chemin.** Les identifiants sont des `Snowflake` ⇒ `Display` = **chiffres uniquement** (pas de `/`, `?`, `#`, `..`). Les **segments texte** (réaction `:emoji`, code d'invite, jeton de webhook) et la **requête de recherche** `?q=` sont **percent-encodés** (liste blanche `A-Za-z0-9-_.~`) ⇒ `/`, `=`, `&`, `#`, espace ne peuvent s'échapper de leur segment/paramètre. Tests de régression d'injection dans `messaging` et `search` (la chaîne `licorne&author_id=1` reste confinée à `q`).
- **Exécution de webhook** : volontairement **non authentifiée** (le jeton dans le chemin **est** la crédentielle) ; le binding n'envoie **aucun bearer** sur ce chemin (pas de fuite du jeton de session) ; comparaison de jeton à temps constant côté serveur, erreur uniforme « inconnu vs jeton invalide ».
- **Secrets.** Aucune persistance de jeton. Changement de mot de passe/e-mail et suppression de compte **ré-authentifient** côté serveur (le binding ne fait que transmettre) ; aucun secret journalisé ; les corps d'erreur sont les messages génériques du serveur (pas d'écho de jeton).
- **Invariant serveur intact.** Additions **client uniquement** : `ozone-api` inchangé ⇒ **zéro `ring`** côté serveur préservé.

Tests : **16 fichiers E2E** contre une vraie instance `ozone-api` (harnais partagé `tests/common`), **43 tests `ozone-core`** au total, tous verts ; `clippy -D warnings` propre. *(Restant hors périmètre de cette tranche : pièces jointes en multipart — tranche suivante ; et le câblage de ces capacités dans l'UI.)*

## 40. S38 — Pièces jointes client (téléversement multipart + téléchargement)

`client_attachments` complète la couverture : `upload_attachment` (form multipart, champ `file`, via `reqwest` feature `multipart`) → `Attachment` ; `download_attachment(url)` → octets bruts. Écrit et audité par le mainteneur. **Aucune faille exploitable.**

| Vecteur examiné | Posture |
|---|---|
| Auth / permission | `upload` passe par `send_json` ⇒ bearer attaché ; le serveur exige `ATTACH_FILES` + appartenance au salon. `download` est bearer-authed et le serveur garde par `VIEW_CHANNEL` (+ `nosniff`/CSP/`Content-Disposition`, cf. §26/F8). |
| Chemin de téléchargement | `download_attachment` prend le `url` **renvoyé par le serveur** (`/attachments/<id>/<nom assaini>`) ; appelé sur la même base bearer-authed — pas de surface d'injection nouvelle. |
| `ring` serveur | Feature `multipart` **côté client uniquement** ; `ozone-api` inchangé ⇒ invariant zéro-`ring` intact. |

Test : `upload_then_download_roundtrip` (les octets téléchargés == octets téléversés ; `size`/`filename` corrects). **Couverture client des routes non-vocales : complète.**

---
*Document vivant — revue effectuée pour S1 → S38 ; à reconduire à chaque couche. À compléter par : câblage UI des nouvelles capacités, stockage chiffré des jetons + `zeroize` (UI/registre), plafond de sessions/utilisateur + rate-limit des opcodes (R9), renégociation WS (mesh N-à-N) + E2EE DAVE/MLS (média) et leur audit, applications/bots/OAuth, rate-limiting REST (R1/R6), URLs signées pour pièces jointes, fuzzing du parseur gateway, tests de charge, et consommation transactionnelle des invitations (R5).*
