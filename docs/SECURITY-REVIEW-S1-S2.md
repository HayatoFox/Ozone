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
| **R1** | ~~Moyenne~~ **Résolu (S109)** | ~~**Pas de rate-limiting** (login, création d'invitations, envoi de messages, gate).~~ | Token bucket **en mémoire** (`ratelimit.rs`, sans Redis ni `ring`) : `login`/`register`/`gate` par **IP**, `create_message`/`create_invite` par **utilisateur** → 429 + `Retry-After`. Rate-limit des opcodes Gateway (R9 résiduel) reste futur (cf. §109). |
| **R2** | Faible | **Énumération de comptes** : `register` distingue « pseudo/e-mail déjà utilisé ». | Message générique + vérification par e-mail (slice comptes/anti-abus). **Non clos** : exige une vérification d'e-mail (cf. §108). |
| **R3** | ~~Faible~~ **Résolu (S107)** | ~~**Politique de mot de passe** minimale (≥ 8).~~ | Longueur ≥ 8 + denylist des mots de passe courants + interdiction de contenir le pseudo, à l'inscription et au changement (cf. §108). zxcvbn/HIBP restent un durcissement futur optionnel. |
| **R4** | ~~Info~~ **Résolu (S6b)** | ~~`join_invite` ne vérifie pas un **bannissement**~~. | Contrôle de ban ajouté dans `join_invite` (cf. §10). |
| **R5** | ~~Faible~~ **Résolu (S107)** | ~~**Course sur le quota d'invitation d'instance**~~. | Consommation **atomique** : `UPDATE … SET uses = uses + 1 WHERE code = ? AND (max_uses = 0 OR uses < max_uses)` (l'incrément conditionnel verrouille ; SQLite sérialise les écritures) + remboursement si l'inscription échoue ensuite (cf. §108). |
| **R6** | ~~Moyenne~~ **Résolu (S109)** | ~~**Exécution de webhook sans rate-limit**~~ : un détenteur du jeton pouvait spammer sans quota. | **Quota par webhook** (clé = `webhook_id`) dans `execute_webhook` → 429 + `Retry-After` (cf. §109). Rotation/désactivation du jeton restent disponibles via la gestion des webhooks. |
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
| R9 — accumulation d'acteurs de session pendant la grâce (connexions/déconnexions rapides) | ~~Faible~~ **Résolu (S107)** | Plafond de **10 sessions par utilisateur** (purge des plus anciennes à l'émission, cf. §108). Le rate-limit IDENTIFY (opcodes gateway) reste un durcissement futur. |

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

## 41. S39 — Inscription dans l'UI (bascule connexion ⇄ inscription)

L'écran d'auth gagne un mode **inscription** (pseudo + e-mail + mot de passe) en plus de la connexion, au-dessus du binding `register` **déjà audité** (§35/§39, jeton de gate inclus). **Aucune faille exploitable.** Posture : même flux gardé (gate si requis) ; mot de passe **et e-mail effacés** de la mémoire après succès ; rendu **texte brut** (pas d'exécution) ; aucune nouvelle surface réseau (réutilise `ApiClient::register`). Test : `register_mode_requires_email` (bascule + e-mail requis avant soumission). ozone-ui = 9 tests, clippy propre.

## 42. S40 — Refonte visuelle « façon Discord » (UI)

Refonte purement **présentationnelle** : module `style` (palette Discord + styles conteneurs/boutons/champs), nouvelle architecture (rail serveurs/instances, sidebar salons + en-tête + panneau utilisateur, zone de chat avec avatars + composeur, écrans de connexion/inscription en carte). **Aucune logique réseau/état modifiée** ⇒ aucune nouvelle surface d'attaque. Le rendu reste **texte brut** (avatars = initiale sur disque coloré, couleur dérivée d'un simple hachage du nom — purement cosmétique). Compile + `clippy -D warnings` propre ; ozone-ui = 9 tests (réducteur inchangé). *(Validation visuelle = exécuter le binaire.)*

## 43. S41 — Liste des membres + présences (UI)

Panneau des membres (droite) alimenté par `list_members` + statuts via `list_presences` (au changement de guilde) et **temps réel** via `PRESENCE_UPDATE` de la Gateway. Réutilise des bindings/flux **déjà audités** (§39, §31). **Aucune faille exploitable.** Posture : l'UI **affiche** ce que le serveur autorise (membres/présences filtrés serveur) ; pas de ré-dérivation de droits ; les résultats async sont étiquetés par index d'instance (ignorés si périmés) ; rendu **texte brut** ; map de présences indexée par `user_id`. Compile ; `clippy -D warnings` propre ; ozone-ui = 9 tests.

## 44. S42 — Finitions du fil de chat (UI)

Polissage **présentationnel** du chat : groupement des messages consécutifs d'un même auteur (avatar/en-tête une seule fois), horodatage `HH:MM` (UTC, calcul arithmétique pur), état vide (« début du salon »), placeholder du composeur incluant le nom du salon. **Aucune logique réseau/état modifiée** ⇒ aucune nouvelle surface. `fmt_time` est de l'arithmétique sur un entier (pas de fuite, pas de panique). Compile ; `clippy -D warnings` propre ; ozone-ui = 11 tests (ajout `fmt_time`/`initial`).

## 45. S44 — Actions guildes dans l'UI (créer/rejoindre serveur, créer salon)

Modales (pile `stack` + voile assombri) câblées sur des bindings **déjà audités** : `create_guild`, `join_invite`, `create_channel`. **Aucune faille exploitable.** Posture : l'UI ne fait que transmettre nom/code via `ApiClient` (bearer attaché) ; toute autorisation (créer une guilde, `MANAGE_CHANNELS`, validité/expiration de l'invitation) reste **côté serveur** ; les résultats async sont étiquetés par index d'instance (ignorés si périmés) ; rendu **texte brut**. Compile ; `clippy -D warnings` propre ; ozone-ui = 12 tests (ajout `dialog_open_input_and_close`).

## 46. S45 — Accueil : MP + amis (UI)

Vue d'accueil (route `Home`) câblée sur des bindings **déjà audités** : `list_dm_channels`, `open_or_create_dm`, `list_relationships`, `add_relationship`, `accept_relationship`, `remove_relationship`, plus `list_messages`/`send_message` réutilisés pour le fil d'un MP. **Aucune faille exploitable.** Posture : l'UI transmet pseudo/identifiants via `ApiClient` ; toute la logique (anti-auto-ami, blocages, appartenance au MP, autorisation d'écriture) reste **côté serveur** ; les résultats async sont étiquetés par index d'instance ; les listes MP/amis sont **vidées** au changement d'instance ; rendu **texte brut**. Compile ; `clippy -D warnings` propre ; ozone-ui = 13 tests (ajout `home_navigation_and_dm_open`).

## 47. S46 — Paramètres (UI)

Écran Paramètres câblé sur des bindings **déjà audités** : `update_profile`, `set_presence`, `change_password`, `change_email`, + déconnexion locale. **Aucune faille exploitable.** Posture : changement de mot de passe/e-mail **ré-authentifiés côté serveur** (le binding ne fait que transmettre) ; les mots de passe saisis sont **prélevés** des champs (`mem::take`) à l'envoi (non conservés) ; la déconnexion **efface le jeton en mémoire** (`set_token(None)`, `token=None`, `authed=false`) et renvoie à l'écran d'auth ⇒ l'abonnement Gateway se ferme (plus de jeton). *Note* : après un changement de mot de passe, le serveur **révoque les sessions** (§28) ; le jeton d'accès en mémoire expire dans sa fenêtre courte — une reconnexion sera nécessaire. Rendu **texte brut**. Compile ; `clippy -D warnings` propre ; ozone-ui = 14 tests (ajout `settings_navigation_and_logout`).

## 48. S47 — Fiches de profil (UI)

Clic sur un membre / un auteur de message / un ami → modale de profil chargée via `user_profile` (binding **déjà audité**), avec actions « Message » (`open_or_create_dm`) et « Ajouter en ami » (`add_relationship`). **Aucune faille exploitable.** Posture : seules les **données publiques** renvoyées par le serveur sont affichées (bio/pronoms en **texte brut**, aucune exécution) ; les actions transmettent identifiant/pseudo via `ApiClient` ; résultats étiquetés par index d'instance. Compile ; `clippy -D warnings` propre ; ozone-ui = 15 tests (ajout `view_profile_opens_and_closes`).

**Bilan UI (S40→S47)** : interface reproduisant Discord (rail, sidebar+panneau utilisateur, chat groupé+avatars, liste membres+présence) **et** fonctions câblées : guildes (créer/rejoindre/salons), MP, amis, paramètres (profil/présence/thème/mot de passe/e-mail/déconnexion), profils.

## 49. S48 — Réécriture du client en web (React + TS + Vite, futur Tauri 2)

Décision produit (utilisateur) : l'UI native Iced ne reproduit pas Discord assez fidèlement ⇒ **réécriture du client** en application web (TypeScript + React + Tailwind, empaquetage Tauri 2 ultérieur). **Le serveur `ozone-api` n'est pas touché** (client-agnostic) ⇒ **invariant zéro-`ring` serveur intact**. Échafaudage `desktop/` : `types.ts` (DTOs miroir, **Snowflake = `string`**), `api.ts` (REST typé, base `/api`, bearer), `gateway.ts` (WS IDENTIFY/HELLO/READY + RESUME, heartbeat, reconnexion back-off), `store.ts` (Zustand + application des événements Gateway), UI (auth, rail, sidebar, chat, membres, amis). `tsc -b` propre, `vite build` OK.

**Revue adversariale du nouveau front (revue statique + raisonnement sur les vecteurs web) :**

| Vecteur examiné | Posture |
|---|---|
| **XSS par contenu serveur** (message, bio, pseudo, nom de salon/guilde) | Rendu **exclusivement** via interpolation JSX `{...}` ⇒ **auto-échappement React**. **Aucun `dangerouslySetInnerHTML`, aucun `eval`, aucun `innerHTML`** dans le code (vérifié par recherche). Le contenu hostile s'affiche en texte inerte. |
| **Injection d'URL / `javascript:`** (lien de pièce jointe) | `href` construit comme `` `/api${a.url}` `` ⇒ **toujours préfixé `/api`**, donc une valeur `url` hostile reste une URL **relative** (impossible d'obtenir un schéma `javascript:`/`data:`). `target="_blank"` accompagné de **`rel="noreferrer"`** (implique `noopener`) ⇒ pas de `window.opener` ni de fuite de `Referer`. |
| **Injection de chemin REST** | Identifiants = `Snowflake` **émis par le serveur** (chaînes numériques). Le seul segment de chemin d'origine non contrôlée est l'**emoji de réaction** ⇒ **`encodeURIComponent`**. Les paramètres de recherche `?q=` passent par **`URLSearchParams`** (encodage). `/`, `=`, `&`, `#`, espace ne peuvent s'échapper de leur segment/paramètre. |
| **CSRF** | Auth par **jeton Bearer** (en-tête `Authorization`), **pas de cookie** ⇒ surface CSRF nulle (une page tierce ne peut pas forger l'en-tête). |
| **Jeton dans la Gateway** | Transmis dans le **corps** d'IDENTIFY/RESUME (pas dans l'URL WS ⇒ pas de fuite via logs serveur/historique). Reconnexion : RESUME tente `session_id`+`seq` ; `INVALID_SESSION` ⇒ repli sur IDENTIFY propre. `seq` borné par le serveur (tampon 512, cf. §gateway). |
| **Fuite de secret dans les erreurs** | `ApiError` ne contient que le message générique du serveur (pas d'écho du jeton) ; aucun secret journalisé. |
| **Proxy de dev** | `/api`→serveur et `/gateway`→ws **uniquement en développement** (Vite) ; non présent dans le build de production. Pas de contournement CORS exposé en prod. |
| **Invariant serveur** | Ajouts **front uniquement** ; `ozone-api` inchangé ⇒ **zéro `ring` serveur** préservé. |

**Risque identifié — R10 (moyen, à durcir) : persistance du jeton en `localStorage`.** Pour survivre au rechargement de page, `api.ts` stocke la paire de jetons dans `localStorage` (clé `ozone.tokens`). Contrairement au client Iced (jeton **en mémoire seule**), un jeton en `localStorage` serait **lisible par tout script s'exécutant dans la page** en cas de faille XSS. **Atténuation actuelle** : la surface XSS est fermée par construction (auto-échappement React, aucun `innerHTML`/`eval`, `href` rooté `/api`) ⇒ pas de vecteur d'exécution connu pour atteindre `localStorage`. **Durcissement prévu** (empaquetage Tauri) : déplacer les jetons vers le **stockage sécurisé de l'OS** via le backend Rust Tauri (jeton hors du DOM), ou jeton d'accès **en mémoire** + rotation par refresh ; ajouter une **CSP stricte** (`default-src 'self'`) sur le HTML servi en prod. **Aucune faille exploitable** en l'état (R10 = défense en profondeur, à traiter avant release).

**Bilan** : aucune faille exploitable introduite ; surface XSS fermée par construction ; CSRF nul (Bearer) ; invariant zéro-`ring` serveur intact. Une dette de durcissement (R10, stockage du jeton) est consignée pour l'étape Tauri.

## 50. S49 — Phase 0 : système de tokens visuels + coque (client web)

Après recherche référentielle (consignée dans `docs/UI-DISCORD-REFERENCE.md`), première phase de la reproduction fidèle : refonte du **système de couleurs** en **variables CSS à 2 couches** (primitives + alias sémantiques sous `:root`/`.theme-dark`), Tailwind ne faisant que **référencer** ces variables ; voiles de survol/sélection passés en **rgba translucides** ; ajout de `prefers-reduced-motion` ; remplacement des icônes (emoji/SVG inline) par **`lucide-react`** ; police substituée par **Inter** (gg sans étant propriétaire). `tsc -b` propre, `vite build` OK. **Slice purement présentationnelle — aucune logique réseau/auth/état modifiée ⇒ aucune nouvelle surface d'attaque applicative.**

| Vecteur examiné | Posture |
|---|---|
| **Nouvelle dépendance `lucide-react`** | Bibliothèque d'icônes très répandue (ISC), rendue en **composants SVG inline** : pas d'appel réseau, pas d'`eval`, pas de `dangerouslySetInnerHTML`. Surface supply-chain ajoutée = 1 paquet (audit `npm` : 0 vulnérabilité). Tree-shaking effectif (+~1,4 Ko gzip). |
| **Variables CSS / `style` inline** | Les valeurs de token (couleurs, statut) sont des **constantes littérales** injectées en CSS/`style` — aucune donnée utilisateur n'alimente une propriété CSS ⇒ pas d'injection de style. `prefers-reduced-motion` est une media query passive. |
| **Auth / données / réseau** | **Inchangés** : `api.ts`, `gateway.ts`, `store.ts` non modifiés. La posture XSS (auto-échappement React, zéro HTML brut), CSRF (Bearer), et l'encodage des chemins restent ceux de §49. |
| **Invariant serveur** | `ozone-api` non touché ⇒ **zéro `ring`** préservé. R10 (jeton en `localStorage`) inchangé — toujours à durcir à l'étape Tauri. |

**Bilan** : refactor visuel sans incidence sécurité ; aucune faille exploitable ; dépendance ajoutée à faible surface ; invariants (XSS fermé, CSRF nul, zéro-`ring`) intacts.

## 51. S50 — Phase 1 : modes Cozy/Compact, formes de statut, couleurs de rôle (client web)

Slice **présentationnelle** : points de présence aux **formes exactes** de Discord (masques SVG : disque/croissant/barre/anneau), couleurs de rôle appliquées aux pseudos (message + liste de membres), modes d'affichage **Cozy/Compact** (réglage persistant `localStorage`), chargement des rôles par guilde. **Aucune logique réseau/auth modifiée** (réutilise `list_roles`/`list_members`/`list_presences` déjà audités) ⇒ aucune nouvelle surface.

| Vecteur | Posture |
|---|---|
| Couleur de rôle (`style={{color}}`) | `roleColorHex` masque sur 24 bits (`color & 0xffffff`) et formate en hex zéro-paddé ⇒ valeur CSS **toujours** de forme `#rrggbb`, **non contrôlable** par l'utilisateur pour injecter du CSS (la couleur vient d'un `u32` serveur). |
| Données de statut / rôles | **Affichage** de ce que le serveur autorise (membres/rôles/présences filtrés serveur) ; pas de ré-dérivation de droits côté client. |
| `localStorage` (mode d'affichage) | Valeur bornée (`"cozy"`/`"compact"`) ; aucune donnée sensible. |
| Rendu | Pseudos/contenus restent du **texte React échappé**. |

**Bilan** : aucune incidence sécurité ; invariants intacts.

## 52. S51 — Phase 2 : rendu Markdown « façon Discord » (client web) — **vérif adversariale**

Renderer Markdown maison à **sortie React** (`src/lib/markdown.tsx`) : gras/italique/souligné/barré, code inline & blocs, spoilers, citations (`>`/`>>>`), titres, listes, **liens masqués**, **mentions** (résolues via contexte), **timestamps**. Choix délibéré : **sortie en éléments React** (jamais `dangerouslySetInnerHTML`/HTML brut) ⇒ tout texte reste **auto-échappé** par React. **Aucune faille exploitable.** Couvert par **12 tests vitest** (`markdown.test.ts`), dont les scénarios adverses :

| Vecteur adverse | Défense | Test |
|---|---|---|
| **Lien masqué `javascript:`** `[x](javascript:alert(1))` | `safeUrl` n'autorise que `http:`/`https:` (parse via `URL`) ⇒ **aucun `<a>` émis**, le texte brut reste inerte | `neutralise les liens javascript:` |
| **Lien masqué `data:`** (`data:text/html,<script>…`) | idem ⇒ aucun `<a>` | `neutralise les liens data:` |
| **URL nue** | n'émet un `<a>` que si `safeUrl` valide ; `target=_blank` + `rel="noreferrer"` | `transforme une URL nue` |
| **Injection via code inline** `` `**x**` `` | contenu du code **non reparsé** (règle prioritaire, pas de récursion) ⇒ reste littéral | `n'interprète pas le contenu d'un code inline` |
| **Balise inattendue** | l'arbre n'émet que des balises d'une **liste blanche** (`div,p,span,strong,em,u,s,code,pre,a,ul,ol,li` + composants internes) ; aucun HTML arbitraire | `n'émet jamais de balise non prévue` |
| **Entrée pathologique** (`***…___…```…`) | la boucle de parsing **avance toujours** d'au moins la longueur du match (pas de match vide) ⇒ pas de boucle infinie / DoS | `ne boucle pas sur une entrée pathologique` |
| **Mentions** `<@id>`/`<#id>` | résolues via **contexte de confiance** (données serveur) ; fallback texte si inconnu ; rendues en pastilles **texte** (pas d'exécution) | `résout une mention …` |

Notes : les **emoji custom** (`<:nom:id>`) sont rendus en texte `:nom:` (images/jumbo → Phase 3) ; la **coloration syntaxique** (highlight.js) est différée et sera ajoutée en **sortie échappée** (les blocs de code sont actuellement du texte React, donc sûrs). `ozone-api` non touché ⇒ **zéro `ring`** préservé.

**Bilan** : renderer sûr par construction (sortie React, zéro HTML brut), schémas d'URL filtrés, anti-DoS de parsing, couvert par tests adverses rejouables (`npm test`). Aucune faille exploitable.

## 53. S52 — Phase 5 : réglages (thème, Cozy/Compact, profil) (client web)

Panneau de réglages : sélection de **thème** (sombre/clair/midnight — bascule de classe racine `theme-*`, simple échange de tokens CSS), **mode d'affichage** (Cozy/Compact), édition du **nom d'affichage** (binding `update_profile` **déjà audité** §39), **déconnexion** locale. **Aucune faille exploitable.**

| Vecteur | Posture |
|---|---|
| Bascule de thème | Écrit `theme-{dark\|light\|midnight}` sur `documentElement.className` à partir d'une valeur **bornée** (énum) ; aucune entrée utilisateur libre n'alimente la classe ⇒ pas d'injection. Persistance `localStorage` de valeurs énumérées. |
| Édition de profil | Transmet `display_name` via `ApiClient` (bearer) ; toute validation reste **côté serveur** ; champs en lecture seule pour `username`/`email` (affichage). |
| Déconnexion | `logout` efface le jeton (`localStorage`) + ferme la Gateway + réinitialise l'état (déjà audité §47, posture identique). |
| Rendu | Données de compte affichées en **texte React échappé**. |

`ozone-api` non touché ⇒ **zéro `ring`** préservé ; R10 (jeton `localStorage`) inchangé.

**Bilan** : aucune nouvelle surface applicative ; bascule de thème par token ; invariants intacts.

## 54. S53 — Phase 3 : pièces jointes, réponses, édition/suppression, réactions (client web)

Interactions de messagerie câblées sur des bindings REST **déjà audités** : **téléversement** de pièces jointes (multipart), affichage **images inline + lightbox**, **réponses** (`reply_to` + ligne de référence), **édition/suppression** (barre d'actions au survol), **réactions** cliquables, emoji custom rendu en texte `:nom:`, **jumbo** unicode. **Aucune logique d'autorisation côté client.** **Aucune faille exploitable.**

| Vecteur | Posture |
|---|---|
| Téléversement multipart | `requestForm` attache le **bearer** et laisse le navigateur poser le boundary ; le serveur garde par `ATTACH_FILES` + appartenance + **plafond 26 Mo** (`DefaultBodyLimit`, §26/§40). Le client ne fait que relayer le `File`. |
| Affichage des pièces jointes | `src`/`href` = `` `/api${att.url}` `` ⇒ **toujours rooté `/api`** (chemin relatif, pas de schéma exotique) ; le serveur sert avec `nosniff`/CSP/`Content-Disposition` (§26 F8). Le lightbox réutilise la même URL. |
| Réponses / édition / suppression | `reply_to`, édition et suppression sont **autorisés côté serveur** (auteur / `MANAGE_MESSAGES`) ; le client relaie via `ApiClient`. `referenced_message` rendu en **texte React échappé**. |
| Réactions | emoji **percent-encodé** dans le chemin (`encodeURIComponent`) ; ajout/retrait gardés serveur ; agrégat mis à jour depuis les events Gateway (`{channel_id,message_id,user_id,emoji}`) — `me` recalculé via l'id courant. |
| Emoji custom / jumbo | rendu **texte** (`:nom:`) — pas de fetch d'image arbitraire ; jumbo = simple classe CSS. |
| Rendu | tout via React (auto-échappement) ; aucune nouvelle surface XSS. |

**Bilan** : interactions relayées, autorité serveur préservée, URLs de média bornées à `/api`, rendu échappé. Aucune faille exploitable.

## 55. S54 — Phase 4 : système de non-lus (client + champ serveur `last_message_id`)

Non-lus « façon Discord » : salons/guildes non-lus (gras + point), **badges de mention**, marquage de lecture à l'ouverture et auto-lecture du salon actif. **Petite addition serveur** : champ **`last_message_id`** sur le DTO `Channel`, calculé par sous-requête `(SELECT MAX(id) FROM messages WHERE channel_id = …)` dans `CHANNEL_SELECT`. **Aucune faille exploitable.**

| Vecteur | Posture |
|---|---|
| **Changement serveur** (`last_message_id`) | Champ **dérivé en lecture seule** (MAX(id) des messages), exposé **uniquement** via les routes salon **déjà gardées** (`list_channels`/`get_channel` requièrent appartenance/VIEW) ⇒ aucune nouvelle fuite : qui voit le salon voit déjà ses messages. `#[serde(default)]` ⇒ compat ascendante des clients. **Pas de crypto** ⇒ invariant **zéro-`ring`** intact. Tests `ozone-api` **tous verts** (0 échec) après le changement. |
| Comparaison d'ids | `idGt` via **`BigInt`** ⇒ comparaison exacte des Snowflakes (au-delà de la précision `Number`) ; couvert par test (`9007199254740993 > …992`). |
| Détection de mention (client) | purement **cosmétique** (compteur de badge) par recherche de `<@id>`/`@everyone`/`@here` ; **n'accorde aucun droit** — la livraison/permission réelle des mentions reste **côté serveur**. |
| Marquage de lecture | `ackMessage` relayé au serveur (binding §39) ; l'auto-lecture du salon actif n'expose rien (état de lecture propre à l'utilisateur). |

Note (non sécurité) : les non-lus au démarrage ne couvrent que les guildes dont les salons sont chargés (pas de pré-chargement global) — limitation fonctionnelle, pas un risque.

**Bilan** : addition serveur minimale, dérivée et gardée par les permissions existantes ; zéro-`ring` préservé ; comparaison d'ids robuste ; détection de mention sans incidence de droits. Aucune faille exploitable.

## 56. S55 — Phase 6 : overlays accessibles Radix (tooltips, sélecteur d'emoji)

Primitives accessibles **Radix UI** (MIT) : **tooltips** sur le rail des serveurs, **popover** de sélection d'emoji pour ajouter une réaction (comble le manque de la Phase 3). **Aucune faille exploitable.**

| Vecteur | Posture |
|---|---|
| Dépendances ajoutées | `@radix-ui/react-tooltip` / `-popover` / `-context-menu` (MIT, très répandues ; `npm audit` : **0 vulnérabilité**). Rendu en **portails DOM** sans `eval`/HTML brut. |
| Sélecteur d'emoji | Propose un jeu d'emoji **statique** ; à la sélection, ajoute une réaction via `toggleReaction` → binding `add_reaction` **gardé serveur** (emoji percent-encodé, §53). L'emoji est du **contenu** (réaction), pas de l'iconographie d'UI. |
| Tooltips | Affichent un `label` **interne** (nom de serveur, déjà du texte de confiance) en texte React échappé ; aucune donnée hostile injectée. |
| Rendu | Contenu des overlays via React (auto-échappement) ; les variables de thème (CSS) sont héritées dans les portails (`:root`). |

`ozone-api` non touché ⇒ **zéro `ring`** préservé ; R10 inchangé.

**Bilan** : overlays accessibles ajoutés sans nouvelle surface applicative ; dépendances MIT auditées ; invariants intacts. **Plan UI S49→S55 (Phases 0–6) terminé.**

## 57. S56 — Profil popout, menu contextuel, épingles, invitations (client web)

Au-delà du plan : carte de **profil** (clic avatar/pseudo), **menu contextuel** de message (clic droit), **épingles** (API), **invitations** (créer + copier un code). Tous câblés sur des bindings **gardés serveur**. **Aucune faille exploitable.**

| Vecteur | Posture |
|---|---|
| Profil popout | `user_profile` renvoie le profil **public** (sans e-mail, §48) ; bio/pronoms rendus en **texte React échappé** ; `accent_color` → `roleColorHex` (hex borné) ; « Message » via `open_or_create_dm` (gardé serveur). |
| Menu contextuel | Répondre/Modifier/Supprimer/Épingler relaient des bindings **autorisés côté serveur** (auteur / `MANAGE_MESSAGES`) ; « Copier » utilise `navigator.clipboard.writeText` sur du **texte brut** (contenu / id) — pas d'injection. |
| Épingles | `pin`/`unpin`/`listPins` gardés serveur (`MANAGE_MESSAGES`) ; le client relaie. |
| Invitations | `create_invite` requiert `CREATE_INVITE` **côté serveur** ; le client affiche/copie le **code** (destiné au partage) — aucune autre donnée sensible exposée. |
| Quitter la guilde | `leave_guild` gardé serveur ; le client rafraîchit puis revient à l'accueil. |

`ozone-api` non touché par cette tranche ⇒ **zéro `ring`** préservé.

**Bilan** : ajouts d'UI relayant des bindings déjà audités ; autorité serveur intacte ; rendu échappé ; presse-papiers en texte brut. Aucune faille exploitable.

## 58. S57 — Recherche, épingles (panneau), découverte de guildes (client web)

En-tête de salon : **panneau d'épingles** et **recherche** (popovers) ; **annuaire de découverte** (boussole du rail) avec adhésion. Bindings **gardés serveur**. **Aucune faille exploitable.**

| Vecteur | Posture |
|---|---|
| Recherche | `search_channel` filtré **côté serveur** par `VIEW_CHANNEL` (R: fuite inter-salons fermée, §12) ; la requête `q` est passée via `URLSearchParams` (encodée) ; résultats rendus en **texte échappé** (tronqué). |
| Épingles | `listPins` gardé par `VIEW_CHANNEL` côté serveur ; rendu texte échappé. |
| Découverte | `list_discovery` n'expose que les guildes **publiques** (discoverable) ; `join_discovery` gardé serveur (politique d'adhésion) ; description/nom rendus en **texte échappé** ; `member_count` numérique. |
| Rendu | tout via React (auto-échappement) ; aucune nouvelle surface XSS ; `line-clamp` purement CSS. |

`ozone-api` non touché ⇒ **zéro `ring`** préservé.

**Bilan** : trois surfaces de lecture/adhésion relayant des bindings audités, filtrage et autorité **côté serveur**, rendu échappé. Aucune faille exploitable.

## 59. S58 — Groupement des membres par rôle (hoist) + barre « nouveaux messages »

Améliorations **présentationnelles** : liste de membres groupée par rôle affiché (`hoist`, triés par position), barre « nouveaux messages » à la frontière de non-lu (ancre capturée à l'ouverture du salon, avant marquage lu). **Aucune logique réseau/auth modifiée** ⇒ aucune nouvelle surface. Le groupement/ancre sont des calculs purs sur des données déjà chargées (membres/rôles/read-states) ; rendu **texte échappé** ; couleurs via `roleColorHex` (hex borné). Aucune faille exploitable.

## 60. S59 — Gestion de salon (clic droit : marquer lu / renommer / supprimer)

Menu contextuel de salon relayant des bindings **gardés serveur** : `update_channel` / `delete_channel` requièrent `MANAGE_CHANNELS` **côté serveur** ; « marquer comme lu » = `ack` (état propre à l'utilisateur). Le client ne fait que transmettre nom/sujet via `ApiClient` ; toute autorisation reste serveur ; un non-autorisé reçoit **403** (la liste se met à jour via les events `CHANNEL_UPDATE`/`CHANNEL_DELETE`). Rendu **texte échappé**. `ozone-api` non touché ⇒ **zéro `ring`**. Aucune faille exploitable.

## 61. S60 — Paramètres du serveur (renommer / description / découvrable / supprimer)

Modale relayant `update_guild` / `delete_guild` (gardés `MANAGE_GUILD` / propriétaire **côté serveur**). Le bouton « Supprimer » n'est montré qu'au **propriétaire** (UX), mais l'autorité reste serveur (un non-propriétaire recevrait **403**) ; suppression à **double confirmation**. Le toggle « découvrable » transmet un booléen ; nom/description en **texte échappé**. `ozone-api` non touché ⇒ **zéro `ring`**. Aucune faille exploitable.

## 62. S61 — Démarrer une conversation depuis l'accueil

Popover « Trouver ou démarrer une conversation » listant les **amis** (relations de type `friend`, déjà chargées) → `open_or_create_dm` (binding **déjà audité** §46). Aucune nouvelle surface réseau ; rendu **texte échappé** ; l'autorisation (être ami / pouvoir ouvrir un MP) reste **côté serveur**. Aucune faille exploitable.

## 63. S62 — Modération depuis la fiche de profil (expulser / bannir)

Actions **Expulser** (`kick_member`) / **Bannir** (`ban_member`) dans le popout de profil, en contexte de guilde. **Aucune faille exploitable.**

- **La frontière de sécurité est le serveur.** `kick`/`ban` sont gardés par `KICK_MEMBERS` / `BAN_MEMBERS` **et la hiérarchie des rôles** côté serveur (le propriétaire est protégé, §11) ; un appel non autorisé reçoit **403**.
- Le **filtre client** (boutons visibles uniquement au **propriétaire** de la guilde, et jamais sur soi-même) est une **commodité UX**, pas un contrôle d'accès — il ne fait que masquer des actions que le serveur refuserait de toute façon.
- `ban` envoie `delete_message_seconds: 0` (pas de purge). Après action, la liste des membres est **re-récupérée** (`list_members`, filtré serveur).
- `ozone-api` non touché par cette tranche ⇒ **zéro `ring`** préservé.

**Bilan** : actions sensibles **entièrement autorisées côté serveur** ; le client ne fait que relayer + masquer par confort ; aucune élévation de privilège possible depuis le client. Aucune faille exploitable.

## 64. S63 — Glisser-déposer de fichiers sur le chat

Zone de dépôt sur la vue de chat (overlay) réutilisant `upload_attachment` (binding **déjà audité** §53/§40 : garde `ATTACH_FILES`, plafond 26 Mo côté serveur). Aucune nouvelle surface réseau ; le compteur de profondeur de drag est purement local ; les fichiers déposés suivent le **même chemin** que le sélecteur. Aucune faille exploitable.

## 65. S64 — Sourdine (mute) des serveurs et salons

Mise en sourdine via `set_guild_notification` / `set_channel_notification` (réglages **propres à l'utilisateur**, authentifiés, gardés par appartenance/visibilité côté serveur) ; chargement initial via `list_notification_settings`. **Aucune faille exploitable.** Les réglages ne concernent **que le compte courant** (`/users/@me/...`) ⇒ pas d'accès à l'état d'autrui ; la mise à jour optimiste est locale et bornée (booléen → `mute_seconds` -1/0) ; l'affichage (atténuation, suppression de l'indicateur non-lu) est purement présentationnel. `ozone-api` non touché ⇒ **zéro `ring`**.

## 66. S65 — Gestion des rôles (créer / éditer / supprimer / assigner)

Modale de rôles (créer, renommer, couleur, hoist, mentionnable, supprimer) + assignation depuis la fiche de profil. Bindings `create_role` / `update_role` / `delete_role` / `add_member_role` / `remove_member_role` **gardés `MANAGE_ROLES` + hiérarchie côté serveur**. **Aucune faille exploitable.**

- **Autorité serveur** : toute opération de rôle est autorisée côté serveur (permission + position hiérarchique : on ne peut pas manipuler un rôle ≥ au sien) ; un appel non autorisé reçoit **403**. L'UI (modale via le menu de guilde, chips dans le profil) est **gardée par confort** au propriétaire mais ne constitue pas la frontière de sécurité.
- Les rôles `managed` et `@everyone` sont **exclus** de l'assignation côté client (cohérent avec le refus serveur).
- `permissions` (bitfield) n'est **pas** éditable depuis cette UI (réservé à une itération ultérieure) ⇒ aucune surface d'escalade de permission introduite. Couleur bornée (`roleColorHex`), noms en **texte échappé**.
- `ozone-api` non touché ⇒ **zéro `ring`**.

**Bilan** : CRUD de rôles et assignation entièrement **autorisés côté serveur** (permission + hiérarchie) ; l'UI relaie + masque par confort ; pas d'édition de bitfield de permission ⇒ pas de nouvelle voie d'escalade. Aucune faille exploitable.

## 67. S66 — Éditeur de permissions de rôle (bitfield)

L'éditeur de rôle gagne des cases à cocher de **permissions** (bitfield u64). Manipulé en **`BigInt`** (chaîne décimale) ⇒ pas de perte de précision sur les bits hauts (≥ 2⁵³) — couvert par tests (`permissions.test.ts` : bit 51, voisins non confondus). **Aucune faille exploitable.**

- **La frontière de sécurité est le serveur.** `update_role` valide côté serveur : `MANAGE_ROLES`, **hiérarchie** (on ne modifie pas un rôle ≥ au sien) et la règle anti-escalade (on ne peut **accorder que des permissions qu'on possède**, sauf `ADMINISTRATOR`/propriétaire) — cf. couche permissions auditée (§1). Le client n'envoie qu'une **chaîne** de bitfield ; toute tentative d'escalade est rejetée **côté serveur** (403/400).
- L'UI (modale réservée par confort au propriétaire) ne constitue pas le contrôle d'accès ; elle expose un **sous-ensemble** de permissions libellées, calculées par OU/ET binaires purs (aucune entrée libre).

**Bilan** : l'édition de bitfield ne crée **pas** de voie d'escalade côté client — l'autorité (permission + hiérarchie + anti-grant) reste **entièrement serveur** ; manipulation `BigInt` correcte et testée. Aucune faille exploitable. *(Vérifié à la revue : `routes_roles::update_role` masque les permissions demandées par celles de l'auteur — `p & actor`, ligne 157 — en plus de `MANAGE_ROLES` + contrôle de hiérarchie ; `create_role` idem.)*

## 68. S67 — Journal d'audit (lecture seule)

Visionneuse du journal d'audit (`list_audit_logs`, **gardé `VIEW_AUDIT_LOG` côté serveur** ⇒ un non-autorisé reçoit **403**, affiché comme « Accès refusé »). Lecture seule ; entrées rendues en **texte échappé** (type d'action humanisé via table fixe, raison/identifiants échappés) ; résolution des noms via les membres déjà chargés. `ozone-api` non touché ⇒ **zéro `ring`**. Aucune faille exploitable.

## 69. S68 — Gestion des bannissements (lister / révoquer)

Modale listant les bannissements (`list_bans`) avec révocation (`unban_member`), tous deux **gardés `BAN_MEMBERS` côté serveur** (non-autorisé ⇒ **403** affiché « Accès refusé »). Le client relaie ; raisons/noms en **texte échappé**. `ozone-api` non touché ⇒ **zéro `ring`**. Aucune faille exploitable.

## 70. S69 — Événements programmés (lister / créer / RSVP / supprimer)

Vue des événements : `list_events` / `create_event` (gardé `CREATE_EVENTS`) / `delete_event` (créateur ou `MANAGE_EVENTS`) / RSVP `interested` (propre à l'utilisateur). **Tous gardés + isolés par guilde côté serveur** (§7/§12) ; un appel non autorisé reçoit **403**, un accès inter-guilde **404**. Le client relaie ; nom/description/lieu en **texte échappé** ; `scheduled_start` dérivé d'un `datetime-local` → epoch ms (entier). État RSVP suivi localement (le DTO n'expose pas `me_interested`). `ozone-api` non touché ⇒ **zéro `ring`**. Aucune faille exploitable.

## 71. S70 — Fils (threads) : créer / lister / naviguer

Création de fil depuis le menu contextuel d'un message (`create_thread`), affichage des fils sous leur salon parent dans la sidebar (`list_threads`). **Gardés côté serveur** (création : droit d'écrire / `CREATE_PUBLIC_THREADS` ; listage : `VIEW_CHANNEL`) ; un fil est un salon `type` 11/12 avec `parent_id` ⇒ il réutilise **toute l'infrastructure de messages déjà auditée** (chargement, envoi, non-lus) sans chemin nouveau. Nom de fil dérivé du contenu du message (tronqué à 90, **texte échappé**). `ozone-api` non touché ⇒ **zéro `ring`**. Aucune faille exploitable.

## 72. S71 — Sondages (addition serveur `Message.poll` + rendu/vote client) — **vérif adversariale**

Première addition serveur de cette phase : champ **`poll: Option<Poll>`** sur le DTO `Message`, attaché lors de l'hydratation. Côté client : rendu du sondage (barres + vote) + création. **Zéro `ring`** (aucune crypto ; pur SQL + DTO). Tests `ozone-api` **tous verts** (exit 0), `clippy` propre.

| Vecteur adverse | Défense |
|---|---|
| **Fuite de sondage inter-salons** | Le sondage n'est attaché qu'aux messages renvoyés par des chemins **déjà gardés `VIEW_CHANNEL`** (`list_messages`, `hydrate`) ; `load_poll_ids` ne requête que des `message_id` **déjà visibles** par l'appelant. `get_poll` revérifie que le sondage appartient au salon (`WHERE channel_id = ?`). |
| **Fuite du `me_voted` d'autrui** | `me_voted` est calculé **pour le spectateur** (`viewer`). Le `MESSAGE_UPDATE` émis à la création est construit avec `viewer = créateur`, mais **à la création aucun vote n'existe** (`me_voted` partout faux, décomptes à 0) ⇒ rien à fuiter. Ensuite chaque vote diffuse `MESSAGE_POLL_VOTE` (sans état), et **chaque client re-fetch `get_poll`** avec **son propre** `viewer` ⇒ `me_voted`/décomptes corrects et privés par client. |
| **Vote invalide / triche** | `cast_vote` valide **côté serveur** : `answer_id` appartenant au sondage, règle mono/multi, expiration, `VIEW_CHANNEL` ; remplace l'ensemble des votes de l'utilisateur (pas d'empilement). Le client ne fait que relayer. |
| **Portée de diffusion** | `emit_message_update` réutilise `emit()` qui calcule la portée **salon** ⇒ `should_deliver` ne livre qu'aux membres autorisés. |
| **Compat DTO** | `#[serde(default, skip_serializing_if=None)]` ⇒ clients existants ignorent le champ ; aucune rupture. |
| **Rendu** | question/réponses en **texte React échappé** ; pourcentages = arithmétique pure. |

**Bilan** : extension serveur minimale et **dérivée** (le sondage ne s'expose que là où les messages le sont déjà) ; `me_voted` privé par spectateur (vérifié : pas de fuite à la création) ; vote validé serveur ; zéro-`ring` préservé ; tests + clippy verts. Aucune faille exploitable.

## 73. S72 — Emojis personnalisés (stockage serveur + service public + rendu) — **vérif adversariale**

Nouveau sous-système : **téléversement** d'image d'emoji (`POST /guilds/:gid/emojis/image`, multipart) + **service public** (`GET /emojis/:emoji_id`) + rendu client de `<:nom:id>` en `<img>`. Aucune crypto (SQL + fs) ⇒ **zéro `ring`**. `cargo check`/`clippy` propres.

| Vecteur adverse | Défense |
|---|---|
| **Exécution d'un fichier téléversé** (polyglotte HTML/JS) | Le **Content-Type est forcé** depuis les **octets magiques** (PNG/GIF/WEBP/JPEG, sinon `application/octet-stream`), **jamais** depuis l'entrée ; `X-Content-Type-Options: nosniff` + **CSP `default-src 'none'; sandbox`** ⇒ même un fichier malveillant ne s'exécute pas dans l'origine. Upload **rejeté** si les octets magiques ne correspondent pas à une image. |
| **Traversée de chemin** | Le fichier est écrit/lu par **identifiant numérique** uniquement ; à la lecture, `image_id` (colonne texte) est **parsé en `i64`** avant de composer le chemin ⇒ `..`/`/` impossibles. |
| **Autorisation d'upload** | `require_expression_create` (`CREATE_/MANAGE_GUILD_EXPRESSIONS`) **côté serveur** ; un non-autorisé reçoit 403. Taille bornée (256 Kio + `DefaultBodyLimit` 2 Mio). |
| **Service public (sans bearer)** | Choix **délibéré** : les images d'emoji sont **décoratives** (modèle CDN public de Discord) et le `<img>` du navigateur ne peut pas porter le bearer. Aucune donnée privée exposée ; il faut connaître un Snowflake. Sert uniquement les emoji `available = 1`. |
| **Injection via le token `<:nom:id>`** | La regex impose `id = \d+` ⇒ `src` = `/api/emojis/<chiffres>` (jamais `javascript:`/`data:`/`..`) ; `nom` rendu en `alt`/`title` **échappés** par React. Le nom est aussi validé serveur (`[A-Za-z0-9_]`, 2–32). |
| **`ring` serveur** | Pas de crypto ni de fetch externe ⇒ invariant **zéro-`ring`** intact. |

*Note (non sécurité)* : un upload d'image sans `create_emoji` ultérieur laisse un fichier orphelin (faible ; nettoyage périodique possible). 

*Réactions avec emoji custom* (ajout client) : le picker propose les emoji de la guilde et `add_reaction` reçoit la chaîne `<:nom:id>` (binding **déjà audité** §53 ; emoji ≤ 64 caractères, validé serveur, percent-encodé dans le chemin) ; les puces détectent `<a?:nom:id>` et rendent un `<img>` vers `/api/emojis/:id` (même service public audité ci-dessus). *Autocomplétion `:nom:`* dans le composeur : insère le token texte `<:nom:id>` (pure manipulation de chaîne locale, envoi via le chemin message déjà audité). Aucune nouvelle surface.

**Bilan** : upload validé par octets magiques + servi en type forcé/`nosniff`/CSP (pas d'exécution), localisation par id numérique (pas de traversée), upload gardé par permission, service public assumé pour des assets décoratifs, rendu client sans surface XSS. Aucune faille exploitable.

## 74. S73 — Affichage des médias privés via fetch authentifié (résout R12)

Les pièces jointes (`/api/attachments/:id/:file`) restent **gardées par bearer** côté serveur, mais le navigateur n'envoie pas l'en-tête `Authorization` sur `<img>/<a>`. Correctif **100 % client, sans changement serveur** : `AuthedImage`/`authedDownload` (`src/components/AuthedMedia.tsx`) récupèrent la ressource via `fetch` **avec le bearer**, puis exposent un **`blob:` URL** local (révoqué au démontage). **Aucune faille introduite.**

- **Pas de jeton dans l'URL** : le bearer reste dans l'en-tête `fetch` (pas de fuite via logs/historique/Referer, contrairement à `?token=`). 
- **Autorité serveur inchangée** : `serve_attachment` garde son contrôle `VIEW_CHANNEL` (un non-membre obtient 403, le `fetch` échoue → placeholder).
- **Cycle de vie** : les `blob:` URL sont **révoqués** (`URL.revokeObjectURL`) au démontage (pas de fuite mémoire).
- **Emoji** : non concernés (servis en public, §73).

**Bilan** : R12 **résolu** sans affaiblir l'authentification (bearer en en-tête, jamais en URL) ni toucher au serveur. Aucune faille exploitable.

## 75. S74 — Embeds média (opt-in, atténue R11)

Rendu inline des **liens média directs** (images/vidéos) dans les messages, **désactivé par défaut** ; activable via Réglages → Apparence avec **avertissement de confidentialité explicite**. **Aucune faille exploitable.**

| Vecteur | Posture |
|---|---|
| **Fuite d'IP / pixel traceur** | Récupérer un média externe expose l'IP du spectateur (pas de proxy possible sous zéro-`ring`). **Atténué par OFF par défaut** + **consentement éclairé** (le réglage décrit le risque). Aucune régression sans action explicite de l'utilisateur. |
| **Injection** | Seules les URL **`http(s)://…`** finissant par une extension média (`png/jpe?g/gif/webp/bmp/avif/mp4/webm/mov`) sont rendues, en `<img>`/`<video>` (pas d'exécution de script ; `javascript:`/`data:` impossibles). SVG **exclu** (vecteur de script). |
| **DoS** | Plafonné à **4 médias** par message ; `loading="lazy"`. |
| **OG unfurling** | **Toujours différé** (R11) : nécessite une récupération HTTPS serveur (incompatible zéro-`ring` sans pile TLS pure-Rust) + garde anti-SSRF. |

**Bilan** : feature opt-in à consentement éclairé, rendu média strictement borné (http(s) + extensions, pas de SVG, cap 4), aucune régression par défaut. Aucune faille exploitable.

## 76. S75 — Présence vocale (signalisation, **sans média**)

Première brique de l'axe vocal côté client : **rejoindre/quitter** un salon vocal, **liste des connectés** (avec indicateurs micro/casque), **barre de contrôle** (mute/sourdine/déconnexion), temps réel via `VOICE_STATE_UPDATE`. **Aucun transport média** (pas de `getUserMedia`, pas de WebRTC) ⇒ **aucune nouvelle surface média**. **Aucune faille exploitable.**

- **Signalisation reléyée** : `update_own_voice_state` / `leave_voice` / `list_voice_states` sont **gardés côté serveur** (appartenance à la guilde, validation que la cible est un salon vocal de la guilde, §S\*) ; le client ne fait que transmettre. La réponse `VoiceJoinResponse` (incluant l'`endpoint` SFU, « emplacement à configurer ») n'est **pas exploitée** ici.
- **Isolation** : les états vocaux sont diffusés en portée **guilde** (`VOICE_STATE_UPDATE`) ⇒ livrés aux seuls membres ; le client indexe par guilde et retire l'utilisateur quand `channel_id` est nul.
- **Rendu** : noms/avatars résolus depuis les membres déjà chargés, en **texte échappé**.
- **`ring`** : aucune crypto ni média ⇒ invariant zéro-`ring` serveur intact. *(Le transport média, quand il sera implémenté, vivra dans le nœud `ozone-sfu` où `ring` est admis — hors `ozone-api`.)*

**Bilan** : présence vocale = signalisation reléyée + UI, autorité serveur intacte, **aucun média** donc aucune surface WebRTC/`ring` ajoutée. Aucune faille exploitable. *(Reste : transport média audio/vidéo = sous-projet SFU.)*

## 77. S76 — Sélecteur de statut de présence (panneau utilisateur)

Popover de statut (en ligne / absent / ne pas déranger / invisible) sur le binding **déjà audité** `set_presence` (§39). Le statut **invisible** est affiché localement « hors ligne » (le serveur reste la source de vérité de la diffusion). Aucune nouvelle surface réseau ; rendu **texte/icônes**. `ozone-api` non touché ⇒ **zéro `ring`**. Aucune faille exploitable.

## 78. S77 — Gestion des webhooks (UI)

Modale de webhooks par salon (lister / créer / régénérer / supprimer) sur le CRUD **déjà audité** (§7). **Aucune faille exploitable.**

- **Autorité serveur** : `list`/`create`/`delete`/`regenerate` exigent `MANAGE_WEBHOOKS` **côté serveur** ; un non-autorisé reçoit 403 (affiché « Accès refusé »).
- **Jeton = crédentielle** : le jeton d'exécution n'est renvoyé **qu'à la création/régénération** (cf. §7) ; le client l'affiche **une fois** pour copie (URL `…/api/webhooks/:id/:token`) et **ne le persiste pas** (état composant éphémère, perdu à la fermeture). C'est le comportement attendu (le jeton dans l'URL **est** le secret d'exécution, transmis volontairement à un admin autorisé). La régénération **invalide** l'ancien jeton (serveur).
- **Pas de fuite passive** : la liste des webhooks (sans jeton) n'expose que des métadonnées ; les jetons existants restent masqués (« régénère pour une nouvelle URL »).
- `ozone-api` non touché ⇒ **zéro `ring`**.

**Bilan** : UI relayant le CRUD webhook gardé `MANAGE_WEBHOOKS` ; jeton affiché une seule fois et non persisté ; aucune fuite de secret au-delà du partage volontaire par l'admin. Aucune faille exploitable.

## 79. S78 — Passe de polissage & animations (présentationnel)

Lissage visuel : animations d'entrée/sortie des overlays Radix (tooltips/popovers/menus, via `tailwindcss-animate` + états `data-[state]`/`data-[side]`), modales (backdrop fade + pop-in), fondu au changement de salon, **skeletons** de chargement (démarrage, chat, membres), états vides illustrés, points de saisie animés, barre d'actions en fondu, anneaux de focus clavier (`:focus-visible`), couleur de sélection, transitions douces sur les éléments interactifs. **Aucune logique réseau/auth/état modifiée** ⇒ **aucune nouvelle surface d'attaque**.

- **Dépendance ajoutée** : `tailwindcss-animate` (**plugin build-time** : génère des classes CSS, **aucun code exécuté au runtime** au-delà du CSS ; `npm audit` : 0 vulnérabilité).
- **Reduced-motion** : toutes les animations sont neutralisées par la règle globale `prefers-reduced-motion` existante.
- **Rendu** : inchangé côté données (toujours du React échappé) ; les skeletons/animations n'affichent aucune donnée.

**Bilan** : tranche purement cosmétique, sans incidence sécurité ; dépendance build-only à surface nulle ; invariants intacts.

## 80. S79 — Alignement des tokens sur le vrai Discord (inspection live) + vérif visuelle

Avec l'accord de l'utilisateur, inspection en **lecture seule** des variables CSS réelles de Discord (via l'extension navigateur) pour aligner la palette : accent/hover, statuts (vert/jaune/rouge/lien), greys de texte, surfaces Midnight, rayons, ombres, polices. Tokenisation des **bordures** (`--border`, claires en Midnight pour rester visibles sur le noir) et du **survol de message** (`--message-hover`, indépendant du fond). **Aucune incidence sécurité** : changements **purement CSS/présentationnels** ; aucune donnée personnelle lue/exfiltrée (uniquement `getComputedStyle`/tokens) ; **aucune saisie d'identifiant** par l'assistant (login effectué par l'utilisateur) ; `ozone-api` non touché ⇒ **zéro `ring`**. Vérification visuelle de la coque connectée (dark + midnight) : rendu propre, états vides illustrés, animations OK, séparations nettes sur les deux thèmes.

*Incident corrigé (sans impact run-time)* : un remplacement de masse via `Get-Content/Set-Content` (PS 5.1) avait ré-encodé 20 fichiers source (mojibake UTF-8) ; **intégralement restauré** (inversion du double-encodage + retrait du BOM parasite), build/tests reverts au vert, **zéro résidu** (vérifié par recherche).

**Bilan** : fidélité accrue (valeurs réelles), Midnight lisible, aucune surface d'attaque ; invariants intacts.

## 81. S80 — Thème dark neutre par défaut, police gg sans, persistance de session

Sur retour utilisateur (« ça fait 2020 / trop bleu ») : (a) **cause racine identifiée** — la police retombait sur **Arial** (le stack `font-sans` n'était pas rechargé par le dev-server) ; corrigé en mettant **`gg sans` en tête** (référence CSS, pas de redistribution ; police présente sur la machine, repli **Inter** chargé via `@fontsource`) ; (b) **thème dark par défaut refait en quasi-noir NEUTRE** (gris purs, zéro teinte bleue ; accent blurple conservé ; bordures/survols en blanc translucide) + login en fond uni ; (c) ajout des éléments du « refresh » (en-tête de salon, séparateurs de date, catégories repliables). **Présentationnel — aucune incidence sécurité.**

**Changement fonctionnel** : **auto-refresh du jeton au démarrage** (`boot` tente `refresh` si `me()` échoue avant d'abandonner). Utilise l'endpoint `/auth/token/refresh` **déjà audité** (rotation du refresh token) ; aucune nouvelle surface ni exposition (le jeton rotatif est stocké comme avant — cf. **R10**, stockage `localStorage` toujours à durcir sous Tauri). Améliore l'UX (sessions survivant au rechargement) sans affaiblir la sécurité.

**Bilan** : look modernisé (vraie police, dark neutre), session persistante via binding audité ; invariants (XSS fermé, CSRF nul, zéro-`ring`) intacts.

## 82. S81 — Modal des paramètres (repro Discord) : compte / profil / apparence

Sur demande utilisateur (« regarde la modal des paramètres Discord et reproduis l'utile »), inspection live de la modal Discord puis reproduction : nav groupée (Paramètres utilisateur / de l'app), **Mon compte** (pseudo+e-mail en lecture, **modale changement de mot de passe**, **modale changement d'e-mail**, **suppression de compte**, déconnexion), **Profil** (nom d'affichage, pronoms, bio, couleur d'accent + **aperçu live**), **Apparence** (thème, Cosy/Compact, aperçus média). **Aucune faille exploitable.**

- **Bindings sensibles ré-authentifiés côté serveur** (déjà audités §28) : `change_password` (exige le mot de passe actuel, **révoque les sessions** → le client **déconnecte** après succès), `change_email` (exige le mot de passe), `delete_account` (exige le mot de passe + **double action** via modale dédiée). Le client **relaie** ; les mots de passe saisis vivent dans l'état local du composant le temps de la requête (non persistés) ; erreurs = messages serveur génériques.
- **Profil** : `update_profile` (déjà audité §39) ; bio/pronoms rendus en **texte échappé** ; `accent_color` borné (`number` → hex). Aperçu purement local.
- `ozone-api` non touché ⇒ **zéro `ring`**.

**Bilan** : opérations de compte **entièrement autorisées/ré-authentifiées côté serveur** ; le client relaie + déconnecte aux moments adéquats ; rendu échappé ; aucun secret persisté. Aucune faille exploitable.

## 83. S82 — Thème personnalisé (dégradés de fond) + correctif de purge des thèmes

Thème « Perso » façon Nitro : **dégradé de fond** (presets + 2 sélecteurs de couleur) derrière des **surfaces translucides**, + couleur d'accent ; appliqué en direct (variables CSS posées en inline par le store, persistées `localStorage`). **Présentationnel — aucune incidence sécurité** (valeurs de couleur bornées ; aucune entrée utilisateur libre injectée hors `linear-gradient(...)` construit à partir de hex validés par `<input type=color>`).

**Bug corrigé (sans incidence sécurité)** : Tailwind **purgeait** les règles `@layer base` des classes de thème posées dynamiquement (`theme-light/midnight/custom` absentes du contenu scanné) ⇒ light/midnight retombaient silencieusement sur dark. Réglé par `safelist` dans `tailwind.config`. Vérifié en live (dégradé rendu) + build (`theme-* : présents`).

**Bilan** : thème avancé livré ; régression de thèmes (purge) corrigée ; invariants intacts.

## 84. S83 — Notifications bureau + section Réglages

Notifications bureau (API `Notification` du navigateur) déclenchées par la **Gateway** sur `MESSAGE_CREATE` pour les **mentions** et **MP**, uniquement si activées (permission demandée à l'activation), si la fenêtre est en arrière-plan (`document.hidden`) ou le salon non actif, et **en respectant la sourdine** (`isMuted` salon + guilde). Section Réglages > Notifications (activation + état de permission + rappel sourdine). **Aucune faille exploitable.**

- **Données** : la notification n'affiche que des messages **déjà reçus** par l'utilisateur via la Gateway (aucune nouvelle donnée/surface) ; contenu tronqué, rendu par l'OS (le navigateur gère l'affichage). Activation = **opt-in explicite** + permission navigateur.
- **Sourdine** : réglages propres à l'utilisateur (`set_*_notification`, audités §64) ; le filtrage côté client ne fait que respecter ces préférences.
- `ozone-api` non touché ⇒ **zéro `ring`**.

**Bilan** : notifications opt-in, déclenchées par des données déjà reçues, respectant la sourdine ; aucune nouvelle surface ni secret. Aucune faille exploitable.

## 85. S84 — Client vocal/vidéo fonctionnel (WebRTC ↔ `ozone-sfu`)

Sur demande utilisateur (« la partie voix & vidéo fonctionnelle »), implémentation du **plan média** côté client contre le **vrai SFU webrtc-rs** : `lib/voice.ts` (getUserMedia → `RTCPeerConnection` → offre **ICE complet** car le SFU est non-trickle → `POST /sfu/rooms/:cid/peers {sdp, token}` → réponse SDP → pistes distantes relayées ; audio joué via `<audio>` cachés, vidéo exposée à l'UI), câblage store (`joinVoice`/`leaveVoiceChannel`/`toggleSelfMute`/`toggleSelfDeaf`/`toggleSelfVideo`/`reconnectVoice`), et scène d'appel `VoiceStage` (tuiles participants/caméras + barre de contrôle). Côté serveur, seul `OZONE_VOICE_ENDPOINT` rendu configurable (présentationnel : le client signale via le **proxy Vite `/sfu`**, pas via la valeur `endpoint`). **Vérification adversariale du plan média : 7/7.**

- **Authentification du plan média (cœur)** — le SFU **fail-closed** sans `OZONE_VOICE_SECRET` (refus `503`), sinon vérifie le **jeton vocal** (JWT HS256 **pur-Rust**, `ozone-proto::token`, **sans `ring`**) sur 4 axes, tous testés en live (`scripts/sfu-auth-probe.mjs`) :
  | Cas | Attendu | Obtenu |
  |---|---|---|
  | aucun jeton / illisible | 401 | **401** |
  | bon format, **mauvais secret** (forge de signature) | 401 | **401** |
  | bon secret, **mauvais `kind`** (jeton `access` réutilisé) | 401 | **401** |
  | bon secret/kind, **mauvais salon** (`sub.cid ≠ room`) | 403 | **403** |
  | **jeton expiré** | 401 | **401** |
  | **jeton valide** (auth passe → 400 sur SDP bidon) | 400 | **400** |
  Identité (signature), finalité (`kind="voice"`), **portée** (`sub = "<user>.<channel>"`, `channel` doit == `room`) et expiration sont **toutes** imposées. `DELETE` (quitter) repasse par la même `authorize()` **+** vérifie la **propriété du pair** (`peer_id` appartient à `uid`, sinon 403) → on ne peut éjecter le pair d'autrui.
- **Émission du jeton (API)** — `update_own_voice_state` ne mint le jeton (TTL **3600 s**, `sub = "<me>.<cid>"`) **qu'après** `require_channel_perm(VIEW_CHANNEL|CONNECT)` (audité §… permissions) ; livré **au seul intéressé** (réponse de jonction + `VOICE_SERVER_UPDATE` en **portée user**). Le secret n'apparaît jamais côté client ; seul le jeton court-vécu et scellé transite.
- **Invariant zéro-`ring`** — `ozone-api` mint via HS256 pur-Rust (aucun `ring`) ; `webrtc-rs`/`ring` restent **cantonnés à `ozone-sfu`** (nœud média séparé, process distinct, bind `127.0.0.1:8081`). Invariant **respecté et confirmé** par le build (`ozone-api` compile sans `ring`).
- **Client** — aucune saisie de secret/identifiant ; **STUN public** uniquement (pas de TURN ; candidats hôte suffisent en local) ; média **micro/caméra sous permission explicite** du navigateur (`getUserMedia`) ; à la déconnexion le pair est **retiré du SFU** (DELETE + jeton) et toutes les pistes locales **stoppées**. Aucune donnée perso en URL (le jeton voyage en **corps** POST ; en query pour DELETE, mais c'est un jeton éphémère scellé, pas une donnée perso — cf. politique URL).
- **Limite connue (disponibilité, non sécurité)** — le SFU **ne pousse pas** de renégociation : un membre déjà présent ne reçoit pas spontanément les pistes d'un arrivant. Contourné par un **resync débounce** (le client se reconnecte quand un *autre* membre rejoint **son** salon). Les pistes vidéo distantes ne sont pas mappables à un `user_id` précis côté SFU → affichées en tuile « Caméra » générique (cosmétique).

**Bilan** : plan média **entièrement authentifié** (identité/finalité/portée/expiration, fail-closed, propriété du pair), jeton court-vécu scellé émis après contrôle de permission et livré au seul intéressé, **zéro-`ring`** préservé (`ring` cantonné au SFU), média sous permission explicite, nettoyage à la déconnexion. **Aucune faille exploitable** (7/7 au probe adversarial). Reste différé : **E2EE média** (DAVE/MLS) et TURN/production.

## 86. S85 — Attribution des flux par membre (SFU) + qualité/latence vocale

Deux améliorations média : (A) **qualité vocale + faible latence** (client seul) ; (B) **attribution des pistes relayées à leur propriétaire** côté SFU (pour afficher la caméra de chacun sur SA tuile, et distinguer micro/caméra/écran). Probe adversarial du plan média **toujours 7/7** après reconstruction ; whitelist de nature **testée unitairement** (Rust).

- **(A) Client uniquement, aucune surface serveur** : contraintes micro (AEC/NS/AGC, mono), réglage Opus par *SDP munging* sur **l'offre** (`useinbandfec`/`usedtx`/`stereo=0`/`maxaveragebitrate`), `setParameters` (débit plafonné, **priorité réseau audio haute**), `playoutDelayHint=0`, `bundlePolicy:max-bundle`. **Présentationnel/QoS — aucune incidence sécurité** ; le SFU **relaie** le RTP sans transcoder (les paramètres d'encodage de l'émetteur ne changent pas le modèle de confiance). `tuneOpus` couvert par 4 tests unitaires.
- **(B) Anti-usurpation d'identité — invariant central** : le `stream_id` de la piste relayée encode `"<uid>.<kind>"`. **L'`uid` est imposé par le serveur** (`peer.owner`, extrait du **jeton vocal vérifié** — même chemin `authorize()` audité §85), **jamais** par le client. Le client ne fournit qu'un **manifeste `id_de_piste → nature`** ; la nature est **filtrée par liste blanche** (`sanitize_kind` → `&'static str` parmi `mic`/`cam`/`screen`, sinon défaut selon le média). Conséquences vérifiées :
  - Un pair **ne peut pas** attribuer son flux à l'`uid` d'autrui (il ne contrôle pas l'`uid`) → pas de *spoof* « c'est la caméra d'Alice ».
  - Une **nature malveillante** (`"cam\r\na=evil:1"`, chemins, vide…) **ne se retrouve jamais** dans le `stream_id`/SDP (retour `&'static`) → **pas d'injection SDP**. *(test `sanitize_kind_whitelists_only_known_natures`.)*
  - Le manifeste est lu **après** `authorize()` (auth inchangée) ; un manifeste hostile ne contourne rien et ne provoque pas d'erreur 5xx (probe : jeton valide + SDP bidon → 400, jamais de bypass).
- **Zéro-`ring`** : changements cantonnés au SFU (`ozone-sfu`) ; `ozone-api` non touché.

**Bilan** : attribution par membre **sûre par construction** (identité serveur, nature en liste blanche, aucune chaîne client dans le SDP), auth média intacte (7/7), QoS vocale améliorée sans incidence sécurité. **Aucune faille exploitable.** *(Correction fonctionnelle de l'affichage caméra-par-personne à confirmer en live avec deux comptes — propriété de design : le `stream_id` d'une `TrackLocalStaticRTP` devient l'`id` du `MediaStream` reçu par l'abonné.)*

## 87. S86 — Scène vocale (polish + détection de parole) & partage d'écran (Go Live)

Deux tranches **côté client uniquement** (aucun nouveau code serveur) : (C) refonte visuelle de la scène d'appel + **détection de parole** ; (D) **partage d'écran** (Go Live). Le plan média serveur est **inchangé** ⇒ probe adversarial **toujours 7/7** (régression).

- **(C) Détection de parole** : un `AnalyserNode` Web Audio par flux audio (mon micro + chaque flux distant **déjà reçu**, étiqueté par uid via la tranche B), RMS échantillonné localement → anneau « parle ». **Aucune donnée nouvelle, aucune sortie réseau** : on n'analyse que de l'audio **déjà présent** dans le client ; le tap `AnalyserNode` est passif (non relié à la sortie → pas de double son). Présentationnel — **aucune incidence sécurité**.
- **(D) Partage d'écran** : `getDisplayMedia` (**la sélection de la source est imposée par le navigateur** — l'app ne choisit ni ne voit l'écran sans geste explicite de l'utilisateur), publié comme **piste vidéo `kind=screen`** via le **même chemin authentifié** que la caméra (manifeste → `sanitize_kind` **liste blanche**, uid **imposé par le jeton**). Arrêt natif (« Arrêter le partage ») intercepté → coupure propre + reconnexion sans la piste. **Aucune nouvelle surface serveur** : `kind=screen` est déjà couvert et **testé** (B : `sanitize_kind_whitelists_only_known_natures`) ; l'attribution par uid empêche d'usurper « l'écran d'Alice ».
- **Nettoyage** : à la déconnexion / arrêt, les pistes écran (et caméra) sont **stoppées** (`track.stop()`) ; aucun flux ne survit.
- `ozone-api`/`ozone-sfu` **non modifiés** par ces tranches ⇒ **zéro-`ring`** et plan média audité (§85-86) **inchangés**.

**Bilan** : refonte UI et partage d'écran **sans nouvelle surface serveur** ; capture d'écran **gated navigateur** ; flux écran soumis au **même modèle d'attribution sûr** que la caméra ; détection de parole purement locale. Auth média **7/7** (régression). **Aucune faille exploitable.**

## 88. S87 — Profil de serveur enrichi (bannière, jeux, profil privé) + images icône/bannière

Slice serveur : `guilds` gagne `banner_color`/`banner_id`/`games`/`private_profile` (migration 0018) ; `update_guild` les persiste ; nouveaux endpoints **upload** (`POST /guilds/:id/images`) et **service public** (`GET /guilds/:id/icon|banner`). **Probe adversarial : 5/5.**

- **Autorisation** : `update_guild` et `upload_guild_image` exigent **MANAGE_GUILD** (`require_guild_perm`) ; sans jeton → **401** (extracteur `AuthUser`). *(probe 1, 4)*
- **Pas de traversée de chemin** : les fichiers sont localisés **par `image_id` numérique** (parse `i64`) ; un id non numérique ou une tentative `..%2f..` → **400**, jamais d'accès disque arbitraire. *(probe 3, 5)*
- **Pas d'injection / surcharge** : `games` est **filtré** (clés `[A-Za-z0-9_-]`, ≤40 car., **≤12** entrées) puis sérialisé via `serde_json` (jamais de concaténation SQL/JSON) ; `banner_color` est un `i64` ; `private_profile` un booléen. Le `name`/`description` gardent leurs bornes (100/300).
- **Upload sûr** : limite **2 Mio** (`DefaultBodyLimit` + contrôle), **validation par octets magiques** (png/gif/webp/jpeg, détecteur réutilisé des emojis), écriture sous `upload_dir/<id>` numérique.
- **Service décoratif** : `serve_guild_icon`/`banner` sont **publics** (comme les emojis — chargés en `<img>`), `X-Content-Type-Options: nosniff` + CSP `default-src 'none'; sandbox`, et ne servent que l'image **désignée par la guilde** (pas d'énumération de fichiers tiers). Guilde/Icône absente → **404** propre (pas de 500). *(probe 2)*
- **Profil privé** : le drapeau est stocké et exposé dans le DTO `Guild`. L'**application complète** (masquer le profil aux non-membres lors d'un clic sur emoji custom / aperçu d'invitation) reste à brancher dans les chemins de lecture concernés — **dette notée** (R13).
- **Zéro-`ring`** : aucune dépendance crypto ajoutée ; pipeline d'image en pur Rust (octets magiques + écriture disque).

**Bilan** : nouveaux champs/endpoints de profil **autorisés (MANAGE_GUILD), validés et bornés**, upload restreint et typé, service public décoratif sans fuite de fichiers ni traversée. **Aucune faille exploitable** (5/5). Reste : enforcement runtime du profil privé (R13).

**Suivi (icône rail + bannière en direct)** : `update_guild` est désormais **rate-limité** (5 / 10 min glissantes / guilde, en mémoire) — l'icône/bannière se propageant à **tous** via `GUILD_UPDATE`, on évite le matraquage (réponse **429** au-delà). Toujours MANAGE_GUILD ; le compteur en mémoire se réinitialise au redémarrage (anti-spam, pas un contrôle de sécurité critique). Côté client, l'icône/bannière sont servies en **lecture publique décorative** (déjà couvert) et rafraîchies via `?v=<image_id>` (cache-bust). Aucune nouvelle surface.

## 89. S88 — Page Membres : méthode d'adhésion (code d'invitation)

Slice serveur : `guild_members` gagne `invite_code` (migration 0019) ; `join_invite` y enregistre le code **vérifié** (l'invitation est résolue avant la jonction) ; `Member` expose `joined_via`. UI : table des membres (nom, membre depuis, ancienneté du compte via snowflake, **méthode**, rôles, actions ⋮).

- **Pas de spoof** : `invite_code` stocké = le code **réellement résolu** côté serveur lors de la jonction (lookup de l'invitation), jamais une valeur arbitraire fournie par le client.
- **Divulgation maîtrisée (corrigé en revue)** : `list_members` sert aussi la **liste latérale** (tous les membres). Exposer la méthode d'adhésion (codes d'invitation) à tous fuirait des codes encore valides. ⇒ `joined_via` n'est renvoyé **qu'aux porteurs de MANAGE_GUILD** (on capture les permissions effectives retournées par `require_guild_perm`, et on met `None` sinon). Les membres ordinaires ne voient pas les codes.
- **Autorisation** : `list_members` exige toujours VIEW_CHANNEL (membre) ; actions par membre (expulser/bannir/rôles) déjà auditées (KICK/BAN/MANAGE_ROLES), propriétaire protégé. `/members` sans jeton → **401** (vérifié).
- **Ancienneté du compte** : calculée **côté client** depuis le snowflake (epoch 2025-01-01) — aucune donnée serveur supplémentaire exposée.
- **Zéro-`ring`** ; pas de nouvelle dépendance.

**Bilan** : méthode d'adhésion fiable (code serveur, pas client), **révélée aux seuls gestionnaires** (fuite de codes évitée), reste des actions déjà autorisées. **Aucune faille exploitable.** *(Hors périmètre, non implémentés : « Les signalements » — pas de système de signalement — et « Congédier »/prune — pas de suivi d'inactivité.)*

## 90. S89 — Page Rôles : liste + éditeur à onglets (Affichage / Permissions / Gérer les membres, bulk add/remove)

Slice **côté client uniquement** : nouveau composant `RolesPage` (vue liste → éditeur à onglets) remplaçant `RolesModal` ; deux actions store de rafraîchissement (`refreshRoles`/`refreshMembers`) qui n'appellent que des **GET existants** (`listRoles`/`listMembers`). **Aucun nouvel endpoint, aucune migration, `ozone-api`/`ozone-sfu` non touchés.** Toutes les mutations réutilisent des endpoints **déjà audités** : `createRole`/`updateRole`/`deleteRole` + `add_member_role`/`remove_member_role`.

- **Le serveur reste la frontière d'autorisation** : chaque appel (création/édition/suppression de rôle, ajout/retrait d'un membre) est **individuellement** soumis à `require_guild_perm(MANAGE_ROLES)` **et** au contrôle de **hiérarchie** (`actor_pos <= role_pos` → 403). Le bulk add/remove **itère** ces appels unitaires côté client : **aucun contournement** (pas d'endpoint en lot qui shunterait la vérif par membre) ; un membre non autorisé reçoit 401/403 par appel.
- **Gating client = défense en profondeur, pas l'enforcement** : `canEditRole` (hiérarchie/`managed`) et le verrouillage des permissions non détenues ne sont **que cosmétiques**. Le backend re-vérifie et **masque les permissions** via `permissions & actor` dans `update_role` — un client modifié ne peut donc **pas s'octroyer** une permission qu'il ne détient pas, ni éditer un rôle ≥ au sien.
- **Correctif `@everyone`** : le rôle `@everyone` (id == guildId) est créé `managed=1` mais **doit rester éditable** (permissions par défaut) ; le gating client le traite explicitement (les autres rôles `managed` — bots/intégrations — restent verrouillés). Le backend autorise déjà l'édition de `@everyone` (seule la hiérarchie s'applique ; `role_pos=0`) et **interdit sa suppression** (`rid == gid → 400`). Aucune élévation : `permissions & actor` borne toujours ce qui est accordé.
- **Pas de divulgation nouvelle** : la modale « Ajouter des membres » liste des membres déjà obtenus via `listMembers` (déjà servie à tout membre pour la liste latérale) ; aucun champ sensible supplémentaire (les codes `joined_via` restent gated MANAGE_GUILD, §89). Décompte des membres par rôle calculé **côté client** depuis la liste déjà chargée.
- **Robustesse UI** : mutations optimistes avec **rollback** en cas d'échec (toggle de rôle, bulk), barre d'enregistrement explicite (brouillon vs rôle) ; ESC de la sous-modale capturé (`stopImmediatePropagation`) pour ne pas fermer les Paramètres. Aucune chaîne client injectée dans une URL ; pas de secret exposé.
- **Zéro-`ring`** ; aucune dépendance ajoutée.

**Bilan** : tranche **purement cliente** réutilisant des endpoints rôles **déjà audités** (MANAGE_ROLES + hiérarchie + `perms & actor`) ; le bulk n'ouvre **aucune** surface nouvelle (itération d'appels unitaires autorisés) ; `@everyone` éditable sans élévation ni suppression. Le gating front est cosmétique, l'enforcement reste serveur. **Aucune faille exploitable.** Vérifié en live (création, édition couleur/nom + save bar, permissions catégorisées, bulk add/remove, `@everyone` permissions). *(Hiérarchie multi-rôles à confirmer avec un second compte non-propriétaire.)*

## 91. S90 — Style de couleur de rôle (dégradé / néon / vague) + couleur du pseudo uniquement

Deux changements : (A) **client seul** — dans la liste des membres, l'en-tête de groupe de rôle n'est **plus coloré** ; seule la couleur du **pseudo** change (fidélité Discord). (B) **slice serveur** — les rôles gagnent `secondary_color` (couleur secondaire) et `color_style` (`solid`|`gradient`|`neon`|`wave`) pour des noms dégradés/animés, appliqués au pseudo (liste des membres) et à l'auteur des messages. Migration **0020** (`ALTER TABLE roles ADD secondary_color INTEGER`, `color_style TEXT NOT NULL DEFAULT 'solid'`), additive, vérifiée en live (les rôles existants → `NULL`/`'solid'`). Test d'intégration rôles **toujours vert** (`roles_and_overwrites_crud`).

- **Style en liste blanche (anti-injection)** : `color_style` n'est **jamais** stocké tel quel depuis le client — `sanitize_color_style()` le réduit à un `&'static str` ∈ {`solid`,`gradient`,`neon`,`wave`} (valeur inconnue → `solid`). À l'`update`, l'absence du champ conserve l'existant (lui-même re-validé). **Aucune chaîne arbitraire** n'atteint la base ni le rendu.
- **Pas d'injection CSS** : côté client, le style est construit **uniquement** à partir d'entiers couleur (`roleColorHex` masque `& 0xffffff` → 6 hexadécimaux) et d'un `color_style` en liste blanche mappé vers des fragments CSS **fixes** (`linear-gradient`, `text-shadow`, classe d'animation `.role-name-wave`). Aucune valeur contrôlée par l'utilisateur n'est concaténée dans une feuille de style → **pas de CSS/style injection**, pas de XSS.
- **Autorisation inchangée** : `secondary_color`/`color_style` passent par `create_role`/`update_role` déjà audités (MANAGE_ROLES + hiérarchie + `permissions & actor`). Aucun nouvel endpoint. `@everyone` peut recevoir un style comme tout champ, sans élévation (les permissions restent bornées par `& actor`).
- **`secondary_color` borné** : entier (`Option<u32>` → `i64`), rendu masqué `& 0xffffff` ; pas de débordement exploitable. `NULL` ⇒ style uni.
- **Cosmétique pur (A)** : retrait de la couleur sur l'en-tête de groupe = changement d'affichage sans incidence donnée/sécurité.
- **Zéro-`ring`** ; aucune dépendance ajoutée.

**Bilan** : style de rôle **validé par liste blanche** côté serveur, rendu CSS construit à partir d'entiers masqués et de fragments fixes ⇒ **pas d'injection de style ni XSS** ; autorisation des rôles inchangée (réutilise les chemins audités) ; migration additive sûre. **Aucune faille exploitable.** Vérifié en live (rôle « Modérateur » passé en *vague* rouge→bleu : pseudo dégradé animé dans la liste des membres et sur l'auteur des messages, en-tête de groupe resté gris).

## 92. S91 — Réorganisation des rôles (hiérarchie) + endpoint de positions

Nouvel endpoint **`PATCH /guilds/:guild_id/roles`** (`reorder_roles`) : accepte la liste **complète** des id de rôles hors `@everyone`, du plus haut au plus bas, et recalcule les positions (`n..1` ; `@everyone` figé à `0`). Drag-and-drop côté client dans **les deux** listes (page liste + liste de l'éditeur), optimiste puis persisté. L'ordre **compte** : la hiérarchie est déjà appliquée sur les actions fortes (kick/ban/édition de rôle) ; cette tranche permet enfin de **contrôler** cet ordre. Probes adversariales **5/5**.

- **Hiérarchie déjà en place (rappel, inchangé)** : `kick_member` (routes_guild) et `ban_member` (routes_moderation) exigent `actor_pos > target_pos` (sinon **403** « ce membre est au-dessus ou égal à vous ») ; `highest_role_position` renvoie `i32::MAX` pour le propriétaire et `0` pour @everyone. ⇒ **un membre ne peut pas cibler quelqu'un au-dessus de lui** ; le propriétaire est intargetable. La réorganisation alimente précisément ce comparateur.
- **Endpoint protégé** : `require_guild_perm(MANAGE_ROLES)` (sans jeton → **401** via `AuthUser`, blocage confirmé). 
- **Anti-élévation (hiérarchie du réordonnancement)** : pour un non-propriétaire d'`actor_pos = P`, le serveur **rejette (403)** toute opération qui (a) placerait un rôle à une position `≥ P`, ou (b) déplacerait un rôle déjà `≥ P`. Un porteur de MANAGE_ROLES **ne peut donc pas** hisser un rôle (ni le sien) au niveau/au-dessus du sien pour ensuite cibler des membres plus haut placés. Le propriétaire (`i32::MAX`) n'est pas contraint.
- **Validation stricte de l'entrée (permutation exacte)** : `ids` doit être **exactement** l'ensemble des rôles hors @everyone, **sans doublon** ; sinon **400**. Probes live : liste incluant `@everyone` → **400**, doublon → **400**, id étranger → **400**, liste incomplète → **400**. Empêche un client de « perdre » des rôles, d'en injecter, ou de corrompre les positions.
- **@everyone immuable en position** : son id est interdit dans `ids` (400) et reste à `0` (plancher de hiérarchie). Sa suppression est toujours interdite (§90).
- **Atomicité** : mises à jour des positions dans une **transaction** (`begin`/`commit`) ; positions résultantes uniques (`n..1`) → pas d'état incohérent ni de collision. Diffusion `GUILD_ROLE_UPDATE` par rôle ; les clients rafraîchissent l'ordre.
- **Client = confort, pas barrière** : DnD désactivé sur les rôles non éditables et pendant une recherche (liste partielle) ; mais c'est le **serveur** qui tranche (permutation + hiérarchie). Mutation optimiste avec **retour à la vérité** via `refreshRoles` (y compris en cas de 403/400).
- **Zéro-`ring`** ; aucune dépendance ; pas de nouvelle table (réutilise `roles.position`).

**Bilan** : réordonnancement **autorisé (MANAGE_ROLES), borné par la hiérarchie (anti-élévation) et strictement validé (permutation exacte)** ; `@everyone` ancré à 0 ; transactionnel. La hiérarchie sur kick/ban/édition (déjà auditée) devient pleinement pilotable sans permettre à un modérateur de cibler plus haut que lui. **Aucune faille exploitable** (probes 5/5). Vérifié en live (drag dans la page liste **et** dans l'éditeur, positions persistées en base). *(Anti-élévation non-propriétaire à reconfirmer avec un second compte.)*

## 93. S92 — Vocal « flawless » : renégociation WebRTC (canal WS de signalisation, SFU)

Slice **SFU** (`ring` autorisé, processus séparé — `ozone-api` **non modifié**) : ajout d'un **canal WebSocket de signalisation par pair** (`GET /sfu/rooms/:room/peers/:peer_id/signal?token=`) portant la **renégociation** (negotiation parfaite, SFU « impoli »). Élimine le rechargement complet du flux : (dés)activer caméra/écran et l'arrivée/cam d'un autre membre passent désormais par une **renégociation poussée**, sans reconnexion. Chaque pair a une **tâche-acteur** sérialisant toute sa signalisation. Tests SFU verts ; vérifié en live (2 onglets : toggle cam local sans reconnexion **et** push serveur à l'arrivée/cam d'un 2ᵉ pair, **zéro** `POST /peers`).

- **Authentification du WS = identique à l'HTTP** : `authorize()` vérifie la **signature** du jeton vocal (`OZONE_VOICE_SECRET`, fail-closed), l'**expiration** et la **correspondance du salon**. *Probes : jeton bidon → **401**, sans jeton → **401** (jamais d'upgrade 101).* Puis `signal_handle` exige que l'**uid du jeton possède le `peer_id`** (sinon **403**) → un membre du salon ne peut pas détourner le canal d'un autre pair.
- **Identité toujours imposée par le serveur** : la renégociation ne change rien au modèle §85-86 — le `stream_id` relayé reste « `<uid>.<kind>` » avec `uid` issu du **jeton** et `kind` **filtré par liste blanche** (`sanitize_kind`, `&'static str`). Le manifeste mis à jour aux offres client ne fournit que la **nature** (étiquette), jamais l'identité → **aucune usurpation**, **aucune injection** dans le SDP.
- **`unpublish` borné au demandeur** : l'acteur est lié à **son** `peer_id` ; `unpublish` ne parcourt que la **liste publiée de ce pair** et ne retire que **ses** pistes (chez les autres). Un `id` forgé ne peut matcher que les pistes du demandeur → **pas de retrait croisé**.
- **Glare maîtrisé (negotiation parfaite)** : SFU **impoli** (ignore l'offre client hors état `Stable` ; le client **poli** rollback + ré-offre). Sérialisation par acteur ⇒ pas de `create_offer`/`set_remote` concurrents → pas d'état SDP incohérent.
- **Repli sûr (anti-régression)** : toute défaillance de la signalisation (WS fermé/échec de renégociation) déclenche `onNeedsReconnect` → **reconnexion complète** (comportement historique). Le `VOICE_STATE_UPDATE` ne resync (reconnexion) **que** si le WS n'est pas sain. ⇒ le vocal **ne peut pas régresser** sous l'état actuel.
- **Jeton dans l'URL du WS** : comme le `DELETE` existant (query `?token=`), jeton vocal **éphémère** (≈1 h) ; exposition mineure (journaux). Cohérent avec l'existant.
- **Dev** : le WebSocket de signalisation vise **directement** le SFU (8081) car le proxy ws de Vite n'aboutit pas vers une cible http:// ; en prod, même origine. **Aucune incidence sécurité** (auth SFU inchangée ; pas de CSP affaiblie).
- **Zéro-`ring` dans l'API** : `ozone-api` intact ; tout le WebRTC reste cantonné au SFU.

**Dette/durcissement** : pas de **rate-limit** sur les renégociations WS (un membre authentifié pourrait spammer des offres → charge SFU). À plafonner (par pair). Les **pairs orphelins** (client rechargé sans `DELETE`) subsistent jusqu'à l'échec ICE serveur — prévoir un GC par état de connexion. *(Le rendu visuel d'une **caméra distante** n'a pu être confirmé qu'au niveau protocole : le test 2-onglets partage le même `uid` (Hayato), donc l'UI fusionne les tuiles ; à reconfirmer avec deux comptes distincts.)*

**Bilan** : nouveau canal WS **authentifié (jeton vocal) et borné à la propriété du pair** ; identité serveur + nature en liste blanche inchangées ; `unpublish` non-croisé ; glare maîtrisé ; **repli reconnexion** garantissant l'absence de régression. **Aucune faille exploitable** (probes auth 2/2 ; relais/push vérifiés en live sans reconnexion). Durcissements notés : rate-limit renégociation + GC pairs orphelins.

**Suivi (S92b — connexion vocale ≤1 s, suppression de STUN)** : pour réduire le temps de join, le **STUN externe** (`stun.l.google.com`) est retiré côté client **et** SFU ; seul le rassemblement des **candidats hôtes** (localhost/LAN) est utilisé, et l'attente ICE est plafonnée (≈600 ms, sortie anticipée au 1ᵉʳ candidat). **Incidence sécurité : neutre à positive** — le chiffrement **DTLS-SRTP** du média reste **inchangé** (non désactivable côté navigateur, et ce n'était pas la source de latence) ; on **n'envoie plus** d'infos réseau à un tiers (Google STUN) et on supprime une dépendance externe. Le seul compromis est **fonctionnel** (pas de traversée NAT inter-réseaux → à rétablir via STUN/TURN pour un déploiement hors LAN), explicitement accepté par l'utilisateur. **Mesuré en live : ICE connecté en ≈280 ms** (rassemblement 258 ms) depuis le clic. Auth média et modèle d'attribution **inchangés**. **Aucune faille introduite.**

## 94. S93 — @everyone explicite à l'arrivée + menu clic-droit serveur + mises à jour rôles/membres en direct

Trois éléments : (A) **slice serveur** — le rôle `@everyone` (id == guild_id) est désormais inscrit dans `member_roles` à **chaque adhésion** (`create_guild`, `join_invite`, `discovery`) + migration **0021** de rattrapage des membres existants ; (B/C) **client seul** — menu clic-droit sur l'icône de serveur (réutilise les actions déjà gated) + **handlers Gateway** pour `GUILD_ROLE_*` / `GUILD_MEMBER_*` (permissions et affichages gated **en direct**).

- **(A) Aucune élévation, aucune entrée utilisateur** : l'`INSERT OR IGNORE INTO member_roles` n'utilise que `guild_id`/`user_id` **dérivés du contexte authentifié** (jamais du corps de requête) et `role_id == guild_id` (le rôle `@everyone` qui existe déjà, perms `DEFAULT_EVERYONE` inchangées). Idempotent. **Ne change pas** les permissions effectives : `permsIn`/`guild_permissions` ajoutaient déjà `@everyone` **implicitement** → rendre l'appartenance explicite est cosmétique côté autorisation (dédup par Set/OU de bits). Migration 0021 = `INSERT … SELECT` depuis `guild_members`, additive et sûre.
- **Affichages robustes** : tout le client filtre déjà `@everyone` (`r.id !== guildId`) dans la liste des membres, le profil, le tableau Rôles → l'ajout de `guild_id` dans `member.roles` n'introduit ni doublon visible ni rôle « fantôme ». `highest_role_position` ignore `@everyone` (position 0) → hiérarchie inchangée.
- **(B) Menu clic-droit = mêmes garde-fous** : `GuildContextMenu` ne fait qu'**ouvrir** des actions/modales déjà auditées (Invitations, Paramètres, Créer salon/événement, Sourdine) en réutilisant exactement le **gating de permissions** du menu d'en-tête (`canIn`/`permsIn`). « Copier l'identifiant » et « Marquer comme lu » sont locaux (presse-papiers / read-state) sans appel privilégié. **Aucune nouvelle surface serveur.**
- **(C) Live = consommation d'événements déjà émis** : les handlers Gateway `GUILD_ROLE_CREATE/UPDATE/DELETE` et `GUILD_MEMBER_UPDATE/ADD/REMOVE` ne font que **mettre à jour le cache local** (`rolesByGuild`/`membersByGuild`) à partir d'événements **déjà publiés** par des endpoints autorisés (scope `Guild` ⇒ uniquement les membres de la guilde les reçoivent). Conséquence **sécurité positive** : si un mod **retire un rôle/une permission** à un utilisateur, l'UART de ce dernier (menus, accès Paramètres, actions) se **restreint en direct** au lieu de rester ouverte jusqu'au prochain rechargement. Le client reste **non-autoritaire** : le serveur revérifie chaque action.
- **Vérifié en live** : `@everyone` rattrapé pour tous les membres (base) ; clic droit sur l'icône serveur → menu complet (propriétaire) ; **hot-swap inter-onglets** — un changement de rôle dans l'onglet B se reflète **instantanément** dans l'onglet A **inactif** (couleur de pseudo passée live de teal à rose via `GUILD_ROLE_UPDATE`, sans rechargement).
- **Zéro-`ring`** ; pas de nouvel endpoint ; pas de nouvelle dépendance.

**Note (durcissement mineur)** : `remove_member_role` n'a pas le garde `rid == gid` (présent sur `add_member_role`) ; un retrait de `@everyone` est sans effet sur les permissions (réajouté implicitement par `permsIn`/`guild_permissions`) et l'UI ne le propose jamais — **non exploitable**, garde de cohérence à ajouter ultérieurement.

**Bilan** : appartenance `@everyone` explicite **sans incidence d'autorisation** (entrées serveur, idempotent, migration additive) ; menu clic-droit **réutilisant le gating existant** ; mises à jour rôles/membres **en direct** qui **resserrent** les permissions affichées en temps réel (gain de sécurité). **Aucune faille exploitable.**

## 95. S94 — Propagation du profil public en direct (`USER_UPDATE`) + audit du temps réel

Audit « tout-en-live » : croisement événements **publiés** (serveur) vs **consommés** (store). **Gap majeur trouvé** : `update_profile` ne publiait **aucun** événement → pseudo/avatar/bio restaient périmés chez les amis, membres de guilde, destinataires de MP et auteurs de messages jusqu'à un rechargement. **Fix** : `broadcast_user_update` (gateway) émet `USER_UPDATE` ; handler client met à jour les caches (`me`, `relationships`, `dms`, `membersByGuild`, `messagesByChannel`).

- **Aucune donnée privée diffusée** : la charge ne contient que le **profil public** (`id`, `username`, `display_name`, `avatar_id`) — **jamais** l'e-mail ni un champ sensible. Construite via `serde_json` à partir de valeurs DB (pas de concaténation → pas d'injection).
- **Pas de divulgation nouvelle (portée = qui voit déjà le profil)** : `USER_UPDATE` n'est envoyé qu'à **soi**, ses **amis** (relation `friend`, deux sens), ses **co-destinataires de MP**, et les **guildes partagées** — exactement les acteurs qui peuvent **déjà** voir ce profil public (liste de membres, liste d'amis, MP, `get_profile`). Aucune fuite vers un tiers non autorisé. Les portées `User`/`Guild` filtrent la livraison via `should_deliver` (membre de la guilde / destinataire).
- **Déclenchement borné à soi-même** : `update_profile` est protégé par `AuthUser` et n'agit que sur **son** `uid` → un utilisateur ne peut pas provoquer la diffusion du profil d'autrui ni forger l'identité diffusée (l'`uid` vient du jeton, le contenu de la table `users`).
- **Idempotent côté client** : un destinataire ami **et** membre d'une guilde partagée reçoit l'événement deux fois → le handler ne fait que recopier les mêmes champs (sans effet de bord). Requêtes de fan-out **paramétrées**.
- **Vérifié en live** : `PATCH /users/@me {display_name}` (200) → la **liste des membres** (qui lit `membersByGuild`, non touché par l'appel) affiche le nouveau pseudo **instantanément** (« Hayato » → « Hayato★Live » → « Hayato »), sans rechargement.
- **Gaps mineurs notés** (publiés mais non consommés, ou rafraîchis à l'ouverture d'une modale — donc acceptables) : `THREAD_CREATE`, `GUILD_SCHEDULED_EVENT_*`, `MESSAGE_DELETE_BULK`, `CHANNEL_PINS_UPDATE`, `CHANNEL_RECIPIENT_ADD/REMOVE`, `GUILD_BAN_ADD/REMOVE`. À brancher au besoin (faible priorité).
- **Durcissement** : pas de **rate-limit** sur `update_profile` (un spam de mises à jour ⇒ rafales de diffusion) — à plafonner comme pour l'icône de guilde (§88). **Zéro-`ring`** ; pas de nouvel endpoint.

**Bilan** : profil public propagé **en direct** aux seules vues légitimes (gain UX) ; **aucune donnée privée**, **aucune divulgation nouvelle**, déclenchement borné à soi. **Aucune faille exploitable.** Reste : rate-limit profil + branchement des événements mineurs.

## 96. S95 — Interface MP/Groupes : panneau profil + « Ajouter au MP » + endpoint `mutual`

UX MP de type Discord : panneau profil à droite (1:1), bouton « Ajouter au MP » → création de **groupe**, toggle d'affichage, et nouvel endpoint **`GET /users/:user_id/mutual`** (guildes & amis en commun). Pins préexistants. Tout le reste est **client** (réutilise `openDM`/`refreshDMs` déjà audités).

- **`mutual` borné aux données du demandeur** : `AuthUser` requis. Les **guildes communes** sont l'intersection des appartenances ⇒ ne révèle que des guildes où le **demandeur est déjà membre** (donc dont il voit déjà la liste des membres) — aucune info nouvelle. Les **amis communs** sont l'intersection avec **les amis du demandeur** (qu'il connaît déjà) ⇒ on révèle seulement « tel de mes amis est aussi ami avec la cible », jamais la liste d'amis complète de la cible. Disclosure **bornée et cohérente avec Discord**. **Aucune donnée privée** (id/username/display_name/avatar uniquement ; pas d'e-mail). Requêtes **paramétrées**.
- **Création de groupe = chemin existant** : « Créer un groupe privé » appelle `openDM({recipients})` (déjà audité) ; le serveur reste l'autorité (appartenance MP vérifiée). Plafond client (10) cosmétique ; le backend valide.
- **Pas de fuite via le panneau** : il n'affiche que le **profil public** (déjà accessible via `get_profile`) + les communs bornés. `USER_UPDATE` (§95) le garde à jour en direct.
- **Zéro-`ring`** ; un seul endpoint en lecture, sans effet de bord.

**Bilan** : endpoint `mutual` en **lecture, authentifié, borné aux données déjà connues du demandeur** (aucune divulgation nouvelle, aucune donnée privée) ; reste de la fonctionnalité purement client sur des chemins audités. **Aucune faille exploitable.** Vérifié en live (panneau profil 1:1, modal « Ajouter au MP », toggle).

## 97. S96 — Paramètres de salon (vocal/texte), permissions par type, catégories & focus vocal

Slice serveur : migration `0022` (colonnes `bitrate`, `user_limit`, `rtc_region`, `video_quality_mode`, `default_auto_archive` sur `channels`), `update_channel` étendu, nouvel endpoint **`GET /channels/:id/permissions`** (liste des surcharges), sentinelle **racine `"0"`** + événement live **`CHANNELS_REORDER`** dans `reorder_channels`, et notification live des **enfants détachés** lors de la suppression d'une catégorie. Le reste (modale de réglages, page Permissions tri-état, catégories, focus/plein-écran vocal) est **client**.

- **Tous les nouveaux champs sont bornés côté serveur** (jamais de confiance au client) : `bitrate` clampé **8 000–512 000** (débit poussé volontairement au-delà de Discord — *aucune* incidence sécurité, c'est un paramètre d'encodage local que le client applique), `user_limit` clampé **0–99**, `video_quality_mode` ∈ {1,2} sinon **400**, `default_auto_archive` ∈ liste blanche {60,1440,4320,10080} sinon **400**, `rtc_region` **trim + longueur ≤ 32 + alphanum/-/_** (sinon 400) ⇒ ni injection ni valeur sauvage. Chaîne vide ⇒ NULL (auto). **Vérifié en live** : `bitrate:999999→512000`, `user_limit:200→99`, `video_quality_mode:9→400`.
- **`update_channel` reste gated `MANAGE_CHANNELS`** (inchangé) ; les nouveaux champs n'ouvrent aucun chemin d'écriture supplémentaire. Requêtes **paramétrées**.
- **`GET …/permissions` gated `MANAGE_ROLES`** (cohérent avec `set_overwrite`/`delete_overwrite`) ⇒ la configuration de permissions d'un salon **n'est pas exposée** à un simple membre. Ne renvoie que `target_id/type/allow/deny` (déjà détenus par qui peut gérer). **Vérifié en live** (liste→PUT→liste→DELETE→liste). L'écriture des surcharges passe par `set_overwrite`, **déjà audité** : masque `actor & !ADMINISTRATOR` (on ne peut accorder/refuser que ce qu'on possède), cible validée dans la guilde.
- **Réordonnancement** : `reorder_channels` reste gated `MANAGE_CHANNELS`, **borné à 500 éléments**, chaque salon **vérifié appartenir à la guilde**, parent vérifié **catégorie** via `ensure_category`. La **sentinelle `"0"`** ne fait que `parent_id = NULL` (sortie de catégorie) — aucune élévation. L'événement `CHANNELS_REORDER` est **`EventScope::Guild`** (membres uniquement) et ne porte que `{id, position, parent_id}` (déjà visibles). Le client n'applique les positions qu'aux salons déjà connus (`map` sur la liste locale).
- **Suppression de catégorie** : la détaché des enfants (`parent_id → NULL`) émet désormais un `CHANNEL_UPDATE` par enfant (scope salon) — **donnée déjà visible**, corrige seulement un bug d'affichage (enfants « orphelins » invisibles jusqu'au rechargement). Aucune nouvelle divulgation.
- **Catégories** : `type = 4` déjà géré et validé (`ALLOWED_KINDS`, pas de parent pour une catégorie). Création via le chemin `create_channel` **déjà audité**.
- **Zéro-`ring`** ; aucune dépendance ajoutée ; tout en lecture/écriture paramétrée et permission-gated.

**Bilan** : extension de salon **entièrement bornée et permission-gated** (clamps + listes blanches + 400 sur entrée invalide), nouvel endpoint de lecture **réservé aux gestionnaires de permissions**, réordonnancement et détaché borné à la guilde sans élévation, événements live ne portant que des données déjà visibles. **Aucune faille exploitable.** Vérifié en live (réglages texte/vocal, page Permissions adaptée au type, catégories créées/déplacées/supprimées en direct, focus + plein-écran vocal) ; 42 tests client + `cargo check` verts.

## 98. S97 — Images de profil utilisateur + événements live complémentaires

Slice serveur : **images de profil utilisateur** (`POST /users/@me/images`, `GET /users/:id/avatar`, `GET /users/:id/banner`), **`GUILD_EMOJIS_UPDATE`** émis à la création/renommage/suppression d'emoji, et enrichissement du payload **`CHANNEL_PINS_UPDATE`** (`message_id` + `pinned`). Le reste du lot (avatars dans toute l'UI, statut personnalisé, surnoms, gestion des groupes MP, niveaux de notification, autocomplétion @/#, ack guilde, révocation d'invitations) est **client** et passe par des routes existantes déjà auditées.

- **Upload utilisateur = mêmes gardes que les guildes** : authentifié (`AuthUser`), taille **≤ 2 Mio** (`DefaultBodyLimit` + revérification), **type vérifié par octets magiques** (png/gif/webp/jpeg uniquement), identifiant de fichier **numérique généré serveur** (aucune traversée de chemin possible), écrit dans `upload_dir`. Vérifié en live : upload 200 → `image_id`, PATCH profil 200, service 200 `image/png`.
- **Service public décoratif** : `serve_user_avatar/banner` réutilisent `serve_stored_image` (id numérique, `nosniff`, type forcé en liste blanche). Même posture que les icônes de guilde — l'avatar est une donnée **publique** (déjà exposée via `avatar_id` dans les DTO).
- **Parité de risque connue (héritée des guildes)** : `avatar_id`/`banner_id` sont des champs texte posés via PATCH — un utilisateur pourrait référencer l'`image_id` d'un AUTRE upload s'il le devine (snowflakes semi-prévisibles). Impact : afficher une image déjà servie publiquement par ailleurs — pas de fuite nouvelle, pas d'élévation ; noté pour un futur durcissement commun (lier `image_id` → uploader).
- **`GUILD_EMOJIS_UPDATE`** : portée `Guild`, payload `{ guild_id }` (aucune donnée) ; déclenché uniquement par des mutations déjà permission-gated (`CREATE/MANAGE_GUILD_EXPRESSIONS`).
- **`CHANNEL_PINS_UPDATE` enrichi** : ajoute `message_id`/`pinned` — métadonnées d'un message du salon, livrées aux mêmes destinataires que le message lui-même (portée salon inchangée).
- Aucun nouvel état serveur, aucune nouvelle dépendance, **zéro-`ring`**.

**Bilan** : surface nouvelle limitée à l'upload/service d'images **clonée des chemins guilde audités** avec les mêmes validations, et deux événements ne portant que des identifiants déjà visibles. **Aucune faille exploitable.** Vérifié en live (avatar de bout en bout + propagation USER_UPDATE dans liste membres/messages/panneau ; statut personnalisé propagé via PRESENCE_UPDATE) ; 42 tests client + `cargo check` verts.

## 99. S98 — Stickers sur messages, assets soundboard, acks multi-sessions — **vérif adversariale**

Slice serveur : **stickers attachés aux messages** (migration 0023 `messages.sticker_id`, `CreateMessage.sticker_id`, hydratation `Message.sticker {id, name, format_type}` via LEFT JOIN), **service public des assets d'expression** (`GET /stickers/:id` image, `POST /guilds/:gid/soundboard/audio` + `GET /soundboard-sounds/:id/audio`), **événements** `GUILD_STICKERS_UPDATE` / `GUILD_SOUNDBOARD_UPDATE` (scope Guild) et `MESSAGE_ACK` / `GUILD_ACK` (scope User). Le reste du lot (pickers, pages de gestion, mixeur soundboard WebAudio, modération vocale UI, admin d'instance UI, sync des réglages, son de notification) est **client** sur des routes déjà auditées (§9, §11, §15, §76).

- **Sticker lié à la guilde du salon — vérifié adversarialement** : `create_message` valide `WHERE id = ? AND guild_id = ?` (guilde du salon cible). Sondes live : sticker de la guilde A posté dans la guilde B → **400** ; sticker en **MP** (salon sans guilde) → **400** ; message vide sans pièce jointe ni sticker → **400** (la règle « contenu vide autorisé » exige une PJ **ou** un sticker). L'hydratation n'expose que `{id, name, format_type}` — pas d'`asset_id` ni de métadonnées d'uploader.
- **Audio soundboard = mêmes gardes que les images** : authentifié + `CREATE_GUILD_EXPRESSIONS`, plafond **1 Mio** (`DefaultBodyLimit` + revérification), **type vérifié par octets magiques** (`detect_audio_type` : ID3/sync MP3, OggS, RIFF+WAVE), id de fichier **numérique généré serveur** (aucune traversée de chemin). Sonde live : HTML déguisé en `.mp3` → **400**. Service : Content-Type **forcé en liste blanche audio** + `nosniff` — pas de contexte d'exécution de script possible.
- **Service public des stickers** : `serve_sticker` résout id numérique → `asset_id` → `serve_stored_image` (liste blanche image + `nosniff`). Même posture « contenu décoratif public » que les emojis (§73) ; le risque d'énumération d'ids reste celui, déjà accepté, des assets d'expression.
- **Acks multi-sessions sans fuite** : `MESSAGE_ACK`/`GUILD_ACK` sont émis en **scope User** (seules les autres sessions du même compte les reçoivent) et ne portent que `channel_id`/`guild_id`/`last_read_id` — l'état de lecture d'autrui n'est jamais diffusé. `ack_message` reste gardé `VIEW_CHANNEL`, `ack_guild` filtre par permission **salon par salon**.
- **Événements d'expression** : payload `{guild_id}` uniquement, déclenchés par des mutations permission-gated ; le client refait un `GET` filtré serveur (pas de données poussées).
- **Côté client** : lecture soundboard = `fetch` même-origine + `decodeAudioData` (parseur navigateur sandboxé) mixé dans la piste publiée — aucune nouvelle surface serveur ; le mute coupe le **micro brut en amont** du mixeur (pas de fuite voix pendant un son). Sync des réglages = blob `/users/@me/settings` déjà audité (objet JSON ≤ 64 Ko, par compte) — le client n'y synchronise **pas** les préférences de périphériques. Section admin **feature-détectée** par un GET qui répond 403 aux non-admins (vérifié live) — la frontière reste serveur (§11). Modération vocale UI = relais de `moderate_voice_state` (gates `MUTE/DEAFEN/MOVE_MEMBERS` + protections propriétaire, côté serveur). Rendu : noms/émojis en texte échappé React, `src` d'images construits sur des **ids numériques**.
- Aucune nouvelle dépendance, **zéro-`ring`** préservé.

**Bilan** : la nouvelle surface se réduit à (1) une référence sticker validée contre la guilde du salon, (2) un pipeline audio cloné des chemins image audités avec liste blanche par octets magiques, (3) quatre événements ne portant que des identifiants déjà visibles par leurs destinataires. Sondes adversariales : inter-guilde, MP, HTML-en-audio, message vide → toutes rejetées **400**. Vérifié en live de bout en bout (sticker : upload → création → message → hydratation → rendu navigateur ; soundboard : wav → création → service ; sync réglages round-trip ; refresh live `GUILD_STICKERS_UPDATE` dans le picker). 42 tests client + `tsc` + `cargo check` verts. **Aucune faille exploitable.**

## 100. S99 — Messages système, salon système, transfert de propriété, héritage de permissions de catégorie, durcissements — **vérif adversariale**

Slice serveur : **salon système** par guilde (migration 0024 `guilds.system_channel_id`, exposé dans `Guild`/`UpdateGuild`) recevant un **message système type 7** à l'arrivée d'un membre (`insert_system_message`, best-effort) ; **transfert de propriété** (`POST /guilds/:id/transfer`) ; **héritage des surcharges de permission** d'une catégorie aux salons créés dessous + re-synchronisation manuelle (`POST /channels/:id/sync-permissions`) ; **R13 résolu** sur l'aperçu d'invitation ; et trois **durcissements** issus de l'audit (garde `@everyone` au retrait de rôle, rate-limit du profil, parité de validation). Le reste (volume par participant, push-to-talk, anneau « parle » en sidebar, slowmode/@rôle au composeur, confirmation de suppression, raccourcis vocaux, jumbo émoji custom) est **client** sur des chemins existants. **Aucune faille exploitable.**

- **Salon système — cible validée serveur** : `UpdateGuild.system_channel_id` n'accepte que `"0"` (désactiver) ou un **salon texte/annonces de CETTE guilde** (`type IN (0,5) AND guild_id = ?`). Sondes live : salon d'une **autre guilde** → 400 ; salon **vocal** → 400 ; salon texte valide → 200. Le message d'arrivée est inséré via un `JOIN channels` (salon supprimé entre-temps ⇒ aucun message, pas d'erreur), et `announce_member_join` est **best-effort** : il ne peut pas faire échouer l'adhésion. Idempotence : le message n'est émis que si l'`INSERT OR IGNORE` du membre a réellement inséré (`rows_affected > 0`) — un re-join ne spamme pas, et l'invitation n'est consommée qu'à une **vraie** première adhésion (corrige au passage un double-comptage latent).
- **Transfert de propriété — autorité serveur** : gardé **propriétaire actuel uniquement** (un non-propriétaire → 403, vérifié), cible **doit être membre** (sinon 400, vérifié), transfert vers soi-même → 400. La confirmation côté client (saisie du nom du serveur) est une **commodité UX**, pas la frontière. `GUILD_UPDATE` diffuse le nouvel `owner_id` → toutes les gardes UI (couronne, accès propriétaire) se recalculent en direct. Audit `guild_owner_transfer` écrit.
- **Héritage de permissions de catégorie** : à la création d'un salon sous une catégorie, ses `channel_overwrites` sont **copiés** (DELETE puis INSERT…SELECT) — un salon né sous une catégorie privée naît **privé** (corrige le défaut « visible de tous » noté à l'audit). `sync-permissions` re-copie sur demande, **gardé `MANAGE_ROLES` sur le salon** (alice non-membre → 403 ; salon sans parent → 400, vérifiés). La copie ne fait que **dupliquer** des surcharges déjà autorisées sur le parent ⇒ aucune élévation.
- **R13 — aperçu d'invitation d'un serveur privé** : pour un **non-membre**, `preview_invite` masque l'icône (`None`) et l'effectif (`0`) ; seul le **nom** reste (nécessaire pour décider d'accepter). Membre → aperçu complet. Vérifié live (membre : icône+count présents ; non-membre : `None`/`0`, nom conservé). La dette R13 sur ce chemin est close ; le chemin « emoji custom d'un serveur privé » reste à brancher (noté).
- **Durcissements** : `remove_member_role` refuse désormais de retirer `@everyone` (`rid == gid` → 400, parité avec `add_member_role`) ; `update_profile` est plafonné à **10 modifications / 10 min / utilisateur** (les rafales `USER_UPDATE` en fan-out ne peuvent plus être déclenchées en boucle, parité avec l'icône de guilde).
- **Côté client** : volume par participant = `HTMLAudioElement.volume` **local** (0..1, persisté localStorage, jamais transmis) ; push-to-talk = `track.enabled` piloté par une touche, avec **relâche sur perte de focus** (pas de micro resté ouvert après Alt+Tab) et un `applyMicGate()` centralisant l'état effectif `selfMute || (ptt && !held)` ; raccourcis `Ctrl+Shift+M/D` ne touchent que **mon** état vocal ; slowmode au composeur = **report d'affichage** (le serveur reste l'autorité — §, déjà testé) exempté pour `MANAGE_MESSAGES/CHANNELS` comme côté serveur ; suggestions `@rôle` filtrées par `mentionable || MENTION_EVERYONE` (le serveur tranche au rendu) ; confirmation de suppression = pure UX (le serveur autorise déjà la suppression). Rendu système en **texte échappé** React, jamais de HTML brut.
- Aucune nouvelle dépendance, **zéro-`ring`** préservé.

**Bilan** : surface serveur = un champ de guilde validé contre ses propres salons, un transfert entièrement autorisé serveur (propriétaire + membre), un héritage qui **duplique** des surcharges déjà permises, et un aperçu d'invitation **plus restrictif** qu'avant. Sondes adversariales — salon système cross-guilde/vocal, transfert non-propriétaire/non-membre/soi, sync sans droit/sans parent → **toutes rejetées**. Vérifié en live de bout en bout (message d'arrivée rendu « alice a rejoint le serveur » ; transfert aller-retour ; héritage + re-sync d'overwrite ; aperçu privé limité ; PTT + sélecteur de touche dans les réglages). 42 tests client + `tsc` + `cargo check` verts. **Aucune faille exploitable.**

## 101. S100 — Couverture & filtres du journal d'audit

`record_audit` est désormais appelé sur les mutations structurelles : `channel_create/update/delete`, `role_create/delete`, `guild_update`, `invite_create/delete`, `webhook_create/delete`, `guild_owner_transfer` (§100), `automod_rule_create`/`automod_trigger` (§102). Nouveau helper `record_audit_changes` (+`audit_named`) stockant un détail JSON (`{name}`) dans la colonne `changes` existante. `list_audit_logs` gagne des **filtres** (`?before` curseur id décroissant, `?limit` 1..100, `?action_type`, `?user_id`) via paramètres SQLite **numérotés** (`?1..?5`, liés positionnellement) et renvoie `changes`. **Aucune faille exploitable** : route inchangée côté garde (`VIEW_AUDIT_LOG`), filtres bornés et liés (pas d'injection), `changes` ne contient que des noms d'entités déjà visibles du lecteur. Vérifié live : `channel_create` journalisé avec le nom, filtre `action_type` exclusif.

## 102. S101 — Auto-modération (mots filtrés, anti-spam de mentions) — **vérif adversariale**

Slice serveur : table `automod_rules` (migration 0025) + CRUD `/guilds/:id/automod/rules` (gate `MANAGE_GUILD`) + hook `check_message` appelé par `create_message` **avant l'insertion**. `keyword` (sous-chaîne insensible à la casse) et `mention_spam` (seuil de mentions) ; action `block` (refus 403) ou `alert` (laisse passer + événement `AUTOMOD_ACTION` portée Guild). **Aucune faille exploitable.**

- **Autorité serveur, fail-open contrôlé** : la vérification est faite **côté serveur** dans le chemin d'envoi ; toute erreur interne (lecture règles, rôles) **laisse passer** le message — l'auto-mod ne peut pas casser l'envoi ni servir de déni de service. Les porteurs de `MANAGE_MESSAGES` et les rôles exemptés ne sont pas filtrés (cohérent avec slowmode).
- **Entrées bornées** : ≤ 100 mots-clés (≤ 60 car. chacun, minuscule), seuil mentions clampé 1..50, action/déclencheur en liste blanche, nom 1..60. Le salon d'alerte n'est qu'un identifiant ; `AUTOMOD_ACTION` ne porte qu'un aperçu tronqué (80 car.) diffusé aux membres de la guilde (même portée que les messages).
- Sondes live : un **plain member** (alice, sans `MANAGE_MESSAGES`) voit son message « interdit » **bloqué (403)**, son message normal passe (200), et le **propriétaire** (exempté) passe outre — conforme.

## 103. S102 — Réglages de guilde : notifications par défaut, salon AFK, invitation vanity

Migration 0026 (`default_message_notifications`, `afk_channel_id`, `afk_timeout`, `vanity_code`), exposés dans `Guild`/`UpdateGuild`. **Aucune faille exploitable.**

- **Cibles validées serveur** : `afk_channel_id` doit être un salon **vocal de cette guilde** (sinon 400) ; `default_message_notifications` ∈ {0,1} ; `afk_timeout` clampé 60..3600. Le **code vanity** est `[a-z0-9-]{2,32}`, **unique** parmi les guildes (vérifié), et la résolution dans `join_invite` est un repli : code introuvable dans `invites` → recherche `guilds.vanity_code` ; le vanity est **permanent** (pas d'incrément de `uses`). Le **code d'invitation personnalisé** (`CreateInvite.code`) exige `MANAGE_GUILD` (réserver un code lisible est plus sensible qu'un code jetable), même charset, unicité vérifiée.
- Tous les chemins de construction de `Guild` passent par le helper unique `fetch_guild_full`/`row_to_guild` → plus de risque d'oubli de champ (la régression `list_guilds`→500 de §100 ne peut plus se reproduire). Vérifié live : vanity invalide → 400, valide → stocké et résolu.

## 104. S103 — Pagination & recherche des membres (suppression du N+1)

`list_members` agrège les rôles en **une seule requête** (`GROUP_CONCAT` corrélé) au lieu d'un `SELECT` par membre (N+1 supprimé) ; gagne `?after` (curseur `user_id` croissant), `?limit` (1..1000), `?query` (LIKE minuscule sur pseudo/nom, ≥ 3 car.). **Aucune faille exploitable** : garde `VIEW_CHANNEL` inchangée, `invite_code` toujours masqué aux non-`MANAGE_GUILD`, paramètres liés (pas d'injection), comportement par défaut **identique** (tous les membres, tri par date d'arrivée). Vérifié live : recherche `hay` → hayato, `limit=1` → 1 membre, rôles agrégés présents.

## 105. S104 — Embeds de message & de webhook — **vérif adversariale**

Migration 0027 (`messages.embeds` JSON). `Message.embeds`, `CreateMessage.embeds`, `ExecuteWebhook.embeds` + rendu client (carte `EmbedCard`). **Aucune faille exploitable.**

- **Gate + assainissement serveur** : les embeds sur `create_message` exigent `EMBED_LINKS` (403 sinon) ; `sanitize_embeds` borne tout (≤ 10 embeds, titre ≤ 256, desc ≤ 4096, ≤ 25 champs, footer ≤ 2048, couleur masquée 24 bits) et **force les URLs en http(s)** — toute `url`/`image_url` non http(s) est **retirée**. Sonde live : `url: "javascript:alert(1)"` → **retirée** (`null`), champs préservés, embed hydraté. Les webhooks peuvent envoyer des embeds (cas CI/monitoring) avec contenu vide.
- **Rendu client sûr** : `EmbedCard` rend titre/desc/champs/footer en **texte échappé React** ; les liens passent par `target="_blank" rel="noreferrer"` ; l'`image_url` est déjà filtrée http(s) côté serveur. Aucune nouvelle surface XSS.
- Note : `@everyone` porte `EMBED_LINKS` par défaut (parité Discord) — un membre standard peut donc poster un embed ; le gate s'applique à qui ne détient pas le bit (retiré via surcharge de rôle/salon).

## 106. S105 — Cycle de vie des fils (archivage / verrouillage / membres) — **vérif adversariale**

Migration 0028 (`channels.archived/locked/archived_at` + table `thread_members`). `PATCH /channels/:id` accepte `archived`/`locked` **réservés aux fils** (type 11/12, sinon 400) ; `PUT/DELETE /channels/:id/thread-members/@me`. **Aucune faille exploitable.**

- **Verrou = autorité serveur** : écrire dans un fil **verrouillé** exige `MANAGE_CHANNELS` (sinon 403, vérifié avec alice) ; un fil **archivé** non verrouillé se **réactive automatiquement** à l'écriture (comportement Discord). L'archivage/verrouillage transitent par `update_channel` déjà gardé `MANAGE_CHANNELS` et audité. `join/leave_thread` sont gardés `VIEW_CHANNEL` et ne touchent que la ligne de l'appelant (`@me`).
- Sondes live : création de fil OK, verrouillage OK, **alice bloquée (403)** sur fil verrouillé, **auto-désarchivage** confirmé à l'écriture du modérateur, `join` OK. Le client filtre les fils archivés de la liste active (sauf celui ouvert) et `CHANNEL_UPDATE` patche `threadsByChannel` en direct.

## 107. S106 — Finitions client (picker émojis, aperçu MP, spoilers, événements)

Purement **client**, aucune surface serveur nouvelle. Picker d'émojis avec recherche/catégories/fréquents (dataset unicode curé local, 242 émojis, fréquents en `localStorage`) ; aperçu du statut perso / effectif en 2ᵉ ligne de la liste MP (sélecteurs **scalaires**, pas d'objet neuf — cf. règle getSnapshot) ; **spoilers de pièces jointes** via la convention de nom `SPOILER_` (flou + clic pour révéler au rendu ; le composeur préfixe le nom à l'upload — **aucun changement serveur**, l'octet du fichier suit le chemin d'upload audité) ; les événements `THREAD_CREATE`/`MESSAGE_DELETE_BULK`/`GUILD_MEMBER_*` sont consommés en direct. Rendu **texte échappé** partout. Vérifié live : composeur + boutons émoji/spoiler présents, picker ouvre avec recherche, `EmbedCard` rendu (titre/champ/footer). **Aucune faille exploitable.**

## 108. S107 — Durcissements : R5 (invitations), R9 (sessions), R3 (mots de passe) — **vérif adversariale**

- **R5 résolu** — la consommation d'invitation d'instance est désormais **atomique** : `UPDATE instance_invites SET uses = uses + 1 WHERE code = ? AND (max_uses = 0 OR uses < max_uses)` ; `rows_affected == 0` ⇒ « épuisée ». SQLite sérialise les écritures ⇒ deux inscriptions concurrentes sur une invitation à usage unique **ne peuvent plus** dépasser le quota. La double-consommation (ancien incrément final) est supprimée ; le slot est **remboursé** (`uses = MAX(0, uses-1)`) si l'inscription échoue après réservation (pseudo/mot de passe/e-mail invalide, conflit).
- **R9 résolu** — plafond de **10 sessions par utilisateur** (`issue_tokens` purge les plus anciennes au-delà) : borne l'accumulation d'acteurs/sessions lors de connexions répétées.
- **R3 résolu** — politique de mot de passe : ≥ 8 caractères + **denylist** des mots de passe les plus courants + interdiction de contenir le pseudo, sans dépendance externe (pas de zxcvbn/HIBP). Appliquée à l'inscription **et** au changement de mot de passe. Sondes live : `password` → 400 (trop courant), mot de passe contenant le pseudo → 400, < 8 car. → 400 ; **aucun compte sonde créé**.
- **R2 (énumération de comptes)** : non clos — fermer complètement exige une **vérification d'e-mail** (le 409 « pseudo/e-mail déjà utilisé » reste un canal d'énumération). Documenté ci-dessous.

**Bilan S100-S107** : 8 tranches (audit, automod, réglages de guilde, pagination membres, embeds, fils, finitions client, durcissements). Toute nouvelle autorité reste **côté serveur** ; entrées bornées et liées ; URLs d'embed forcées http(s) ; auto-mod fail-open ; invitations consommées atomiquement. Migrations 0024-0028 appliquées, `cargo check` + `tsc` + 42 tests vitest verts, vérifications live + sondes adversariales (automod block/exempt, embed `javascript:` retiré, fil verrouillé 403, vanity invalide 400, mots de passe faibles 400) **toutes conformes**. **Aucune faille exploitable.**

## 109. S109 — Rate-limiting REST (R1/R6) — **vérif adversariale**

Limiteur de débit **token bucket en mémoire** (`ratelimit.rs`) appliqué comme garde dans les handlers sensibles. Mono-process (mode tout-en-un) ⇒ **aucune dépendance externe** (ni Redis ni `ring`). **Aucune faille exploitable.**

- **Classes & clés** : `login`/`register`/`gate` par **IP cliente** (anti brute-force / inscription en masse) ; `create_message` et `create_invite` (guilde) par **utilisateur** (anti-spam, en plus du slowmode par salon) ; `execute_webhook` par **webhook_id** (R6). Bursts généreux + recharge linéaire (ex. message : rafale 30, 5/s ; login : rafale 20, 1/s) — transparents pour l'usage humain et les tests, bloquants pour un script.
- **IP cliente fiable** : extracteur `ClientIp` = `X-Forwarded-For` (premier hop, reverse-proxy de prod) → `ConnectInfo` (socket réel, activé via `into_make_service_with_connect_info`) → `"unknown"` (tests `oneshot`). **Infaillible** : l'absence d'IP ne rejette jamais, elle partage un bucket commun. Le serveur réel passe désormais l'adresse de connexion aux extracteurs.
- **Réponse normalisée** : dépassement → **429** (`code 20016`) avec header **`Retry-After`** (secondes) + champ JSON `retry_after`. Le client peut donc temporiser proprement.
- **Bornage mémoire** : purge opportuniste des seaux rechargés à plein au-delà de 50 000 entrées (aucune perte de protection — un seau plein équivaut à « jamais vu »). Désactivable via `OZONE_RATE_LIMIT=0` (bench/CI). Limiteur **par-instance** d'`AppState` (chaque test a le sien).
- **Sondes live** : brute-force `login` depuis une IP → **429 + `Retry-After: 1`** (code 20016, champ `retry_after`) ; **isolation par IP** confirmée (une IP jamais vue repart à zéro, 401 ≠ 429) ; **`create_message`** sature exactement à **30 messages** (= capacité) puis **15× 429** — keying par utilisateur prouvé, messages de test nettoyés.
- **Effet de bord corrigé** : la denylist de mots de passe (§108) rejetait le mot de passe de test historique `motdepasse` (« password » en français) — comportement **correct** ; les 42 tests d'intégration ont été migrés vers un mot de passe fort. Toute la suite `cargo test -p ozone-api` repasse au vert.

**Bilan** : R1 et R6 clos par un token-bucket en mémoire respectant l'invariant zéro-`ring`/zéro-dépendance ; IP cliente extraite de manière fiable et infaillible ; 429 normalisé avec `Retry-After`. Sondes adversariales (brute-force login, saturation message, isolation IP) **conformes** ; suite de tests serveur + 42 tests client verts. **Aucune faille exploitable.**

## 110. S110 — Passe de debug général (audit multi-agents + corrections) — **vérif adversariale**

Audit de bugs de correction mené par fan-out multi-agents sur les zones à risque (store Zustand, routes serveur récentes, auth/rate-limit, rendu/média), avec **réfutation adversariale** de chaque finding (11 trouvés → **9 confirmés**, 2 réfutés). Corrigés et vérifiés :

- **(Critique) Fuite de contenu via `AUTOMOD_ACTION`** : l'alerte d'auto-modération était diffusée en `EventScope::Guild` → l'aperçu (80 car.) d'un message d'un **salon privé** fuitait à **tous** les membres du serveur (le routage `Guild` n'impose pas `VIEW_CHANNEL`). Corrigé en `EventScope::Channel { channel_id: alert }` ⇒ seuls les habilités à voir le salon d'alerte reçoivent l'aperçu. *(routes_automod.rs)*
- **(Critique) `X-Forwarded-For` trusté inconditionnellement** : un client en accès direct usurpait son IP (un XFF par requête) et **annulait** tout le rate-limit par IP (login/register/gate). Corrigé : XFF n'est cru **que** derrière un reverse-proxy déclaré (`OZONE_TRUSTED_PROXY=1`) ; par défaut on clé sur l'IP socket (`ConnectInfo`). Vérifié live : 30 logins à XFF distinct → toujours rate-limités (5×429). *(extract.rs, state.rs, db.rs)*
- **(Majeur) Perte du statut personnalisé** : un simple changement de statut (online→dnd) effaçait le statut perso quand le cache client était vide (boot 2ᵉ appareil) car le serveur écrasait `custom_status` à chaque appel. Corrigé par une **sémantique à 3 états** (`Option<Option<String>>` côté DTO/désérialiseur) : champ absent = préserver, `null` = effacer, valeur = définir ; le client n'envoie plus le champ lors d'un changement de statut. Vérifié live : statut perso **préservé** au changement de statut, **effacé** sur null. *(dto.rs, routes_presence.rs, presence.rs, store.ts, types.ts)*
- **(Majeur) Débordement arithmétique sur `max_age` d'invitation** : `now + max_age*1000` avec `max_age` client non borné → panic (debug) / expiration aberrante (release). Borné (≤ 30 j guilde, ≤ 90 j instance) + validé en amont. Vérifié live : `i64::MAX` → **400** (plus de panic/500). *(routes_guild.rs, routes_instance_admin.rs)*
- **(Majeur) Purge de buckets de rate-limit cross-classe** : le `retain` évaluait chaque seau avec les paramètres (capacity/refill) de la **classe appelante**, pas de la sienne → éviction erronée d'autres classes au-delà de 50 k buckets. Corrigé en stockant capacity/refill **par bucket**. *(ratelimit.rs)*
- **(Majeur) Le plafond de sessions pouvait supprimer la session courante** : à `created_at` égal (reconnexions dans la même ms), le `ORDER BY created_at DESC LIMIT 10` pouvait exclure la session fraîchement émise → refresh immédiatement invalide. Corrigé par un départage `id DESC` (Snowflake monotone). *(routes_auth.rs)*
- **(Majeur) Slot d'invitation non remboursé sur erreur `?`** : une création de compte échouant après la réservation atomique (course UNIQUE, etc.) consommait un usage définitivement. Corrigé : la création post-réservation est enveloppée et **rembourse** le slot sur toute erreur. *(routes_auth.rs)*
- **(Majeur) Pièce jointe rattachée au mauvais salon (race)** : un upload résolu après un changement de salon réinjectait la PJ dans le composeur du nouveau salon. Corrigé : capture du salon cible + comparaison à une ref du salon courant avant `setPending`/`setUploading`. *(ChatView.tsx)*
- **(Mineur) Démutage incohérent pendant la sourdine** : démuter laissait `selfDeaf=true` (micro ouvert, n'entend personne), puis `preDeafMute` écrasait le démutage. Corrigé : démuter **lève aussi** la sourdine (comportement Discord). *(store.ts)*

**Réfutés** (non-bugs) : 2 findings écartés par le sceptique (faux positifs). **Nettoyage qualité** : 3 warnings clippy résolus (`////`, `contains`, `#[allow]` ciblé) → workspace **clippy clean**.

**Bilan** : 2 fuites/contournements critiques fermés (alerte automod, spoof XFF), 6 bugs de correction majeurs corrigés, 1 mineur. Migrations inchangées. **Toute la suite `cargo test -p ozone-api` (45 fichiers) + 42 tests client + `tsc` + clippy workspace au vert** ; sondes live conformes (préservation statut perso, XFF ignoré, `max_age` borné). **Aucune faille exploitable.**

---
**Dettes/risques identifiés à traiter (client web) :**
- **R11 — Embeds de liens** : *embeds média directs* **implémentés en opt-in** (§75, OFF par défaut, consentement éclairé). L'**unfurling OG** (aperçus titre/description de pages) reste différé : nécessite une **récupération HTTPS côté serveur** (incompatible zéro-`ring` sans pile TLS pure-Rust) + garde **anti-SSRF**.
- **R12 — Médias privés en `<img>` + bearer** : **RÉSOLU (§74)** par récupération `fetch` authentifiée → `blob:` URL côté client (pas de jeton en URL, serveur inchangé).
- **R13 — Enforcement du profil privé (§88)** : **PARTIELLEMENT RÉSOLU (§100)** — l'aperçu d'invitation masque désormais icône/effectif aux non-membres d'un serveur privé (seul le nom subsiste). Reste à brancher : le chemin « clic sur emoji custom d'un serveur privé » (réponse limitée si non-membre).

*Document vivant — revue effectuée pour S1 → S86 ; à reconduire à chaque couche. **Transport média vocal livré et audité (S84-S86)** : WebRTC ↔ `ozone-sfu`, plan média authentifié (jeton vocal HS256), attribution des flux par membre sûre par construction (identité serveur, nature en liste blanche), caméra-par-personne + partage d'écran (Go Live) + détection de parole, qualité/latence Opus réglées ; `ring` cantonné au nœud média. **Résolus depuis** : R1/R6 (rate-limiting REST, §109), R3/R5 (politique de mot de passe + invitations atomiques, §108), R9 (plafond de sessions, §108 — le rate-limit des **opcodes Gateway** reste à faire). À compléter par : R11 (unfurling OG serveur), **E2EE média** (DAVE/MLS) + TURN/production et leur audit, durcissement du stockage des jetons (R10 : stockage sécurisé OS via Tauri + CSP stricte), rate-limit des opcodes Gateway (IDENTIFY/RESUME), R2 (vérification d'e-mail anti-énumération), applications/bots/OAuth, fuzzing du parseur gateway, et tests de charge.*
