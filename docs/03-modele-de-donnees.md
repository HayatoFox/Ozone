# 03 — Modèle de données

Toutes les entités sont identifiées par un **Snowflake** 64 bits. Les schémas ci-dessous sont la **source de vérité** ; le crate `ozone-proto` en dérive les types Rust partagés client/serveur.

## Snowflakes

```
 63                                    22   17   12             0
  ┌─────────────────────────────────────┬────┬────┬─────────────┐
  │  timestamp ms depuis l'epoch Ozone   │ wkr│ pid│  séquence   │
  │              (42 bits)               │(5) │(5) │   (12)      │
  └─────────────────────────────────────┴────┴────┴─────────────┘
```
- **Epoch Ozone** : constante fixe (ex. 2025-01-01). `created_at = (id >> 22) + EPOCH`.
- Triables chronologiquement, générables sans coordination (worker+pid uniques par nœud), 4096 IDs/ms/worker.
- Exposés en **string** dans l'API JSON (un nombre 64 bits dépasse la précision d'un double JS — comme Discord).

---

## 1. Entités principales (relationnel — PostgreSQL)

### InstanceConfig (configuration de l'instance — quasi-singleton)
Décrit **l'instance elle-même** (le déploiement). Une seule ligne active par déploiement.
| Champ | Type | Notes |
|---|---|---|
| `instance_id` | snowflake | identité de l'instance |
| `name` / `description` | text | branding |
| `icon_id` / `splash_id` / `accent_color` | | branding écran de connexion |
| `owner_user_id` | snowflake | propriétaire (créé au bootstrap) |
| `registration_policy` | enum | `open` · `invite` · `closed` |
| `access_gate_hash` | text? | **mot de passe d'instance** (Argon2id) ; `null` = pas de gate |
| `require_email_verification` | bool | |
| `max_users`, `max_guilds_per_user`, `max_upload_bytes`, `storage_quota` | | limites/quotas |
| `voice_limits` / `features` / `limits` | json | perks débloqués (bitrate, résolution partage…), sans paiement |
| `default_locale` | text | langue par défaut des nouveaux comptes |
| `public_key` | bytea | identité cryptographique de l'instance (vérif TOFU / pinning client) |
| `version` | text | version du serveur |

Tables liées au niveau **instance** : `instance_roles (user_id, role ∈ owner|admin|moderator|user)`, `instance_invites (code, created_by, max_uses, uses, expires_at)`, `instance_bans (user_id, reason, moderator_id, at)`, `instance_audit_log`.

### User (compte — propre à l'instance)
> Tous les comptes (et toutes les entités ci-dessous) existent **au sein d'une seule instance**. Rien n'est partagé entre instances : le même e-mail peut posséder des comptes indépendants sur deux instances.

| Champ | Type | Notes |
|---|---|---|
| `id` | snowflake | PK |
| `username` | text | identifiant unique (pseudonyme), `[a-z0-9_.]`, 2–32 |
| `display_name` | text? | nom affiché libre |
| `discriminator` | smallint? | déprécié (legacy `#1234`) — on adopte le modèle **pseudo unique** |
| `email` | citext | unique, vérifié |
| `phone` | text? | vérifié (option 2FA SMS) |
| `password_hash` | text | **Argon2id** |
| `avatar_id` / `banner_id` | text? | clés média |
| `accent_color` | int? | couleur de profil |
| `bio` | text? | « À propos » (190 → 4096) |
| `pronouns` | text? | |
| `flags` | bigint | badges/états (staff, early, bot, system…) |
| `mfa_enabled` | bool | 2FA actif |
| `mfa_secret` | bytea? | TOTP (chiffré au repos) |
| `locale` | text | langue |
| `created_at` | dérivé de l'id | |
| `is_bot` / `is_system` | bool | |

Tables liées : `user_settings` (JSONB versionné), `user_connections`, `user_mfa_backup_codes`, `user_sessions` (appareils/jetons), `user_notes` (note privée sur un autre user), `blocked_relationships`.

### Relationship (amis/blocages)
| Champ | Type | Notes |
|---|---|---|
| `user_id`, `target_id` | snowflake | paire |
| `type` | enum | `friend` · `pending_incoming` · `pending_outgoing` · `blocked` |
| `nickname` | text? | surnom d'ami (privé) |
| `since` | timestamp | |

### Guild (serveur)
| Champ | Type | Notes |
|---|---|---|
| `id` | snowflake | PK |
| `name` | text | 2–100 |
| `icon_id` / `banner_id` / `splash_id` / `discovery_splash_id` | text? | |
| `owner_id` | snowflake | propriétaire |
| `description` | text? | communautaire |
| `verification_level` | enum | none/low/medium/high/very_high |
| `explicit_content_filter` | enum | disabled/no_role/all |
| `default_message_notifications` | enum | all/mentions |
| `afk_channel_id` / `afk_timeout` | | salon AFK |
| `system_channel_id` / `system_channel_flags` | | messages système (bienvenue, boost) |
| `rules_channel_id`, `public_updates_channel_id`, `safety_alerts_channel_id` | | communauté |
| `vanity_url_code` | text? | invitation personnalisée |
| `preferred_locale` | text | |
| `premium_tier` / `premium_boost_count` | int | paliers de boost (mécanique) |
| `features` | text[] | drapeaux (`COMMUNITY`, `DISCOVERABLE`, `ANIMATED_ICON`, `INVITE_SPLASH`, …) |
| `nsfw_level` | enum | |
| `max_members` | int | jusqu'à 25M |
| `tag` / `tag_badge_id` | | **Server Tag** (clan) |

Tables liées : `guild_members`, `roles`, `channels`, `emojis`, `stickers`, `soundboard_sounds`, `guild_bans`, `invites`, `webhooks`, `integrations`, `scheduled_events`, `automod_rules`, `audit_log_entries`, `welcome_screen`, `onboarding`, `guild_templates`.

### GuildMember
| Champ | Type | Notes |
|---|---|---|
| `guild_id`, `user_id` | snowflake | PK composite |
| `nick` | text? | pseudo serveur |
| `avatar_id` / `banner_id` | text? | **profil par serveur** |
| `bio` | text? | bio par serveur |
| `roles` | snowflake[] | rôles attribués |
| `joined_at` | timestamp | |
| `premium_since` | timestamp? | boost |
| `communication_disabled_until` | timestamp? | **timeout** |
| `pending` | bool | onboarding non terminé |
| `flags` | int | (a complété l'onboarding, contourne la vérif, …) |

### Role
| Champ | Type | Notes |
|---|---|---|
| `id` | snowflake | (`@everyone` = id de la guilde) |
| `guild_id` | snowflake | |
| `name` | text | |
| `color` / `colors` | int / json | couleur unie **ou dégradé/holographique** |
| `hoist` | bool | affiché séparément dans la liste |
| `position` | int | **hiérarchie** |
| `permissions` | bigint | bitfield (voir [10](features/10-roles-permissions.md)) |
| `managed` | bool | géré par une intégration/bot |
| `mentionable` | bool | |
| `icon_id` / `unicode_emoji` | | icône de rôle |
| `tags` | json | bot_id, integration_id, premium_subscriber, … |
| `flags` | int | (sélectionnable à l'onboarding…) |

### Channel
Tous types confondus (voir [03-salons](features/03-salons.md)). Champs selon le type :
| Champ | Type | Pour |
|---|---|---|
| `id`, `type`, `guild_id?`, `name`, `position` | | tous |
| `topic` | text? (0–1024, 4096 forum) | texte/forum/annonce |
| `nsfw` | bool | texte/voix |
| `rate_limit_per_user` | int (slowmode s) | texte/forum/thread |
| `parent_id` | snowflake? | catégorie ou salon parent (thread) |
| `permission_overwrites` | json[] | rôle/membre → allow/deny |
| `bitrate`, `user_limit`, `rtc_region`, `video_quality_mode` | | voix/stage |
| `last_message_id`, `last_pin_at` | | texte |
| `thread_metadata` (archived, auto_archive_duration, locked, invitable) | | threads |
| `message_count`, `member_count`, `total_message_sent` | | threads |
| `available_tags`, `applied_tags`, `default_reaction_emoji`, `default_sort_order`, `default_forum_layout`, `default_thread_rate_limit_per_user` | | forum/média |
| `flags` | int | (épinglé en forum, masquer le média, require_tag…) |

**Types de salons** (numériques, compat Discord) :
`0` texte · `1` MP · `2` vocal · `3` groupe MP · `4` catégorie · `5` annonces · `10` thread d'annonce · `11` thread public · `12` thread privé · `13` stage · `14` répertoire (hub) · `15` forum · `16` média.

### PermissionOverwrite
`{ id (role|member), type, allow (bigint), deny (bigint) }` — appliqué par-dessus les permissions de base, par salon/catégorie.

### Invite
| Champ | Notes |
|---|---|
| `code` (texte court unique), `guild_id`, `channel_id`, `inviter_id` |
| `max_uses`, `uses`, `max_age` (expiration), `temporary` (membre temporaire) |
| `created_at`, `expires_at`, `target_type` (stream/embedded app) |

### Emoji / Sticker / SoundboardSound / AvatarDecoration / Nameplate
- **Emoji** : `id, guild_id, name, image_id, animated, roles[] (restreint), available, managed`.
- **Sticker** : `id, guild_id?, pack_id?, name, description, tags, format (png/apng/lottie/gif), available`.
- **SoundboardSound** : `id, guild_id?, name, sound_id, volume, emoji, available`.
- **AvatarDecoration / Nameplate / ProfileEffect** : assets cosmétiques (débloqués sans paiement chez Ozone).

### Webhook / Integration / Application(bot)
- **Webhook** : `id, type (incoming/channel_follower/application), channel_id, name, avatar, token`.
- **Integration** : lien vers une application/bot, scopes, rôle géré.
- **Application** : `id, name, icon, description, bot_user_id, owner, public_key, commands[]`.

### ScheduledEvent
`id, guild_id, channel_id?, name, description, scheduled_start/end, privacy_level, status (scheduled/active/completed/canceled), entity_type (stage/voice/external), location, cover_image, user_count, recurrence_rule`.

### AutoModRule
`id, guild_id, name, event_type, trigger_type (keyword/spam/keyword_preset/mention_spam/member_profile), trigger_metadata (mots, regex, presets, seuils), actions (block/alert/timeout), exempt_roles/channels, enabled`.

### AuditLogEntry
`id, guild_id, user_id, target_id, action_type, changes[], reason, options`. ~50 types d'actions (create/update/delete sur chaque ressource, ban, kick, member_move, etc.).

---

## 2. Messages (ScyllaDB — fort débit)

Table `messages` partitionnée par salon, triée par id décroissant (lecture « derniers messages » triviale).
```
PRIMARY KEY ((channel_id), id)  WITH CLUSTERING ORDER BY (id DESC)
```
| Champ | Type | Notes |
|---|---|---|
| `id` | snowflake | |
| `channel_id` | snowflake | clé de partition |
| `author_id` | snowflake | |
| `type` | int | normal, reply, system (join, boost, pin, call…), thread_starter… |
| `content` | text | ≤ 2000 (ou 4000 selon config) |
| `flags` | int | crossposted, suppress_embeds, ephemeral, loading, voice_message, suppress_notifications… |
| `tts` | bool | |
| `mention_everyone` | bool | |
| `mentions` / `mention_roles` / `mention_channels` | list | |
| `attachments` | json | id, filename, size, url, content_type, width/height, duration, waveform (voix), alt/description, flags (spoiler) |
| `embeds` | json | jusqu'à 10 (title, desc, url, color, fields, author, footer, image, thumbnail, video, provider, timestamp) |
| `reactions` | json | emoji → count, me, **burst (super)** count, burst_colors |
| `components` | json | rangées de boutons/menus/inputs |
| `sticker_items` | json | |
| `poll` | json | question, réponses, multiselect, expiry, résultats |
| `message_reference` | json | reply / forward (channel_id, message_id, guild_id, type) |
| `referenced_message` | json? | message cité (inliné) |
| `edited_at` | timestamp? | |
| `pinned` | bool | |
| `webhook_id` | snowflake? | |
| `nonce` | text? | déduplication d'envoi optimiste |
| `thread` | json? | thread créé depuis ce message |

Index secondaires (matérialisés / Meilisearch) : par auteur, par mention, recherche full-text. Voir [14 — Recherche](features/14-recherche.md).

Tables annexes Scylla : `pins (channel_id, message_id)`, `reactions_detail (message_id, emoji, user_id)`, `read_states (user_id, channel_id, last_read_id, mention_count)`, `message_attachments` (si déportées).

---

## 3. Presence & état volatile (Redis)

| Clé | Contenu |
|---|---|
| `presence:{user_id}` | statut (online/idle/dnd/offline/invisible), activités, plateformes (desktop/mobile/web) |
| `session:{session_id}` | état gateway (user, séquence, guildes abonnées) |
| `voice_state:{guild_id}:{user_id}` | salon vocal, mute/deaf/self_*, stream/vidéo |
| `typing:{channel_id}` | set d'utilisateurs en train d'écrire (TTL court) |
| `ratelimit:{bucket}:{id}` | compteurs de débit |
| `gateway_sub:{guild_id}` | sessions abonnées (fan-out) |

---

## 4. Diagramme relationnel (condensé)

```
User ─┬─< GuildMember >─┬─ Guild ─┬─< Channel ─┬─< Message ─< Reaction
      │                 │         ├─< Role      ├─< PermissionOverwrite
      ├─< Relationship  │         ├─< Emoji/Sticker/Sound
      ├─< UserSession   │         ├─< Invite / Webhook / Integration
      ├─< UserSettings  │         ├─< ScheduledEvent / AutoModRule
      └─< UserNote      │         └─< AuditLogEntry / Ban / Onboarding
                        └─ (member.roles ►─ Role.id)
```

## 5. Côté client — registre d'instances (local)

Le client est **multi-instances** ; il stocke localement (SQLite chiffré + **keystore OS**) un enregistrement **par instance**, sans jamais mélanger les identités.

| Champ | Notes |
|---|---|
| `address` | URL/hôte de l'instance (clé) |
| `instance_id` / `public_key` | identité épinglée (TOFU, alerte si elle change) |
| `branding_cache` | nom, icône, couleur (sondés via `GET /instance`) |
| `account_id`, `access_jwt`, `refresh_token` | **session propre à l'instance** (jeton dans le keystore) |
| `display` | pseudo/avatar de **ce** compte (cache) |
| `pinned`, `position`, `muted`, `last_active` | affichage du switcher |

Les caches d'entités (guildes, messages, membres…) sont **partitionnés par instance** dans SQLite — aucune fuite inter-instances.

Suite : **[04 — API REST](04-api-rest.md)**.
