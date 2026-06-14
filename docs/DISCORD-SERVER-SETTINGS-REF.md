# Référence — Menu serveur & Paramètres du serveur (Discord, vue admin CS2 Cyka)

Inspection **en lecture seule** du vrai Discord (compte admin) pour reproduire l'UI dans Ozone.
Aucune donnée perso reproduite : on copie la **structure/layout**, pas le contenu du serveur.

## Menu déroulant du serveur (clic sur le nom)
Groupes séparés par des filets. **Chaque entrée est à gater selon les permissions.**

| Entrée | Permission requise (Ozone) |
|---|---|
| Boost de serveur | aucune (toute personne) — *Ozone : pas de paiement → omis ou désactivé* |
| Inviter sur le serveur | CREATE_INSTANT_INVITE |
| Paramètres du serveur | MANAGE_GUILD (ou toute perm de gestion) |
| Créer un salon | MANAGE_CHANNELS |
| Créer une catégorie | MANAGE_CHANNELS |
| Créer un événement | CREATE_EVENTS |
| Répertoire d'applications | — *(omis Ozone)* |
| Paramètres de notifications | aucune |
| Paramètres de confidentialité | aucune |
| Modifier le profil par serveur | aucune |
| Masquer les salons muets (toggle) | aucune |
| Copier l'identifiant du serveur | aucune (mode dev) |
| **Quitter le serveur** (rouge) | affiché si **non-propriétaire** |

## Paramètres du serveur — navigation (18 pages, groupées)
Plein écran, nav à gauche groupée + colonne de contenu centrée + bouton ESC/✕ haut-droite.

- **CS2 CYKA**
  - **Profil du serveur** (Aperçu) — Nom (input), Icône (changer/supprimer, ≥512²), Bannière (grille de dégradés 2×5 + upload), Particularités (jusqu'à 5 tags emoji). *Page haute → scroll.* → wire `update_guild`
  - Tag du serveur — tag de clan court. *(placeholder)*
  - Participation — ~6 toggles (écran d'accueil, etc.). *(placeholder/partiel)*
  - Avantages de boost — Nitro. *(placeholder, pas de paiement)*
- **EXPRESSION**
  - **Émoji** — grille + upload. → wire (EmojiModal)
  - Autocollants — *(placeholder)*
  - Soundboard — *(placeholder)*
- **PERSONNES**
  - **Membres** — recherche + table (avatar, nom, membre depuis, rôles +, menu ⋮ expulser/bannir/rôles). *Haute → scroll.* → wire (membres + kick/ban/roles)
  - **Rôles** — liste + « Création de rôle » + éditeur de permissions par rôle. → wire (RolesModal)
  - **Invitations** — table des invitations actives (auteur, code, usages, expiration) + révoquer. → wire (listInvites/revoke)
  - Accès — « Comment rejoindre ? », Règles, 2 toggles (onboarding). *(placeholder/partiel)*
- **APPLICATIONS**
  - **Intégrations** — Webhooks (créer), Salons suivis, Bots & applications (vide). → wire (WebhooksModal)
  - Répertoire d'applications — lien externe. *(omis)*
- **MODÉRATION**
  - Configuration de Sécurité — niveau de vérification (select), alertes d'activité, ~3 toggles. *(partiel)*
  - **Logs du serveur** (journal d'audit) — entrées. → wire (AuditLogModal)
  - **Bannissements** — liste + recherche par id/nom. → wire (BansModal)
  - AutoMod — règles. *(placeholder)*
  - Activer la communauté — onboarding marketing. *(placeholder)*
  - Modèle de serveur — template. *(placeholder)*

## Plan Ozone
- Composant unifié `ServerSettings.tsx` (plein écran, nav groupée gatée par permission, pages).
- Réutiliser la logique des modals existantes (Rôles/Bans/Audit/Émoji/Webhooks) en **pages**.
- Pages nouvelles : Aperçu (nom/icône/desc), Membres, Invitations + placeholders fidèles.
- Le menu serveur pointe « Paramètres du serveur » vers ce composant ; Rôles/Émoji/Audit/Bans
  quittent le menu (deviennent des pages), conformément à Discord.
- Helpers : `permsIn(state, guildId)` / `canIn(state, guildId, PERM.x)` (store) pour le gating.
