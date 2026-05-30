# Fonctionnalités — Messagerie texte

Réf. : [03-modele-de-donnees](../03-modele-de-donnees.md#2-messages-scylladb--fort-débit) · [14-recherche](14-recherche.md).

## Composition & envoi
- [ ] Saisie multi-ligne, envoi (Entrée), nouvelle ligne (Maj+Entrée), brouillons persistés par salon.
- [ ] **Rendu optimiste** (le message apparaît avant l'ACK serveur, état « envoi… / échec / renvoyer »).
- [ ] File d'attente hors-ligne (envoi à la reconnexion), dédup par `nonce`.
- [ ] Limite de caractères (2000, configurable 4000), compteur.
- [ ] **TTS** (`/tts`), messages **silencieux** (sans notification, `@silent`).

## Markdown (parité complète)
- [ ] **Gras** `**`, *italique* `*`/`_`, souligné `__`, ~~barré~~ `~~`, `code inline`.
- [ ] Blocs de code ```` ``` ```` avec **coloration syntaxique** par langage.
- [ ] Citations `>` et blocs de citation `>>>`.
- [ ] **Spoilers** `||texte||` (révélation au clic).
- [ ] Titres `#`, `##`, `###` ; **sous-texte** `-#`.
- [ ] Listes à puces / numérotées (imbriquées).
- [ ] **Liens masqués** `[texte](url)`, auto-liens, **liens email/téléphone** `<…>`.
- [ ] **Horodatages** `<t:unix:format>` (relatif, date, heure… selon fuseau du lecteur).
- [ ] **Mentions** : `@utilisateur`, `@rôle`, `#salon`, `@everyone`, `@here` (avec permission).
- [ ] **Emojis** custom `<:nom:id>` et **animés** `<a:nom:id>`, emojis unicode (`:shortcode:`).
- [ ] Échappement `\`, prévisualisation live pendant la frappe.

## Pièces jointes & médias
- [ ] Upload images/vidéos/audio/fichiers (drag & drop, presse-papiers, bouton).
- [ ] Aperçus inline (image/vidéo/audio player), **texte alternatif/description**, **spoiler** sur média.
- [ ] Upload direct S3 via URL présignée, barre de progression, annulation.
- [ ] Limite de taille configurable (selon palier de boost), multi-fichiers.
- [ ] Galerie/lightbox, zoom, téléchargement, copie d'image.

## Embeds & liens
- [ ] **Embeds riches** (jusqu'à 10) : titre, description, couleur, champs, auteur, footer, image, thumbnail, vidéo, timestamp.
- [ ] **Unfurling** automatique des liens (aperçu généré côté serveur, cache).
- [ ] Suppression d'embed par message, masquage des aperçus de liens.

## Interactions sur un message
- [ ] **Réactions** emoji + **super-réactions/burst** (animées, comptées séparément).
- [ ] **Répondre** (cite le message, mention optionnelle de l'auteur).
- [ ] **Transférer** un message vers un autre salon/MP (forward).
- [ ] **Épingler / désépingler** (permission `PIN_MESSAGES`, cap 250).
- [ ] **Éditer** (badge « modifié », historique optionnel) / **supprimer** (le sien, ou avec permission).
- [ ] **Copier le texte / le lien / l'ID**, marquer comme non lu, signaler.
- [ ] **Créer un fil** depuis un message.
- [ ] Suppression en masse (bulk delete, modération).

## Sondages (polls)
- [ ] Question + 2 à N réponses (emoji + texte), **choix multiple** optionnel, **durée** d'expiration.
- [ ] Vote/retrait de vote, résultats en direct, liste des votants, clôture anticipée.

## Messages vocaux
- [ ] Enregistrement audio in-app, **forme d'onde** + durée, lecture, vitesse, transcription (option).

## Stickers, emojis, GIF
- [ ] Sélecteur d'emojis (custom serveur + unicode), récents/favoris, recherche, catégories.
- [ ] **Stickers** (statiques/animés/Lottie), packs.
- [ ] **GIF** (recherche type Tenor, favoris, catégories, trending).
- [ ] **Soundboard** dans les vocaux (voir [12](12-expressions.md)).

## États de lecture & frappe
- [ ] **Indicateurs de frappe** (« X est en train d'écrire… »).
- [ ] Compteurs de **non-lus**, badge de mentions, séparateur « nouveaux messages », « marquer comme lu ».
- [ ] **Jump to present / jump to message** (sauter à un message, contexte autour).
- [ ] Accusés de mention (le nombre de mentions non lues par salon/serveur).

## Commandes & interactions d'apps
- [ ] **Slash commands** (auto-complétion, options typées, permissions).
- [ ] **Composants** : boutons, menus déroulants (select), **modales**, **file upload component**.
- [ ] Réponses éphémères, messages d'app, suivi (follow-up).

## Système
- [ ] Messages **système** : arrivée d'un membre, boost, épingle, appel, création de fil, changement de nom de groupe, etc.
- [ ] Messages d'appel (MP) avec durée/statut.

## Definition of Done
- Un utilisateur envoie un message markdown riche (gras, code coloré, spoiler, mention, horodatage, emoji custom), y joint une image avec texte alternatif et un fichier, reçoit des réactions normales et burst, le transfère, l'épingle, crée un sondage et un message vocal, le tout en rendu optimiste fluide.
