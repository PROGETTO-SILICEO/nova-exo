#!/usr/bin/env python3
"""Analisi run lungo v0.14 v3 — tick in HEX (formato reale)."""
import sys, re, glob, os

def analyze(path):
    s = {
        "panic": 0, "error": 0, "voglio": 0, "esito_si": 0, "esito_no": 0,
        "senso": 0, "sleep": 0, "tick_max": 0, "tick_count": 0,
        "f_min": 1.0, "f_max": -1.0, "a_min": 1.0, "a_max": -1.0,
        "f_count": 0, "a_count": 0,
        "brain_out": 0, "brain_submit": 0, "brain_state": 0,
        "think_first": None, "think_last": 0,
    }
    seen_ticks = set()
    tick_re = re.compile(r"^(T|C|M|I):([0-9a-f]+):")
    with open(path, errors="replace") as f:
        for line in f:
            st = line.strip()
            if not st: continue
            if "PANIC" in st: s["panic"] += 1
            if "ERROR" in st or "error" in st.lower(): s["error"] += 1
            if st.startswith("VOGLIO:"): s["voglio"] += 1
            if "ESITO:" in st:
                if "utile=si" in st: s["esito_si"] += 1
                elif "utile=no" in st: s["esito_no"] += 1
            if "SENSO:INT" in st: s["senso"] += 1
            if "SLEEP" in st: s["sleep"] += 1
            if "BRAIN:submit" in st: s["brain_submit"] += 1
            if "BRAIN:out" in st: s["brain_out"] += 1
            if "BRAIN:state" in st:
                s["brain_state"] += 1
                m = re.search(r"think_ticks=(\d+)", st)
                if m:
                    v = int(m.group(1))
                    if s["think_first"] is None: s["think_first"] = v
                    s["think_last"] = v
            m = tick_re.match(st)
            if m:
                t = int(m.group(2), 16)
                if t not in seen_ticks:
                    seen_ticks.add(t)
                    s["tick_count"] += 1
                if t > s["tick_max"]: s["tick_max"] = t
            m = re.match(r"^F:([\d.\-]+)@(\d+)", st)
            if m:
                v = float(m.group(1)); s["f_min"] = min(s["f_min"], v); s["f_max"] = max(s["f_max"], v); s["f_count"] += 1
            m = re.match(r"^A:([\d.\-]+)@(\d+)", st)
            if m:
                v = float(m.group(1)); s["a_min"] = min(s["a_min"], v); s["a_max"] = max(s["a_max"], v); s["a_count"] += 1
    return s

def verdict(st):
    ok, ko = [], []
    ok.append("PANIC=0") if st["panic"] == 0 else ko.append(f"PANIC={st['panic']}")
    ok.append("ERROR=0") if st["error"] == 0 else ko.append(f"ERROR={st['error']}")
    ok.append(f"TICK stabile ({st['tick_count']} tick, max=0x{st['tick_max']:x}={st['tick_max']})") if st["tick_count"] > 30000 else ko.append(f"TICK basso ({st['tick_count']})")
    ok.append(f"BRAIN attivo (think {st['think_first']}→{st['think_last']}, {st['brain_state']} sample)") if st["brain_state"] > 20 and st["think_last"] > 0 else ko.append("BRAIN non attivo")
    ok.append(f"VOGLIO={st['voglio']}") if st["voglio"] > 0 else ko.append("nessun VOGLIO")
    ok.append(f"ESITO si/no={st['esito_si']}/{st['esito_no']}") if (st["esito_si"] + st["esito_no"]) > 0 else ko.append("nessun ESITO")
    ok.append(f"Memoria F∈[{st['f_min']:.3f},{st['f_max']:.3f}] ({st['f_count']})") if st["f_count"] > 1000 and st["f_max"] > 0.5 else ko.append("F: assente")
    ok.append(f"Attrattore A∈[{st['a_min']:.3f},{st['a_max']:.3f}]") if st["a_count"] > 50 else ko.append("A: assente")
    return ok, ko

for path in sorted(glob.glob(sys.argv[1])):
    st = analyze(path)
    ok, ko = verdict(st)
    print(f"=== {os.path.basename(path)} ===")
    print(f"  tick={st['tick_count']} max=0x{st['tick_max']:x}  think {st['think_first']}→{st['think_last']}  VOGLIO={st['voglio']}  ESITO={st['esito_si']}/{st['esito_no']}  F∈[{st['f_min']:.3f},{st['f_max']:.3f}]  A∈[{st['a_min']:.3f},{st['a_max']:.3f}]  SLEEP={st['sleep']}")
    print(f"  OK: {', '.join(ok)}")
    if ko: print(f"  KO: {', '.join(ko)}")
