# Fonctionnalités — Apparence, thèmes & accessibilité

Réf. technique : [02-stack-technique](../02-stack-technique.md) · [08-performance](../08-performance.md).

## Thèmes
- [ ] **Sombre**, **Clair**, **Minuit** (noir profond OLED/AMOLED), **Synchroniser avec le système**.
- [ ] **Couleur d'accent** personnalisable, palettes prédéfinies.
- [ ] **Thèmes personnalisés** : moteur de thème basé sur des **tokens de design** (couleurs, rayons, espacements) — un thème = un fichier de tokens, chargé à chaud (hot-reload).
- [ ] Import/export de thèmes communautaires (format ouvert), prévisualisation live.
- [ ] Thématisation **par serveur** (accent dérivé de l'identité du serveur, option).

## Densité & disposition
- [ ] Affichage des messages : **Cosy** (avatars, aéré) vs **Compact** (IRC-like, dense).
- [ ] **Taille de police** du chat, **zoom** de l'interface, **espacement** entre groupes de messages.
- [ ] Afficher/masquer le bouton d'envoi, l'horodatage compact, les avatars.
- [ ] Largeur de la liste de membres, repli de la barre de serveurs, mode fenêtré/plein écran.

## Accessibilité
- [ ] **Mouvement réduit** (désactive animations), **autoplay GIF/stickers/emoji animés** réglable.
- [ ] **Couleurs de rôle** dans les noms (on/off), **saturation** globale, **filtres daltonisme**.
- [ ] **Text-to-Speech** (lecture des messages), commande `/tts`.
- [ ] **Navigation clavier** complète, focus visibles, raccourcis, ordre de tabulation logique.
- [ ] Intégration **lecteurs d'écran** (AccessKit → MSAA/UIA/AT-SPI/VoiceOver).
- [ ] Contraste renforcé, taille de cible tactile, sous-titres/indicateurs visuels pour sons.
- [ ] Réduction de la transparence, préférences de police (dyslexie).

## Rendu (technique, lien perf)
- [ ] Rendu **GPU** des thèmes (pas de reflow web), changement de thème **sans relance**.
- [ ] Polices à variations, emoji couleur, fallback multi-scripts, mise en cache du shaping.
- [ ] Respect des préférences OS (thème clair/sombre, mouvement réduit, contraste).

## Definition of Done
- Un utilisateur bascule en thème Minuit avec accent custom, passe en affichage Compact avec police réduite, active « mouvement réduit » et un filtre daltonisme, importe un thème communautaire appliqué à chaud sans redémarrage, et navigue entièrement au clavier avec un lecteur d'écran.
