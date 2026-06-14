#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# Ozone — RÉINITIALISATION COMPLÈTE (instance vierge).
#
# DESTRUCTIF. Supprime TOUTES les données et secrets pour repartir d'une instance
# neuve : comptes, serveurs, messages (la base SQLite), les fichiers téléversés
# (avatars/emojis/pièces jointes), et tous les secrets (.env locaux + /etc/ozone/*.env).
# Au prochain démarrage, l'API régénère un nouveau jwt_secret, et deploy.sh régénère
# un OZONE_VOICE_SECRET aléatoire.
#
# Usage :
#   sudo ./reset.sh            # demande confirmation
#   sudo ./reset.sh --yes      # sans confirmation (CI / automatisation)
#
# Variables reconnues (mêmes défauts que deploy.sh) :
#   OZONE_DATA_DIR   répertoire de données systemd   (défaut : /var/lib/ozone)
#   OZONE_DB_PATH    chemin DB                        (défaut : $OZONE_DATA_DIR/ozone.db)
#   OZONE_UPLOAD_DIR répertoire des téléversements    (défaut : $OZONE_DATA_DIR/uploads)
#   OZONE_ETC_DIR    répertoire des secrets systemd   (défaut : /etc/ozone)
#   OZONE_DIR        clone du projet (pour le .env de dev) (défaut : dossier parent du script)
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

OZONE_DATA_DIR="${OZONE_DATA_DIR:-/var/lib/ozone}"
OZONE_DB_PATH="${OZONE_DB_PATH:-$OZONE_DATA_DIR/ozone.db}"
OZONE_UPLOAD_DIR="${OZONE_UPLOAD_DIR:-$OZONE_DATA_DIR/uploads}"
OZONE_ETC_DIR="${OZONE_ETC_DIR:-/etc/ozone}"
OZONE_DIR="${OZONE_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"

log()  { printf '\033[1;36m[ozone]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[ozone] ATTENTION:\033[0m %s\n' "$*" >&2; }

SUDO=""
if [ "$(id -u)" -ne 0 ]; then
  command -v sudo >/dev/null 2>&1 && SUDO="sudo" || warn "pas root et sudo absent : certaines suppressions peuvent échouer."
fi

# ── Confirmation ─────────────────────────────────────────────────────────────
if [ "${1:-}" != "--yes" ]; then
  printf '\033[1;31mCeci EFFACE toutes les données Ozone (comptes, serveurs, messages, fichiers) et les secrets.\033[0m\n'
  printf '  Base   : %s\n  Uploads: %s\n  Secrets: %s/*.env + %s/.env\n' \
    "$OZONE_DB_PATH" "$OZONE_UPLOAD_DIR" "$OZONE_ETC_DIR" "$OZONE_DIR"
  printf 'Taper "RESET" pour confirmer : '
  read -r ans
  [ "$ans" = "RESET" ] || { warn "Annulé."; exit 1; }
fi

# ── 1. Arrêt des services (s'ils existent) ───────────────────────────────────
if command -v systemctl >/dev/null 2>&1; then
  for unit in ozone-sfu ozone-api; do
    if $SUDO systemctl list-unit-files "$unit.service" >/dev/null 2>&1 \
       && $SUDO systemctl is-active --quiet "$unit" 2>/dev/null; then
      log "Arrêt de $unit…"
      $SUDO systemctl stop "$unit" || true
    fi
  done
fi
# Filet : processus lancés hors systemd (mode dev nohup).
if command -v pkill >/dev/null 2>&1; then
  pkill -TERM -f "$OZONE_DIR/target/release/ozone-sfu" 2>/dev/null || true
  pkill -TERM -f "$OZONE_DIR/target/release/ozone-api" 2>/dev/null || true
fi

# ── 2. Base de données (SQLite + WAL + SHM) ──────────────────────────────────
log "Suppression de la base de données…"
$SUDO rm -f "$OZONE_DB_PATH" "$OZONE_DB_PATH-wal" "$OZONE_DB_PATH-shm"
# Base de dev éventuelle à la racine du clone.
rm -f "$OZONE_DIR/ozone.db" "$OZONE_DIR/ozone.db-wal" "$OZONE_DIR/ozone.db-shm" 2>/dev/null || true

# ── 3. Fichiers téléversés ───────────────────────────────────────────────────
if [ -d "$OZONE_UPLOAD_DIR" ]; then
  log "Suppression des téléversements ($OZONE_UPLOAD_DIR)…"
  $SUDO rm -rf "${OZONE_UPLOAD_DIR:?}/"* 2>/dev/null || true
fi
# Répertoire d'uploads par défaut en mode dev (TMP/ozone-uploads).
rm -rf "${TMPDIR:-/tmp}/ozone-uploads" 2>/dev/null || true

# ── 4. Secrets (.env de dev + /etc/ozone) ────────────────────────────────────
log "Suppression des secrets…"
rm -f "$OZONE_DIR/.env" "$OZONE_DIR/.env.local" 2>/dev/null || true
$SUDO rm -f "$OZONE_ETC_DIR/ozone.env" "$OZONE_ETC_DIR/voice.env" 2>/dev/null || true

log "Réinitialisation terminée. L'instance est vierge."
log "Relance via deploy.sh : nouveaux secrets aléatoires + nouveau jwt_secret au 1er démarrage."
