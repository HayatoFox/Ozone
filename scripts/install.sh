#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# Ozone — installation serveur tout-en-un (AlmaLinux / RHEL-family).
#
# Récupère le projet, installe les dépendances (gcc, git, Node, toolchain Rust),
# compile l'API + le nœud média SFU + le client web, configure le pare-feu
# (firewalld), génère un secret vocal partagé, puis lance les processus.
#
# Idempotent : ré-exécutable sans casse (réutilise le clone, le .env, le secret).
#
# Usage :
#   REPO_URL=https://github.com/<owner>/ozone.git ./install.sh
#   (ou : ./install.sh https://github.com/<owner>/ozone.git)
#
# Variables d'environnement reconnues :
#   REPO_URL         URL git du projet (obligatoire si le dossier n'existe pas encore)
#   OZONE_DIR        Dossier d'installation        (défaut : $HOME/ozone)
#   OZONE_BIND       Adresse d'écoute API          (défaut : 0.0.0.0:8080)
#   OZONE_SFU_BIND   Adresse d'écoute SFU (signal) (défaut : 0.0.0.0:8081)
#   OZONE_BRANCH     Branche git à déployer        (défaut : main)
#   SKIP_FIREWALL=1  Ne pas toucher firewalld
#   SKIP_FRONTEND=1  Ne pas builder le client web
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

# ── Paramètres ───────────────────────────────────────────────────────────────
# URL du dépôt (défaut : le dépôt officiel). Surchargeable en 1er argument ou via REPO_URL.
REPO_URL="${REPO_URL:-${1:-https://github.com/HayatoFox/Ozone.git}}"
OZONE_BIND="${OZONE_BIND:-0.0.0.0:8080}"
OZONE_SFU_BIND="${OZONE_SFU_BIND:-0.0.0.0:8081}"
OZONE_BRANCH="${OZONE_BRANCH:-main}"

# Dossier d'installation. Si on est lancé DEPUIS un clone existant (le script vit dans
# <clone>/scripts/), on utilise CE clone — pas $HOME/ozone — afin de ne pas re-cloner à côté.
SCRIPT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [ -z "${OZONE_DIR:-}" ]; then
  if [ -d "$SCRIPT_ROOT/.git" ]; then
    OZONE_DIR="$SCRIPT_ROOT"   # lancé depuis un clone → on l'utilise tel quel
  else
    OZONE_DIR="$HOME/ozone"    # script isolé → clone dans $HOME/ozone
  fi
fi

log()  { printf '\033[1;36m[ozone]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[ozone] ATTENTION:\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31m[ozone] ERREUR:\033[0m %s\n' "$*" >&2; exit 1; }

# `sudo` seulement si on n'est pas déjà root.
SUDO=""
if [ "$(id -u)" -ne 0 ]; then
  command -v sudo >/dev/null 2>&1 || die "sudo introuvable et pas root : relance en root."
  SUDO="sudo"
fi

# Port (sans l'adresse) à partir d'un "host:port".
port_of() { echo "${1##*:}"; }

# ── 1. Dépendances système ───────────────────────────────────────────────────
log "Installation des dépendances système (gcc, git, curl, Node.js)…"
if command -v dnf >/dev/null 2>&1; then
  $SUDO dnf -y install gcc gcc-c++ make git curl >/dev/null
  # Node.js pour builder le client web (module AppStream d'AlmaLinux).
  if [ "${SKIP_FRONTEND:-0}" != "1" ] && ! command -v npm >/dev/null 2>&1; then
    $SUDO dnf -y module enable nodejs:20 >/dev/null 2>&1 || true
    $SUDO dnf -y install nodejs npm >/dev/null
  fi
else
  warn "dnf introuvable (pas une distrib RHEL-family) — j'assume gcc/git/curl/node déjà présents."
fi

# ── 2. Toolchain Rust (rustup) ───────────────────────────────────────────────
if ! command -v cargo >/dev/null 2>&1; then
  log "Installation de la toolchain Rust (rustup)…"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi
# Charge cargo dans le shell courant (rustup l'installe dans ~/.cargo/env).
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
command -v cargo >/dev/null 2>&1 || die "cargo indisponible après installation de rustup."

# ── 3. Récupération du projet ────────────────────────────────────────────────
if [ -d "$OZONE_DIR/.git" ]; then
  log "Dépôt déjà présent dans $OZONE_DIR — mise à jour (git pull)…"
  git -C "$OZONE_DIR" fetch --quiet origin
  git -C "$OZONE_DIR" checkout --quiet "$OZONE_BRANCH"
  git -C "$OZONE_DIR" pull --quiet --ff-only origin "$OZONE_BRANCH" || warn "git pull non fast-forward, on garde l'état local."
else
  [ -n "$REPO_URL" ] || die "REPO_URL non fourni et $OZONE_DIR absent. Ex: REPO_URL=https://… ./install.sh"
  log "Clonage de $REPO_URL → $OZONE_DIR…"
  git clone --branch "$OZONE_BRANCH" "$REPO_URL" "$OZONE_DIR"
fi
cd "$OZONE_DIR"

# ── 4. Secret vocal partagé (.env) ───────────────────────────────────────────
# L'API et le SFU DOIVENT partager OZONE_VOICE_SECRET (sinon le SFU rejette tout, fail-closed).
ENV_FILE="$OZONE_DIR/.env"
if [ ! -f "$ENV_FILE" ]; then
  log "Génération du fichier .env (secret vocal partagé)…"
  SECRET="$(openssl rand -hex 32 2>/dev/null || head -c32 /dev/urandom | od -An -tx1 | tr -d ' \n')"
  cat > "$ENV_FILE" <<EOF
# Généré par install.sh — secret PARTAGÉ entre l'API et le SFU. Ne pas committer.
OZONE_VOICE_SECRET=$SECRET
OZONE_BIND=$OZONE_BIND
OZONE_SFU_BIND=$OZONE_SFU_BIND
RUST_LOG=info
EOF
else
  log ".env déjà présent — réutilisé tel quel."
fi
# Durcit les droits INCONDITIONNELLEMENT (le .env contient le secret vocal) : un .env réutilisé,
# restauré ou édité à la main a pu hériter d'un umask permissif (lisible par d'autres comptes).
chmod 600 "$ENV_FILE"

# ── 5. Compilation ───────────────────────────────────────────────────────────
log "Compilation de l'API et du nœud média SFU (cargo build --release)… (peut être long)"
cargo build --release -p ozone-api -p ozone-sfu

if [ "${SKIP_FRONTEND:-0}" != "1" ]; then
  if command -v npm >/dev/null 2>&1; then
    log "Build du client web (desktop/)…"
    ( cd "$OZONE_DIR/desktop" && npm ci --no-audit --no-fund && npm run build )
    log "Client web généré dans desktop/dist (à servir derrière le reverse-proxy)."
  else
    warn "npm indisponible — client web non buildé (SKIP_FRONTEND ou Node manquant)."
  fi
fi

# ── 6. Pare-feu (firewalld) ──────────────────────────────────────────────────
# AlmaLinux utilise firewalld par défaut. On ouvre :
#   - le port TCP de l'API (signalisation HTTP),
#   - le port TCP de signalisation du SFU,
#   - une plage UDP pour le média RTP/SRTP du SFU (ports éphémères WebRTC).
if [ "${SKIP_FIREWALL:-0}" != "1" ] && command -v firewall-cmd >/dev/null 2>&1; then
  if $SUDO firewall-cmd --state >/dev/null 2>&1; then
    API_PORT="$(port_of "$OZONE_BIND")"
    SFU_PORT="$(port_of "$OZONE_SFU_BIND")"
    log "Configuration de firewalld (TCP $API_PORT, TCP $SFU_PORT, UDP média)…"
    $SUDO firewall-cmd --permanent --add-port="${API_PORT}/tcp"  >/dev/null
    $SUDO firewall-cmd --permanent --add-port="${SFU_PORT}/tcp"  >/dev/null
    # Média WebRTC : RTP/SRTP en UDP sur des ports éphémères. Sans mux de port fixe,
    # on ouvre la plage UDP éphémère standard. Restreindre si un mux fixe est configuré.
    $SUDO firewall-cmd --permanent --add-port="49152-65535/udp" >/dev/null
    $SUDO firewall-cmd --reload >/dev/null
    log "firewalld configuré. (Derrière un reverse-proxy TLS, n'exposez que 443 et restreignez le reste.)"
  else
    warn "firewalld installé mais inactif — pare-feu non configuré."
  fi
else
  [ "${SKIP_FIREWALL:-0}" = "1" ] && log "Pare-feu ignoré (SKIP_FIREWALL=1)." || warn "firewall-cmd absent — pare-feu non configuré."
fi

# ── 7. Lancement ─────────────────────────────────────────────────────────────
log "Installation terminée. Lancement des processus via restart.sh…"
exec "$OZONE_DIR/scripts/restart.sh"
