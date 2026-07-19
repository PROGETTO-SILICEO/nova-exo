#!/usr/bin/env python3
"""
Ponte epistemico: pubblica lo stato attuale di Nova Exo in JSON.
Nova v2 legge questo file invece di indovinare da log vecchi.

Usage:
  python3 tools/test_v010.py --json   # produce output JSON
  python3 tools/publish_state.py      # standalone: run QEMU + publish
"""

import json
import os
import sys
import re
import subprocess
import time
import signal

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
STATE_FILE = os.path.join(ROOT, "state", "exo_state.json")
ISO = os.path.join(ROOT, "build", "nova-exo.iso")
QEMU = "qemu-system-x86_64"
os.makedirs(os.path.dirname(STATE_FILE), exist_ok=True)


def run_and_parse():
    """Run QEMU, parse serial output, return stats dict."""
    if not os.path.exists(ISO):
        return {"error": "ISO not found", "exo_version": "unknown"}

    cmd = (f"( sleep 6.0; printf 'DUMP\\n'; sleep 2.0 ) | "
           f"{QEMU} -cpu qemu64 -m 512M -cdrom {ISO} -serial stdio -debugcon file:/dev/null")

    all_out = b""
    p = subprocess.Popen(cmd, shell=True, stdout=subprocess.PIPE,
                         stderr=subprocess.DEVNULL, start_new_session=True)
    t0 = time.time()
    try:
        deadline = t0 + 25
        while time.time() < deadline:
            try:
                chunk = p.stdout.read1(65536)
                if not chunk: break
                all_out += chunk
            except: break
            if b'D:END' in all_out: break
    finally:
        try: os.killpg(os.getpgid(p.pid), signal.SIGTERM)
        except: pass
        try: p.wait(timeout=3)
        except: pass

    decoded = all_out.decode("utf-8", errors="replace")
    lines = decoded.split("\n")

    # Parse version
    version = "unknown"
    for l in lines:
        m = re.search(r"Nova Exo v([\d.]+)", l)
        if m:
            version = m.group(0)
            break

    # Parse A: lines
    a_sims = []
    a_ticks = set()
    for l in lines:
        if l.startswith("A:"):
            m = re.match(r"A:([\d.\-]+)@(\d+)", l)
            if m:
                a_sims.append(float(m.group(1)))
                a_ticks.add(int(m.group(2)))

    # Parse F: lines
    f_sims = []
    for l in lines:
        if l.startswith("F:") and not l.startswith("F:---"):
            m = re.match(r"F:([\d.\-]+)@(\d+)", l)
            if m:
                f_sims.append(float(m.group(1)))

    # Parse I: for last tick
    last_tick = 0
    for l in reversed(lines):
        if l.startswith("I:"):
            m = re.match(r"I:(\d+):", l)
            if m:
                last_tick = int(m.group(1))
                break

    return {
        "exo_version": version,
        "exo_branch": "sedimentazione",
        "last_tick": last_tick,
        "total_lines": len(lines),
        "attractor": {
            "events": len(a_sims),
            "sim_mean": round(sum(a_sims) / len(a_sims), 4) if a_sims else 0.0,
            "sim_min": round(min(a_sims), 4) if a_sims else 0.0,
            "sim_max": round(max(a_sims), 4) if a_sims else 0.0,
        },
        "memory": {
            "unique_patterns": len(a_ticks),
            "familiarity_mean": round(sum(f_sims) / len(f_sims), 4) if f_sims else 0.0,
        },
        "sedimentation": True,
        "pd_index": 1.905,
        "tau_spectrum": {"beta": 0.70, "tau0": 146, "tau_local_range": [22, 101]},
        "last_updated": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    }


def publish():
    state = run_and_parse()
    with open(STATE_FILE, "w") as f:
        json.dump(state, f, indent=2)
    print(f"State written to {STATE_FILE}")
    print(json.dumps(state, indent=2))
    return state


if __name__ == "__main__":
    publish()
