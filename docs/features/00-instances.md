# Fonctionnalités — Instances (self-hosting & connexion)

> **C'est le point d'entrée du client.** Avant tout compte, tout ami, toute guilde, l'utilisateur choisit **à quelle instance Ozone se connecter**.

Réf. : [01-architecture](../01-architecture.md) · [04-api-rest](../04-api-rest.md) · [07-securite](../07-securite-chiffrement.md) · [01-comptes-authentification](01-comptes-authentification.md).

---

## 1. Concept & terminologie (à ne pas confondre)

| Terme | Définition | Analogie |
|---|---|---|
| **Instance** | Un **déploiement serveur Ozone complet**, auto-hébergé et **isolé**. Possède **ses propres** comptes utilisateurs, amis, MP, guildes, emojis, fichiers. | Un *homeserver* Matrix / une instance Mastodon / un serveur Revolt auto-hébergé |
| **Guilde** (UI : « serveur ») | Une **communauté** créée **à l'intérieur** d'une instance, qu'on rejoint via **invitation**. C'est le « serveur » au sens Discord pour l'utilisateur final. | Un *serveur Discord* |
| **Compte** | Une identité **propre à une instance**. Le même e-mail peut avoir des comptes distincts sur deux instances différentes. | Un compte par homeserver |

⚠️ **« Serveur » est ambigu** : dans la doc technique, on dit **instance** (le backend) et **guilde** (la communauté). Dans l'UI utilisateur, « serveur » désigne la **guilde** ; l'instance s'appelle « instance Ozone » (ou par son nom de marque).

### Isolation (pas de fédération, par défaut)
- Chaque instance est un **silo indépendant** : aucune communication inter-instances. Les comptes/amis/guildes d'une instance n'existent pas sur une autre.
- C'est volontaire (simplicité, contrôle, vie privée du self-host) et conforme à la demande.
- **Extension future possible** (hors périmètre) : une fédération optionnelle type Matrix pourrait être ajoutée sans casser ce modèle (un champ d'origine sur les entités, des clés d'instance). Non implémentée pour l'instant.

---

## 2. Écran de connexion à une instance (premier écran)

- [ ] **Champ « Adresse de l'instance »** : URL ou hôte (`ozone.mondomaine.fr`, `192.168.1.10:8443`, `https://chat.exemple.org`). Schéma `https` par défaut, port custom accepté.
- [ ] **Sondage de l'instance** : à la saisie, le client appelle `GET /instance` et affiche le **branding** (nom, logo, description, couleur), la **politique d'inscription** et si un **mot de passe d'instance** est requis.
- [ ] **Mot de passe d'instance (facultatif)** : si l'instance est protégée, un champ mot de passe apparaît (gate d'accès partagé, voir §4).
- [ ] Boutons **« Se connecter »** (compte existant) et **« Créer un compte »** (si l'inscription est autorisée).
- [ ] **Instances enregistrées** (favoris) : liste des instances déjà utilisées, reconnexion en un clic (session mémorisée), épingler/renommer/supprimer.
- [ ] **Indicateurs d'état** : instance joignable/injoignable, version, latence, certificat TLS (valide / auto-signé → avertissement).
- [ ] Gestion des erreurs claires : hôte introuvable, version incompatible, instance pleine, inscription fermée, mauvais mot de passe d'instance.
- [ ] **Découverte facultative** : possibilité de coller un lien d'invitation d'instance (`ozone://instance/...` ou URL) qui pré-remplit l'adresse (+ éventuellement un code d'invitation d'instance).

### Séquence de connexion
```
1. Utilisateur saisit l'adresse ──► Client: GET https://<hote>/.well-known/ozone  (ou /api/v1/instance)
2. Instance ──► { name, icon, description, version, registration: open|invite|closed,
                  access_gate: { required: bool }, features, limits }
3. Si access_gate.required ──► champ mot de passe ──► POST /instance/gate { password }
                            ──► { gate_token (court) }       (sinon: pas de gate_token)
4. Connexion: POST /auth/login   (+ header X-Instance-Gate: <gate_token> si requis)
   ou Inscription: POST /auth/register (+ gate_token, + code d'invitation d'instance si invite-only)
5. ──► { access_jwt, refresh_token } stockés dans le keystore OS, liés à CETTE instance.
6. Le client ouvre la WSS Gateway de l'instance et charge READY (guildes, amis, MP…).
```

---

## 3. Multi-instances côté client (modèle recommandé)

Le client **mémorise plusieurs instances** et permet d'**en changer** sans se déconnecter (chaque instance = une identité/session séparée). Le mode mono-instance n'est qu'un sous-cas.

- [ ] **Registre d'instances** local (chiffré) : adresse, branding caché, **session/jeton par instance** (keystore OS).
- [ ] **Switcher d'instance** : sélecteur compact (rail d'instances ou menu) en tête de l'UI ; sélectionner une instance révèle **son** rail de guildes, **ses** MP, **ses** amis.
- [ ] **Notifications agrégées** multi-instances (badge global + par instance), respect des mute par instance.
- [ ] **Identités séparées** : pseudo/avatar/amis distincts par instance ; aucune fuite d'une instance à l'autre.
- [ ] **Ajouter une instance** depuis l'app (rouvre l'écran de connexion), **se déconnecter** d'une instance, **oublier** une instance (purge locale).
- [ ] Statut de connexion par instance (en ligne / reconnexion / hors-ligne), reprise indépendante.

```
┌───┬─────────────────────────────────────────────┐
│ I │  [rail de guildes de l'instance active]      │   I = rail d'INSTANCES
│ N │  # général   # annonces   🔊 vocal …          │       (chaque pastille = 1 instance,
│ S │                                              │        avec sa propre identité)
│ T │  ───────────────── messages ───────────────  │
└───┴─────────────────────────────────────────────┘
```

> **Décision UX à confirmer** : multi-instances simultanées (recommandé, plus puissant) **vs** une seule instance à la fois (plus simple). Le plan retient le multi-instances ; c'est un superset.

---

## 4. Mot de passe d'instance (gate d'accès facultatif)

Barrière **partagée** optionnelle pour limiter qui peut **atteindre** l'instance (login **et** inscription), distincte des mots de passe de comptes individuels.

- [ ] Activable/désactivable par l'admin d'instance ; **facultatif** (instance ouverte si non défini).
- [ ] Stocké **haché (Argon2id)** côté serveur ; jamais en clair.
- [ ] Vérifié via `POST /instance/gate` → délivre un **gate token court** exigé par `/auth/login` et `/auth/register` quand le gate est actif.
- [ ] Anti-bruteforce sur le gate (rate-limit, délai), captcha optionnel.
- [ ] Rotation : changer le mot de passe d'instance n'expulse pas les sessions déjà établies (le gate ne protège que l'**entrée**).
- [ ] Cas d'usage : instance privée entre amis (un seul secret à partager), bêta fermée, instance familiale.

> À distinguer de : (a) le **mot de passe de compte** (par utilisateur, auth réelle) ; (b) les **invitations de guilde** (rejoindre une communauté) ; (c) les **invitations d'instance** (autoriser la création d'un compte sur une instance invite-only).

---

## 5. Politique d'inscription (par instance)

- [ ] **Ouverte** : quiconque passe le gate (le cas échéant) peut créer un compte.
- [ ] **Sur invitation** : nécessite un **code d'invitation d'instance** (généré par un admin, à durée/usages limités) — différent des invitations de guilde.
- [ ] **Fermée** : aucune inscription publique ; les comptes sont créés par l'admin.
- [ ] Options : **vérification e-mail** requise on/off, **captcha**, validation manuelle (file d'approbation), domaines e-mail autorisés.

---

## 6. Administration de l'instance (tableau de bord du self-hoster)

Réglages **au niveau de l'instance entière** (distincts des réglages de guilde). Accessibles aux **rôles d'instance** (voir §7).

### Identité & branding
- [ ] Nom de l'instance, **logo/icône**, description, couleur d'accent, image de fond de l'écran de connexion, message d'accueil.
- [ ] Domaine, mentions légales (CGU / confidentialité), langue par défaut, fuseau.

### Accès & comptes
- [ ] **Mot de passe d'instance** (activer/désactiver/changer) — §4.
- [ ] **Politique d'inscription** (ouverte/invitation/fermée) + **invitations d'instance** (créer, lister, révoquer) — §5.
- [ ] Vérification e-mail, captcha, validation manuelle.
- [ ] Liste des comptes : rechercher, **suspendre / bannir au niveau instance**, réinitialiser, supprimer, promouvoir admin/modérateur d'instance.

### Limites & quotas
- [ ] Nombre **max d'utilisateurs**, max de **guildes par utilisateur**, **taille max d'upload**, quotas de **stockage** par user/instance, limites de débit.
- [ ] Qualité vocale/vidéo max (bitrate, résolution de partage) — débloque les « perks » sans paiement (cf. [02-serveurs](02-serveurs.md#boosts-mécanique-de-paliers-sans-facturation-réelle)).

### Modération & sûreté (niveau instance)
- [ ] **AutoMod global** (en plus de celui par guilde), listes de blocage d'instance, filtres d'upload.
- [ ] **Bannissement d'instance** (l'utilisateur perd l'accès à toute l'instance, pas seulement à une guilde).
- [ ] Journal d'audit **au niveau instance** (créations de guildes, comptes, actions admin).
- [ ] File de **signalements** centralisée.

### Infrastructure
- [ ] Configuration **stockage objets** (S3/MinIO), rétention des médias, **sauvegardes**.
- [ ] Réglages **vocal/SFU** (régions, nœuds), **e-mail** (SMTP), **push** (clés APNs/FCM).
- [ ] Bascule du backend messages (Postgres ↔ ScyllaDB), observabilité, mises à jour de l'instance.
- [ ] Réglages **par défaut** des nouveaux comptes (thème, notifications).

---

## 7. Rôles au niveau de l'instance (distincts des rôles de guilde)

| Rôle d'instance | Pouvoir |
|---|---|
| **Propriétaire** | le self-hoster ; premier compte créé au bootstrap ; contrôle total, transférable |
| **Admin d'instance** | gère l'instance (branding, accès, limites, comptes, modération globale) |
| **Modérateur d'instance** | modération transversale (signalements, bans d'instance), sans config |
| **Utilisateur** | compte normal ; crée/rejoint des guildes, a ses amis/MP |

> Ces rôles concernent **l'instance**. À l'intérieur, chaque **guilde** a ses propres rôles & permissions (voir [10-roles-permissions](10-roles-permissions.md)). Un simple utilisateur peut être **propriétaire d'une guilde** sans être admin de l'instance.

---

## 8. Bootstrap (première installation d'une instance)

- [ ] **Assistant de configuration** au premier lancement du serveur (ou via variables d'env / CLI) : nom de l'instance, branding, **création du compte propriétaire**, politique d'accès & d'inscription, stockage, e-mail.
- [ ] Génération des secrets (clés JWT, clés d'instance), migrations de base, vérification de santé.
- [ ] `docker-compose up` « tout-en-un » pour démarrer une instance complète (API, Gateway, SFU, Postgres, Redis, NATS, MinIO) — voir [09-roadmap](../09-roadmap.md).
- [ ] Mode **mono-binaire** optionnel (tout-en-un) pour petites instances familiales : un seul exécutable embarquant les services, SQLite + stockage disque local, zéro dépendance externe.

---

## 9. Sécurité (rappel, détail dans [07](../07-securite-chiffrement.md))

- [ ] **TLS** obligatoire en pratique : support **Let's Encrypt** automatique (ACME) pour le self-host ; certificats auto-signés acceptés avec **avertissement explicite + pinning** au premier usage (TOFU).
- [ ] **Vérification d'identité d'instance** : empreinte de clé affichée, épinglée localement, alerte si elle change (anti-MITM).
- [ ] Gate d'accès, rate-limit, isolation des données par instance.
- [ ] Jetons stockés **par instance** dans le keystore OS ; déconnexion d'une instance n'affecte pas les autres.

---

## 10. Definition of Done
- Un self-hoster déploie une instance via `docker-compose`, l'assistant crée le propriétaire et active un **mot de passe d'instance** + inscription **sur invitation**.
- Un ami ouvre le client, saisit l'adresse de l'instance, voit son branding, entre le mot de passe d'instance, crée un compte avec un **code d'invitation d'instance**, puis crée et rejoint des **guildes**.
- Le même utilisateur **ajoute une seconde instance** (publique, ouverte) dans le même client et **bascule** entre les deux : identités, amis et guildes restent totalement séparés, les notifications sont agrégées.
