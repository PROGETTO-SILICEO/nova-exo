#!/usr/bin/env python3
"""Guardiano del concetto: afferra il momento in cui Exo esce da `riposo`.

Perché esiste (07/10/2026): su 4059 pacchetti il concept non è MAI cambiato —
`riposo` per tutto. Quando finalmente cambierà, vogliamo non solo *che* è
cambiato ma *cosa c'era nei due minuti prima*. Perciò tengo un anello di
ultimi N pacchetti e allo scatto scarico tutto l'antefatto.

Input : il JSONL scritto da exo_listen.py (chiavi: recv,from,t,k,c,w,l,r,v)
Output: concept_events.jsonl  (evento completo + antefatto, macchina)
        concept_events.log    (riga umana, per leggerlo in un colpo d'occhio)
        avviso Telegram        (opzionale, con rate limit)

Non tocca mai il log di telemetria: è in sola lettura.

Uso:
    python3 tools/exo_concept_watch.py                     # sul log vero
    python3 tools/exo_concept_watch.py --file /tmp/x.jsonl # per i test
    python3 tools/exo_concept_watch.py --no-notify         # silenzioso
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_LOG = os.path.join(REPO, "experiments/net_telemetry/telemetry.jsonl")
# (default: nella cartella vera; con --file di test, accanto a quel file)
AVVISO = os.path.expanduser("~/Documenti/GitHub/opencode-identity/scripts/avviso.sh")

LEADUP = 120          # pacchetti tenuti in antefatto (~2 min a 0,91 s)
NOTIFY_GAP = 60       # max 1 avviso Telegram al minuto


def event_paths(logpath: str) -> tuple[str, str]:
    base = os.path.dirname(os.path.abspath(logpath))
    return (os.path.join(base, "concept_events.jsonl"),
            os.path.join(base, "concept_events.log"))


def follow(path: str):
    """Legge il file dal fondo, come `tail -f` (tollerante a righe parziali)."""
    with open(path, "r", encoding="utf-8") as fh:
        fh.seek(0, os.SEEK_END)
        buf = ""
        while True:
            chunk = fh.read()
            if not chunk:
                time.sleep(0.5)
                continue
            buf += chunk
            *lines, buf = buf.split("\n")
            for ln in lines:
                ln = ln.strip()
                if ln:
                    yield ln


def fmt_pkt(r: dict) -> str:
    c = r.get("c", [])
    chemio = c[1][:4] if len(c) > 1 else []
    l = r.get("l", [0, 0])
    rr = r.get("r", [])
    diag = ""
    if len(rr) >= 8:
        diag = f" rx={rr[0]} err={rr[1]} ovf={rr[7]}"
    return (f"t={r.get('t')} concept={r.get('k')} chemio={[round(x,3) for x in chemio]} "
            f"imparato={l[0]} esplorato={l[1]}{diag}")


def notify(text: str) -> None:
    try:
        subprocess.run([AVVISO, "--testo", text], timeout=30,
                       check=False, capture_output=True)
    except Exception as e:  # il canale non deve mai far morire il guardiano
        print(f"[concept-watch] avviso fallito: {e}", file=sys.stderr)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--file", default=DEFAULT_LOG)
    ap.add_argument("--no-notify", action="store_true")
    ap.add_argument("--leadup", type=int, default=LEADUP)
    args = ap.parse_args()

    # baseline: l'ultimo concept già noto — al via NON scatto
    try:
        last = None
        with open(args.file, "r", encoding="utf-8") as fh:
            for ln in fh:
                ln = ln.strip()
                if ln:
                    try:
                        last = json.loads(ln)
                    except json.JSONDecodeError:
                        pass
        baseline = last.get("k") if last else "riposo"
    except FileNotFoundError:
        baseline = "riposo"
    print(f"[concept-watch] seguo {args.file}  baseline={baseline}", flush=True)

    # pre-carico l'anello con la coda già presente: al primo scatto
    # l'antefatto deve esserci già, non vuoto
    ring: list[dict] = []
    try:
        with open(args.file, "r", encoding="utf-8") as fh:
            tail = fh.readlines()[-args.leadup:]
        for ln in tail:
            ln = ln.strip()
            if ln:
                try:
                    ring.append(json.loads(ln))
                except json.JSONDecodeError:
                    pass
    except FileNotFoundError:
        pass
    cur = baseline
    last_notify = 0.0
    events = 0

    for raw in follow(args.file):
        try:
            r = json.loads(raw)
        except json.JSONDecodeError:
            continue
        ring.append(r)
        if len(ring) > args.leadup:
            ring.pop(0)

        k = r.get("k")
        if k is None or k == cur:
            continue

        # ── LO SCATTO ──
        old, cur = cur, k
        events += 1
        event = {
            "data": time.strftime("%Y-%m-%d %H:%M:%S"),
            "da": old, "a": k,
            "t_scatto": r.get("t"),
            "tick_delta": (r.get("t", 0) - (ring[0].get("t", 0) if ring else 0)),
            "scatto": r,
            "antefatto": list(ring[:-1]),
        }
        ev_jsonl, ev_log = event_paths(args.file)
        os.makedirs(os.path.dirname(ev_jsonl), exist_ok=True)
        with open(ev_jsonl, "a", encoding="utf-8") as fh:
            fh.write(json.dumps(event, ensure_ascii=False) + "\n")

        head = (f"\n===== CONCEPT {old} -> {k}  alle {event['data']}  "
                f"t={r.get('t')}  ({len(ring)-1} pacchetti di antefatto) =====\n")
        with open(ev_log, "a", encoding="utf-8") as fh:
            fh.write(head)
            for p in ring:
                fh.write(("  > " if p is r else "    ") + fmt_pkt(p) + "\n")
        print(head, end="", flush=True)
        for p in ring[-3:]:
            print(("  > " if p is r else "    ") + fmt_pkt(p), flush=True)

        now = time.time()
        if not args.no_notify and now - last_notify >= NOTIFY_GAP:
            last_notify = now
            notify(f"Exo: concept {old} → {k} (t={r.get('t')}) — "
                   f"vedi concept_events.log")

    return 0


if __name__ == "__main__":
    sys.exit(main())
