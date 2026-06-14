#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# Ozone — (re)lancement de l'instance (API + nœud média SFU).
#
# Vérifie si des processus tournent déjà (via fichiers PID et, à défaut, par nom),
# les arrête proprement (SIGTERM puis SIGKILL si récalcitrants), puis relance les
# deux binaires release en arrière-plan avec le secret vocal PARTAGÉ du .env.
#
# Usage : ./restart.sh [start|stop|status|restart]   (défaut : restart)
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

OZONE_DIR="${OZONE_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
RUN_DIR="${OZONE_RUN_DIR:-$OZONE_DIR/run}"
LOG_DIR="${OZONE_LOG_DIR:-$OZONE_DIR/logs}"
mkdir -p "$RUN_DIR" "$LOG_DIR"

API_BIN="$OZONE_DIR/target/release/ozone-api"
SFU_BIN="$OZONE_DIR/target/release/ozone-sfu"

log()  { printf '\033[1;36m[ozone]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[ozone] ATTENTION:\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31m[ozone] ERREUR:\033[0m %s\n' "$*" >&2; exit 1; }

# Charge le .env (secret vocal partagé + binds) pour les deux processus.
if [ -f "$OZONE_DIR/.env" ]; then
  set -a; . "$OZONE_DIR/.env"; set +a
else
  warn ".env absent : sans OZONE_VOICE_SECRET partagé, le vocal sera indisponible (SFU fail-closed)."
fi

# pid_running <pidfile> → imprime le PID s'il tourne, vide sinon.
pid_running() {
  local f="$1"
  [ -f "$f" ] || return 0
  local pid; pid="$(cat "$f" 2>/dev/null || true)"
  if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then echo "$pid"; fi
}

# stop_one <nom> <pidfile> <bin> — SIGTERM, attend, puis SIGKILL ; filet de sécurité résiduel
# scopé au CHEMIN ABSOLU du binaire de CETTE install (jamais une autre instance ni un homonyme).
stop_one() {
  local name="$1" pidfile="$2" bin="$3" pid
  pid="$(pid_running "$pidfile")"
  if [ -n "$pid" ]; then
    log "Arrêt de $name (pid $pid)…"
    kill "$pid" 2>/dev/null || true
    for _ in $(seq 1 20); do kill -0 "$pid" 2>/dev/null || break; sleep 0.25; done
    if kill -0 "$pid" 2>/dev/null; then warn "$name ne répond pas — SIGKILL."; kill -9 "$pid" 2>/dev/null || true; fi
  fi
  rm -f "$pidfile"
  # Filet de sécurité : un binaire orphelin de CETTE install (lancé hors script) verrouillerait le
  # port. On matche le CHEMIN ABSOLU exact ($bin) → jamais une install voisine ni un process homonyme.
  if [ -n "$bin" ] && command -v pkill >/dev/null 2>&1; then
    if pgrep -f "$bin" >/dev/null 2>&1; then
      warn "Processus '$name' résiduels détectés (hors PID file) — arrêt."
      pkill -TERM -f "$bin" 2>/dev/null || true
      sleep 1
      pkill -9 -f "$bin" 2>/dev/null || true
    fi
  fi
}

start_one() {
  local name="$1" bin="$2" pidfile="$3" logfile="$4"
  [ -x "$bin" ] || die "Binaire $name introuvable ($bin). Lance d'abord install.sh (cargo build --release)."
  local existing; existing="$(pid_running "$pidfile")"
  if [ -n "$existing" ]; then warn "$name tourne déjà (pid $existing) — ignoré."; return 0; fi
  log "Démarrage de $name…"
  nohup "$bin" >>"$logfile" 2>&1 &
  echo $! > "$pidfile"
  # Vérifie que le binaire n'est pas mort immédiatement (port pris, secret manquant, panic) :
  # sinon on annoncerait « démarré » pour un PID déjà mort.
  sleep 0.4
  if ! kill -0 "$(cat "$pidfile")" 2>/dev/null; then
    rm -f "$pidfile"
    die "$name a quitté immédiatement — voir $logfile (port déjà pris ? OZONE_VOICE_SECRET manquant ?)."
  fi
  log "$name démarré (pid $(cat "$pidfile")), logs : $logfile"
}

API_PID="$RUN_DIR/ozone-api.pid"
SFU_PID="$RUN_DIR/ozone-sfu.pid"

do_stop() {
  stop_one "ozone-sfu" "$SFU_PID" "$SFU_BIN"   # SFU d'abord (dépend de l'API pour les jetons)
  stop_one "ozone-api" "$API_PID" "$API_BIN"
}

do_start() {
  start_one "ozone-api" "$API_BIN" "$API_PID" "$LOG_DIR/ozone-api.log"
  # Petit délai pour que l'API soit à l'écoute avant que le SFU/les clients ne s'y connectent.
  sleep 1
  start_one "ozone-sfu" "$SFU_BIN" "$SFU_PID" "$LOG_DIR/ozone-sfu.log"
}

do_status() {
  local a s
  a="$(pid_running "$API_PID")"; s="$(pid_running "$SFU_PID")"
  printf 'ozone-api : %s\n' "${a:+en cours (pid $a)}"; if [ -z "$a" ]; then echo '            arrêté'; fi
  printf 'ozone-sfu : %s\n' "${s:+en cours (pid $s)}"; if [ -z "$s" ]; then echo '            arrêté'; fi
  return 0  # ne jamais retourner le code d'un test final (sinon set -e tue restart/status)
}

case "${1:-restart}" in
  start)   do_start ;;
  stop)    do_stop ;;
  status)  do_status ;;
  restart) do_stop; do_start; log "Instance relancée."; do_status ;;
  *)       die "Usage : $0 [start|stop|status|restart]" ;;
esac
