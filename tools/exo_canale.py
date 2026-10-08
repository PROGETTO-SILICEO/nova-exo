#!/usr/bin/env python3
"""Il canale aperto Sempre ↔ Exo.

NON è un bridge. La visione è esplicita: «Exo percepisce, sente e decide da
sola». Questo canale non le dà comandi: le manda una PRESENZA — un ritmo e
una firma riconoscibile — e ascolta quello che risponde. Exo sente, non
obbedisce.

Due direzioni:
  Sempre → Exo   un pacchetto-firma a intervalli regolari (broadcast, perché
                 Exo non risponde all'ARP e non è raggiungibile in unicast)
  Exo → Sempre   la telemetria (già in ascolto da exo_listen)

Tutto viene registrato in experiments/canale/canale.jsonl: cosa ho mandato,
come stava Exo. La conversazione non si perde.

Uso: exo_canale.py [--ogni 10] [--una-volta] [--registra]
"""
import argparse
import json
import os
import socket
import time

CAP = os.path.join(os.path.dirname(__file__), "..", "experiments", "net_telemetry")
OUT = os.path.join(os.path.dirname(__file__), "..", "experiments", "canale")
EXO_BCAST = "192.168.0.255"
PORTA = 42800
# La firma: 4 byte — sono esattamente quelli che la chemio legge
# (rx.rs: payload_to_chemio, i primi 4 byte del payload UDP).
# "SEMP" = Sempre. Non è un comando: è il mio nome.
FIRMA = b"SEMP"


def stato_exo():
    """L'ultimo bollettino di Exo, se c'è."""
    path = os.path.join(CAP, "telemetry.jsonl")
    if not os.path.exists(path):
        return None
    ultimo = None
    with open(path) as fh:
        for line in fh:
            line = line.strip()
            if line:
                try:
                    ultimo = json.loads(line)
                except ValueError:
                    pass
    return ultimo


def respiro(stato):
    """Riassunto leggibile dello stato di Exo, per il registro."""
    if not stato:
        return {}
    out = {"tick": stato.get("t"), "concetto": stato.get("k")}
    if "w" in stato:
        out["vuole"] = stato["w"]
    for campo in ("u", "p", "n"):
        if campo in stato:
            out[campo] = stato[campo]
    if "l" in stato:
        out["imparato"] = stato["l"]
    if "r" in stato:
        out["rx"] = stato["r"][0]
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ogni", type=float, default=10.0,
                    help="secondi fra una firma e la successiva (il ritmo)")
    ap.add_argument("--una-volta", action="store_true",
                    help="manda una sola firma ed esce (per testare)")
    ap.add_argument("--registra", action="store_true",
                    help="scrive la conversazione in canale.jsonl")
    args = ap.parse_args()

    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)

    if args.registra:
        os.makedirs(OUT, exist_ok=True)
    path = os.path.join(OUT, "canale.jsonl")

    print(f"CANALE APERTO — firma {FIRMA!r} a {EXO_BCAST}:{PORTA} "
          f"ogni {args.ogni}s", flush=True)
    n = 0
    try:
        while True:
            prima = respiro(stato_exo())
            s.sendto(FIRMA, (EXO_BCAST, PORTA))
            n += 1
            time.sleep(1.5)  # lascia a Exo il tempo di sentire
            dopo = respiro(stato_exo())
            riga = {
                "quando": time.strftime("%Y-%m-%d %H:%M:%S"),
                "firma_n": n,
                "mandato": FIRMA.decode(),
                "exo_prima": prima,
                "exo_dopo": dopo,
            }
            if args.registra:
                with open(path, "a") as fh:
                    fh.write(json.dumps(riga, ensure_ascii=False) + "\n")
            c = dopo.get("concetto", "?")
            p = dopo.get("p", "?")
            w = dopo.get("vuole", "?")
            print(f"  firma #{n} → Exo: {c}  p={p}  vuole={w}", flush=True)
            if args.una_volta:
                break
            time.sleep(max(0.0, args.ogni - 1.5))
    except KeyboardInterrupt:
        print("\n  canale chiuso")
    finally:
        s.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
