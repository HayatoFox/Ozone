# Fonctionnalités — Comptes & authentification

Réf. technique : [03-modele-de-donnees](../03-modele-de-donnees.md) · [04-api-rest](../04-api-rest.md) · [07-securite](../07-securite-chiffrement.md).

## Création & connexion
> ⚠️ **Tout compte appartient à une instance.** L'inscription/connexion se fait *après* avoir choisi une instance et franchi son éventuel **gate** (mot de passe d'instance facultatif). Le tout premier écran du client est la connexion à une instance → voir **[00-instances](00-instances.md)**.
- [ ] **Choisir l'instance** d'abord : adresse, sondage du branding, mot de passe d'instance si requis, politique d'inscription.
- [ ] Inscription par **email + pseudo + mot de passe** (captcha anti-bot).
- [ ] **Pseudo unique** (modèle moderne sans discriminateur `#1234`) + **nom affiché** libre.
- [ ] Migration/compat éventuelle des anciens discriminateurs.
- [ ] Connexion par email **ou** pseudo + mot de passe.
- [ ] Vérification d'email (lien/code), rappel si non vérifié, certaines actions bloquées tant que non vérifié.
- [ ] Mot de passe oublié → réinitialisation par email (lien à durée limitée).
- [ ] Validation force du mot de passe (zxcvbn), interdiction des mots de passe compromis (HIBP-like).

## 2FA & sécurité du compte
- [ ] **2FA TOTP** (QR code, apps authenticator) + **codes de secours** téléchargeables.
- [ ] 2FA par **SMS** (option, déconseillée mais présente).
- [ ] **WebAuthn / Passkeys** (clé matérielle, biométrie) — cible.
- [ ] Exigence 2FA pour les actions sensibles (suppression de serveur, modération) si « 2FA requise » activée par un serveur.
- [ ] Alertes nouvel appareil / nouvelle IP (email + in-app).
- [ ] Verrouillage/anti-bruteforce, captcha adaptatif.

## Gestion du compte (Paramètres → Mon compte)
- [ ] Modifier **pseudo**, **nom affiché**, **email**, **téléphone**, **mot de passe** (re-auth requise).
- [ ] **Avatar** (image/GIF), **bannière**, **couleur d'accent**, **bio**, **pronoms**.
- [ ] **Badges** du compte (affichage).
- [ ] Voir/gérer les **appareils & sessions** actifs → déconnexion à distance (par session ou « toutes les autres »).
- [ ] **Applications autorisées** (OAuth2) → révocation.
- [ ] **Connexions** de comptes externes (optionnel/désactivable — hors périmètre marketing).
- [ ] **Désactiver** (réversible) vs **Supprimer** (définitif, délai de grâce) le compte.
- [ ] **Export de mes données** (RGPD) : génération d'archive téléchargeable.

## Sessions & tokens (technique)
- [ ] Access JWT court + refresh token rotatif (voir [07](../07-securite-chiffrement.md)).
- [ ] Révocation immédiate à la déconnexion / changement de mot de passe / activation 2FA.
- [ ] Persistance « rester connecté » via refresh sécurisé dans le keystore OS.
- [ ] Reconnexion gateway transparente après reprise de session.

## États du compte
- [ ] Comptes **bot** et **système** (flags), comptes utilisateurs normaux.
- [ ] Suspension / restriction (modération plateforme) avec messages explicites.
- [ ] Statut **mineur** / vérification d'âge pour le contenu NSFW.

## Definition of Done
- Un utilisateur s'inscrit, vérifie son email, active la 2FA, se connecte sur 2 appareils, en déconnecte un à distance, change son mot de passe (ce qui invalide les autres sessions), et peut exporter puis supprimer son compte.
