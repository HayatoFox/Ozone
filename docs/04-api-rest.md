# 04 — API REST

API HTTP versionnée pour toutes les **actions** (le temps réel passe par la [Gateway](05-gateway-temps-reel.md), le média par le [SFU](06-infrastructure-vocale.md)).

## 1. Conventions

- **Base** : `https://api.ozone.app/v1`. Versionnée dans l'URL ; breaking changes = nouvelle version.
- **Format** : JSON. Snowflakes en **string**. Dates ISO-8601. `snake_case`.
- **Auth** : `Authorization: Bearer <access_jwt>` (users) ou `Bot <token>` (apps). Access JWT court (~10 min) + **refresh token rotatif** (révocable, lié à une session/appareil).
- **Erreurs** : enveloppe `{ "code": 50013, "message": "Missing Permissions", "errors": { … } }` avec codes stables (calqués sur Discord : 10003 unknown channel, 50001 missing access, 50013 missing permissions, 40001 unauthorized, 429 rate limited…).
- **Idempotence** : `nonce` sur les messages (dédup), header `Idempotency-Key` sur les créations sensibles.
- **Pagination** : `?limit=&before=&after=&around=` (curseurs par snowflake), jamais d'offset.
- **Audit** : header `X-Audit-Log-Reason` enregistré dans l'audit log.

## 2. Rate limiting

Modèle « buckets » à la Discord :
- Par **route + ressource majeure** (ex. `POST /channels/{id}/messages` → bucket lié au salon).
- Réponses incluent `X-RateLimit-Limit`, `-Remaining`, `-Reset`, `-Reset-After`, `-Bucket`, `-Scope`.
- **429** → `{ retry_after, global }`. Limite **globale** par token + limites par bucket.
- Implémentation : Redis (token bucket / sliding window), partagée entre réplicas API.

## 3. Endpoints par ressource

### Instance (connexion, métadonnées, gate) — *point d'entrée*
| Méthode | Route | Rôle |
|---|---|---|
| GET | `/.well-known/ozone` | découverte : pointe vers la base d'API de l'instance |
| GET | `/instance` | **métadonnées publiques** (sans auth) : nom, icône, description, version, `registration_policy`, `access_gate.required`, `features`, `limits` |
| GET | `/instance/health` | sonde de disponibilité/latence |
| POST | `/instance/gate` | vérifie le **mot de passe d'instance** → `{ gate_token }` (court) ; requis ensuite par login/register si le gate est actif |

**Admin d'instance** (rôles owner/admin — voir [features/00-instances](features/00-instances.md#7-rôles-au-niveau-de-linstance-distincts-des-rôles-de-guilde)) :
| Méthode | Route | Rôle |
|---|---|---|
| GET/PATCH | `/instance/admin/config` | branding, politique d'inscription, limites/quotas, réglages par défaut |
| PUT | `/instance/admin/gate` | activer / désactiver / changer le mot de passe d'instance |
| GET/POST/DELETE | `/instance/admin/invites` | **invitations d'instance** (créer/lister/révoquer) |
| GET/PATCH | `/instance/admin/users` | gérer les comptes (suspendre, **bannir au niveau instance**, promouvoir admin/modérateur) |
| GET | `/instance/admin/audit-logs` | journal d'audit au niveau instance |

### Authentification & compte
> Sur une instance protégée, `register`/`login` exigent l'en-tête `X-Instance-Gate: <gate_token>` (obtenu via `/instance/gate`). Sur une instance `invite`, `register` exige aussi un **code d'invitation d'instance**.

| Méthode | Route | Rôle |
|---|---|---|
| POST | `/auth/register` | créer un compte (email, pseudo, mdp, captcha ; + gate_token / code d'invitation d'instance selon politique) |
| POST | `/auth/login` | login (mdp) → tokens ; `mfa: true` si 2FA requis |
| POST | `/auth/mfa/totp` | valider le code TOTP |
| POST | `/auth/logout` | révoquer la session |
| POST | `/auth/token/refresh` | rotation du refresh token |
| POST | `/auth/verify/email` · `/auth/verify/phone` | vérifications |
| POST | `/auth/password/forgot` · `/reset` | réinitialisation |
| GET/PATCH | `/users/@me` | profil global |
| GET/PATCH | `/users/@me/settings` | paramètres |
| GET | `/users/@me/sessions` · DELETE `/sessions/{id}` | appareils connectés |
| POST/DELETE | `/users/@me/mfa/totp` | activer/désactiver 2FA |
| GET | `/users/{id}` | profil public d'un user |
| DELETE | `/users/@me` | suppression de compte |

### Relations (amis/blocages)
| Méthode | Route |
|---|---|
| GET | `/users/@me/relationships` |
| POST | `/users/@me/relationships` (demande d'ami par pseudo) |
| PUT/DELETE | `/users/@me/relationships/{id}` (accepter / supprimer / bloquer) |
| PATCH | `/users/@me/relationships/{id}` (surnom d'ami) |
| GET/PUT | `/users/@me/notes/{id}` (note privée) |

### Guildes (serveurs)
| Méthode | Route |
|---|---|
| POST | `/guilds` (créer, depuis zéro ou **template**) |
| GET/PATCH/DELETE | `/guilds/{id}` |
| GET | `/guilds/{id}/preview` (aperçu public) |
| GET/PATCH | `/guilds/{id}/settings` (notifs par défaut, vérif, filtre…) |
| GET | `/guilds/{id}/audit-logs` (filtres : user, action, before) |
| GET/POST/PATCH/DELETE | `/guilds/{id}/roles` (+ `/roles/{rid}`, réordonner) |
| GET/PATCH | `/guilds/{id}/members` (liste, recherche, `/members/{uid}`) |
| PUT/DELETE | `/guilds/{id}/members/{uid}/roles/{rid}` |
| PATCH | `/guilds/{id}/members/{uid}` (nick, mute, deaf, timeout, move) |
| GET/PUT/DELETE | `/guilds/{id}/bans` (+ `/bans/{uid}`, bulk-ban) |
| DELETE | `/guilds/{id}/members/{uid}` (kick) |
| GET/POST | `/guilds/{id}/invites` |
| GET/POST/PATCH/DELETE | `/guilds/{id}/emojis` · `/stickers` · `/soundboard` |
| GET/PUT/PATCH | `/guilds/{id}/onboarding` · `/welcome-screen` |
| GET/POST/PATCH/DELETE | `/guilds/{id}/scheduled-events` |
| GET/POST/PATCH/DELETE | `/guilds/{id}/automod/rules` |
| GET/POST | `/guilds/{id}/templates` (créer/synchroniser) |
| PATCH | `/guilds/{id}/vanity-url` |
| GET | `/guilds/{id}/widget` · `/integrations` · `/webhooks` |

### Salons & messages
| Méthode | Route |
|---|---|
| POST | `/guilds/{id}/channels` (créer) ; PATCH réordonner (bulk positions) |
| GET/PATCH/DELETE | `/channels/{id}` |
| PUT/DELETE | `/channels/{id}/permissions/{overwrite_id}` (overrides) |
| GET/POST | `/channels/{id}/messages` (liste paginée / envoyer) |
| GET/PATCH/DELETE | `/channels/{id}/messages/{mid}` |
| POST | `/channels/{id}/messages/bulk-delete` |
| PUT/DELETE | `/channels/{id}/messages/{mid}/reactions/{emoji}/@me` (réagir, +`?burst` super-réaction) |
| GET/DELETE | `/channels/{id}/messages/{mid}/reactions/{emoji}` (lister/purger) |
| PUT/DELETE/GET | `/channels/{id}/pins/{mid}` (épingler, cap 250) |
| POST | `/channels/{id}/messages/{mid}/crosspost` (annonces) |
| PUT | `/channels/{id}/followers` (suivre un salon d'annonces) |
| POST | `/channels/{id}/typing` (indicateur de frappe) |
| POST | `/channels/{id}/threads` · `/messages/{mid}/threads` (créer un fil) |
| PUT/DELETE | `/channels/{id}/thread-members/@me` (rejoindre/quitter) |
| GET | `/channels/{id}/threads/archived/public|private` |
| POST | `/channels/{id}/messages/{mid}/forward` (transfert) |
| POST | `/channels/{id}/messages` avec `poll` (sondage) ; POST `/polls/{id}/answers/{aid}/voters` |

### Messages privés
| Méthode | Route |
|---|---|
| POST | `/users/@me/channels` (ouvrir un MP / créer un groupe MP) |
| PUT/DELETE | `/channels/{id}/recipients/{uid}` (groupe MP) |
| GET | `/users/@me/channels` (liste des MP) |

### Upload de médias
| Méthode | Route |
|---|---|
| POST | `/channels/{id}/attachments` (demande d'**URLs présignées** S3) |
| PUT | `<url présignée>` (upload direct vers S3) |
| (puis) POST | `/channels/{id}/messages` en référençant les `uploaded_filename` |
| POST | `/users/@me/avatar` · `/banner` (upload profil, retourne un asset id) |

> Le média ne transite jamais par l'API applicative : upload direct S3 via URL présignée, livraison via CDN. L'API ne fait que valider tailles/types/permissions.

### Voix (signaling initial via Gateway, pas REST)
- Rejoindre/quitter un vocal = `VOICE_STATE_UPDATE` sur la Gateway (voir [05](05-gateway-temps-reel.md)/[06](06-infrastructure-vocale.md)).
- REST : `PATCH /channels/{id}/voice-status` (statut de salon vocal), `GET /voice/regions`.

### Webhooks & apps
| Méthode | Route |
|---|---|
| POST/GET/PATCH/DELETE | `/channels/{id}/webhooks` · `/webhooks/{id}` |
| POST | `/webhooks/{id}/{token}` (exécuter — sans auth user) |
| GET/POST | `/applications/{id}/commands` (slash commands, global/guilde) |
| POST | `/interactions/{id}/{token}/callback` (réponse aux interactions) |
| GET | `/applications/{id}` · `/oauth2/authorize` · `/oauth2/token` |

### Découverte
| Méthode | Route |
|---|---|
| GET | `/discovery/search` · `/discovery/categories` |
| GET | `/guilds/{id}/discovery-metadata` ; PATCH (mots-clés, catégories) |

## 4. Validation des permissions (côté serveur)

Chaque endpoint mutant calcule les permissions effectives de l'appelant : `@everyone` → rôles cumulés → overrides catégorie → overrides salon → propriétaire/admin court-circuite. Algorithme détaillé dans [10 — Rôles & permissions](features/10-roles-permissions.md). **Le client ne fait jamais autorité** ; il pré-désactive l'UI pour l'ergonomie, le serveur revérifie tout.

## 5. Cohérence avec la Gateway

Toute mutation REST réussie publie l'événement correspondant sur **NATS**, que la Gateway diffuse (`MESSAGE_CREATE`, `GUILD_ROLE_UPDATE`, …). Le client émetteur reçoit la réponse REST **et** l'événement : le `nonce` permet de dédupliquer/réconcilier le rendu optimiste.

Suite : **[05 — Gateway temps réel](05-gateway-temps-reel.md)**.
