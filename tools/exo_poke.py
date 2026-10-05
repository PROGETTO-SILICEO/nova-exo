#!/usr/bin/env python3
"""Bussa alla porta d'ingresso di Exo: manda un broadcast sulla rete locale.

Perché esiste (05/10/2026): Exo non ha stack IP. L'unico modo di fargli
arrivare qualcosa è mettere un frame sul cavo, e la sua RTL810xE lo riceve
se è broadcast (lo accetta per config: RCR_AB). Da qui si verifica che l'RX
della Realtek funzioni davvero — cosa che QEMU non può dire, perché non
emula il chip.

NON serve nessun privilegio: un socket UDP con SO_BROADCAST basta, e il
frame che parte è Ethernet(broadcast) + IPv4 + UDP + payload. La porta
d'ingresso di Exo legge i byte all'offset 14 (inizio dell'header IP), quindi
per il primo test va bene anche così; per pilotare byte precisi servirebbe
un frame grezzo (CAP_NET_RAW).

Uso:
    python3 tools/exo_poke.py                 # una raffica di default
    python3 tools/exo_poke.py --n 20 --delay 0.2
    python3 tools/exo_poke.py --bytes 255,128,64,0
"""

from __future__ import annotations

import argparse
import socket
import sys
import time

BROADCAST = "192.168.0.255"
PORT = 42800


def bussa(quattro: bytes, n: int, delay: float, porta: int, verbose: bool,
          pad: int = 0) -> int:
    """`pad` = lunghezza TOTALE del payload UDP. Serve perché un frame
    Ethernet sotto i 60 byte è un RUNT: se la RxConfig non ha AcceptRunt
    (0x10), il chip lo scarta. Con 4 byte il frame è 46 byte -> rifiutato.
    Portandolo a 64 il frame è 106 byte e passa."""
    payload = quattro + bytes(max(0, pad - len(quattro)))
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
    inviati = 0
    try:
        for i in range(n):
            try:
                s.sendto(payload, (BROADCAST, porta))
                inviati += 1
            except OSError as e:
                print(f"[exo-poke] errore: {e}", file=sys.stderr)
                break
            if verbose:
                print(f"[exo-poke] {i + 1}/{n} -> {BROADCAST}:{porta} {quattro.hex()} "
                      f"(frame {14 + 20 + 8 + len(payload)} byte)")
            if delay:
                time.sleep(delay)
    finally:
        s.close()
    print(f"[exo-poke] inviati {inviati} pacchetti broadcast")
    return 0 if inviati else 1


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description="Bussa alla porta d'ingresso di Exo")
    ap.add_argument("--bytes", default="255,0,255,0",
                    help="4 byte di payload, separati da virgola")
    ap.add_argument("--n", type=int, default=10)
    ap.add_argument("--delay", type=float, default=0.3)
    ap.add_argument("--port", type=int, default=PORT)
    ap.add_argument("--pad", type=int, default=64,
                    help="lunghezza totale del payload UDP (min 60-byte frame: "
                         "con 4 byte il frame e' un runt e il chip lo scarta)")
    ap.add_argument("--verbose", action="store_true")
    a = ap.parse_args(argv)
    try:
        vals = [int(v) & 0xFF for v in a.bytes.split(",")]
    except ValueError:
        print("[exo-poke] --bytes vuole interi 0..255 separati da virgola",
              file=sys.stderr)
        return 2
    if len(vals) != 4:
        print(f"[exo-poke] servono esattamente 4 byte, ne ho {len(vals)}",
              file=sys.stderr)
        return 2
    return bussa(bytes(vals), a.n, a.delay, a.port, a.verbose, a.pad)


if __name__ == "__main__":
    sys.exit(main())
