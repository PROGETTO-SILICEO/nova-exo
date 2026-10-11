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
HB="$REPO/experiments/net_telemetry/listener_heartbeat.json"
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

# caso 2: processo vivo -> il listener sta ricevendo? La metrica è il BATTITO
# (aggiornato su ogni datagramma), NON il log. Il log può essere fermo perché i
# pacchetti sono RIFIUTATI, non perché il listener è morto: usare il log come
# salute ha causato 1.719 riavvii inutili in 2,5 giorni (lezione 11/10/2026).
if [ -f "$HB" ]; then
  hb_age=$(( $(date +%s) - $(stat -c %Y "$HB") ))
  if [ "$hb_age" -gt "$STALE" ]; then
    restart "nessun datagramma da ${hb_age}s (listener inceppato)"
  elif command -v jq >/dev/null 2>&1; then
    # Battito fresco: se riceve ma SCARTA tutto, il formato è cambiato.
    # Non è un listener morto: è un CONTRATTO rotto. Allarme, non restart.
    validi=$(jq -r '.validi // 0' "$HB" 2>/dev/null || echo 0)
    scartati=$(jq -r '.scartati // 0' "$HB" 2>/dev/null || echo 0)
    if [ "$scartati" -gt 0 ] && [ "$validi" -eq 0 ]; then
      echo "$(date '+%F %T') [watchdog] ALLARME: listener riceve ma scarta tutto (validi=$validi scartati=$scartati) — contratto telemetria rotto?" >> "$WLOG"
    fi
  fi
elif [ -f "$LOG" ]; then
  # fallback (vecchia metrica) se il battito non esiste ancora
  age=$(( $(date +%s) - $(stat -c %Y "$LOG") ))
  [ "$age" -gt "$STALE" ] && restart "log fermo da ${age}s (nessun battito disponibile)"
fi
exit 0
