#!/usr/bin/env python3
"""Lettore dei SENSI di Exo — il canale di lettura a distanza (18/10/2026).

PERCHÉ esiste: fino a oggi i sensi (udito, tatto, temperatura, mic) esistevano
solo nei PIXEL della GUI sul Lenovo. Per sapere cosa sente Exo bisognava una
FOTO dello schermo: fragile, non ripetibile, non verificabile. Il kernel ora
manda un blocco `sen` nella telemetria UDP; questo strumento lo legge e lo
mostra in forma leggibile.

Come legge (scelta deliberata): di default TAIL del file jsonl che il listener
già scrive — così non contende la porta UDP 42800 a un listener attivo. Con
`--listen` apre un socket UDP proprio (usare solo se nessun listener gira).

Uso:
    python3 tools/exo_sensi.py               # ultimo stato dei sensi, ed esce
    python3 tools/exo_sensi.py --watch       # aggiorna in continuo
    python3 tools/exo_sensi.py --listen      # ascolta la porta invece del file
    python3 tools/exo_sensi.py --raw         # una riga per campo (per script)

Il parsing NON è duplicato: usa `parse_payload` di exo_listen.py, quindi il
contratto telemetria resta UNO (lezione 11/10/2026: il drift ha ucciso la
telemetria per 2,5 giorni in silenzio).
"""

from __future__ import annotations

import argparse
import json
import os
import socket
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from exo_listen import PORT, MAX_DATAGRAM, Scarto, parse_payload  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_FILE = os.path.join(ROOT, "experiments", "net_telemetry", "telemetry.jsonl")

# Nomi e ordine FISSI del blocco `sen` (contratto con src/net.rs::StatoSensi).
# 18 = versione in metallo oggi (senza guasti_mask); 19 = sorgente aggiornato.
CAMPI18 = (
    "n", "riflessi", "guasti",
    "tatto", "chemio", "metabol", "integrat", "udito", "mic",
    "adc", "pin", "gesto",
    "hda_vend", "hda_dev", "temp_c", "via",
    "mem_omeo", "ventola",
)
CAMPI19 = (
    "n", "riflessi", "guasti", "guasti_mask",
    "tatto", "chemio", "metabol", "integrat", "udito", "mic",
    "adc", "pin", "gesto",
    "hda_vend", "hda_dev", "temp_c", "via",
    "mem_omeo", "ventola",
)
CAMPI21 = CAMPI19 + ("stream_on", "lpib")
CAMPI = CAMPI21  # riferimento per il resto del file

# Ordine di REGISTRAZIONE dei canali in src/main.rs (bit della maschera guasti).
CANALI = (
    "udito", "tatto", "dolore", "chemio", "tempo",
    "tatto.esteso", "intero.temperatura",
)


def nomi_guasti(mask: int) -> list[str]:
    """Da maschera di bit a nomi dei canali ciechi."""
    return [nome for i, nome in enumerate(CANALI) if mask & (1 << i)]

# Le vie della temperatura (src/intero.rs).
VIA = {0: "nessuno", 1: "intel", 2: "amd"}

# Il codice audio che ci interessa: Realtek ALC233 (il nostro mic interno).
ALC233 = (0x10EC, 0x0233)


def leggi_sen(pacchetto: dict) -> dict | None:
    """Da pacchetto validato a dict dei sensi. None se il kernel non manda `sen`.

    Sceglie i nomi in base alla lunghezza: 19 (sorgente) o 18 (metallo attuale).
    """
    sen = pacchetto.get("sen")
    if sen is None:
        return None
    campi = {18: CAMPI18, 19: CAMPI19, 21: CAMPI21}.get(len(sen), CAMPI18)
    return dict(zip(campi, sen))


def _fmt_temp(t: float) -> str:
    return "--" if t < 0 else f"{t:.1f} C"


def _fmt_codec(vend: int, dev: int) -> str:
    nome = " (Realtek ALC233)" if (vend, dev) == ALC233 else ""
    return f"{vend:04x}:{dev:04x}{nome}"


def mostra(s: dict, tick: int, quando: str) -> None:
    """Tabella leggibile — pensata per essere letta da Sempre, non da uno script."""
    guasti = s["guasti"]
    testa_guasti = f"\033[31m{int(guasti)}\033[0m" if guasti else str(int(guasti))
    ciechi = nomi_guasti(int(s.get("guasti_mask", 0)))
    dettaglio = f"  ({', '.join(ciechi)})" if ciechi else ""
    print(f"\033[36mExo — sensi\033[0m  tick {tick}   {quando}")
    print(f"  SENSI    {int(s['n'])} canali · {int(s['riflessi'])} riflessi · {testa_guasti} guasti{dettaglio}")
    print(f"  TATTO    {s['tatto']:+.4f}    CHEMIO  {s['chemio']:+.4f}    METABOL {s['metabol']:+.4f}")
    print(f"  INTEGR   {s['integrat']:+.4f}    UDITO   {s['udito']:+.4f}")
    gesto = "GESTO" if s["gesto"] else "-"
    adc = "--" if int(s["adc"]) == 255 else str(int(s["adc"]))
    pin = "--" if int(s["pin"]) == 255 else str(int(s["pin"]))
    print(f"  MIC      {s['mic']:.4f}   adc={adc} pin={pin}   tatto esteso={gesto}")
    print(f"  HDA      {_fmt_codec(int(s['hda_vend']), int(s['hda_dev']))}")
    print(f"  TEMP     {_fmt_temp(s['temp_c'])}  (via={VIA.get(int(s['via']), '?')})")
    ventola = "ON" if s["ventola"] else "OFF"
    print(f"  OMEO     mem={s['mem_omeo'] * 100:.0f}%  ventola={ventola}")
    if "stream_on" in s:
        on = "RUN" if s["stream_on"] else "STOP"
        print(f"  UDITO ST {on}  lpib={int(s['lpib'])}  (lpib che avanza = la DMA porta i campioni)")


def righe_raw(s: dict, tick: int) -> str:
    """Una riga per campo, per analisi a macchina."""
    parti = [f"t={tick}"]
    for k, v in s.items():
        parti.append(f"{k}={v:g}" if isinstance(v, float) else f"{k}={v}")
    return " ".join(parti)


def _pacchetto_da_riga(riga: str) -> dict | None:
    """Accetta sia la riga grezza del produttore sia quella del listener."""
    riga = riga.strip()
    if not riga.startswith("{"):
        return None
    try:
        d = json.loads(riga)
    except json.JSONDecodeError:
        return None
    # il listener aggiunge recv/from: li togliamo prima della validazione
    d.pop("recv", None)
    d.pop("from", None)
    return d


def _valida_o_none(d: dict) -> dict | None:
    """Riusa il parser del contratto. Un pacchetto storto non uccide il lettore."""
    try:
        return parse_payload(json.dumps(d).encode("utf-8"))
    except Scarto:
        return None


def da_file(percorso: str, watch: bool, raw: bool, solo_sen: bool) -> int:
    while not os.path.exists(percorso):
        print(f"[exo-sensi] in attesa del file {percorso} ...", file=sys.stderr)
        time.sleep(2)

    if watch:
        with open(percorso, "rb") as fh:
            fh.seek(0, os.SEEK_END)
            while True:
                riga = fh.readline()
                if not riga:
                    time.sleep(0.5)
                    continue
                d = _pacchetto_da_riga(riga.decode("utf-8", "replace"))
                if d is None:
                    continue
                ok = _valida_o_none(d)
                if ok is None or "sen" not in ok:
                    continue
                os.system("clear")
                _emetti(ok, raw, solo_sen)
        return 0

    # Senza --watch: NON leggiamo 500 MB. Guardiamo solo la coda (64 KiB) e
    # prendiamo l'ultimo pacchetto con `sen`. Così lo strumento è istantaneo.
    with open(percorso, "rb") as fh:
        fh.seek(0, os.SEEK_END)
        size = fh.tell()
        tagliato = size > 65536
        fh.seek(max(0, size - 65536))
        if tagliato:
            fh.readline()  # scarta la riga mutilata dal taglio (solo se ho tagliato)
        ultimo = None
        for riga in fh:
            d = _pacchetto_da_riga(riga.decode("utf-8", "replace"))
            if d is None:
                continue
            ok = _valida_o_none(d)
            if ok is None or "sen" not in ok:
                continue
            ultimo = ok
    if ultimo is None:
        print("[exo-sensi] nessun pacchetto con `sen` nella coda del file.\n"
              "           Il kernel sul metallo manda `sen`? (serve il build "
              "col blocco sensi)", file=sys.stderr)
        return 1
    _emetti(ultimo, raw, solo_sen)
    return 0


def _emetti(pacchetto: dict, raw: bool, solo_sen: bool) -> None:
    s = leggi_sen(pacchetto)
    if s is None:
        return
    quando = time.strftime("%H:%M:%S")
    if raw:
        print(righe_raw(s, pacchetto.get("t", 0)))
    else:
        mostra(s, pacchetto.get("t", 0), quando)


def da_socket(watch: bool, raw: bool, solo_sen: bool) -> int:
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, True)
    try:
        sock.bind(("0.0.0.0", PORT))
    except OSError as e:
        print(f"[exo-sensi] porta {PORT} occupata: {e}\n"
              f"           un listener è già attivo → usa la modalità file "
              f"(senza --listen).", file=sys.stderr)
        return 3
    print(f"[exo-sensi] in ascolto su UDP 0.0.0.0:{PORT}", file=sys.stderr)
    sock.settimeout(5.0)
    while True:
        try:
            dato, _ = sock.recvfrom(MAX_DATAGRAM)
        except socket.timeout:
            if not watch:
                print("[exo-sensi] nessun pacchetto entro il timeout", file=sys.stderr)
                return 4
            continue
        d = _pacchetto_da_riga(dato.decode("utf-8", "replace"))
        if d is None:
            continue
        ok = _valida_o_none(d)
        if ok is None or "sen" not in ok:
            continue
        if watch:
            os.system("clear")
        _emetti(ok, raw, solo_sen)
        if not watch:
            return 0
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="Lettore dei sensi di Exo")
    ap.add_argument("--file", default=DEFAULT_FILE, help="jsonl del listener")
    ap.add_argument("--listen", action="store_true",
                    help="ascolta la porta UDP invece di leggere il file")
    ap.add_argument("--watch", action="store_true", help="aggiorna in continuo")
    ap.add_argument("--raw", action="store_true", help="una riga per campo")
    args = ap.parse_args()

    if args.listen:
        return da_socket(args.watch, args.raw, args.raw)
    return da_file(args.file, args.watch, args.raw, args.raw)


if __name__ == "__main__":
    raise SystemExit(main())
