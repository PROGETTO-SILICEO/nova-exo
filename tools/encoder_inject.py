#!/usr/bin/env python3
"""
encoder_inject.py
=================
Prende testo (da stdin, argomento, o file), lo codifica via ExoChemio Encoder Server,
e invia i 4 valori a Exo via seriale.

Flusso: testo → encoder_server (HTTP) → [c, u, p, n] → seriale → Exo (Chemio)

Uso:
  echo "ERR: page fault" | python3 encoder_inject.py
  python3 encoder_inject.py "ERR: page fault"
  python3 encoder_inject.py --file events.txt --interval 1.0
  python3 encoder_inject.py --listen /dev/ttyUSB0  # ascolta seriale esterna
"""

import argparse
import json
import sys
import time
import socket
import os
from pathlib import Path

# ── Config ─────────────────────────────────────────────────────────────
ENCODER_URL = "http://127.0.0.1:5006/encode"
SERIAL_PORT = None  # auto-detect: /tmp/exo-serial o stdin→QEMU
DEFAULT_SERIAL = "/tmp/exo-serial"  # pipe per QEMU seriale in

# ── Encoder client ────────────────────────────────────────────────────
def encode_text(text: str, url: str = ENCODER_URL) -> list[float] | None:
    """Invia testo all'encoder server e restituisce [c, u, p, n]."""
    import urllib.request
    import urllib.error

    data = json.dumps({"text": text.strip()}).encode()
    req = urllib.request.Request(url, data=data,
                                 headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=5) as resp:
            result = json.loads(resp.read())
            return result["values"]
    except (urllib.error.URLError, urllib.error.HTTPError, ConnectionError) as e:
        print(f"[encoder_inject] ERRORE encoder: {e}", file=sys.stderr)
        return None


# ── Serial writer ─────────────────────────────────────────────────────
def send_to_exo(values: list[float], port: str = None):
    """Invia 4 float a Exo via seriale (pipe QEMU)."""
    line = ",".join(f"{v:.4f}" for v in values) + "\n"
    # Se c'è una porta seriale specifica, usa quella
    if port and os.path.exists(port):
        with open(port, "w") as f:
            f.write(line)
            f.flush()
        return
    
    # Altrimenti, usa stdout (se siamo in pipe verso QEMU)
    sys.stdout.write(line)
    sys.stdout.flush()


# ── Listen mode (seriale esterna → encoder → Exo) ─────────────────────
def listen_serial(device: str, baud: int = 115200):
    """Ascolta una seriale esterna, codifica ogni linea, invia a Exo."""
    import serial
    print(f"[encoder_inject] Ascolto su {device} @ {baud} baud", file=sys.stderr)
    ser = serial.Serial(device, baud, timeout=1)
    buffer = ""
    while True:
        data = ser.read(1024).decode("utf-8", errors="replace")
        if not data:
            continue
        buffer += data
        while "\n" in buffer:
            line, buffer = buffer.split("\n", 1)
            line = line.strip()
            if not line:
                continue
            print(f"[encoder_inject] << {line}", file=sys.stderr)
            values = encode_text(line)
            if values:
                send_to_exo(values)
                print(f"[encoder_inject] >> {values}", file=sys.stderr)


# ── File watch mode ────────────────────────────────────────────────────
def watch_file(path: str, interval: float = 0.5):
    """Legge un file riga per riga e invia ogni linea codificata."""
    print(f"[encoder_inject] Watching {path} (interval={interval}s)", file=sys.stderr)
    with open(path, "r") as f:
        # Go to end
        f.seek(0, 2)
        while True:
            line = f.readline()
            if line:
                line = line.strip()
                if line:
                    print(f"[encoder_inject] << {line}", file=sys.stderr)
                    values = encode_text(line)
                    if values:
                        send_to_exo(values)
                        print(f"[encoder_inject] >> {values}", file=sys.stderr)
            else:
                time.sleep(interval)


# ── Main ───────────────────────────────────────────────────────────────
def main():
    global ENCODER_URL
    parser = argparse.ArgumentParser(
        description="Inietta testo codificato come input Chemio a Exo")
    
    # Input source
    group = parser.add_mutually_exclusive_group()
    group.add_argument("text", nargs="?", help="Testo da codificare (argomento)")
    group.add_argument("--file", "-f", help="Leggi da file (tail -f)")
    group.add_argument("--listen", "-l", help="Ascolta seriale esterna (es. /dev/ttyUSB0)")
    
    parser.add_argument("--interval", "-i", type=float, default=0.5,
                        help="Intervallo polling file (default: 0.5s)")
    parser.add_argument("--baud", type=int, default=115200,
                        help="Baud rate seriale (default: 115200)")
    parser.add_argument("--port", "-p", default=None,
                        help="Porta seriale Exo (default: stdout)")
    parser.add_argument("--url", default=ENCODER_URL,
                        help=f"URL encoder server (default: {ENCODER_URL})")
    parser.add_argument("--stdin", action="store_true",
                        help="Leggi da stdin (riga per riga)")
    
    args = parser.parse_args()
    
    # Aggiorna ENCODER_URL
    ENCODER_URL = args.url
    
    # Modalità: listen seriale
    if args.listen:
        listen_serial(args.listen, args.baud)
        return
    
    # Modalità: watch file
    if args.file:
        watch_file(args.file, args.interval)
        return
    
    # Modalità: stdin interattivo
    if args.stdin or (not args.text and not sys.stdin.isatty()):
        print("[encoder_inject] Leggo da stdin riga per riga...", file=sys.stderr)
        for line in sys.stdin:
            line = line.strip()
            if not line:
                continue
            values = encode_text(line, args.url)
            if values:
                send_to_exo(values, args.port)
                print(f"[encoder_inject] >> {values}", file=sys.stderr)
        return
    
    # Modalità: testo singolo da argomento
    if args.text:
        values = encode_text(args.text, args.url)
        if values:
            send_to_exo(values, args.port)
            print(f"[encoder_inject] >> {values}", file=sys.stderr)
            print(f"  Interpretazione: {interpret(values)}", file=sys.stderr)
        return
    
    parser.print_help()


def interpret(values: list[float]) -> str:
    ctx = ["ERR", "NEU", "VIT"][int(max(-1, min(1, values[0])) + 1)]
    urg = "CRIT" if values[1] > 0.5 else ("WARN" if values[1] > 0 else "norm")
    pol = ["NEG", "NEU", "POS"][int(max(-1, min(1, values[2])) + 1)]
    nov = "NUOVO" if values[3] > 0.3 else ("insolito" if values[3] > 0 else "fam")
    return f"[{ctx}|{urg}|{pol}|{nov}]"


if __name__ == "__main__":
    main()
