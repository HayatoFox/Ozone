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

---
*Document vivant — revue effectuée pour S1, S2, S3 ; à reconduire à chaque couche. À compléter par : fuzzing du parseur de protocole gateway, tests de charge (rate-limit), et un audit du futur chiffrement vocal DAVE/MLS.*
