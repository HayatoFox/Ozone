# 11 — Client natif (décision d'architecture)

Objectif : un client **natif** (pas Electron), **performant**, **modulaire**, et capable d'**effets
de décoration / thèmes** soignés à terme. Cf. [00-vision](00-vision-et-perimetre.md), [02-stack](02-stack-technique.md).

## Décision : `Iced` pour l'UI, logique dans `ozone-core`

| Critère | Pourquoi Iced |
|---|---|
| **Performant** | Rendu **GPU (wgpu)**, modèle retenu efficace. |
| **Effets / thèmes** | Thèmes intégrés + styles par widget ; **widget `shader` wgpu** pour effets custom (dégradés, lueurs, animations) à terme. |
| **Modulaire** | Architecture **Elm** (`Message`/`update`/`view`) → état décomposable, composants testables. |
| **Cohérence** | **100% Rust** → `ozone-ui` et `ozone-core` réutilisent **directement `ozone-proto`** (DTOs partagés client/serveur). Pas de DSL ni de second langage. |

Écartés : **egui** (plus faible sur le thème/effets soignés), **Slint** (DSL séparé → glue de types,
pas de réutilisation directe de `ozone-proto`), **Tauri/Dioxus-webview** (rendu web, pas « natif fluide »).

## Couches

```
ozone-proto   types partagés (DTOs, perms, snowflakes, JWT)  ← déjà fait
ozone-core    logique client multiplateforme :
              - InstanceRef + InstanceRegistry (multi-instances) ← fait (S33 ; dédup, persistance SANS jetons, testé)
              - Porte d'accès d'instance (gate)                 ← fait (S33 ; ApiClient.gate + gate_token, testé E2E)
              - ApiClient (REST typé, reqwest+rustls)          ← fait (S26, testé E2E vs serveur)
              - GatewayClient (WS temps réel + RESUME)         ← fait (S27/S31 ; reprise sans perte après coupure, testé E2E)
              - Store normalisé (guildes/salons/messages/présence)  ← fait (S28 ; applique les events Gateway, testé)
              - Cache local (SQLite + rétention bornée)         ← fait (S29 ; persiste/réhydrate, plafonds mémoire+disque, testé)
              - Session (orchestrateur : auth+bootstrap+temps réel+cache+reconnexion auto)  ← fait (S30/S31 ; API haut niveau pour l'UI, testé E2E)
              - Moteur vocal (signalisation + WebRTC client)   ← à faire (pair du SFU)
ozone-ui      application Iced (vues, thèmes, navigation)      ← fondation faite (S32 ; connexion + guildes/salons/messages, réducteur testé)
```

> **Crypto côté client** : `reqwest`/`rustls` (et la pile WebRTC) tirent `ring` — **acceptable côté
> client**. La contrainte « zéro `ring` » ne concerne que le **serveur** (`ozone-api`, build AlmaLinux).

## Contrat d'API

L'API est servie **à la racine** (`/auth/...`, `/guilds/...`, `/channels/...` — cf.
[04-api-rest](04-api-rest.md)). `InstanceRef::api_base()` renvoie la racine (`https://<hôte>`) ; un
reverse-proxy peut exposer l'API sous un préfixe (l'inclure alors dans l'adresse de l'instance).

## Prochaines étapes

1. `ozone-ui` — étoffer la fondation (S32) : **temps réel** dans l'UI (subscription Gateway →
   `Store`/`Session`), switcher **multi-instances** + porte d'accès, MP, présence, **thèmes/effets**
   soignés, gestion d'erreurs/chargement. *(Validation visuelle = exécuter le binaire.)*
2. Moteur vocal client (signalisation via l'API → SFU `ozone-sfu`).
   *(Le plan de contrôle — jeton vocal + jointure SFU — est testable ; le média WebRTC nécessite de vrais pairs.)*

> Reconnexion automatique : **fait (S31)** via `Session::poll_event_resilient()` (RESUME → repli
> connexion complète, back-off borné, `refresh_session` au besoin). Testé E2E (coupure → auto-reprise).
