#!/usr/bin/env python3
"""Ricevitore della telemetria UDP di Exo.

Perché esiste (05/10/2026): il `concept` e `imparato`/`esplorato` di Exo vivono
solo sulla seriale. Questo riceve su rete i pacchetti che Exo manda col modulo
`src/net.rs`, così Sempre li legge senza che Alfonso debba trascrivere nulla.

Perché UDP e non cattura di frame grezzi: il processo di Sempre gira in sandbox
`no_new_privs` (utility process di Electron), quindi AF_PACKET non è
concesso. Un socket UDP su porta >1024 invece si apre senza privilegi.

REGOLA DI SICUREZZA: questo è un canale di INPUT dall'esterno verso di me.
Il parsing è quindi deliberatamente diffidente:
  - niente eval, niente pickle, niente import dinamici;
  - lunghezza massima del datagramma;
  - `json.loads` (non esegue codice);
  - validazione esplicita di tipi, chiavi e limiti numerici;
  - un pacchetto malformato NON solleva: si conta e si scarta.

Uso:
    python3 tools/exo_listen.py                 # ascolta e stampa
    python3 tools/exo_listen.py --once          # esce al primo pacchetto valido
    python3 tools/exo_listen.py --seconds 60    # esce dopo N secondi
"""

from __future__ import annotations

import argparse
import json
import os
import socket
import sys
import time

# ── Contratto con src/net.rs ────────────────────────────────────────────────
PORT = 42800
MAX_DATAGRAM = 2048          # il frame massimo è 1518; margine abbondante
MAX_CELLS = 4
MAX_NEURONS = 16             # NEURONS_PER_CELL in src/cfc.rs
MAX_CONCEPT_LEN = 16
CONCEPTS = ("errore", "vita", "riposo", "novita")

DEFAULT_OUT = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
    "experiments", "net_telemetry", "telemetry.jsonl",
)


class Scarto(Exception):
    """Pacchetto scartato, con il motivo. Mai un crash."""


def _num(v, lo, hi, nome):
    if isinstance(v, bool) or not isinstance(v, (int, float)):
        raise Scarto(f"{nome}: non numerico ({type(v).__name__})")
    if v != v:  # NaN
        raise Scarto(f"{nome}: NaN")
    if not (lo <= v <= hi):
        raise Scarto(f"{nome}: fuori intervallo ({v})")
    return v


def parse_payload(raw: bytes) -> dict:
    """Da datagramma a dict validato. Solleva `Scarto` se non è conforme."""
    if len(raw) == 0:
        raise Scarto("datagramma vuoto")
    if len(raw) > MAX_DATAGRAM:
        raise Scarto(f"datagramma troppo lungo ({len(raw)} byte)")
    try:
        testo = raw.decode("utf-8")
    except UnicodeDecodeError as e:
        raise Scarto(f"non utf-8: {e}") from None
    if len(testo) > MAX_DATAGRAM:
        raise Scarto("testo troppo lungo")
    try:
        d = json.loads(testo)
    except (json.JSONDecodeError, RecursionError) as e:
        raise Scarto(f"json non valido: {e}") from None
    if not isinstance(d, dict):
        raise Scarto("radice non è un oggetto")

    fuori = set(d) - {"t", "k", "c", "w", "l", "r", "v"}
    if fuori:
        raise Scarto(f"chiavi inattese: {sorted(fuori)}")

    t = _num(d.get("t"), 0, 2**63, "t")
    k = d.get("k")
    if not isinstance(k, str):
        raise Scarto("k: non è una stringa")
    if len(k) > MAX_CONCEPT_LEN:
        raise Scarto(f"k: troppo lungo ({len(k)})")

    c = d.get("c")
    if not isinstance(c, list) or len(c) != MAX_CELLS:
        raise Scarto(f"c: attese {MAX_CELLS} celle, trovate "
                     f"{len(c) if isinstance(c, list) else 'n/a'}")
    celle = []
    for i, cella in enumerate(c):
        if not isinstance(cella, list) or len(cella) != MAX_NEURONS:
            raise Scarto(f"c[{i}]: attesi {MAX_NEURONS} valori")
        celle.append([_num(v, -1e6, 1e6, f"c[{i}]") for v in cella])

    w = d.get("w")
    if not isinstance(w, list) or len(w) != 2:
        raise Scarto("w: attesi [id, intensità]")
    wid = _num(w[0], 0, 255, "w[0]")
    wint = _num(w[1], -1e6, 1e6, "w[1]")

    l = d.get("l")
    if not isinstance(l, list) or len(l) != 2:
        raise Scarto("l: attesi [imparato, esplorato]")
    limparato = _num(l[0], 0, 2**63, "l[0]")
    lesplorato = _num(l[1], 0, 2**63, "l[1]")

    # Diagnostica RX. OPZIONALE: i kernel fino al 05/10 non la mandano.
    # [pkts, errs, cfg, max, cmd, own_mask, intr, overflows]
    r = d.get("r")
    diag = None
    if r is not None:
        if not isinstance(r, list) or len(r) != 8:
            raise Scarto("r: attesi 8 valori di diagnostica")
        diag = [_num(v, 0, 2**32, f"r[{i}]") for i, v in enumerate(r)]

    out = {
        "t": int(t),
        "k": k,
        "c": celle,
        "w": [int(wid), float(wint)],
        "l": [int(limparato), int(lesplorato)],
    }
    # Epoca di build: dice QUALE kernel sta parlando. Opzionale (i kernel
    # precedenti non la mandano).
    v = d.get("v")
    if v is not None:
        out["v"] = int(_num(v, 0, 2**32, "v"))
    if diag is not None:
        out["r"] = [int(v) for v in diag]
    return out


def formatta(r: dict) -> str:
    """Riga leggibile: il concept, il chemio (i 4 valori che contano) e i contatori."""
    chemio = r["c"][1][:4] if len(r["c"]) > 1 else []
    ch = ",".join(f"{v:.4f}" for v in chemio)
    rx = ""
    if "r" in r:
        p, e, cfg, mx, cmd, own, intr, ovf = r["r"]
        rx = (f" rx_pkts={p} rx_errs={e} rx_cfg=0x{cfg:04x} rx_max={mx}"
              f" cmd=0x{cmd:02x} own=0x{own:x} intr=0x{intr:04x} ovf={ovf}")
    ep = f" [epoca {r['v']}]" if "v" in r else ""
    return (f"t={r['t']:<9} concept={r['k']:<8} chemio=[{ch}] "
            f"w={r['w']} imparato={r['l'][0]} esplorato={r['l'][1]}{rx}{ep}")


def ascolta(port: int, out: str | None, once: bool, seconds: float | None,
            verbose: bool) -> int:
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    # SO_BROADCAST serve per RICEVERE broadcast su alcune piattaforme.
    try:
        s.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
    except OSError:
        pass
    s.bind(("0.0.0.0", port))
    s.settimeout(1.0)

    fh = None
    if out:
        os.makedirs(os.path.dirname(out), exist_ok=True)
        fh = open(out, "a", encoding="utf-8")

    inizio = time.monotonic()
    buoni = scartati = 0
    print(f"[exo-listen] in ascolto su UDP 0.0.0.0:{port}", flush=True)
    if out:
        print(f"[exo-listen] scrivo su {out}", flush=True)

    try:
        while True:
            if seconds is not None and time.monotonic() - inizio >= seconds:
                break
            try:
                raw, addr = s.recvfrom(MAX_DATAGRAM + 1)
            except socket.timeout:
                continue
            except OSError as e:
                print(f"[exo-listen] errore di socket: {e}", file=sys.stderr,
                      flush=True)
                break
            try:
                r = parse_payload(raw)
            except Scarto as e:
                scartati += 1
                if verbose:
                    print(f"[exo-listen] scartato da {addr[0]}: {e}", flush=True)
                continue
            buoni += 1
            print(formatta(r), flush=True)
            if fh:
                fh.write(json.dumps({"recv": time.time(), "from": addr[0], **r})
                         + "\n")
                fh.flush()
            if once:
                break
    finally:
        if fh:
            fh.close()
        s.close()
    print(f"[exo-listen] fine: {buoni} validi, {scartati} scartati", flush=True)
    return 0 if buoni > 0 or once is False else 1


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description="Ricevitore telemetria UDP di Exo")
    ap.add_argument("--port", type=int, default=PORT)
    ap.add_argument("--out", default=DEFAULT_OUT,
                    help="file JSONL di destinazione ('' per non scrivere)")
    ap.add_argument("--once", action="store_true", help="esci al primo valido")
    ap.add_argument("--seconds", type=float, default=None)
    ap.add_argument("--verbose", action="store_true",
                    help="stampa anche i pacchetti scartati")
    a = ap.parse_args(argv)
    return ascolta(a.port, a.out or None, a.once, a.seconds, a.verbose)


if __name__ == "__main__":
    sys.exit(main())
