# 10 — Déploiement (Linux / AlmaLinux)

Le serveur Ozone (`ozone-api`) est du **Rust portable** : il tourne sous **Linux (AlmaLinux/RHEL, Rocky, Debian/Ubuntu)**, Windows et macOS. Aucune dépendance à `ring`/OpenSSL (JWT HS256 maison, hachage Argon2id pur Rust). La **seule** brique compilée en C est **SQLite embarqué** (via `libsqlite3-sys`, mode *bundled*) → il faut un **compilateur C** au moment du *build*, mais **aucune** dépendance système au *runtime* (SQLite est statiquement inclus dans le binaire).

> Cible serveur de référence : **AlmaLinux 9**. La CI compile et teste sur `almalinux:9` (cf. [.github/workflows/ci.yml](../.github/workflows/ci.yml)).

## A. Build natif sur AlmaLinux (depuis les sources)

### 1. Prérequis
```bash
sudo dnf -y install gcc git          # gcc : pour le SQLite embarqué
# Toolchain Rust (rustup) :
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
```
*(Équivalent : `sudo dnf groupinstall "Development Tools"`.)*

### 2. Compiler
```bash
git clone <repo> ozone && cd ozone
cargo build --release -p ozone-api
# binaire : target/release/ozone-api
```

### 3. Installer en service systemd
```bash
sudo cp target/release/ozone-api /usr/local/bin/
sudo useradd --system --home /var/lib/ozone --shell /sbin/nologin ozone
sudo mkdir -p /var/lib/ozone && sudo chown ozone:ozone /var/lib/ozone
sudo cp deploy/ozone-api.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now ozone-api
systemctl status ozone-api
```
Le service écoute par défaut sur `127.0.0.1:8080` (voir [deploy/ozone-api.service](../deploy/ozone-api.service)). Secrets (mot de passe d'instance…) via `EnvironmentFile=/etc/ozone/ozone.env`.

### 4. Pare-feu (firewalld, par défaut sur AlmaLinux)
On expose **derrière un reverse-proxy TLS** (recommandé). Si accès direct :
```bash
sudo firewall-cmd --permanent --add-port=8080/tcp
sudo firewall-cmd --reload
```
SELinux est en mode *enforcing* par défaut sur AlmaLinux : le binaire en `/usr/local/bin` et les données en `/var/lib/ozone` ne posent pas de souci. Pour un port < 1024, préférer le reverse-proxy.

## B. TLS / reverse-proxy

Le serveur parle **HTTP en clair** ; on met un proxy TLS devant (terminaison HTTPS, ACME/Let's Encrypt) :
- **Caddy** (TLS automatique) :
  ```
  chat.exemple.fr {
      reverse_proxy 127.0.0.1:8080
  }
  ```
- **Nginx** : `proxy_pass http://127.0.0.1:8080;` + `proxy_set_header Upgrade/Connection` pour la **Gateway WebSocket** (`/gateway`).

> Le mot de passe d'instance et l'identité d'instance (TOFU/pinning) sont décrits dans [features/00-instances](features/00-instances.md) et [07-securite](07-securite-chiffrement.md).

## C. Conteneurs (Docker / Podman)

AlmaLinux fournit **Podman** (compatible Docker). Deux images :

```bash
# Image Debian (runtime léger) — tourne très bien sur un hôte AlmaLinux :
docker compose up --build                      # ou : podman-compose up --build

# Image base AlmaLinux 9 (si vous voulez du RHEL-family de bout en bout) :
docker build -f deploy/Dockerfile.almalinux -t ozone/ozone-api:alma .
docker run -d -p 8080:8080 -v ozone-data:/data \
  -e OZONE_INSTANCE_NAME="Mon Instance" ozone/ozone-api:alma
```
Voir [docker-compose.yml](../docker-compose.yml) (profil `full` pour Postgres/Redis/NATS/MinIO à venir).

## D. Binaire statique (musl) — zéro dépendance

Pour un binaire **entièrement statique** portable sur toute distribution Linux (aucune dépendance glibc) :
```bash
# Sur Debian/Ubuntu (build host) :
sudo apt-get install -y musl-tools
rustup target add x86_64-unknown-linux-musl
cargo build --release -p ozone-api --target x86_64-unknown-linux-musl
# → target/x86_64-unknown-linux-musl/release/ozone-api (statique, copiable tel quel sur AlmaLinux)
```
Pratique pour distribuer un seul fichier, ou pour des images `FROM scratch`/`distroless`.

## E. Variables d'environnement

| Variable | Défaut | Rôle |
|---|---|---|
| `OZONE_BIND` | `127.0.0.1:8080` | adresse d'écoute |
| `OZONE_DB_PATH` | `ozone.db` | fichier SQLite (mode tout-en-un) |
| `OZONE_INSTANCE_NAME` | `Ozone` | nom de l'instance |
| `OZONE_INSTANCE_DESCRIPTION` | — | description |
| `OZONE_REGISTRATION` | `open` | `open` / `invite` / `closed` |
| `OZONE_INSTANCE_PASSWORD` | — | mot de passe d'instance (gate). Vide = pas de gate |
| `OZONE_VOICE_SECRET` | (secret JWT) | secret **partagé** API ↔ nœud média (jetons vocaux) |
| `RUST_LOG` | `info` | niveau de logs |

Cf. [.env.example](../.env.example).

## F. Mises à jour & sauvegardes
- **MAJ** : `git pull && cargo build --release -p ozone-api && sudo systemctl restart ozone-api` (ou re-build de l'image).
- **Sauvegarde** : copier `OZONE_DB_PATH` (SQLite). En mode WAL, sauvegarder `*.db`, `*.db-wal`, `*.db-shm` ensemble, ou via `sqlite3 ozone.db ".backup ..."`.

## G. Nœud média SFU (vocal/vidéo)

Le vocal s'appuie sur un **processus séparé** `ozone-sfu` (WebRTC). Il est facultatif : sans lui,
toute la messagerie fonctionne ; seul le vocal est indisponible.

- **Secret partagé obligatoire** : définir la **même** valeur de `OZONE_VOICE_SECRET` sur l'API
  **et** le SFU (`openssl rand -hex 32`). Sans secret, le SFU refuse toute connexion (*fail-closed*).
- **Lancer** : binaire `target/release/ozone-sfu` (unité [deploy/ozone-sfu.service](../deploy/ozone-sfu.service)),
  image [deploy/Dockerfile.sfu](../deploy/Dockerfile.sfu), ou service `ozone-sfu` du `docker-compose`.
- **Réseau** : signalisation HTTP sur `OZONE_SFU_BIND` (déf. `:8081`) ; le **média RTP/SRTP** est en
  **UDP** (ports éphémères) → préférez le **réseau hôte** pour le nœud média (ou un mux de port UDP
  fixe à venir). Ouvrir l'UDP correspondant au pare-feu.
- **Isolation** : le SFU embarque `ring`/`rustls` (via WebRTC) ; l'API en reste exempte.

> État : signalisation + relais RTP + authz du jeton vocal en place. Renégociation N-à-N (WS) et
> E2EE DAVE/MLS à venir — cf. [crates/ozone-sfu/README.md](../crates/ozone-sfu/README.md).

## H. Montée en charge (full-stack)
À grande échelle, on bascule du tout-en-un SQLite vers l'architecture de [01-architecture](01-architecture.md) (PostgreSQL + ScyllaDB + Redis + NATS + MinIO + SFU), chaque service scalant indépendamment. Le profil `full` du `docker-compose` fournit déjà ces briques pour le développement.
