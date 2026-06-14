#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# Ozone — déploiement de production sur AlmaLinux / RHEL-family (systemd).
#
# Récupère/maj le projet, compile l'API + le nœud média SFU en release, crée les
# utilisateurs système, GÉNÈRE des secrets ALÉATOIRES (secret vocal partagé +
# mot de passe d'instance), installe les unités systemd, configure firewalld, et
# démarre les services. Idempotent : ré-exécutable (réutilise les secrets existants).
#
# Pour repartir d'une instance VIERGE (nouveaux secrets, base/uploads effacés) :
#   sudo ./reset.sh --yes && sudo ./deploy.sh
#
# Usage :
#   sudo REPO_URL=https://github.com/<owner>/ozone.git ./deploy.sh
#   (ou, depuis un clone existant :)  sudo ./deploy.sh
#
# Variables reconnues :
#   REPO_URL          URL git (obligatoire si OZONE_DIR n'est pas déjà un clone)
#   OZONE_DIR         clone du projet                 (défaut : /opt/ozone)
#   OZONE_BRANCH      branche à déployer              (défaut : main)
#   OZONE_DATA_DIR    données (DB + uploads)          (défaut : /var/lib/ozone)
#   OZONE_ETC_DIR     secrets systemd                 (défaut : /etc/ozone)
#   OZONE_BIND        écoute API                      (défaut : 127.0.0.1:8080, derrière proxy TLS)
#   OZONE_SFU_BIND    écoute SFU (signalisation)      (défaut : 0.0.0.0:8081)
#   OZONE_INSTANCE_NAME / OZONE_INSTANCE_DESCRIPTION / OZONE_REGISTRATION
#   NO_GATE=1         n'active PAS le mot de passe d'instance (instance ouverte)
#   SKIP_FIREWALL=1   ne touche pas firewalld
#   SKIP_FRONTEND=1   ne build pas le client web
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

# URL du dépôt (défaut : dépôt officiel). Surchargeable en 1er argument ou via REPO_URL.
REPO_URL="${REPO_URL:-${1:-https://github.com/HayatoFox/Ozone.git}}"
OZONE_BRANCH="${OZONE_BRANCH:-main}"

# Dossier du projet. Si on est lancé DEPUIS un clone (le script vit dans <clone>/scripts/), on
# utilise CE clone au lieu de /opt/ozone → pas de re-clone ni de demande d'URL inutile.
SCRIPT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [ -z "${OZONE_DIR:-}" ]; then
  if [ -d "$SCRIPT_ROOT/.git" ]; then
    OZONE_DIR="$SCRIPT_ROOT"
  else
    OZONE_DIR="/opt/ozone"
  fi
fi
OZONE_DATA_DIR="${OZONE_DATA_DIR:-/var/lib/ozone}"
OZONE_ETC_DIR="${OZONE_ETC_DIR:-/etc/ozone}"
OZONE_BIND="${OZONE_BIND:-127.0.0.1:8080}"
OZONE_SFU_BIND="${OZONE_SFU_BIND:-0.0.0.0:8081}"
OZONE_INSTANCE_NAME="${OZONE_INSTANCE_NAME:-Mon Instance Ozone}"
OZONE_INSTANCE_DESCRIPTION="${OZONE_INSTANCE_DESCRIPTION:-Une instance Ozone auto-hébergée}"
OZONE_REGISTRATION="${OZONE_REGISTRATION:-open}"

DB_PATH="$OZONE_DATA_DIR/ozone.db"
UPLOAD_DIR="$OZONE_DATA_DIR/uploads"
API_ENV="$OZONE_ETC_DIR/ozone.env"
VOICE_ENV="$OZONE_ETC_DIR/voice.env"

log()  { printf '\033[1;36m[ozone]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[ozone] ATTENTION:\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31m[ozone] ERREUR:\033[0m %s\n' "$*" >&2; exit 1; }

# Doit s'exécuter en root (install systemd, /etc, /usr/local/bin).
SUDO=""
if [ "$(id -u)" -ne 0 ]; then
  command -v sudo >/dev/null 2>&1 || die "Lance ce script en root (ou installe sudo)."
  SUDO="sudo"
fi

port_of() { echo "${1##*:}"; }

# Génère une valeur aléatoire. Garde explicite sur openssl : un `openssl … || repli` dans un PIPE
# lierait le `||` au DERNIER maillon du pipe (pas à openssl) → repli jamais pris et valeur VIDE si
# openssl manque. On teste donc openssl en amont et on valide que le résultat est non vide.
gen_secret() {
  local v
  if command -v openssl >/dev/null 2>&1; then
    v="$(openssl rand -hex 32)"
  else
    v="$(head -c32 /dev/urandom | od -An -tx1 | tr -d ' \n')"
  fi
  [ -n "$v" ] || die "génération du secret échouée (openssl et /dev/urandom indisponibles)."
  printf '%s' "$v"
}
# Mot de passe lisible (base64 url-safe, ~32 caractères) ; repli hex sur /dev/urandom.
gen_password() {
  local v
  if command -v openssl >/dev/null 2>&1; then
    v="$(openssl rand -base64 24 | tr '+/' '-_' | tr -d '=\n')"
  else
    v="$(head -c18 /dev/urandom | od -An -tx1 | tr -d ' \n')"
  fi
  [ -n "$v" ] || die "génération du mot de passe échouée."
  printf '%s' "$v"
}

# ── 1. Dépendances système ───────────────────────────────────────────────────
log "Installation des dépendances (gcc, git, curl, openssl, Node.js)…"
if command -v dnf >/dev/null 2>&1; then
  $SUDO dnf -y install gcc gcc-c++ make git curl openssl >/dev/null
  if [ "${SKIP_FRONTEND:-0}" != "1" ] && ! command -v npm >/dev/null 2>&1; then
    $SUDO dnf -y module enable nodejs:20 >/dev/null 2>&1 || true
    $SUDO dnf -y install nodejs npm >/dev/null
  fi
else
  warn "dnf introuvable — j'assume gcc/git/curl/openssl/node déjà présents."
fi

# ── 2. Toolchain Rust ─────────────────────────────────────────────────────────
if ! command -v cargo >/dev/null 2>&1; then
  log "Installation de la toolchain Rust (rustup)…"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
command -v cargo >/dev/null 2>&1 || die "cargo indisponible après rustup."

# ── 3. Récupération du projet ─────────────────────────────────────────────────
if [ -d "$OZONE_DIR/.git" ]; then
  log "Mise à jour du dépôt ($OZONE_DIR)…"
  git -C "$OZONE_DIR" fetch --quiet origin
  git -C "$OZONE_DIR" checkout --quiet "$OZONE_BRANCH"
  # Échec dur si la branche a divergé : un déploiement de prod ne doit JAMAIS livrer un arbre
  # obsolète/inattendu en silence. (Forcer la MAJ : OZONE_FORCE_RESET=1 → reset --hard.)
  if ! git -C "$OZONE_DIR" pull --quiet --ff-only origin "$OZONE_BRANCH"; then
    if [ "${OZONE_FORCE_RESET:-0}" = "1" ]; then
      warn "branche divergente — reset --hard sur origin/$OZONE_BRANCH (OZONE_FORCE_RESET=1)."
      git -C "$OZONE_DIR" reset --hard "origin/$OZONE_BRANCH"
    else
      die "git pull non fast-forward (branche divergente). Corrige le clone, ou relance avec OZONE_FORCE_RESET=1."
    fi
  fi
else
  [ -n "$REPO_URL" ] || die "REPO_URL non fourni et $OZONE_DIR absent."
  log "Clonage de $REPO_URL → $OZONE_DIR…"
  $SUDO mkdir -p "$(dirname "$OZONE_DIR")"
  $SUDO git clone --branch "$OZONE_BRANCH" "$REPO_URL" "$OZONE_DIR"
fi
cd "$OZONE_DIR"
log "Révision déployée : $(git -C "$OZONE_DIR" rev-parse --short HEAD 2>/dev/null || echo '?')"

# ── 4. Compilation (release) ──────────────────────────────────────────────────
log "Compilation API + SFU (cargo build --release)… (peut être long)"
cargo build --release -p ozone-api -p ozone-sfu
[ -x "$OZONE_DIR/target/release/ozone-api" ] || die "binaire ozone-api manquant après build."
[ -x "$OZONE_DIR/target/release/ozone-sfu" ] || die "binaire ozone-sfu manquant après build."

if [ "${SKIP_FRONTEND:-0}" != "1" ] && command -v npm >/dev/null 2>&1; then
  log "Build du client web (desktop/)…"
  ( cd "$OZONE_DIR/desktop" && npm ci --no-audit --no-fund && npm run build )
  log "Client web généré dans desktop/dist (à servir derrière le reverse-proxy TLS)."
fi

# ── 5. Utilisateurs système + répertoires ─────────────────────────────────────
id ozone    >/dev/null 2>&1 || $SUDO useradd --system --home "$OZONE_DATA_DIR" --shell /sbin/nologin ozone
id ozonesfu >/dev/null 2>&1 || $SUDO useradd --system --shell /sbin/nologin ozonesfu
$SUDO mkdir -p "$OZONE_DATA_DIR" "$UPLOAD_DIR" "$OZONE_ETC_DIR"
$SUDO chown -R ozone:ozone "$OZONE_DATA_DIR"
$SUDO chmod 750 "$OZONE_ETC_DIR"

# ── 6. Secrets (générés une fois, réutilisés ensuite) ──────────────────────────
# Secret vocal PARTAGÉ : doit être identique côté API et côté SFU. On le génère une
# seule fois et on l'écrit dans LES DEUX fichiers d'environnement.
if [ -f "$VOICE_ENV" ] && $SUDO grep -q '^OZONE_VOICE_SECRET=' "$VOICE_ENV" 2>/dev/null; then
  # `sed` imprime la (seule) ligne du secret ; pas de `| head` (footgun SIGPIPE sous pipefail).
  VOICE_SECRET="$($SUDO sed -n 's/^OZONE_VOICE_SECRET=//p' "$VOICE_ENV")"
  log "Secret vocal existant réutilisé."
else
  VOICE_SECRET="$(gen_secret)"
  log "Nouveau secret vocal généré."
fi

# Mot de passe d'instance (gate d'accès) : généré sauf NO_GATE=1. Réutilisé s'il existe.
INSTANCE_PASSWORD=""
SHOW_PASSWORD=""
if [ "${NO_GATE:-0}" != "1" ]; then
  if [ -f "$API_ENV" ] && $SUDO grep -q '^OZONE_INSTANCE_PASSWORD=' "$API_ENV" 2>/dev/null; then
    INSTANCE_PASSWORD="$($SUDO sed -n 's/^OZONE_INSTANCE_PASSWORD=//p' "$API_ENV")"
  else
    INSTANCE_PASSWORD="$(gen_password)"
    SHOW_PASSWORD="$INSTANCE_PASSWORD" # affiché une fois à la fin
  fi
fi

# Confiance au header X-Forwarded-For UNIQUEMENT si l'API est en loopback (donc derrière un
# reverse-proxy TLS de confiance). Si elle est exposée directement (0.0.0.0/IP publique), on NE
# fait PAS confiance au header : sinon un client en accès direct usurperait son IP (contournement
# du rate-limiting et des logs par IP).
case "$OZONE_BIND" in
  127.0.0.1:*|localhost:*|::1:*) TRUST_PROXY=1 ;;
  *) TRUST_PROXY=0 ;;
esac

# Fichier d'environnement de l'API (secrets + identité). chmod 600, propriétaire ozone.
log "Écriture de $API_ENV…"
$SUDO tee "$API_ENV" >/dev/null <<EOF
# Généré par deploy.sh — secrets de l'instance Ozone. NE PAS committer / partager.
OZONE_BIND=$OZONE_BIND
OZONE_DB_PATH=$DB_PATH
OZONE_UPLOAD_DIR=$UPLOAD_DIR
OZONE_INSTANCE_NAME=$OZONE_INSTANCE_NAME
OZONE_INSTANCE_DESCRIPTION=$OZONE_INSTANCE_DESCRIPTION
OZONE_REGISTRATION=$OZONE_REGISTRATION
OZONE_VOICE_SECRET=$VOICE_SECRET
OZONE_SFU_URL=http://127.0.0.1:$(port_of "$OZONE_SFU_BIND")
OZONE_TRUSTED_PROXY=$TRUST_PROXY
RUST_LOG=info
EOF
if [ -n "$INSTANCE_PASSWORD" ]; then
  echo "OZONE_INSTANCE_PASSWORD=$INSTANCE_PASSWORD" | $SUDO tee -a "$API_ENV" >/dev/null
fi
$SUDO chown ozone:ozone "$API_ENV"
$SUDO chmod 600 "$API_ENV"

# Fichier d'environnement du SFU (uniquement le secret vocal partagé). chmod 600.
log "Écriture de $VOICE_ENV…"
$SUDO tee "$VOICE_ENV" >/dev/null <<EOF
# Généré par deploy.sh — MÊME secret vocal que l'API (sinon le SFU rejette tout, fail-closed).
OZONE_VOICE_SECRET=$VOICE_SECRET
EOF
$SUDO chown ozonesfu:ozonesfu "$VOICE_ENV"
$SUDO chmod 600 "$VOICE_ENV"

# ── 7. Binaires + unités systemd ───────────────────────────────────────────────
log "Installation des binaires dans /usr/local/bin…"
$SUDO install -m 755 "$OZONE_DIR/target/release/ozone-api" /usr/local/bin/ozone-api
$SUDO install -m 755 "$OZONE_DIR/target/release/ozone-sfu" /usr/local/bin/ozone-sfu

# Unité API : on charge les secrets via EnvironmentFile (la config d'identité vient du même
# fichier ; pas de doublon avec des Environment= en dur qui masqueraient le fichier).
log "Installation des unités systemd…"
$SUDO tee /etc/systemd/system/ozone-api.service >/dev/null <<EOF
[Unit]
Description=Ozone — instance (API REST + Gateway)
After=network-online.target ozone-sfu.service
Wants=network-online.target

[Service]
Type=simple
User=ozone
Group=ozone
WorkingDirectory=$OZONE_DATA_DIR
EnvironmentFile=$API_ENV
ExecStart=/usr/local/bin/ozone-api
Restart=on-failure
RestartSec=3
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=$OZONE_DATA_DIR
PrivateTmp=true
ProtectKernelTunables=true
ProtectControlGroups=true
RestrictSUIDSGID=true
LockPersonality=true

[Install]
WantedBy=multi-user.target
EOF

$SUDO tee /etc/systemd/system/ozone-sfu.service >/dev/null <<EOF
[Unit]
Description=Ozone — nœud média SFU (WebRTC)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=ozonesfu
Group=ozonesfu
Environment=OZONE_SFU_BIND=$OZONE_SFU_BIND
Environment=RUST_LOG=info
EnvironmentFile=$VOICE_ENV
ExecStart=/usr/local/bin/ozone-sfu
Restart=on-failure
RestartSec=3
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
ProtectKernelTunables=true
ProtectControlGroups=true
RestrictSUIDSGID=true
LockPersonality=true

[Install]
WantedBy=multi-user.target
EOF

# ── 8. Pare-feu (firewalld) ────────────────────────────────────────────────────
if [ "${SKIP_FIREWALL:-0}" != "1" ] && command -v firewall-cmd >/dev/null 2>&1 \
   && $SUDO firewall-cmd --state >/dev/null 2>&1; then
  log "Configuration de firewalld…"
  # SFU : signalisation TCP + média RTP/SRTP UDP (ports éphémères WebRTC).
  $SUDO firewall-cmd --permanent --add-port="$(port_of "$OZONE_SFU_BIND")/tcp" >/dev/null
  $SUDO firewall-cmd --permanent --add-port="49152-65535/udp" >/dev/null
  # API : on n'ouvre le port que si elle écoute sur autre chose que loopback (sinon
  # elle est censée être derrière un reverse-proxy TLS qui, lui, est exposé sur 443).
  case "$OZONE_BIND" in
    127.0.0.1:*|localhost:*) log "API en loopback : port non exposé (reverse-proxy TLS attendu devant)." ;;
    *) $SUDO firewall-cmd --permanent --add-port="$(port_of "$OZONE_BIND")/tcp" >/dev/null ;;
  esac
  $SUDO firewall-cmd --reload >/dev/null
else
  [ "${SKIP_FIREWALL:-0}" = "1" ] && log "Pare-feu ignoré (SKIP_FIREWALL=1)." || warn "firewalld absent/inactif — pare-feu non configuré."
fi

# ── 9. Démarrage des services ──────────────────────────────────────────────────
log "Activation et démarrage des services…"
$SUDO systemctl daemon-reload
$SUDO systemctl enable --now ozone-sfu
$SUDO systemctl enable --now ozone-api

sleep 1
$SUDO systemctl is-active --quiet ozone-sfu && log "ozone-sfu : actif." || warn "ozone-sfu inactif — voir: journalctl -u ozone-sfu"
$SUDO systemctl is-active --quiet ozone-api && log "ozone-api : actif." || warn "ozone-api inactif — voir: journalctl -u ozone-api"

# ── 10. Récapitulatif ──────────────────────────────────────────────────────────
log "Déploiement terminé."
echo "  API     : $OZONE_BIND   (place un reverse-proxy TLS devant, ex. Caddy/Nginx)"
echo "  SFU      : $OZONE_SFU_BIND"
echo "  Secrets  : $API_ENV et $VOICE_ENV (chmod 600)"
echo "  jwt_secret : auto-généré dans la base au 1er démarrage (rien à faire)."
if [ -n "$SHOW_PASSWORD" ]; then
  printf '\n\033[1;32m  ► Mot de passe d'\''instance (gate d'\''accès) — NOTE-LE, affiché une seule fois :\033[0m\n'
  printf '      \033[1m%s\033[0m\n\n' "$SHOW_PASSWORD"
elif [ "${NO_GATE:-0}" = "1" ]; then
  echo "  Gate     : désactivé (instance ouverte, inscription = $OZONE_REGISTRATION)."
else
  echo "  Gate     : mot de passe d'instance déjà défini (réutilisé)."
fi
