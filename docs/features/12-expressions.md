# Fonctionnalités — Emojis, stickers, soundboard & GIF

Réf. : [03-modele-de-donnees](../03-modele-de-donnees.md) · [04-messagerie](04-messagerie.md).

## Emojis custom
- [ ] Ajouter des emojis de serveur (statiques + **animés**), nommage, quota par palier de boost.
- [ ] **Restriction par rôle** (emoji réservé à certains rôles).
- [ ] Utilisation cross-serveur (`USE_EXTERNAL_EMOJIS`), dans messages, réactions, statuts.
- [ ] Sélecteur : catégories, **récents**, **favoris**, recherche par shortcode, skin tones (emoji unicode).
- [ ] Emojis unicode complets (table à jour), suggestions `:shortcode:` à la frappe.

## Stickers
- [ ] Stickers de serveur : formats **PNG / APNG / Lottie / GIF**, nom, description, tag emoji.
- [ ] Envoi d'un sticker (1 par message), aperçu animé, packs.
- [ ] Utilisation cross-serveur (`USE_EXTERNAL_STICKERS`).

## Soundboard (vocal)
- [ ] Sons de serveur courts joués dans un vocal, **volume**, emoji, nommage.
- [ ] Sons **custom** uploadés + sons par défaut, **cooldown** anti-spam, permission `USE_SOUNDBOARD`.
- [ ] Sons d'autres serveurs (`USE_EXTERNAL_SOUNDS`), événement `VOICE_CHANNEL_EFFECT_SEND`.

## GIF
- [ ] Recherche **type Tenor** (intégration fournisseur), **trending**, catégories, **favoris**, récents.
- [ ] Envoi inline avec aperçu animé, autoplay configurable (accessibilité).

## Gestion (Server Settings)
- [ ] Onglets Emojis / Stickers / Soundboard : ajout, quota, restrictions, suppression, qui a créé (`CREATE_GUILD_EXPRESSIONS` vs `MANAGE_GUILD_EXPRESSIONS`).

## Cosmétiques de profil (rappel)
- [ ] Décorations d'avatar, nameplates, effets de profil — voir [08-profil](08-profil.md) (débloqués sans paiement).

## Definition of Done
- Un serveur dispose d'emojis animés restreints à un rôle, de stickers Lottie, et d'un soundboard custom ; un membre les utilise dans un message, une réaction, et dans un vocal, y compris depuis un autre serveur.
