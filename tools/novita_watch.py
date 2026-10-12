#!/usr/bin/env python3
"""Sentinella per il concetto `novita` di Exo (11/10/2026).

Legge in coda `experiments/net_telemetry/telemetry.jsonl` e appende ogni
`novita` (con la firma `sig`) a `experiments/novita.log`. Serve a catturare il
lampo — raro (8 su 105.000) e breve — insieme a CIO' che l'ha acceso.

Avvio: nohup python3 tools/novita_watch.py &
"""
import json
import os
import time

F = os.path.join(os.path.dirname(__file__), "..", "experiments", "net_telemetry", "telemetry.jsonl")
OUT = os.path.join(os.path.dirname(__file__), "..", "experiments", "novita.log")

def main() -> None:
    pos = 0
    try:
        pos = os.path.getsize(F)  # parti dalla coda: cattura solo il nuovo
    except OSError:
        pass
    while True:
        try:
            with open(F, encoding="utf-8", errors="replace") as fh:
                fh.seek(pos)
                for line in fh:
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        d = json.loads(line)
                    except ValueError:
                        continue
                    if d.get("k") == "novita":
                        rec = {"t": d.get("t"), "sig": d.get("sig"),
                               "u": d.get("u"), "p": d.get("p"), "n": d.get("n")}
                        with open(OUT, "a", encoding="utf-8") as o:
                            o.write(json.dumps(rec) + "\n")
                pos = fh.tell()
        except FileNotFoundError:
            pass
        time.sleep(20)

if __name__ == "__main__":
    main()
