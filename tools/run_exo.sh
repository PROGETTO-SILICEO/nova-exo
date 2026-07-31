#!/bin/bash
# run_exo.sh — Avvia Exo con encoder
#
# Lancia in sequenza:
#   1. encoder_server (su porta 5006)
#   2. v2_bridge (QEMU + Exo)
#   3. (opzionale) encoder_inject in ascolto su named pipe

set -e

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENCODER_PORT=${ENCODER_PORT:-5006}
PIPE="${ROOT}/state/encoder_input"

echo "╔═══════════════════════════════════════════════╗"
echo "║         Exo + ExoChemio Encoder              ║"
echo "╚═══════════════════════════════════════════════╝"

# ── 1. Avvia encoder server ────────────────────────
if curl -sf http://127.0.0.1:${ENCODER_PORT}/health >/dev/null 2>&1; then
    echo "[run_exo] Encoder server già attivo su :${ENCODER_PORT}"
else
    echo "[run_exo] Avvio encoder server su :${ENCODER_PORT}..."
    python3 "${ROOT}/dataset/encoder/encoder_server.py" --port "${ENCODER_PORT}" &
    ENC_PID=$!
    # Aspetta che sia pronto
    for i in $(seq 1 30); do
        if curl -sf http://127.0.0.1:${ENCODER_PORT}/health >/dev/null 2>&1; then
            echo "[run_exo] Encoder server pronto (PID $ENC_PID)"
            break
        fi
        sleep 2
    done
    if ! curl -sf http://127.0.0.1:${ENCODER_PORT}/health >/dev/null 2>&1; then
        echo "[run_exo] ERRORE: encoder server non partito" >&2
        exit 1
    fi
fi

# ── 2. Crea named pipe per encoder_inject ──────────
if [ ! -p "$PIPE" ]; then
    mkdir -p "$(dirname "$PIPE")"
    rm -f "$PIPE"
    mkfifo "$PIPE"
    echo "[run_exo] Named pipe creata: $PIPE"
    echo "[run_exo]   Usa: echo 'ERR: page fault' > $PIPE" 
    echo "[run_exo]   Oppure: tail -f /var/log/syslog > $PIPE"
fi

# Avvia encoder_inject in ascolto sulla pipe
python3 "${ROOT}/tools/encoder_inject.py" --file "$PIPE" --interval 0.1 --url "http://127.0.0.1:${ENCODER_PORT}/encode" &
INJECT_PID=$!
echo "[run_exo] Encoder inject attivo (PID $INJECT_PID)"

# ── 3. Avvia bridge ────────────────────────────────
echo "[run_exo] Avvio Exo bridge..."
echo "[run_exo] Premi Ctrl+C per fermare tutto."
echo ""
exec python3 "${ROOT}/tools/v2_bridge.py"
