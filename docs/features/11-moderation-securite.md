# Fonctionnalités — Modération & sécurité

Réf. : [07-securite](../07-securite-chiffrement.md) · [10-roles-permissions](10-roles-permissions.md).

## Actions de modération
- [ ] **Expulser** (kick) un membre (avec raison auditée).
- [ ] **Bannir** : permanent, avec **purge des messages** (dernières 1 h → 7 j), raison, **bulk-ban**.
- [ ] **Débannir**, liste des bannis (recherche, raison, modérateur).
- [ ] **Timeout** (`MODERATE_MEMBERS`) : empêche d'écrire/parler/réagir pour une durée (60 s → 28 j).
- [ ] **Mute / deafen serveur** (vocal), **déplacer / déconnecter** du vocal.
- [ ] Supprimer un message, **suppression en masse**, désépingler.
- [ ] Gérer pseudos (`MANAGE_NICKNAMES`), retirer des rôles.

## AutoMod (modération automatique)
- [ ] Règles par **mots-clés** (listes + **regex** + jokers), **presets** (insultes, contenu sexuel, slurs).
- [ ] **Anti-spam** (messages répétés/rapides), **anti-mention-spam** (seuil de mentions).
- [ ] **Liens nuisibles** / malveillants, blocage de domaines.
- [ ] **Filtre de profil de membre** (pseudo/bio interdits empêchant l'accès).
- [ ] **Actions** : bloquer le message (avec message custom), **alerter** un salon, **timeout** auto.
- [ ] **Exemptions** par rôles / salons, activation/désactivation, journalisation des déclenchements.

## Niveaux & filtres serveur
- [ ] **Niveau de vérification** : aucun / faible (email vérifié) / moyen (inscrit > 5 min) / élevé (membre > 10 min) / très élevé (téléphone vérifié).
- [ ] **Filtre de contenu explicite** : désactivé / sans rôle / tous les membres (scan des médias).
- [ ] **Salons NSFW** (gate d'âge).
- [ ] **Raid protection** : détection de vagues de joins, throttling, verrouillage temporaire, défi de vérification.

## Audit log
- [ ] Journal horodaté de **toutes** les actions sensibles : création/édition/suppression de salons, rôles, emojis, webhooks ; bans/kicks/timeouts ; changements de membres ; déplacements vocaux ; mises à jour de serveur ; AutoMod ; etc.
- [ ] **Filtres** par utilisateur, type d'action, cible ; détail des **changements** (avant/après) et **raison**.
- [ ] Pagination, recherche, export.

## Signalements & sûreté
- [ ] **Signaler** un message / utilisateur / serveur → file de modération.
- [ ] **DM safety** : filtrage des médias explicites / liens dans les MP (réglable utilisateur).
- [ ] **Scan de fichiers** uploadés (hash matching, antivirus).
- [ ] Outils de modération en masse (purge, lockdown d'un salon, slowmode d'urgence).
- [ ] **Alertes de sécurité** dans un salon dédié.

## Modération côté plateforme (admin Ozone)
- [ ] Suspension/restriction de comptes, bannissement de serveurs abusifs, gestion des signalements globaux.

## Definition of Done
- Un modérateur configure une règle AutoMod (mots-clés + anti-spam → bloquer + alerter), timeout un spammeur, bannit un récidiviste avec purge 24 h, et retrouve chaque action détaillée dans l'audit log filtré par son nom.
