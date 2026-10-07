#!/usr/bin/env bash
# Watchdog del listener di telemetria Exo.
#
# Perché esiste (07/10/2026): il listener del 05/10 è rimasto vivo ma
# inceppato in do_poll per 48 ore — 48 h di telemetria perse in silenzio,
# mentre Exo correva. Un processo che non scrive ma non muore non fa
# rumore: l'occhio deve guardare i DATI, non il processo.
#
# Regola: se il log non cresce da >180 s, il listener è morto dentro.
# (Exo trasmette ogni ~0,91 s; 180 s ≈ 200 pacchetti mancanti.)
#
# REGOLA DEL SINGOLO POSSESSORE: mai due listener sulla 42800.
# Con SO_REUSEADDR due socket convivono ma il kernel consegna ogni
# datagramma a UNO solo: il secondo ruba pacchetti in silenzio.
# Quindi: prima si uccide OGNI listener, poi si rilancia il padrone
# (systemd se la unit esiste — LoadState, non ActiveState — altrimenti
# nohup). Mai nohup parallelo a systemd.
set -u
REPO="$HOME/Documenti/GitHub/nova-exo"
LOG="$REPO/experiments/net_telemetry/telemetry.jsonl"
WLOG="$REPO/experiments/net_telemetry/watchdog.log"
STALE=180
SERVICE=exo-listen.service

# la unit esiste? (non: è attiva? — una unit ferma va riavviata, non ignorata)
unit_exists() {
  [ "$(systemctl --user show -p LoadState --value "$SERVICE" 2>/dev/null)" = "loaded" ]
}

restart() {
  local why="$1"
  pkill -9 -f "exo_listen[.]py --out" 2>/dev/null
  sleep 1
  if unit_exists; then
    systemctl --user start "$SERVICE" >/dev/null 2>&1 || \
      systemctl --user restart "$SERVICE" >/dev/null 2>&1
  else
    ( cd "$REPO" && nohup python3 tools/exo_listen.py --out "$LOG" \
        >> "$REPO/experiments/net_telemetry/listener.log" 2>&1 & disown )
  fi
  echo "$(date '+%F %T') [watchdog] $why -> riavviato" >> "$WLOG"
}

# caso 1: nessun processo -> avvia
if ! pgrep -f "exo_listen[.]py --out" >/dev/null 2>&1; then
  restart "nessun processo"
  exit 0
fi

# caso 2: processo vivo ma log fermo -> inceppato
if [ -f "$LOG" ]; then
  age=$(( $(date +%s) - $(stat -c %Y "$LOG") ))
  [ "$age" -gt "$STALE" ] && restart "log fermo da ${age}s (inceppato)"
fi
exit 0
