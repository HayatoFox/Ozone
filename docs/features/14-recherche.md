# Fonctionnalités — Recherche

Réf. technique : [03-modele-de-donnees](../03-modele-de-donnees.md) (Meilisearch/Elastic) · [01-architecture](../01-architecture.md).

## Recherche de messages
- [ ] Recherche full-text dans un **salon**, un **serveur**, ou un **MP**.
- [ ] **Filtres** (parité Discord) :
  - `from:@user` — auteur
  - `mentions:@user` — mentionne
  - `has:link|embed|file|image|video|sound|sticker|poll` — contient
  - `before:`, `after:`, `during:` — dates
  - `in:#salon` — salon
  - `pinned:true|false` — épinglés
- [ ] Combinaison de filtres, auto-complétion des filtres, suggestions.
- [ ] Résultats avec contexte, tri **pertinence / récent / ancien**, **sauter au message** (contexte autour).
- [ ] Surlignage des termes, pagination, nombre de résultats.

## Recherche de membres
- [ ] Rechercher un membre par pseudo/nom dans un serveur, filtres par **rôle**, statut.
- [ ] Liste de membres virtualisée (gros serveurs) avec recherche incrémentale.

## Recherche d'autres entités
- [ ] **Salons** (quick switcher), **serveurs**, **MP/groupes**, **emojis**, **GIF**, **commandes** (slash).
- [ ] **Quick switcher** global (Ctrl/Cmd+K) : sauter à n'importe quel salon/MP/serveur en tapant.
- [ ] Recherche dans les **fils** et **forums** (par tag, titre, auteur).

## Indexation (technique)
- [ ] Indexation asynchrone des messages par les **workers** (à la création/édition/suppression).
- [ ] Respect des **permissions** : on ne renvoie que ce que l'utilisateur peut lire.
- [ ] Index par serveur (sharding logique), reconstruction/backfill, multilingue (tokenisation).

## Definition of Done
- Un utilisateur tape `from:@alice has:image in:#design before:2026-01-01`, obtient les résultats pertinents, saute au message dans son contexte ; un autre utilise Ctrl+K pour changer de salon instantanément.
