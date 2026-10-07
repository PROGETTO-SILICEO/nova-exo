#!/usr/bin/env python3
"""Cosa ha mangiato Exo nello istante in cui le è cambiata un'idea.

Perché esiste (07/10/2026): `rx.rs::payload_to_chemio` calcola l'input della
chemio dai primi 4 byte del payload in arrivo, e `feed()` SOVRASCRIVE — quindi
l'input in un dato istante è l'ultimo pacchetto arrivato. Per capire uno scatto
di concetto basta guardare i pacchetti visibili nella finestra prima.

Attenzione: su rete commutata questa cattura vede solo broadcast e multicast —
il unicast destinato a .200 non arriva alla nostra porta. Una parte del
traffico di Exo resta quindi invisibile da qui; il confronto col contatore
rx_pkts della telemetria quantifica quanto.

    python3 tools/exo_event_chemio.py [--window 2.0]
"""
import argparse
import json
import os
import struct
import time

CAP = os.path.join(os.path.dirname(__file__), "..", "experiments", "net_telemetry")
OFF_UDP_PAYLOAD = 42
OFF_ETH_PAYLOAD = 14
ETHERTYPE_IPV4 = 0x0800
IP_PROTO_UDP = 17


def chemio_of(frame: bytes):
    """Stessa formula di src/rx.rs::payload_to_chemio (tenuta allineata a mano)."""
    if len(frame) < 15:
        return None
    eth = struct.unpack("!H", frame[12:14])[0]
    is_udp = eth == ETHERTYPE_IPV4 and len(frame) > 23 and frame[23] == IP_PROTO_UDP
    base = OFF_UDP_PAYLOAD if is_udp else OFF_ETH_PAYLOAD
    out = []
    for j in range(4):
        idx = base + j
        out.append(frame[idx] / 255.0 if idx < len(frame) else 0.0)
    return out, ("udp" if is_udp else "eth")


def describe(frame: bytes):
    if len(frame) < 14:
        return "corto"
    dst = frame[0:6]
    if dst == b"\xff" * 6:
        dove = "broadcast"
    elif dst[0] & 1:
        dove = "multicast"
    else:
        dove = "unicast"
    eth = struct.unpack("!H", frame[12:14])[0]
    if eth == ETHERTYPE_IPV4 and len(frame) >= 34:
        src = ".".join(str(b) for b in frame[26:30])
        dstip = ".".join(str(b) for b in frame[30:34])
        pr = frame[23]
        if pr == IP_PROTO_UDP and len(frame) >= 38:
            sp, dp = struct.unpack("!HH", frame[34:38])
            return f"{dove} UDP {src}:{sp}→{dstip}:{dp}"
        if pr == 6 and len(frame) >= 38:
            sp, dp = struct.unpack("!HH", frame[34:38])
            return f"{dove} TCP {src}:{sp}→{dstip}:{dp}"
        return f"{dove} IP/{pr} {src}→{dstip}"
    if eth == 0x806:
        return f"{dove} ARP"
    if eth == 0x86DD:
        return f"{dove} IPv6"
    return f"{dove} eth/0x{eth:04x}"


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
                except json.JSONDecodeError:
                    pass
    return out


def pool(sniff, events, window):
    """Analisi su TUTTI gli eventi insieme.

    Un singolo evento lascia ~8 pacchetti visibili: troppo poco, e con una
    perdita di cattura misurata fra il 6% e il 25% qualsiasi media su 8
    numeri è rumore. Con N eventi il campione cresce e la perdita (che e
    non selettiva per tipo di pacchetto) si medioca.
    """
    wins, calms = [], []
    busy = set()
    for e in events:
        t = e["scatto"]["recv"]
        busy.update(range(int(t * 10) - int(window * 10) - 5,
                          int(t * 10) + int(window * 10) + 5))
    for e in events:
        t = e["scatto"]["recv"]
        w = [p for p in sniff if p.get("exo") and t - window <= p["ts"] <= t]
        wins.append(w)
        c = [p for p in sniff
             if p.get("exo")
             and int(p["ts"] * 10) not in busy
             and t - 90 - window <= p["ts"] <= t - 90]
        calms.append(c)
    allw = [p for v in wins for p in v]
    allc = [p for v in calms for p in v]
    print(f"\nPOOL su {len(events)} eventi, finestra {window}s")
    print(f"  campioni di scatto : {len(allw)} pacchetti "
          f"({len(allw)/max(len(events),1):.1f} per evento)")
    print(f"  campioni calmi     : {len(allc)} pacchetti")

    def hist(pk):
        vals = {i: [] for i in range(4)}
        for p in pk:
            ch = p.get("chemio")
            if not ch:
                ch, _ = chemio_of(bytes.fromhex(p["b"]))
            if ch:
                for i, v in enumerate(ch):
                    vals[i].append(v)
        return [sum(v)/len(v) if v else 0.0 for v in vals.values()]

    hw, hc = hist(allw), hist(allc)
    print(f"  chemio media scatto : {[round(x,3) for x in hw]}")
    print(f"  chemio media calma  : {[round(x,3) for x in hc]}")
    print(f"  delta               : {[round(hw[i]-hc[i],3) for i in range(4)]}")
    # differenza per evento: quanti eventi hanno delta positivo su ogni asse
    conta = [0, 0, 0, 0]
    for w, c in zip(wins, calms):
        dw, dc = hist(w), hist(c)
        for i in range(4):
            if dw[i] - dc[i] > 0:
                conta[i] += 1
    print(f"  eventi con delta>0  : {conta} / {len(events)}")
    n = len([w for w in wins if w])
    print(f"  eventi con almeno 1 pkt visibile: {n}/{len(events)}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--window", type=float, default=2.0, help="secondi prima dello scatto")
    ap.add_argument("--last", action="store_true", help="usa l'ultimo evento, non quello più recente non analizzato")
    ap.add_argument("--pool", action="store_true",
                    help="analisi su TUTTI gli eventi insieme, non solo l'ultimo")
    args = ap.parse_args()

    events = load("concept_events.jsonl")
    sniff = load("lan_sniff.jsonl")
    if not events or not sniff:
        print("manca qualcosa: eventi=%d sniff=%d" % (len(events), len(sniff)))
        return 1

    if args.pool:
        pool(sniff, events, args.window)
        return 0

    e = events[-1]
    t = e["scatto"]["recv"]
    print(f"EVENTO  {e['data']}  {e['da']} → {e['a']}  t={e['t_scatto']:,}")
    print(f"  antefatto disponibile: {len(e['antefatto'])} pacchetti di telemetria")

    win = [p for p in sniff if t - args.window <= p["ts"] <= t and p.get("exo")]
    print(f"\nCIÒ CHE EXO MANGIA nei {args.window}s prima ({len(win)} pacchetti visibili da qui):")
    if not win:
        print("  nessun pacchetto broadcast/multicast in finestra — la parte "
              "di rete che vediamo non basta a spiegare lo scatto")
    for p in win[-25:]:
        frame = bytes.fromhex(p["b"])
        ch, _ = chemio_of(frame)
        tag = ""
        if ch:
            tag = f"  chemio={['%.2f' % x for x in ch]}"
        print(f"  {time.strftime('%H:%M:%S', time.localtime(p['ts']))}"
              f".{int((p['ts'] % 1) * 1000):03d}  {describe(frame)}{tag}")

    # finestra calma di DURATA UGUALE, altrove nel tempo (no eventi vicini)
    others = [x for x in events if x is not e]
    busy = set()
    for x in others:
        busy.update(range(int(x["scatto"]["recv"] * 10) - int(args.window * 10) - 5,
                          int(x["scatto"]["recv"] * 10) + int(args.window * 10) + 5))
    calm = [p for p in sniff
            if p.get("exo")
            and int(p["ts"] * 10) not in busy
            and t - 90 - args.window <= p["ts"] <= t - 90]
    print(f"\nCONFRONTO — finestra calma di {args.window}s (80-90 s prima, lontano dagli eventi): "
          f"{len(calm)} pacchetti per Exo")
    if win:
        print(f"  nello scatto: {len(win)}   nella calma: {len(calm)}")
        def hist(packets):
            vals = {i: [] for i in range(4)}
            for p in packets:
                ch, _ = chemio_of(bytes.fromhex(p["b"]))
                if ch:
                    for i, v in enumerate(ch):
                        vals[i].append(v)
            return [round(sum(v) / len(v), 3) if v else 0.0 for v in vals.values()]
        print(f"  media chemio nello scatto  : {hist(win)}")
        print(f"  media chemio finestra calma: {hist(calm)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
