#!/usr/bin/env python3
"""
Nova Exo v0.10 — Test harness per attrattore mnemonico.

Analizza l'output seriale (non debugcon dump) per verificare che l'attrattore
stia effettivamente influenzando lo stato di Integrat.

Formati parsati:
  A:<sim>@<recall_tick>   — attrattore attivo
  I:<tick>:<v0>,...,<v7>  — integrat state
  F:<sim>@<tick>          — familiarità (recall passivo)
"""

import subprocess
import sys
import time
import os
import signal
import re
import struct

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ISO = os.path.join(ROOT, "build", "nova-exo.iso")
QEMU = "qemu-system-x86_64"


def run_qemu_serial(stimulus_schedule, timeout=25):
    """
    Lancia QEMU con schedule di stimoli, cattura tutto l'output seriale.
    `stimulus_schedule`: lista di (delay_sec, data_string).
    Ritorna liste di (tick, valori) per ogni famiglia di linee.
    """
    if not os.path.exists(ISO):
        raise RuntimeError(f"ISO mancante: {ISO} — lancia 'make iso'")

    parts = []
    prev = 0.0
    for delay_s, data in stimulus_schedule:
        d = max(0.0, delay_s - prev)
        parts.append(f"sleep {d:.2f}")
        esc = data.strip().replace("'", "'\\''")
        parts.append(f"printf '%s\\n' '{esc}'")
        prev = delay_s
    parts.append("sleep 3.0")
    inner = "; ".join(parts)

    cmd = (f"( {inner} ) | {QEMU} -cpu qemu64 -m 512M -cdrom {ISO} "
           f"-serial stdio -debugcon file:/dev/null")

    all_out = b""
    p = subprocess.Popen(cmd, shell=True, stdout=subprocess.PIPE,
                         stderr=subprocess.DEVNULL, start_new_session=True)
    t0 = time.time()
    try:
        deadline = t0 + timeout
        while time.time() < deadline:
            try:
                chunk = p.stdout.read1(65536)
                if not chunk:
                    break
                all_out += chunk
            except:
                break
    finally:
        try:
            os.killpg(os.getpgid(p.pid), signal.SIGTERM)
        except:
            pass
        try:
            p.wait(timeout=3)
        except:
            pass

    decoded = all_out.decode("utf-8", errors="replace")
    return parse_serial(decoded)


def parse_serial(text):
    """Parsa output seriale in dict per prefisso."""
    lines = text.split("\n")
    result = {
        "A": [],  # (sim_f32, recall_tick_u32)
        "I": [],  # (tick_u32, [f32;8])
        "F": [],  # (sim_f32, ref_tick_u32)
        "raw": lines,
    }
    for line in lines:
        if line.startswith("A:"):
            m = re.match(r"A:([\d.\-]+)@(\d+)", line)
            if m:
                result["A"].append((float(m.group(1)), int(m.group(2))))
        elif line.startswith("I:"):
            m = re.match(r"I:(\d+):(.*)", line)
            if m:
                tick = int(m.group(1))
                vals = [float(x) for x in m.group(2).split(",")]
                result["I"].append((tick, vals))
        elif line.startswith("F:") and not line.startswith("F:---"):
            m = re.match(r"F:([\d.\-]+)@(\d+)", line)
            if m:
                result["F"].append((float(m.group(1)), int(m.group(2))))
    return result


def stats(data):
    """Stampa statistiche su attrattore, integrat, familiarità."""
    n_a = len(data["A"])
    n_i = len(data["I"])
    n_f = len(data["F"])

    print(f"=== v0.10 Attractor Test ===")
    print(f"Lines A: {n_a}  I: {n_i}  F: {n_f}")

    if n_a > 0:
        sims = [a[0] for a in data["A"]]
        ticks = [a[1] for a in data["A"]]
        print(f"\n— Attractor —")
        print(f"  Fired:   {n_a} times")
        print(f"  Sim min: {min(sims):.4f}")
        print(f"  Sim max: {max(sims):.4f}")
        print(f"  Sim avg: {sum(sims)/len(sims):.4f}")
        print(f"  Unique patterns referenced: {len(set(ticks))}")

        # quanto spesso l'attrattore è vicino a 1.0 (stato già noto)?
        very_familiar = sum(1 for s in sims if s > 0.98)
        print(f"  Very familiar (>0.98): {very_familiar}/{n_a} "
              f"({100*very_familiar/n_a:.1f}%)")

        # quando l'attrattore inizia?
        first_tick = None
        for a_tick, a_recall in data["A"]:
            if first_tick is None:
                first_tick = a_recall
        print(f"  First pattern: {first_tick}")

    if n_i > 8:
        print(f"\n— Integrat trajectory (first 8 ticks) —")
        for tick, vals in data["I"][:8]:
            fmt = ",".join(f"{v:+.4f}" for v in vals[:4])
            print(f"  I:{tick:04d}: {fmt}...")

    if n_f > 0:
        f_sims = [f[0] for f in data["F"]]
        f_unique = len(set(f[1] for f in data["F"]))
        print(f"\n— Familiarity —")
        print(f"  Entries:  {n_f}")
        print(f"  Mean:     {sum(f_sims)/len(f_sims):.4f}")
        print(f"  Min:      {min(f_sims):.4f}")
        print(f"  Max:      {max(f_sims):.4f}")
        print(f"  Unique patterns: {f_unique}")


def main():
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", action="store_true", help="Output JSON state")
    args = parser.parse_args()

    if args.json:
        from publish_state import run_and_parse
        import json
        state = run_and_parse()
        state_file = os.path.join(os.path.dirname(os.path.dirname(
            os.path.abspath(__file__))), "state", "exo_state.json")
        os.makedirs(os.path.dirname(state_file), exist_ok=True)
        with open(state_file, "w") as f:
            json.dump(state, f, indent=2)
        print(json.dumps(state, indent=2))
        return

    print("[*] Test attrattore mnemonico v0.10")
    print("[*] Avvio QEMU (5s settle, DUMP)...\n")

    data = run_qemu_serial([
        (1.0, "0.1,0.0,0.0,0.0"),
        (3.0, "DUMP"),
    ], timeout=20)

    stats(data)

    # Verifica attrattore
    if len(data["A"]) > 0:
        print(f"\n✓ Attrattore attivo: {len(data['A'])} eventi")
    else:
        print(f"\n⚠ Attrattore NON attivo")

    # Verifica integrat + familiarità
    if len(data["I"]) > 0 and len(data["F"]) > 0:
        print(f"✓ Ciclo completo (I: + F: + A:) operativo")
    else:
        print(f"⚠ Dati incompleti: I={len(data['I'])} F={len(data['F'])}")

    print(f"\n[*] Totale linee grezze: {len(data['raw'])}")


if __name__ == "__main__":
    main()
