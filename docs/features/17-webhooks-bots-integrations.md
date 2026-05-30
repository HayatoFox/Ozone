# Fonctionnalités — Webhooks, bots & intégrations

Réf. : [04-api-rest](../04-api-rest.md#webhooks--apps) · [05-gateway](../05-gateway-temps-reel.md) · [07-securite](../07-securite-chiffrement.md).

## Webhooks
- [ ] **Webhooks entrants** : URL par salon, nom + avatar personnalisés, envoi de messages/embeds par POST.
- [ ] **Webhooks de suivi** (channel follower) : relai des salons d'annonces suivis.
- [ ] Gestion : créer/éditer/supprimer, régénérer le token, lister par salon/serveur (`MANAGE_WEBHOOKS`).
- [ ] Exécution avec overrides (username, avatar, thread cible), `wait` pour récupérer le message créé.

## Applications & bots
- [ ] Modèle **Application** : id, nom, icône, description, **bot user**, clé publique, scopes.
- [ ] **OAuth2** : `authorize`/`token`, scopes (`bot`, `applications.commands`, `identify`, `guilds`, `guilds.join`…), moindre privilège.
- [ ] **Bot** : connexion **Gateway** avec intents (dont privilégiés gated), présence, lecture/écriture selon permissions.
- [ ] Ajout d'un bot à un serveur (avec permissions demandées + rôle géré).

## Interactions (apps)
- [ ] **Slash commands** : globales ou par serveur, options typées (string, int, user, channel, role, bool, attachment), autocomplétion, permissions par commande, sous-commandes/groupes.
- [ ] **Menus contextuels** (clic droit sur message/utilisateur).
- [ ] **Composants** : boutons, menus de sélection (string/user/role/channel/mentionable), **modales** (formulaires), **file upload component**.
- [ ] Réponses : immédiate, **différée** (deferred), **éphémère** (visible seulement par l'appelant), follow-up.
- [ ] Vérification de signature **Ed25519** des requêtes d'interaction (endpoint HTTP) ou réception via Gateway.

## Rich Presence (API)
- [ ] API pour qu'une app publie une présence riche (état, détails, timestamps, boutons) — disponible pour les apps tierces, **sans** auto-détection de jeux propriétaire (cf. [périmètre](../00-vision-et-perimetre.md)).

## Intégrations serveur
- [ ] Lister/gérer les intégrations (bots, webhooks, salons suivis), rôles gérés, déconnexion.
- [ ] **App Directory** : annuaire d'apps installables (publication, recherche, fiche).

## Sécurité
- [ ] Tokens de bot révocables, rotation, scopes minimaux.
- [ ] Rate-limits dédiés aux bots/webhooks, **sharding** Gateway pour les gros bots (voir [05](../05-gateway-temps-reel.md#9-sharding)).
- [ ] Sandboxing des exécutions, validation stricte des payloads.

## Definition of Done
- Un développeur crée une app, l'ajoute comme bot à un serveur avec des permissions précises, enregistre une slash command avec autocomplétion qui répond par un message contenant des boutons et ouvre une modale ; un webhook poste un embed dans un salon.
