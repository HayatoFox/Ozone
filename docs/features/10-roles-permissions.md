# Fonctionnalités — Rôles & permissions

Cœur du système d'autorisation. Réf. : [04-api-rest](../04-api-rest.md#4-validation-des-permissions-côté-serveur) · [07-securite](../07-securite-chiffrement.md#2-autorisation).

## Modèle

- Permissions stockées en **bitfield 64 bits** (chaque permission = 1 bit), exactement comme Discord.
- **Rôles** : ensembles de permissions, **hiérarchisés** par `position`. `@everyone` = rôle de base (id = id de la guilde).
- **Overrides** par salon/catégorie : `allow` / `deny` par rôle **ou** par membre.
- Un utilisateur cumule les permissions de **tous ses rôles** (OR des allow).

## Matrice complète des permissions (parité Discord)

### Général / serveur
| Permission | Effet |
|---|---|
| `ADMINISTRATOR` | toutes les permissions, **bypass** des overrides |
| `VIEW_CHANNEL` | voir le salon / rejoindre le vocal |
| `MANAGE_CHANNELS` | créer/éditer/supprimer des salons |
| `MANAGE_ROLES` | gérer les rôles (≤ au sien) et overrides |
| `MANAGE_GUILD` | éditer le serveur, intégrations |
| `MANAGE_WEBHOOKS` | gérer les webhooks |
| `MANAGE_GUILD_EXPRESSIONS` | éditer/supprimer emojis, stickers, sons (de tous) |
| `CREATE_GUILD_EXPRESSIONS` | créer/gérer ses propres emojis/stickers/sons |
| `VIEW_AUDIT_LOG` | consulter le journal d'audit |
| `VIEW_GUILD_INSIGHTS` | voir les statistiques du serveur |
| `VIEW_CREATOR_MONETIZATION_ANALYTICS` | analytics d'abonnements de rôle |
| `CREATE_INSTANT_INVITE` | créer des invitations |

### Membres
| Permission | Effet |
|---|---|
| `KICK_MEMBERS` | expulser |
| `BAN_MEMBERS` | bannir |
| `MODERATE_MEMBERS` | **timeout** (restreindre messages/voix temporairement) |
| `CHANGE_NICKNAME` | changer **son** pseudo |
| `MANAGE_NICKNAMES` | changer le pseudo des autres |

### Texte
| Permission | Effet |
|---|---|
| `SEND_MESSAGES` | envoyer des messages / créer des posts forum |
| `SEND_MESSAGES_IN_THREADS` | écrire dans les fils |
| `CREATE_PUBLIC_THREADS` | créer des fils publics |
| `CREATE_PRIVATE_THREADS` | créer des fils privés |
| `MANAGE_THREADS` | gérer/archiver/voir les fils privés |
| `SEND_TTS_MESSAGES` | messages TTS |
| `MANAGE_MESSAGES` | supprimer les messages des autres, bulk-delete |
| `PIN_MESSAGES` | épingler/désépingler (séparée de MANAGE_MESSAGES) |
| `EMBED_LINKS` | auto-embed des liens |
| `ATTACH_FILES` | joindre des fichiers |
| `READ_MESSAGE_HISTORY` | lire l'historique |
| `MENTION_EVERYONE` | `@everyone`/`@here`/toutes les mentions de rôle |
| `USE_EXTERNAL_EMOJIS` | emojis d'autres serveurs |
| `USE_EXTERNAL_STICKERS` | stickers d'autres serveurs |
| `SEND_VOICE_MESSAGES` | messages vocaux |
| `SEND_POLLS` | créer des sondages |
| `BYPASS_SLOWMODE` | ignorer le slowmode |

### Vocal
| Permission | Effet |
|---|---|
| `CONNECT` | rejoindre un vocal |
| `SPEAK` | parler |
| `STREAM` | vidéo / Go Live (partage d'écran) |
| `USE_VAD` | utiliser la détection d'activité vocale |
| `PRIORITY_SPEAKER` | locuteur prioritaire |
| `MUTE_MEMBERS` | mute serveur |
| `DEAFEN_MEMBERS` | deafen serveur |
| `MOVE_MEMBERS` | déplacer/déconnecter du vocal |
| `USE_SOUNDBOARD` | soundboard |
| `USE_EXTERNAL_SOUNDS` | sons d'autres serveurs |
| `SET_VOICE_CHANNEL_STATUS` | définir le statut du salon vocal |

### Stage & événements
| Permission | Effet |
|---|---|
| `REQUEST_TO_SPEAK` | demander la parole en stage |
| `CREATE_EVENTS` | créer des événements (et gérer les siens) |
| `MANAGE_EVENTS` | éditer/supprimer les événements |

### Apps
| Permission | Effet |
|---|---|
| `USE_APPLICATION_COMMANDS` | slash commands / menus contextuels |
| `USE_EMBEDDED_ACTIVITIES` | activités embarquées (hors périmètre produit, perm conservée) |
| `USE_EXTERNAL_APPS` | réponses publiques d'apps installées par l'utilisateur |

> Total ≈ 45+ permissions, alignées sur les bits Discord (cf. [research permissions](../03-modele-de-donnees.md)). Les valeurs de bits exactes sont définies dans `ozone-proto`.

## Hiérarchie & règles d'application

1. **Propriétaire** et `ADMINISTRATOR` → tout autorisé, overrides ignorés.
2. Un membre ne peut **agir que sur des rôles/membres situés sous son rôle le plus haut** (gestion de rôles, kick/ban/timeout, déplacement vocal).
3. **Permissions de base** = `@everyone` ∪ (tous les rôles du membre).
4. **Overrides** appliqués dans l'ordre : base → `@everyone` overrides de la catégorie → overrides de rôle de la catégorie → override de membre de la catégorie → mêmes étapes au niveau **salon**. `deny` puis `allow` à chaque niveau.
5. `VIEW_CHANNEL` refusé → le salon est invisible (et ses descendants).
6. Certaines permissions n'ont de sens qu'à certains niveaux (ex. `SPEAK` en vocal).

### Algorithme (pseudo-code)
```
fn effective_perms(member, channel):
    if guild.owner == member: return ALL
    base = @everyone.permissions
    for role in member.roles: base |= role.permissions
    if base has ADMINISTRATOR: return ALL
    perms = base
    # overrides catégorie (si salon enfant non synchronisé hérite quand même la résolution)
    for level in [category, channel]:
        ow = level.overwrites
        apply(@everyone) : perms &= ~ow.deny ; perms |= ow.allow
        deny_acc=0; allow_acc=0
        for role in member.roles: deny_acc|=ow[role].deny; allow_acc|=ow[role].allow
        perms = (perms & ~deny_acc) | allow_acc
        apply(member-specific) : perms &= ~ow[member].deny ; perms |= ow[member].allow
    return perms
```

## UX de gestion (Server Settings → Rôles)
- [ ] Créer/renommer/supprimer un rôle, **couleur** (unie ou **dégradé/holographique**), **icône** ou emoji.
- [ ] **Hoist** (afficher séparément), **mentionnable**, **affichable à l'onboarding**.
- [ ] Réordonner par drag & drop (hiérarchie), avertissement sur les rôles gérés (bots).
- [ ] Éditeur de permissions par rôle (toggles avec recherche), badge des permissions dangereuses.
- [ ] **Overrides** par salon/catégorie : ajouter rôle/membre, états ✔/✘/neutre, **synchroniser** un salon avec sa catégorie.
- [ ] Attribuer/retirer des rôles à un membre (carte de profil, liste de membres, en masse).
- [ ] **Rôles via réaction / via bouton** (self-assign) et **rôles d'onboarding**.
- [ ] Simulateur « voir le serveur en tant que ce rôle » (prévisualisation des permissions effectives).

## Definition of Done
- Un admin crée des rôles hiérarchisés colorés, configure une catégorie privée synchronisée visible seulement par un rôle, vérifie qu'un membre sans le rôle ne voit pas les salons, attribue des rôles en self-service via réaction, et qu'un modérateur ne peut pas éditer un rôle au-dessus du sien.
