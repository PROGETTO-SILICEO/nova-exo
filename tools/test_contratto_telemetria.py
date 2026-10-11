#!/usr/bin/env python3
"""Canary del contratto telemetria Exo (lezione 11/10/2026).

Il formato del pacchetto telemetria è descritto in DUE posti:
  - produttore  : src/net.rs::build_payload   (lato Exo, bare metal)
  - consumatore : tools/exo_listen.py         (lato guardiano)

L'08/10/2026 il produttore ha aggiunto le chiavi `u,p,n` (chemio interpretata)
e il consumatore non le ammetteva: ogni pacchetto veniva SCARTATO in silenzio,
telemetria persa per 2,5 giorni. Questo test confronta le chiavi emesse da
net.rs con quelle ammesse dal listener e fallisce se divergono — così il drift
si scopre sul banco, non sul metallo.

Esce 0 se allineato, 1 se c'è drift, 2 se non riesce a leggere la sorgente.
"""

from __future__ import annotations

import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from exo_listen import KEYS_AMMESSE  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
NET = os.path.join(ROOT, "src", "net.rs")

# Le chiavi sono emesse dentro stringhe Rust come  \"chiave\":  → nel file
# sorgente compaiono come  \" t \" :  (backslash-quote chiave backslash-quote).
RE_KEY = re.compile(r'\\"([a-z]{1,4})\\"')


def chiavi_emesse(sorgente: str) -> set[str]:
    return set(RE_KEY.findall(sorgente))


def main() -> int:
    try:
        with open(NET, encoding="utf-8") as fh:
            src = fh.read()
    except OSError as e:
        print(f"non riesco a leggere {NET}: {e}", file=sys.stderr)
        return 2

    emesse = chiavi_emesse(src)
    if not emesse:
        print(f"nessuna chiave trovata in {NET}: il pattern è cambiato?",
              file=sys.stderr)
        return 2

    mancanti = emesse - KEYS_AMMESSE
    print(f"net.rs emette . . : {sorted(emesse)}")
    print(f"listener ammette . : {sorted(KEYS_AMMESSE)}")
    if mancanti:
        print(f"DRIFT: net.rs emette chiavi che il listener RIFIUTA: "
              f"{sorted(mancanti)} — aggiungile a KEYS_AMMESSE in exo_listen.py",
              file=sys.stderr)
        return 1
    print("OK: contratto telemetria allineato")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
