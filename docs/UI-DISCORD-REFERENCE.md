# Référence visuelle Discord — pour la reproduction fidèle (client web Ozone)

> Document de référence pour reproduire fidèlement l'interface Discord (bureau/web) dans le
> client `desktop/` (React + TypeScript + Tailwind). Compilé à partir d'une recherche
> référentielle recoupée (juin 2026).
>
> **Source la plus fiable** : les **variables CSS réelles** de Discord, documentées par
> [BetterDiscord](https://docs.betterdiscord.app/discord/variables). Leurs valeurs HSL,
> converties en hex, retombent **exactement** sur la palette canonique (`--brand-500` =
> `hsl(235 85.6% 64.7%)` = `#5865F2`, confirmé par [discord.com/branding](https://discord.com/branding)).
>
> ⚠️ **Piège de version** : la plupart des listes « couleurs Discord » sur le web publient la
> palette **ancienne** (avant 2022) : chat `#36393F`, sidebar `#2F3136`, online `#43B581`,
> blurple `#7289DA`. **Ne pas utiliser.** Les valeurs ci-dessous sont la palette **actuelle (2023+)**.
>
> ⚠️ **Multi-thèmes 2024+** : Discord ship désormais 4 thèmes (Light, Ash, Dark, Onyx) + 3 densités
> (Default/Spacious/Compact). On cible le **Dark classique + densité Default**. Les valeurs Onyx/Ash
> ne sont pas documentées publiquement (hors périmètre).

---

## 1. Palette — thème sombre actuel

Le principe central : **les surfaces s'assombrissent vers l'arrière** du z-stack (l'inverse de
Material). Et surtout : **survol / sélection = voiles translucides**, PAS des hex pleins (c'est l'erreur
n°1 quand on clone Discord).

### Surfaces

| Rôle (token Discord) | Hex | Usage |
|---|---|---|
| `--background-tertiary` | **#1E1F22** | Rail des serveurs, fonds les plus profonds, barres de recherche |
| `--background-secondary` | **#2B2D31** | Sidebar des salons / liste des MP / liste des membres |
| `--background-secondary-alt` | **#232428** | Panneau utilisateur (bas de la sidebar) |
| `--background-primary` | **#313338** | Zone de chat (fond de la liste des messages) |
| `--channeltextarea-background` | **#383A40** | Champ de saisie du message (composeur) |
| `--background-floating` | **#111214** | Tooltips, menus contextuels, popouts, menus flottants |

### Voiles (survol / actif / sélectionné) — **translucides**

Teinte neutre `--primary-500` (#4E5058) à alpha croissant. Parce qu'ils sont translucides, le
**même** voile rend correctement sur n'importe quelle surface.

| Token | Valeur | Usage |
|---|---|---|
| `--background-modifier-hover` | `rgba(78, 80, 88, 0.30)` | Survol d'un salon / d'une ligne / d'un bouton |
| `--background-modifier-active` | `rgba(78, 80, 88, 0.48)` | Élément pressé |
| `--background-modifier-selected` | `rgba(78, 80, 88, 0.60)` | Salon / MP actuellement sélectionné |
| `--background-mentioned` | `rgba(240, 177, 50, 0.10)` | Message qui vous @mentionne (+ barre d'accent blurple 2px à gauche) |

### Texte & interactif

| Rôle | Hex | Usage |
|---|---|---|
| `--header-primary` | **#F2F3F5** | Texte fort : pseudos, titres d'en-tête, titres |
| `--text-normal` | **#DBDEE1** | Corps des messages |
| `--header-secondary` | **#B5BAC1** | Labels de section, en-têtes secondaires, horodatages |
| `--text-muted` | **#949BA4** | Atténué / placeholder / désactivé |
| `--channels-default` | **#949BA4** | Nom de salon au repos, texte de catégorie |
| `--text-link` | **#00A8FC** | Liens hypertexte |
| `--interactive-normal` | **#B5BAC1** | Icônes/labels au repos |
| `--interactive-hover` | **#DBDEE1** | Icône/label au survol |
| `--interactive-active` | **#FFFFFF** | Icône/label actif/sélectionné |
| `--interactive-muted` | **#4E5058** | Icônes désactivées |

### Accent & statuts (valeurs **in-app**, ≠ couleurs marketing)

> ⚠️ Le rouge/vert **de l'app** ne sont pas le rouge/vert **de marque** (`#ED4245`/`#57F287`). Utiliser ceux-ci :

| Rôle | Hex | Usage |
|---|---|---|
| `--brand-500` (blurple) | **#5865F2** | Boutons primaires, états actifs, badges, accents |
| `--brand-560` (blurple hover) | **#4752C4** | Survol du bouton primaire |
| `--green-360` (online / positif) | **#23A559** | Point « en ligne », texte positif |
| `--green-430` | **#248046** | Fond de bouton positif (« Accepter ») |
| `--yellow-300` (idle) | **#F0B132** | Point « absent », avertissement |
| `--red-400` (dnd / danger) | **#F23F42** | Point « ne pas déranger », danger |
| `--red-430` | **#DA373C** | Fond de bouton danger (« Supprimer », « Expulser ») |
| `--status-offline` | **#80848E** | Point « hors ligne » (anneau creux) |

**Couleurs de rôle** : ce ne sont **pas** des tokens de thème — ce sont des hex arbitraires par rôle,
appliqués en `style={{ color: roleHex }}` sur le pseudo (fallback `--header-primary`).

---

## 2. Architecture de tokens recommandée (à mettre en place)

Refléter le modèle **2 couches** de Discord : **les variables CSS portent le thème**, Tailwind ne fait
que **référencer** ces variables ⇒ les deux thèmes (et plus tard cozy/compact, scaling, saturation,
reduced-motion) deviennent un simple **échange de tokens**, pas un re-render.

```css
/* index.css */
:root {
  /* primitives (constantes) */
  --primary-130:#F2F3F5; --primary-230:#DBDEE1; --primary-330:#B5BAC1; --primary-360:#949BA4;
  --primary-400:#80848E; --primary-500:#4E5058; --primary-500-hsl:228 6% 32.5%;
  --primary-560:#383A40; --primary-600:#313338; --primary-630:#2B2D31; --primary-660:#232428;
  --primary-700:#1E1F22; --primary-800:#111214;
  --brand-500:#5865F2; --brand-560:#4752C4;
  --green-360:#23A559; --green-430:#248046; --yellow-300:#F0B132; --yellow-300-hsl:40 86.4% 56.9%;
  --red-400:#F23F42; --red-430:#DA373C; --blue-345:#00A8FC; --white-500:#FFFFFF;
}
.theme-dark, :root {            /* sombre par défaut */
  --bg-tertiary:var(--primary-700); --bg-secondary:var(--primary-630);
  --bg-secondary-alt:var(--primary-660); --bg-primary:var(--primary-600);
  --bg-input:var(--primary-560); --bg-floating:var(--primary-800);
  --mod-hover:hsl(var(--primary-500-hsl)/.3); --mod-active:hsl(var(--primary-500-hsl)/.48);
  --mod-selected:hsl(var(--primary-500-hsl)/.6); --mentioned:hsl(var(--yellow-300-hsl)/.1);
  --header-primary:var(--primary-130); --header-secondary:var(--primary-330);
  --text-normal:var(--primary-230); --text-muted:var(--primary-360);
  --text-link:var(--blue-345);
  --interactive-normal:var(--primary-330); --interactive-hover:var(--primary-230);
  --interactive-active:var(--white-500); --interactive-muted:var(--primary-500);
  --accent:var(--brand-500); --accent-hover:var(--brand-560);
  --status-online:var(--green-360); --status-idle:var(--yellow-300);
  --status-dnd:var(--red-400); --status-offline:var(--primary-400);
}
/* .theme-light { … } — surcharger uniquement les alias sémantiques */
```

```js
// tailwind.config.js — theme.extend.colors
bg:{primary:'var(--bg-primary)',secondary:'var(--bg-secondary)','secondary-alt':'var(--bg-secondary-alt)',
    tertiary:'var(--bg-tertiary)',input:'var(--bg-input)',floating:'var(--bg-floating)'},
mod:{hover:'var(--mod-hover)',active:'var(--mod-active)',selected:'var(--mod-selected)'},
header:{primary:'var(--header-primary)',secondary:'var(--header-secondary)'},
text:{normal:'var(--text-normal)',muted:'var(--text-muted)',link:'var(--text-link)'},
interactive:{normal:'var(--interactive-normal)',hover:'var(--interactive-hover)',
             active:'var(--interactive-active)',muted:'var(--interactive-muted)'},
accent:{DEFAULT:'var(--accent)',hover:'var(--accent-hover)'},
status:{online:'var(--status-online)',idle:'var(--status-idle)',dnd:'var(--status-dnd)',offline:'var(--status-offline)'},
```

Usage : `bg-bg-primary`, `text-text-normal`, `hover:bg-mod-hover`, `text-header-primary`… Le thème
bascule via une classe racine.

---

## 3. Typographie

- **Police Discord** : `gg sans` (depuis déc. 2022, avant : Whitney). **Propriétaire, non
  redistribuable** ⇒ ne pas l'embarquer. **Substitut libre recommandé : Inter** (OFL, x-height haut,
  pensé pour l'UI 12–16px ; consensus des clones). Pile : `Inter, "Noto Sans", "Helvetica Neue",
  Helvetica, Arial, sans-serif`.
- **Monospace (code)** : `Consolas, "Liberation Mono", Menlo, Monaco, "Courier New", monospace`.

### Échelle de type

| Rôle | taille | interligne | graisse | autres |
|---|---|---|---|---|
| Corps de message | 16px | 1.375 (≈22px) | 400 | — |
| Pseudo (dans un message) | 16px | ≈1.375 | 600 | couleur de rôle |
| Horodatage | 12px | ~16px | 500 | muted |
| Nom de salon (sidebar) | 16px | ~20px | 500 | — |
| En-tête de catégorie | 12px | ~16px | 700 | **MAJUSCULES**, +0.02em, muted |
| Nom de guilde (en-tête) | 16px | ~20px | 600 | — |
| En-têtes liste de membres | 12px | ~16px | 600 | **MAJUSCULES**, +0.02em |
| Titre de modale (H1) | 20–24px | 1.2 | 600–700 | — |
| Labels de champ (« E-MAIL ») | 12px | ~16px | 700 | **MAJUSCULES**, +0.02em — signature Discord |
| Petit / aide | 14px | ~18px | 400 | — |

Réglage in-app « Chat Font Scaling » : 12→24px (défaut 16). « Space Between Message Groups » : 0→24px.

---

## 4. Dimensions, avatars, rayons

### Colonnes (gauche → droite)

| Région | Taille | Fixe/flex |
|---|---|---|
| Rail des serveurs | **72px** large | fixe |
| Sidebar des salons | **240px** (`w-60`) | fixe |
| Contenu principal (chat + composeur) | `flex 1fr` | flexible |
| Liste des membres | **240px** | fixe, repliable |
| En-tête de salon | **48px** haut (`h-12`) | fixe |
| Panneau utilisateur | **~52px** haut (bas de la sidebar) | fixe |
| Fenêtre minimale | **940 × 500** | — |

### Avatars (toujours **ronds**, sauf icônes de serveur)

| Contexte | Diamètre |
|---|---|
| Message (cozy) | **40px** |
| Membre / sidebar / MP | **32px** |
| Icône de serveur (rail) | **48px** |
| Popout de profil | **80px** |
| Profil complet | **128px** |

### Rayons

- Contrôles / champs : **8px** (refresh moderne) — *(3px à l'ère legacy)*.
- Modale : **16px**. Avatars / points de statut : ronds (`9999px`).
- **Icône de serveur (signature)** : `50%` au repos → **squircle ~16px** au survol/sélection,
  transition **~150–200ms ease**.
- **Pastille de sélection** (barre blanche, bord gauche du rail) : largeur ~4px, rayon 3px,
  **hauteur selon l'état** : repos `0` · non-lu `8px` · survol `20px` · actif `40px` (transition ~200ms).

### Rythme des messages

- Gouttière gauche ≈ **72px** (16 padding + 40 avatar + 16) en cozy.
- Message **groupé** (même auteur, < ~7 min) : pas d'avatar/pseudo, padding vertical ~**2px**,
  horodatage à gauche **au survol** uniquement.
- Écart entre groupes ~**16px**.

---

## 5. Composants & états

- **Rail serveurs** : icône 48px, morph cercle→squircle au survol/actif ; pastille blanche à gauche
  (états ci-dessus) ; badge rouge de mention (compte) en bas-droite ; tooltip sombre à droite ;
  bouton Accueil/MP en haut (actif = blurple) ; séparateur ; bouton « + » vert (fond se remplit de
  vert au survol) ; dossiers = grille 2×2.
- **Sidebar salons** : en-tête de catégorie MAJUSCULES + chevron (rotation -90° replié, ~200ms) ;
  ligne de salon ~32px, rayon ~4px ; états repos/survol/actif via les **voiles** ; non-lu = pseudo
  blanc + **point blanc à gauche** ; mention = **badge rouge à droite** ; muet = atténué ; icônes par
  type (texte `#`, vocal 🔊, annonces mégaphone, forum bulles, stage).
- **Ligne de message** : survol = voile pleine largeur ; **barre d'actions flottante** en haut-droite
  (apparait au survol, `top:-16px right:8px`) : Réagir / Fil / Répondre / (•••) (+ Éditer si auteur) ;
  **puces de réaction** (emoji + compte ; « réagi par moi » = **contour blurple**) ; ligne de
  référence de réponse ; tag `(modifié)` ; surbrillance de mention (fond `--background-mentioned` +
  barre d'accent) ; **groupement** même-auteur < 7 min ; messages système = ligne centrée + icône.
- **Tooltips / menus / popouts** : surface `#111214`, rayon 8px, ombre `0 5px 15px rgba(0,0,0,.2)`.
  Menu contextuel : **survol = blurple** (items destructifs = **rouge**) ; séparateurs fins ; sous-menus.
- **Popout de profil** : bannière (image ou couleur d'accent), avatar 80px chevauchant en bas-gauche
  avec anneau de statut, badges, « À propos », membre depuis, rôles (puces colorées), boîte « Message ».
- **Modales** : surface `--background-primary`, rayon ~8–16px, **voile noir ~rgba(0,0,0,.85)**,
  fermeture **X** en haut-droite (overlays plein écran : « X » circulaire + label `ESC` hors colonne).
  Paramètres : **2 colonnes** (nav gauche `--background-secondary` + contenu droite, max ~740px).
- **Statuts (points)** : même silhouette, **masque différent** (cutout SVG) : online = disque plein ;
  idle = croissant (bouchée en haut-gauche) ; dnd = disque avec barre horizontale ; offline = anneau
  creux. Le point est en bas-droite de l'avatar avec un **anneau de la couleur du fond** (`box-shadow:
  0 0 0 3px <bg>`), ~10px sur un avatar 40px.

### Motion (tout ≤ 300ms, discret)

| Élément | Propriété | Durée / easing |
|---|---|---|
| Morph squircle icône | `border-radius` | ~150–200ms ease |
| Pastille de sélection | `height` | ~150–300ms ease-out |
| Survol salon/menu | `background-color`, `color` | ~100–200ms ease |
| Chevron catégorie | `rotate` | ~150–200ms ease |
| Survol ligne message | `background-color` | ~50–150ms ease |
| Tooltips | `opacity`+`scale` | ~100ms |
| Menus/popouts | `opacity`+`scale` (0.95→1) | ~100–150ms ease-out |
| Modales | backdrop fade + `scale`(0.9→1) | ~150–250ms ease-out |

Implémentation : Tailwind `transition-* duration-150/200 ease-out` pour l'eased ; ressorts (Framer
Motion, stiffness ~400–500 / damping ~30) pour squircle/pastille/popout. **Respecter
`prefers-reduced-motion`.**

---

## 6. Fonctionnalités liées à l'apparence

- **Rendu Markdown « façon Discord »** (sous-ensemble volontaire de CommonMark, via la lib
  `simple-markdown` de Khan Academy — ce que Discord utilise réellement) : `**gras**`, `*italique*`,
  `***gras-italique***`, `__souligné__` (⚠️ `__` = **souligné**, pas gras), `~~barré~~`, code inline,
  blocs ``` ```lang ```, citations `>`/`>>>`, listes, sous-texte `-#`, titres `#`/`##`/`###`,
  liens masqués `[txt](url)` (popup de confirmation pour domaines non sûrs), **spoilers** `||x||`
  (flou cliquable), **timestamps** `<t:unix:style>` (styles t/T/d/D/f/F/R, R = relatif live).
- **Mentions** en puces stylées : `<@id>`, `<@&id>` (couleur de rôle), `<#id>` (avec `#`),
  `@everyone`/`@here`. **Emoji** : custom `<:nom:id>`/`<a:nom:id>` (image CDN), unicode ; **jumbo**
  quand le message ne contient que des emoji.
- **Embeds & pièces jointes** : embed lien (barre d'accent gauche, titre/desc/vignette), images
  (lightbox), vidéos (lecteur), fichiers (carte icône+nom+taille+download) ; aperçu de téléversement
  (tuiles retirables au-dessus du composeur).
- **Système de non-lu** : salon non-lu en gras + point blanc à gauche ; badge rouge de compte de
  mention ; **barre « Nouveaux messages »** au premier non-lu ; indicateur par guilde sur le rail ;
  mark-as-read. État requis : `lastReadMessageId` + `mentionCount` par salon (déjà fourni par
  l'API : `GET /users/@me/read-states`).
- **Indicateur de saisie** (bulle 3 points + « X écrit… », expire ~10s), **présence** (points de
  statut + statut perso/activité). Discord **n'a pas** d'accusés de lecture par message → ne pas en
  ajouter.
- **Réglages d'apparence à exposer** (cible minimale) : thème (sombre/clair/midnight), **cozy /
  compact**, scaling de police, espace entre groupes, saturation/grayscale, reduced-motion, zoom.

### Cozy vs Compact (deux gabarits de ligne)

| Aspect | Cozy (défaut) | Compact |
|---|---|---|
| Avatar | 40px à gauche (1ʳᵉ du groupe) | aucun (mini ~16–20px optionnel) |
| Disposition | 2 lignes : pseudo+heure / corps | 1 ligne : `[heure] Pseudo message…` |
| Horodatage | inline près du pseudo (survol si groupé) | en tête de **chaque** ligne (colonne fixe) |
| But | aéré, scannable par auteur | densité maximale |

Le groupement (même auteur < 7 min) s'applique aux **deux** modes ; seule la disposition change.

---

## 7. Méthode recommandée (stack & approche)

**Principe directeur : tokens d'abord, composants maison skinnés Tailwind, primitives accessibles
déléguées.**

1. **Design tokens** : recréer la palette en **variables CSS (primitives + alias sémantiques)** +
   Tailwind qui les référence (cf. §2). ⇒ thèmes, cozy/compact, scaling, saturation, reduced-motion =
   échanges de tokens. *(Recréer NOS valeurs — ne pas embarquer les assets/police propriétaires de
   Discord.)*
2. **Overlays accessibles : [Radix UI](https://www.radix-ui.com)** (MIT) pour menu contextuel,
   dropdown, tooltip, hover-card, dialog, popover, scroll-area, slider, switch — skinnés Tailwind.
   *(Radix a la largeur dont Discord a besoin — notamment menu contextuel + hover-card que Headless UI
   n'a pas.)* Layout/skin (rail, sidebar, messages, composeur, embeds, badges, points de présence) =
   **composants maison Tailwind**.
3. **Markdown : [`@khanacademy/simple-markdown`](https://github.com/discord/simple-markdown)** (ou
   `discord/simple-markdown`) avec **sortie React** + jeu de règles Discord (réf :
   [`brussell98/discord-markdown`](https://github.com/brussell98/discord-markdown), MIT). **Jamais de
   HTML brut** (sortie en éléments React, pas `dangerouslySetInnerHTML`) ; **valider les URL** des
   liens masqués (`http(s):` seulement, bloquer `javascript:`) ; résoudre mentions/emoji via callback
   sur l'état de confiance. *(C'est exactement ce que Discord utilise → quirks fidèles.)*
4. **Coloration syntaxique : [highlight.js](https://highlightjs.org)** (sous-ensemble de langages,
   **lazy-load**) — s'aligne sur les classes émises par `discord-markdown`. *(Éviter Shiki côté client
   : WASM + bundle lourd. Prism = alternative.)*
5. **Icônes : [Lucide](https://lucide.dev)** (`lucide-react`, ISC, fork de Feather, stroke-based,
   tree-shaking) — le plus proche de l'esthétique Discord parmi les sets libres. Tabler en secours.

**Références (lecture seule — licences copyleft, NE PAS copier le code)** : Spacebar `client`
(React+TS+Vite+Tauri, **AGPL**), Revolt `revite` (Preact, **AGPL**), Vencord (**GPL** — meilleur pour
comprendre le DOM/CSS réel + le modèle de variables), [BetterDiscord — variables
CSS](https://docs.betterdiscord.app/discord/variables) (meilleure réf palette/tokens). Assets/police/
icônes propriétaires Discord **non redistribuables** ⇒ recréer.

### Plan par phases (fidélité sans sur-ingénierie)

0. **Tokens & coque** : palette en variables CSS + Tailwind ; coque 3 colonnes ; plomberie
   reduced-motion. *(Débloque tout le reste.)*
1. **Liste de messages statique** : lignes Cozy + Compact, avatars, groupement, points de présence,
   couleurs de rôle.
2. **Markdown cœur** : renderer React simple-markdown (gras/italique/souligné/barré, code inline,
   blocs + highlight.js, citations, listes, titres, liens masqués + modale de confirmation, spoilers,
   `<t:>` avec relatif live).
3. **Mentions, emoji, embeds, pièces jointes** : puces mention/rôle/salon ; emoji custom + unicode
   (jumbo seul) ; embeds lien (barre d'accent) ; images/vidéos/fichiers + aperçu d'upload.
4. **Non-lu & temps réel** : `lastReadMessageId`/`mentionCount` dans le store ; gras + point blanc,
   badges rouges, barre « nouveaux messages », indicateurs de rail, mark-as-read ; indicateur de saisie.
5. **Surface de réglages** : panneaux Apparence/Accessibilité câblés aux tokens (thème, cozy/compact,
   scaling, espace de groupe, saturation/grayscale, reduced-motion, zoom).
6. **Overlays & finitions** : menus contextuels / tooltips / hover-cards / modales Radix ; transitions
   conditionnées à reduced-motion.

**Anti-sur-ingénierie** : coller au **sous-ensemble** réellement rendu par Discord ; lazy-load du
highlighter + données emoji ; réglages = échanges de tokens ; ne construire que les overlays Radix
utilisés.

---

## 8. À vérifier sur un client Discord live (DevTools / Chrome MCP)

Valeurs dépendantes de version (refresh 2025) ou non documentées publiquement, à confirmer en lisant
les variables `:root` réelles : px exacts `--radius-*` / `--space-*` ; graisse du pseudo (500 vs 600) ;
hauteurs composeur/panneau utilisateur ; défaut « espace entre groupes » (0 vs 16) ; tokens Onyx/Ash ;
timings précis des animations popout/modale. *Méthode la plus directe :*
`getComputedStyle(document.documentElement)` puis filtrer `--font` / `--radius` / `--space` / `--text`.

---

## 9. Sources principales

- **Variables CSS Discord (réf. première)** : https://docs.betterdiscord.app/discord/variables
- Branding officiel : https://discord.com/branding · Light theme : https://discord.com/blog/light-theme-redeemed
- Display settings (officiel) : https://discord.com/blog/making-discord-on-desktop-look-just-right-display-settings-to-ease-the-eyes
- Rendu Markdown (confirme simple-markdown) : https://discord.com/blog/how-discord-renders-rich-messages-on-the-android-app
- gg sans FAQ : https://support.discord.com/hc/en-us/articles/9507780972951-gg-sans-Font-Update-FAQ
- Markdown 101 : https://support.discord.com/hc/en-us/articles/210298617 · Spoilers : .../360022320632 · Compact : .../217047657
- Timestamps : https://gist.github.com/LeviSnoot/d9147767abeef2f770e9ddcd91eb85aa
- Libs : Radix https://www.radix-ui.com · discord-markdown https://github.com/brussell98/discord-markdown ·
  simple-markdown https://github.com/discord/simple-markdown · highlight.js https://highlightjs.org ·
  Lucide https://lucide.dev
- Réfs (lecture seule) : Spacebar https://github.com/spacebarchat/client · Revolt
  https://github.com/revoltchat/revite · Vencord https://github.com/Vendicated/Vencord
