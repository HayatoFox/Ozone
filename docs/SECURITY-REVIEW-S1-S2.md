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
| **R4** | Info | `join_invite` ne vérifie pas un **bannissement** (les bans arrivent en S6). | Contrôle de ban au join (S6 — modération). |

## 4. Comment rejouer

```sh
cargo test -p ozone-api --test security      # intrusion S1/S2
cargo test -p ozone-api --test security_s3   # intrusion S3
cargo test -p ozone-api                       # suite complète (21 tests)
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

---
*Document vivant — revue effectuée pour S1 → S5 ; à reconduire à chaque couche. À compléter par : fuzzing du parseur de protocole gateway, tests de charge (rate-limit), et un audit du futur chiffrement vocal DAVE/MLS.*
