# Ozone — client web (React + TS + Vite, futur Tauri 2)

Client « façon Discord » pour le serveur Ozone (`ozone-api`). Réécriture de l'ancienne
interface Iced en application web, plus fidèle à Discord et plus simple à maintenir.

## Développement

```bash
cd desktop
npm install
npm run dev        # http://localhost:1420
```

Le serveur Ozone doit tourner en parallèle (par défaut `http://127.0.0.1:8080`) :

```bash
# depuis la racine du dépôt
cargo run -p ozone-api
```

Le proxy Vite relaie automatiquement :

- `/api/*`   → `http://127.0.0.1:8080/*`   (REST)
- `/gateway` → `ws://127.0.0.1:8080/gateway` (temps réel)

Aucune configuration CORS n'est donc nécessaire en développement. Pour cibler un autre
serveur : `OZONE_SERVER=http://mon-instance:8080 npm run dev`.

## Scripts

- `npm run dev` — serveur de dev (HMR)
- `npm run build` — typecheck + build de production (`dist/`)
- `npm run typecheck` — vérification TypeScript seule
- `npm test` — tests unitaires (vitest)

## Stack

- **Vite + React 18 + TypeScript**
- **Tailwind CSS** v3 — tokens « façon Discord » en **variables CSS à 2 couches**
  (primitives + alias sémantiques) ⇒ thème = échange de tokens (sombre / clair / midnight)
- **Zustand** — état global + application des événements Gateway
- **Radix UI** — overlays accessibles (tooltips, popovers, menus contextuels)
- **lucide-react** — icônes (aucun emoji dans l'UI)
- Rendu Markdown « façon Discord » **maison** (sortie React, zéro HTML brut)

## Structure

- `src/types.ts` — DTOs miroir de `ozone-proto` (Snowflake = `string`)
- `src/api.ts` — client REST typé (base `/api`, auth Bearer, upload multipart)
- `src/gateway.ts` — client WebSocket (IDENTIFY/HELLO/READY + RESUME, heartbeat, reconnexion)
- `src/store.ts` — état global Zustand + événements temps réel + helpers (non-lus, rôles)
- `src/lib/markdown.tsx` — renderer Markdown (gras/italique/souligné/code/spoilers/mentions/timestamps)
- `src/components/` — UI (rail, sidebar, chat, membres, amis, réglages, profil, overlays)
- `src/**/*.test.ts` — tests vitest (parseur Markdown, helpers de non-lus/rôles)

## Fonctionnalités

- **Auth** : connexion / inscription, gate d'instance, code d'invitation.
- **Guildes** : créer / rejoindre / découvrir (annuaire public) ; paramètres (renommer,
  description, découvrable, supprimer) ; inviter (code copiable) ; quitter.
- **Salons** : arbre par catégorie, créer / renommer / supprimer (clic droit), texte & vocal.
- **Messages** : Markdown complet, **modes Cosy / Compact**, groupement, avatars, couleurs de
  rôle, **pièces jointes** (upload + images inline + lightbox), **réponses**, **édition /
  suppression**, **réactions** (sélecteur d'emoji), épingles, emoji jumbo, menu contextuel.
- **Non-lus** : salons / guildes en gras + point, badges de mention, barre « nouveaux messages ».
- **Temps réel** : messages, présences, salons, réactions, saisie, relations (Gateway + RESUME).
- **MP** : liste, démarrer une conversation, fil de discussion.
- **Amis** : tous / en ligne / en attente / bloqués / ajouter.
- **Membres** : liste groupée par rôle (hoist) + présences ; **fiches de profil** (popout).
- **Recherche** de messages, **épingles** (panneau).
- **Réglages** : thème (sombre / clair / midnight), Cosy / Compact, profil, déconnexion.

## Empaquetage Tauri 2

Prévu dans un second temps (`src-tauri/`) : webview native de l'OS, sans Chromium
embarqué, bien plus léger qu'Electron. Durcissement associé : stockage du jeton dans le
coffre sécurisé de l'OS (cf. R10 dans `docs/SECURITY-REVIEW-S1-S2.md`) + CSP stricte.
