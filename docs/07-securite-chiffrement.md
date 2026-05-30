# 07 — Sécurité & chiffrement

Sécurité « par défaut », sans sacrifier l'ergonomie. Périmètre : authentification, autorisation, chiffrement (transport + E2EE média), stockage des secrets, abus & conformité.

## 1. Authentification

| Élément | Choix |
|---|---|
| Mots de passe | hachés **Argon2id** (paramètres mémoire/temps calibrés), jamais stockés en clair |
| Tokens | **access JWT** court (~10 min, signé EdDSA) + **refresh token** rotatif, opaque, révocable, lié à une session/appareil |
| Sessions | table `user_sessions` (appareil, IP, user-agent, dernière activité) → déconnexion à distance |
| **2FA** | **TOTP** (RFC 6238) + **codes de secours** ; option SMS ; **WebAuthn/Passkeys** (clés matérielles) en cible |
| Rotation | changement de mot de passe / activation 2FA → invalide tous les tokens et sessions |
| Anti-bruteforce | rate-limit login, captcha adaptatif, verrouillage temporaire, alertes nouvel appareil |
| Vérification | email obligatoire ; téléphone optionnel (anti-spam, certains serveurs l'exigent) |

## 1.5. Accès & confiance d'instance (self-host)

- **Gate d'instance (facultatif)** : mot de passe **partagé** optionnel protégeant l'**entrée** de l'instance (login + inscription), distinct des mots de passe de comptes. Haché **Argon2id**, vérifié via `/instance/gate` → **gate token court**. Rate-limit + captcha anti-bruteforce. Ne protège que l'accès initial (ne ré-expulse pas les sessions établies). Détail : [features/00-instances](features/00-instances.md#4-mot-de-passe-dinstance-gate-daccès-facultatif).
- **Politique d'inscription** : `open` / `invite` (code d'invitation d'instance) / `closed` — limite qui peut créer un compte.
- **Confiance d'instance (TOFU/pinning)** : chaque instance a une **clé publique d'identité**. Le client l'**épingle au premier usage** et **alerte si elle change** (anti-MITM), comme pour les apps E2EE.
- **TLS pour le self-host** : **ACME / Let's Encrypt** automatique recommandé ; les certificats **auto-signés** sont acceptés avec **avertissement explicite + pinning** (TOFU) pour les instances de labo/LAN.
- **Isolation** : jetons et caches **séparés par instance** (keystore OS + SQLite partitionné) ; aucune donnée ne transite d'une instance à l'autre.
- **Rôles & bannissement d'instance** : propriétaire/admin/modérateur d'instance ; un ban d'instance retire l'accès à **toute** l'instance (≠ ban de guilde).

## 2. Autorisation

- Modèle **rôles + permissions bitfield** (voir [10 — Rôles & permissions](features/10-roles-permissions.md)).
- **Le serveur fait toujours autorité.** Le client pré-désactive l'UI pour l'ergonomie mais chaque mutation est revérifiée côté serveur (permissions effectives recalculées).
- Court-circuits : propriétaire de guilde et `ADMINISTRATOR` bypassent les overrides ; hiérarchie des rôles empêche d'agir sur un rôle ≥ au sien.
- Scopes **OAuth2** pour les apps tierces (principe du moindre privilège) ; intents privilégiés (presence, membres, contenu) gated.

## 3. Chiffrement en transit

- **TLS 1.3** partout (API, Gateway, signaling vocal), HSTS, certificats auto-renouvelés.
- **Pinning** optionnel côté client natif pour les domaines critiques.
- Média : **SRTP** (AES-GCM) sur le saut client↔SFU.

## 4. Chiffrement de bout en bout

| Domaine | E2EE | Mécanisme |
|---|---|---|
| **Vocal/vidéo/partage d'écran** | ✅ par défaut | **DAVE** + **MLS** (voir [06](06-infrastructure-vocale.md)) — le SFU relaie sans déchiffrer |
| Messages texte (MP) | 🟡 option future | possible via MLS par salon ; compromis : casse la recherche serveur, l'historique multi-appareils, les bots. Discord ne chiffre pas les messages E2EE → on garde le chiffrement **au repos** + transport par défaut, E2EE MP en option avancée |
| Médias (pièces jointes) | au repos | chiffrés côté stockage ; URLs présignées à durée de vie courte |

> Choix assumé : on **n'impose pas** l'E2EE sur le texte (comme Discord) pour préserver recherche, modération, historique cross-device et bots. On l'offre en **option** pour les MP sensibles.

## 5. Stockage des secrets

- **Client** : tokens et clés MLS dans le **keystore OS** — Keychain (macOS/iOS), DPAPI/Credential Manager (Windows), Secret Service/KWallet (Linux), Keystore (Android). Jamais en clair sur disque.
- **Serveur** : secrets (clés JWT, DB, S3) via un gestionnaire (Vault / variables chiffrées) ; rotation régulière.
- **Au repos** : TOTP secrets, refresh tokens hachés, données sensibles chiffrées en base.

## 6. Abus, spam & sûreté

- **AutoMod** (voir [11 — Modération](features/11-moderation-securite.md)) : filtres mots-clés/regex, anti-spam, anti-mention-spam, liens malveillants, profils suspects.
- **Rate-limiting** multi-niveaux (API buckets, opcodes gateway, envoi de messages, créations d'invitations).
- **Raid protection** : détection de vagues de joins, throttling, verrouillage temporaire, vérification renforcée.
- **Scan de fichiers** : détection de contenus interdits (hash matching), antivirus sur uploads.
- **Signalements** : utilisateurs, messages, serveurs → file de modération.
- **DM safety** : filtrage optionnel des MP (médias explicites, liens) — réglable par l'utilisateur.

## 7. Vie privée & conformité

- **RGPD** : export de données (`GET /users/@me/data-export`), droit à l'effacement (suppression de compte avec délai de grâce, anonymisation des messages selon politique).
- **Minimisation** : intents limitent les données poussées ; presence masquable (invisible) ; statut de lecture non exposé aux autres.
- **Transparence** : journal des sessions/appareils, applications autorisées révocables, mode invisible.
- **Mineurs** : niveaux de vérification, filtres de contenu explicite, salons NSFW gated par âge.
- **Mode streamer** : masque automatiquement infos perso (voir [20 — Overlay & mode streamer](features/20-overlay-streamer.md)).

## 8. Sécurité applicative (dev)

- `cargo-deny` (vulnérabilités/licences), `cargo audit`, dépendances épinglées.
- Validation/sanitisation systématique des entrées (markdown, noms, uploads) — anti-XSS dans les rendus, anti-injection (requêtes paramétrées sqlx).
- Webhooks/interactions signés (clé publique Ed25519, vérification de signature des requêtes entrantes).
- Isolation des workers (exécution AutoMod/transcodage en sandbox).
- Tests de sécurité : fuzzing du parseur markdown/protocole gateway, revue des chemins de permission.

## 9. Modèle de menace (résumé)

| Menace | Contre-mesure |
|---|---|
| Vol de token | tokens courts + refresh rotatif révocable + keystore OS |
| MITM réseau | TLS 1.3 + pinning + SRTP + DAVE E2EE |
| Serveur compromis (média) | E2EE DAVE : le serveur ne voit pas le clair audio/vidéo |
| Escalade de privilèges | autorité serveur + hiérarchie de rôles + audit log |
| Spam/raid | rate-limit + AutoMod + raid protection + vérification |
| Bruteforce compte | Argon2id + rate-limit + 2FA + captcha + alertes |
| Exfiltration de données | minimisation, chiffrement au repos, scopes OAuth2 |
| Usurpation / MITM d'**instance** | TLS + **pinning d'identité d'instance** (TOFU) + vérification d'empreinte |
| Accès non autorisé à une instance privée | **gate** mot de passe d'instance + politique d'inscription + rate-limit |
| Fuite d'identité entre instances | sessions/caches **partitionnés par instance** (keystore + SQLite) |

Suite : la documentation **fonctionnelle** détaillée → [features/](features/).
