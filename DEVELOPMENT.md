# Développement — Ozone

Guide pour compiler, lancer et tester le code de la **Phase 1** (socle runnable). Pour la conception complète, voir [README.md](README.md) et [docs/](docs/).

## Prérequis
- **Rust** stable (1.91+) via `rustup` (le fichier [rust-toolchain.toml](rust-toolchain.toml) fixe le canal).
- Un **compilateur C** (pour le SQLite embarqué) : Windows = MSVC Build Tools ; **AlmaLinux/RHEL** = `sudo dnf install gcc` ; Debian/Ubuntu = `sudo apt install build-essential`.
- (Optionnel) **Docker/Podman** pour lancer une instance conteneurisée.

> **Déploiement serveur (Linux / AlmaLinux)** : voir [docs/10-deploiement.md](docs/10-deploiement.md) (systemd, Docker AlmaLinux, binaire musl statique, TLS). La CI compile et teste sur `almalinux:9`.

## Organisation du dépôt

```
Ozone/
├── Cargo.toml                 # workspace
├── crates/
│   ├── ozone-proto/           # types partagés client/serveur (snowflakes, DTOs, gateway)
│   ├── ozone-api/             # SERVEUR : API REST + Gateway WS (tout-en-un SQLite)
│   │   ├── migrations/        # schéma SQLite (sqlx)
│   │   ├── src/               # config, crypto, db (bootstrap), routes_*, gateway, lib, main
│   │   └── tests/flow.rs      # test d'intégration de bout en bout
│   └── ozone-sfu/             # SERVEUR : SFU vocal/vidéo (WebRTC)
├── desktop/                   # CLIENT : React + TypeScript, empaqueté en .exe via Tauri
├── docs/                      # plan de conception complet
├── Dockerfile · docker-compose.yml · .env.example
```

## Compiler & tester

```sh
cargo check --workspace      # vérification rapide
cargo test  --workspace      # build + tests (3 tests d'intégration + unitaires)
cargo clippy --workspace     # lints
```

État actuel : **le workspace compile et les tests passent** (parcours instance → inscription → guilde → salon → message ; rejet sans token ; flux du *gate* d'instance).

## Lancer une instance (mode tout-en-un, SQLite, zéro dépendance)

```sh
# instance ouverte par défaut sur 127.0.0.1:8080, base ./ozone.db
cargo run -p ozone-api

# instance protégée par un mot de passe d'instance + inscription sur invitation
OZONE_INSTANCE_NAME="Chez moi" OZONE_INSTANCE_PASSWORD="secret123" OZONE_REGISTRATION="invite" \
  cargo run -p ozone-api
```
> Windows PowerShell : `$env:OZONE_INSTANCE_PASSWORD="secret123"; cargo run -p ozone-api`

Le **premier compte créé devient propriétaire** de l'instance. Variables d'environnement : voir [.env.example](.env.example).

## Endpoints disponibles (Phase 1)

| Méthode | Route | Auth | Rôle |
|---|---|---|---|
| GET | `/instance` | — | métadonnées publiques (branding, politique, gate requis) |
| GET | `/instance/health` | — | sonde |
| POST | `/instance/gate` | — | mot de passe d'instance → `gate_token` |
| POST | `/auth/register` | gate? | créer un compte (1er = propriétaire) |
| POST | `/auth/login` | gate? | connexion → `access_token` + `refresh_token` |
| POST | `/auth/token/refresh` | — | rotation du refresh token |
| GET | `/users/@me` | Bearer | profil courant |
| POST/GET | `/guilds` | Bearer | créer / lister ses guildes |
| POST/GET | `/guilds/{id}/channels` | Bearer | créer / lister les salons |
| GET/POST | `/channels/{id}/messages` | Bearer | lister / envoyer des messages |
| GET (WS) | `/gateway` | via IDENTIFY | flux temps réel (HELLO/IDENTIFY/HEARTBEAT/READY + `MESSAGE_CREATE`) |

### Exemple (curl)
```sh
curl localhost:8080/instance
curl -X POST localhost:8080/auth/register -H 'content-type: application/json' \
  -d '{"username":"alice","email":"a@b.fr","password":"motdepasse"}'
# → {"access_token":"...","refresh_token":"...",...}
curl localhost:8080/users/@me -H "authorization: Bearer <access_token>"
```

### Gateway temps réel
Se connecter en WebSocket sur `/gateway`, recevoir `HELLO`, envoyer
`{"op":2,"d":{"token":"<access_token>"}}` (IDENTIFY) → réception de `READY`,
puis les `MESSAGE_CREATE` diffusés quand un message est posté via l'API.

## Docker

```sh
docker compose up --build          # instance tout-en-un (SQLite) sur :8080
docker compose --profile full up   # + Postgres/Redis/NATS/MinIO (futur full-stack)
```

## Prochaines étapes (cf. [docs/09-roadmap.md](docs/09-roadmap.md))
- Client de référence : `desktop/` (React + TypeScript, empaqueté Tauri). Le client natif iced
  d'origine (`ozone-ui` / `ozone-core`) a été retiré au profit de celui-ci.
- Permissions par guilde (overrides), invitations, threads, etc.
- Bascule full-stack (Postgres/Redis/NATS/S3) + SFU vocal.
