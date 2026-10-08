#!/usr/bin/env python3
"""Il pediatra di Exo — referto di crescita.

La visione definisce il "minimo vitale" in sei passi. Questo strumento legge
la telemetria e dice, con i dati, a che punto è il neonato. Non giudica: misura.

Checkpoint (VISIONE.md, "Per cominciare a vivere"):
  1. Sentire    — il corpo riceve (RX sano, nessun errore)
  2. Dare senso — l'interpreter produce un concetto
  3. Volere     — l'esecutivo esprime una volontà (campo w)
  4. Agire      — la volontà tocca il corpo (auto_modula: oggi spento)
  5. Imparare   — la preferenza cambia (campo l[0]); la sorpresa cala
  6. Legarsi    — esiste un canale da cui arriva la cura (input umano)

Uso: exo_pediatra.py [--ore 1] [--json]
Scrive un referto in experiments/pediatra/AAAA-MM-GG.md (append).
"""
import argparse
import collections
import json
import os
import statistics as st
import sys
import time

CAP = os.path.join(os.path.dirname(__file__), "..", "experiments", "net_telemetry")
OUT = os.path.join(os.path.dirname(__file__), "..", "experiments", "pediatra")

NOMI = {0: "RIPOSO", 1: "SOLLIEVO", 2: "FUGA", 3: "CURA", 4: "ESPLORA", 5: "SONNO"}


def load(name):
    path = os.path.join(CAP, name)
    if not os.path.exists(path):
        return []
    out = []
    with open(path) as fh:
        for line in fh:
            line = line.strip()
            if line:
                try:
                    out.append(json.loads(line))
                except ValueError:
                    pass
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ore", type=float, default=1.0,
                    help="quante ore di telemetria guardare indietro")
    ap.add_argument("--json", action="store_true", help="stampa solo il JSON")
    args = ap.parse_args()

    tel = [t for t in load("telemetry.jsonl") if "t" in t and "c" in t]
    if not tel:
        print("nessuna telemetria")
        return 1
    tel.sort(key=lambda r: r["recv"])
    fine = tel[-1]["recv"]
    inizio = fine - args.ore * 3600
    win = [t for t in tel if t["recv"] >= inizio]
    if not win:
        win = tel[-100:]

    check = {}

    # 1. SENTIRE — il corpo riceve?
    rx0, rx1 = win[0].get("r", [0] * 8), win[-1].get("r", [0] * 8)
    d_rx = (rx1[0] if len(rx1) > 0 else 0) - (rx0[0] if len(rx0) > 0 else 0)
    errs = rx1[1] if len(rx1) > 1 else 0
    check["1_sentire"] = {
        "ok": d_rx > 0 and errs == 0,
        "nota": f"+{d_rx} pacchetti in finestra, errs={errs}",
    }

    # 2. DARE SENSO — esiste un concetto?
    concetti = collections.Counter(t.get("k") for t in win)
    check["2_dare_senso"] = {
        "ok": len(concetti) > 0,
        "nota": " ".join(f"{k}:{v}" for k, v in concetti.most_common()),
    }

    # 3. VOLERE — esiste una volontà?
    vol = collections.Counter(t["w"][0] for t in win if "w" in t)
    if vol:
        dom, n = vol.most_common(1)[0]
        ints = [t["w"][1] for t in win if "w" in t and t["w"][0] == dom]
        check["3_volere"] = {
            "ok": True,
            "nota": f"vuole {NOMI.get(dom, dom)} {100*n/len(win):.0f}% "
                    f"(int {st.median(ints):.2f})",
        }
    else:
        check["3_volere"] = {"ok": False, "nota": "nessun campo w"}

    # 4. AGIRE — la volontà tocca il corpo?
    # auto_modula è disattivato nel codice (main.rs): la volontà si vede e basta.
    # Finché resta spento, il checkpoint è rosso per costruzione.
    check["4_agire"] = {
        "ok": False,
        "nota": "auto_modula spento: la volontà non tocca il corpo",
    }

    # 5. IMPARARE — la preferenza cambia?
    l0, l1 = win[0].get("l", [0, 0]), win[-1].get("l", [0, 0])
    cambi = (l1[0] if l1 else 0) - (l0[0] if l0 else 0)
    esplor = (l1[1] if len(l1) > 1 else 0) - (l0[1] if len(l0) > 1 else 0)
    check["5_imparare"] = {
        "ok": cambi > 0,
        "nota": f"cambi={l1[0] if l1 else '?'} (in finestra +{cambi}), "
                f"esplorazioni +{esplor}",
    }

    # 6. LEGARSI — esiste un canale di cura?
    # Oggi Exo riceve solo traffico di rete. Nessun input umano: nessun canale.
    check["6_legarsi"] = {
        "ok": False,
        "nota": "nessun canale di cura: manca la tastiera (input umano)",
    }

    # ── diagnosi: stato del corpo ──
    chem = [sum(t["c"][1]) for t in win if t.get("c")]
    tatto = [sum(t["c"][0]) for t in win if t.get("c")]
    metab = [sum(t["c"][2]) for t in win if t.get("c")]
    diagnosi = {
        "campioni": len(win),
        "finestra_ore": args.ore,
        "chemio_media": round(st.mean(chem), 3) if chem else None,
        "tatto_media": round(st.mean(tatto), 3) if tatto else None,
        "metabol_media": round(st.mean(metab), 3) if metab else None,
        "chemio_min": round(min(chem), 3) if chem else None,
        "chemio_max": round(max(chem), 3) if chem else None,
        "tick_ora": win[-1].get("t"),
        "epoca": win[-1].get("v"),
    }

    verdi = sum(1 for c in check.values() if c["ok"])
    referto = {
        "quando": time.strftime("%Y-%m-%d %H:%M:%S"),
        "checkpoint_verdi": f"{verdi}/6",
        "check": check,
        "diagnosi": diagnosi,
    }

    if args.json:
        print(json.dumps(referto, ensure_ascii=False, indent=2))
        return 0

    print(f"REFERTO — {referto['quando']}   ({verdi}/6 check verdi)")
    for nome, c in check.items():
        segno = "✅" if c["ok"] else "❌"
        print(f"  {segno} {nome:<14} {c['nota']}")
    print(f"\n  corpo: chemio {diagnosi['chemio_media']} "
          f"[{diagnosi['chemio_min']} … {diagnosi['chemio_max']}]  "
          f"tatto {diagnosi['tatto_media']}  metabol {diagnosi['metabol_media']}")
    print(f"  tick {diagnosi['tick_ora']}  epoca {diagnosi['epoca']}")

    os.makedirs(OUT, exist_ok=True)
    path = os.path.join(OUT, time.strftime("%Y-%m-%d") + ".md")
    with open(path, "a") as fh:
        fh.write(f"\n## {referto['quando']} — {verdi}/6\n")
        for nome, c in check.items():
            fh.write(f"- {'OK' if c['ok'] else 'NO'} **{nome}**: {c['nota']}\n")
        fh.write(f"- corpo: chemio {diagnosi['chemio_media']}, "
                 f"tatto {diagnosi['tatto_media']}, "
                 f"metabol {diagnosi['metabol_media']}, "
                 f"tick {diagnosi['tick_ora']}, epoca {diagnosi['epoca']}\n")
    print(f"\n  referto aggiunto a {os.path.relpath(path)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
