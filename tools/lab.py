#!/usr/bin/env python3
"""
Nova Exo — Lab harness per verifica path-dependency (CfC).

Lancia QEMU, inietta uno schedule di stimoli sul seriale, cattura il log
circolare su debugcon, e lo analizza:
  - misura di τ (costante di decadimento dopo impulso)
  - indice di path-dependency (la risposta a un impulso dipende dallo
    stato passato / da impulsi precedenti?)

Formato dump (debugcon, tra D:BEGIN e D:END):
  D:<tick hex64>,<c0 hex16>,...,<c31 hex16>
Layout celle: [0..7]=tatto [8..15]=chemio [16..23]=metabol [24..31]=integrat
Ogni valore i16 = valore_reale * 100.
"""

import subprocess
import sys
import time
import os
import re
import numpy as np

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ISO = os.path.join(ROOT, "build", "nova-exo.iso")
DEBUGCON = os.path.join(ROOT, "qemu-debug.log")
QEMU = "qemu-system-x86_64"


def run_qemu(stimulus, timeout=30, debugcon_path=None):
    """Lancia QEMU, applica lo schedule `stimulus` (lista di (delay_s, bytes)),
    e torna il path del file debugcon. Lo schedule è relativo all'avvio."""
    if debugcon_path is None:
        debugcon_path = DEBUGCON
    if not os.path.exists(ISO):
        raise RuntimeError(f"ISO mancante: {ISO} — lancia 'make iso'")
    cmd = [
        QEMU, "-cpu", "qemu64", "-m", "512M",
        "-cdrom", ISO, "-serial", "stdio",
        "-debugcon", f"file:{debugcon_path}",
    ]
    p = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                         stderr=subprocess.DEVNULL, text=True, bufsize=1)
    # applica stimoli con ritardi
    t0 = time.time()
    for delay_s, data in stimulus:
        while time.time() - t0 < delay_s:
            if p.poll() is not None:
                break
            time.sleep(0.02)
        try:
            p.stdin.write(data)
            p.stdin.flush()
        except BrokenPipeError:
            break
    # attendi che il DUMP appaia nel file debugcon, poi termina QEMU
    deadline = time.time() + timeout
    dumped = False
    while time.time() < deadline:
        try:
            with open(debugcon_path) as f:
                if "D:BEGIN" in f.read():
                    dumped = True
                    break
        except Exception:
            pass
        if p.poll() is not None:
            break
        time.sleep(0.2)
    # dai tempo al flush su disco, poi termina
    time.sleep(1.0)
    try:
        p.terminate()
    except Exception:
        pass
    try:
        p.wait(timeout=8)
    except subprocess.TimeoutExpired:
        p.kill()
    return debugcon_path


def parse_dump(debugcon_path=None):
    """Parsa il dump tra D:BEGIN e D:END. Ritorna (ticks[N], cells[N,32] float)."""
    if debugcon_path is None:
        debugcon_path = DEBUGCON
    with open(debugcon_path) as f:
        lines = f.read().split("\n")
    rows = []
    in_dump = False
    for line in lines:
        if line == "D:BEGIN":
            in_dump = True
            continue
        if line == "D:END":
            in_dump = False
            continue
        if not in_dump or not line.startswith("D:"):
            continue
        parts = line.split(",")
        tick = int(parts[0][2:], 16)
        vals = [int(x, 16) for x in parts[1:33]]
        # i16 signed decode
        vals = [v - 65536 if v >= 32768 else v for v in vals]
        rows.append((tick, [v / 100.0 for v in vals]))
    if not rows:
        return np.array([]), np.zeros((0, 32))
    ticks = np.array([r[0] for r in rows], dtype=np.int64)
    cells = np.array([r[1] for r in rows], dtype=np.float64)
    return ticks, cells


def autocorr_time(x, max_lag=80):
    """Tempo di autocorrelazione integrato (robusto alle oscillazioni):
    τ = Σ_k |C(k)| / C(0) sui lag 0..max_lag. C(k) è l'autocorrelazione.
    Per un oscillatore smorzato |C| cattura l'inviluppo decadente."""
    x = np.asarray(x, float)
    if len(x) < 4:
        return 0.0
    x = x - x.mean()
    sd = x.std()
    if sd < 1e-9:
        return 0.0
    x = x / sd
    n = len(x)
    C = np.correlate(x, x, mode="full")[n - 1:] / n  # C[0], C[1], ...
    C0 = C[0]
    if C0 < 1e-12:
        return 0.0
    return float(np.sum(np.abs(C[:max_lag])) / C0)


def find_impulses(cells, thr=0.1):
    """Trova tutti i picchi di impulso (massimi locali di |chemio[0]|).
    Ritorna lista di indici nel log."""
    chemio0 = np.abs(cells[:, 8])
    if len(chemio0) < 3:
        return []
    peaks = []
    for i in range(1, len(chemio0) - 1):
        if chemio0[i] >= thr and chemio0[i] >= chemio0[i - 1] and chemio0[i] >= chemio0[i + 1]:
            # prendi il massimo di una piattaforma
            j = i
            while j + 1 < len(chemio0) and chemio0[j + 1] == chemio0[i]:
                j += 1
            peaks.append((i + j) // 2)
    return peaks


def extract_response(cells, imp_idx, cell_slice, window=48):
    """Estrae la traiettoria post-impulso di un gruppo di celle."""
    grp = cells[:, cell_slice]
    lo = max(0, imp_idx - 8)
    baseline = grp[lo:imp_idx].mean(axis=0)
    hi = min(len(cells), imp_idx + window)
    resp = grp[imp_idx:hi] - baseline
    return resp


def run_experiment(gap_s=0.0, settle=3.0, hold=4.0, tag=""):
    """Esegue uno scenario: settle, impulso1, (dopo gap_s) impulso2, DUMP.
    gap_s=0 → impulso singolo. Ritorna (ticks, cells)."""
    stim = [(settle, "1.0,0.0,0.0,0.0\n")]
    if gap_s > 0:
        stim.append((settle + gap_s, "1.0,0.0,0.0,0.0\n"))
    stim.append((settle + gap_s + hold, "DUMP\n"))
    run_qemu(stim, timeout=25)
    return parse_dump()


def report():
    """Esperimento: τ per autocorrelazione su ogni cella + test path-dependency
    (impulso singolo vs doppio a gap misurato)."""
    print("=== Nova Exo Lab: τ (autocorrelazione) + path-dependency ===")

    # ── τ singolo impulso ──
    print("[*] impulso singolo (settle 3s, hold 4s)...")
    ticks, cells = run_experiment(gap_s=0.0)
    if len(ticks) == 0:
        print("[!] nessun dump — controlla qemu-debug.log")
        return
    print(f"[+] dump: {len(ticks)} entry, tick {ticks[0]}..{ticks[-1]}")
    imps = find_impulses(cells)
    if not imps:
        print("[!] impulso non rilevato")
        return
    imp = imps[0]
    print(f"[+] impulso a tick {ticks[imp]} (idx {imp})")
    print("    τ (autocorrelazione, in tick):")
    for name, sl in [("chemio", slice(8, 16)), ("tatto", slice(0, 8)),
                     ("metabol", slice(16, 24)), ("integrat", slice(24, 28))]:
        grp = cells[:, sl]
        # deviazione post-impulso rispetto a baseline pre-impulso
        lo = max(0, imp - 16)
        base = grp[lo:imp].mean(axis=0)
        dev = (grp[imp:] - base).flatten()
        tau = autocorr_time(dev, max_lag=min(80, len(dev)))
        print(f"      τ {name:8s} = {tau:6.1f} tick  (~{tau/100:.2f} s)")

    # ── path-dependency: singolo vs doppio impulso ──
    print("\n[*] test path-dependency: impulso singolo vs doppio (gap ~20 tick)")
    # controllo: singolo impulso, risposta integrat attorno all'impulso
    t1, c1 = run_experiment(gap_s=0.0, hold=4.0)
    i1 = find_impulses(c1)
    if not i1:
        print("[!] controllo senza impulso")
        return
    ctrl_resp = extract_response(c1, i1[0], slice(24, 28), window=48)

    # test: doppio impulso con gap ~0.45s (~20 tick a ~43Hz)
    gap_s = 0.45
    t2, c2 = run_experiment(gap_s=gap_s, hold=4.0)
    i2 = find_impulses(c2)
    if len(i2) < 2:
        print(f"[!] doppio impulso: trovati {len(i2)} impulsi (attesi 2)")
        print("    impulsi:", [int(t2[i]) for i in i2])
        return
    gap_ticks = int(t2[i2[1]] - t2[i2[0]])
    print(f"[+] doppio impulso: gap misurato = {gap_ticks} tick")
    # risposta al SECONDO impulso
    test_resp = extract_response(c2, i2[1], slice(24, 28), window=48)

    # metrica: differenza normalizzata tra risposta2 e controllo
    n = min(len(ctrl_resp), len(test_resp))
    if n < 4:
        print("[!] finestra risposta troppo corta")
        return
    diff = test_resp[:n] - ctrl_resp[:n]
    ctrl_norm = np.sqrt((ctrl_resp[:n] ** 2).mean()) + 1e-9
    pd_index = float(np.sqrt((diff ** 2).mean()) / ctrl_norm)
    print(f"[+] integrat risposta controllo (singolo): RMS = {ctrl_norm:.4f}")
    print(f"[+] integrat risposta test (2° impulso): RMS = "
          f"{float(np.sqrt((test_resp[:n]**2).mean())):.4f}")
    print(f"[+] PATH-DEPENDENCY INDEX = {pd_index:.3f}")
    print(f"    (0 = nessuna dipendenza dallo stato passato; >0 = dipende)")
    return {
        "ticks": ticks, "cells": cells, "impulse_idx": imp,
        "gap_ticks": gap_ticks, "pd_index": pd_index,
    }


if __name__ == "__main__":
    report()
