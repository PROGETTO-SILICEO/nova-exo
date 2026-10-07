#!/usr/bin/env python3
"""Pool sulla TELEMETRIA di Exo (non sui pacchetti di rete).

Per ogni evento di concetto: le 119 telemetrie di antefatto contengono
`c` = 4x16 celle (tatto / chemio / metabol / integrat). Si confronta la
media delle ultime ~2 s (lo scatto) con quella dei ~100 s precedenti,
e lo stesso con finestre "calme" prese da telemetry.jsonl lontano da
ogni evento.

Perché pool: un singolo evento ha 3 campioni nello scatto. Con N eventi
il numero cresce e la perdita della cattura di rete non c'entra nulla —
qui si legge ciò che Exo stessa ha spedito.
"""
import argparse
import json
import os
import statistics as st

CAP = os.path.join(os.path.dirname(__file__), "..", "experiments", "net_telemetry")

TESSUTI = ("tatto", "chemio", "metabol", "integrat")


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


def cells(sample):
    """Restituisce le 64 celle (4 tessuti x 16) oppure None."""
    c = sample.get("c")
    if not c or len(c) != 4:
        return None
    flat = []
    for gruppo in c:
        if not gruppo or len(gruppo) != 16:
            return None
        flat.extend(gruppo)
    return flat


def mean_rows(samples, n=None):
    """Media cella per cella. Se n è dato, usa solo gli ultimi n campioni."""
    rows = [cells(s) for s in samples]
    rows = [r for r in rows if r]
    if n is not None:
        rows = rows[-n:]
    if not rows:
        return None
    return [sum(col) / len(col) for col in zip(*rows)]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--window", type=float, default=2.0,
                    help="secondi di scatto alla fine dell'antefatto")
    ap.add_argument("--calm-gap", type=float, default=300.0,
                    help="secondi di distanza minima da un evento per una calma")
    args = ap.parse_args()

    events = [e for e in load("concept_events.jsonl") if e.get("antefatto")]
    tel = sorted(load("telemetry.jsonl"), key=lambda r: r.get("recv", 0))
    if not events or not tel:
        print(f"manca qualcosa: eventi={len(events)} telemetria={len(tel)}")
        return 1

    # istanti degli eventi, per tenere lontano le calme
    istanti = [e["scatto"]["recv"] for e in events]
    pkt_per_sec = 1.0 / 0.91  # 109,8 tic/s, 1 campione ogni 100 tick

    n_scatta = int(args.window * pkt_per_sec)
    n_scatta = max(n_scatta, 1)

    # ---- finestre di scatto: ultimi n campioni dell'antefatto ----
    scatti, basi = [], []
    for e in events:
        ant = [s for s in e["antefatto"] if s.get("c")]
        if len(ant) < 20:
            continue
        top = mean_rows(ant, n_scatta)
        base = mean_rows(ant[:-n_scatta] or ant)
        if top and base:
            scatti.append(top)
            basi.append(base)

    # ---- finestre calme: telemetria lontana da ogni evento ----
    durata = n_scatta / pkt_per_sec + 90.0
    calme, calme_base = [], []
    i = 0
    while i < len(tel):
        s = tel[i]
        t = s.get("recv", 0)
        if any(abs(t - x) < args.calm_gap for x in istanti):
            i += 1
            continue
        finestra = []
        j = i
        while j < len(tel) and tel[j].get("recv", 0) - t <= durata:
            if tel[j].get("c"):
                finestra.append(tel[j])
            j += 1
        i = j  # salta tutta la finestra
        if len(finestra) >= 20:
            top = mean_rows(finestra, n_scatta)
            base = mean_rows(finestra[:-n_scatta] or finestra)
            if top and base:
                calme.append(top)
                calme_base.append(base)

    if not scatti or not calme:
        print(f"finestre insufficienti: scatto={len(scatti)} calma={len(calme)}")
        return 1

    def media(rows):
        return [sum(col) / len(col) for col in zip(*rows)]

    m_top, m_base = media(scatti), media(basi)
    c_top, c_base = media(calme), media(calme_base)

    print(f"POOL TELEMETRIA — {len(scatti)} eventi, {len(calme)} finestre calme")
    print(f"  scatto: ultimi {n_scatta} campioni (~{args.window}s) vs i ~100s "
          f"prima; calme: stessa forma, lontane >{int(args.calm_gap)}s da ogni evento\n")

    print(f"  {'tessuto':<10} {'delta scatto':>12} {'delta calma':>12}  "
          f"{'eccesso':>9}  {'eventi delta>0':>15}")
    for k, nome in enumerate(TESSUTI):
        d_top = m_top[k * 16:(k + 1) * 16]
        d_base = m_base[k * 16:(k + 1) * 16]
        ct = c_top[k * 16:(k + 1) * 16]
        cb = c_base[k * 16:(k + 1) * 16]
        delta_sc = [d_top[i] - d_base[i] for i in range(16)]
        delta_ca = [ct[i] - cb[i] for i in range(16)]
        # somma sul tessuto (16 celle): l'energia complessiva del tessuto
        s_sc, s_ca = sum(delta_sc), sum(delta_ca)
        conta = sum(
            1 for rows, bs in zip(scatti, basi)
            if sum(rows[k * 16 + j] for j in range(16)) >
               sum(bs[k * 16 + j] for j in range(16))
        )
        print(f"  {nome:<10} {s_sc:>+12.3f} {s_ca:>+12.3f}  {s_sc - s_ca:>+9.3f}  "
              f"{conta:>7}/{len(scatti)}")

    # ---- singola cella più discriminante ----
    print("\n  celle con |eccesso| maggiore (scatto - calma):")
    punti = []
    for i in range(64):
        ds = m_top[i] - m_base[i]
        dc = c_top[i] - c_base[i]
        punti.append((abs(ds - dc), i, ds, dc))
    punti.sort(reverse=True)
    for _, i, ds, dc in punti[:6]:
        print(f"    {TESSUTI[i // 16]}[{i % 16:2d}]  scatto {ds:+.3f}  "
              f"calma {dc:+.3f}  eccesso {ds - dc:+.3f}")

    # ---- statistica per evento sul tessuto discriminante ----
    best = punti[0][1] // 16
    conta = sum(
        1 for rows, bs in zip(scatti, basi)
        if sum(rows[best * 16:(best + 1) * 16]) >
           sum(bs[best * 16:(best + 1) * 16])
    )
    print(f"\n  tessuto discriminante: {TESSUTI[best]} — "
          f"{conta}/{len(scatti)} eventi con delta>0 "
          f"(atteso per caso ~{len(scatti)//2})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
