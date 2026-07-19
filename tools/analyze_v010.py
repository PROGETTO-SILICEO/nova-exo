#!/usr/bin/env python3
"""
Nova Exo v0.11 — Analisi completa: τ spettrale, KWW per cellula, attrattore.

CLI:
  --n-lags N    max lag per autocorrelazione (default: 100)
  --cell        cellula da analizzare: integrat|tatto|metabol|chemio|all (default: all)
  --no-qemu     usa file log esistente invece di lanciare QEMU
  --serial FILE path del log seriale (default: serial_{ts}.log)
  --debugcon FILE path del log debugcon (default: debugcon_{ts}.log)
  --outdir DIR  directory output plot (default: ../plots/)
  --timeout N   timeout secondi per QEMU (default: 35)

Produce:
  1. KWW fit per cellula (tau spectrum)
  2. Pattern activation heatmap
  3. Attractor dynamics

Cell mapping (indici packed 32):
  [0..7]   Tatto h
  [8..15]  Chemio h
  [16..23] Metabol h
  [24..31] Integrat h
"""

import sys, os, time, signal, subprocess, re, argparse
import numpy as np
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ISO = os.path.join(ROOT, "build", "nova-exo.iso")
QEMU = "qemu-system-x86_64"
PLOT_DIR = os.path.join(ROOT, "plots")
os.makedirs(PLOT_DIR, exist_ok=True)

# ── Cell group slice map ──
CELL_SLICES = {
    "tatto":    slice(0, 8),
    "chemio":   slice(8, 16),
    "metabol":  slice(16, 24),
    "integrat": slice(24, 32),
}

# ════════════════════════════════════════════════════════════════
# 1. QEMU runner
# ════════════════════════════════════════════════════════════════

def run_qemu(stimulus_schedule, timeout=35):
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
    dbg_path = f"/tmp/nova_analysis_{int(time.time()*1e6)}.log"
    cmd = (f"( {inner} ) | {QEMU} -cpu qemu64 -m 512M -cdrom {ISO} "
           f"-serial stdio -debugcon file:{dbg_path}")
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
    serial_data = parse_serial(decoded)
    dump_data = parse_debugcon(dbg_path)
    return serial_data, dump_data


# ════════════════════════════════════════════════════════════════
# 2. Parser
# ════════════════════════════════════════════════════════════════

def parse_serial(text):
    lines = text.split("\n")
    result = {
        "A": [], "F": [],
        "raw": lines,
        "cells": [],  # (tick_dec, tattoo[8], chemio[8], metabol[8], integrat[8])
    }
    for line in lines:
        if line.startswith("A:"):
            m = re.match(r"A:([\d.\-]+)@([0-9a-f]+)", line)
            if m:
                result["A"].append((float(m.group(1)), int(m.group(2), 16)))
        elif line.startswith("F:") and not line.startswith("F:---"):
            m = re.match(r"F:([\d.\-]+)@([0-9a-f]+)", line)
            if m:
                result["F"].append((float(m.group(1)), int(m.group(2), 16)))
    # Parse cell state blocks: groups of 4 lines (T/C/M/I) with same tick
    # T:<hex4>:<8 floats>
    # C:<hex4>:<8 floats>
    # M:<hex4>:<8 floats>
    # I:<hex4>:<8 floats>
    cell_re = re.compile(r'([TCMI]):([0-9a-f]+):(.+)')
    cell_lines = []
    for line in lines:
        m = cell_re.match(line)
        if m:
            tag, tick_hex, vals_str = m.groups()
            vals = [float(x) for x in vals_str.split(",")]
            if len(vals) == 8:
                cell_lines.append((tag, int(tick_hex, 16), vals))
    # Group 4 consecutive lines by tick
    i = 0
    while i + 3 < len(cell_lines):
        t0, t1, t2, t3 = cell_lines[i:i+4]
        if t0[0] == 'T' and t1[0] == 'C' and t2[0] == 'M' and t3[0] == 'I':
            ticks = {t0[1], t1[1], t2[1], t3[1]}
            if len(ticks) == 1:
                tick = t0[1]
                result["cells"].append((tick, t0[2], t1[2], t2[2], t3[2]))
        i += 1
    return result


def parse_debugcon(path):
    if not os.path.exists(path):
        return None
    with open(path) as f:
        content = f.read()
    if "D:BEGIN" not in content:
        return None
    rows = []
    in_dump = False
    for line in content.split("\n"):
        if line == "D:BEGIN":
            in_dump = True
            continue
        if line == "D:END":
            break
        if not in_dump or not line.startswith("D:"):
            continue
        parts = line.split(",")
        tick = int(parts[0][2:], 16)
        vals_hex = parts[1:33]
        vals = []
        for h in vals_hex:
            v = int(h, 16)
            if v >= 32768:
                v -= 65536
            vals.append(v / 100.0)
        rows.append((tick, vals))
    if not rows:
        return None
    ticks = np.array([r[0] for r in rows], dtype=np.int64)
    cells = np.array([r[1] for r in rows], dtype=np.float64)
    return ticks, cells


# ════════════════════════════════════════════════════════════════
# 3. KWW analysis — per cell group
# ════════════════════════════════════════════════════════════════

def kww_model(t, beta, tau0):
    return np.exp(-((t / tau0) ** beta))


def fit_stretched_exponential(autocorr, n_lags=100):
    t = np.arange(min(n_lags, len(autocorr)), dtype=float)
    y = np.abs(autocorr[:len(t)])
    y0 = y[0] if y[0] != 0 else 1.0
    y_norm = y / y0
    best = (0.0, 0.0, -1e9)
    for beta in np.linspace(0.3, 1.0, 15):
        for tau0 in np.logspace(1, 3, 20):
            pred = kww_model(t, beta, tau0)
            mask = y_norm > 1e-6
            if not mask.any():
                continue
            log_y = np.log(y_norm[mask] + 1e-12)
            log_p = np.log(pred[mask] + 1e-12)
            ss_res = np.sum((log_y - log_p) ** 2)
            ss_tot = np.sum((log_y - np.mean(log_y)) ** 2)
            r2 = 1 - ss_res / (ss_tot + 1e-12)
            if r2 > best[2]:
                best = (beta, tau0, r2)
    return best


def bootstrap_kww(autocorr, n_iter=100, n_lags=100, seed=42):
    """Bootstrap standard errors for beta, tau0, R2.
    Returns (beta_mean, beta_std, tau0_mean, tau0_std, r2_mean, r2_std,
             ci_beta_lo, ci_beta_hi, ci_tau0_lo, ci_tau0_hi).
    """
    rng = np.random.default_rng(seed)
    t = np.arange(min(n_lags, len(autocorr)), dtype=float)
    y = np.abs(autocorr[:len(t)])
    y0 = y[0] if y[0] != 0 else 1.0
    y_norm = y / y0
    resid = np.arange(len(y_norm))
    betas, tau0s, r2s = [], [], []
    for _ in range(n_iter):
        idx = rng.choice(resid, size=len(resid), replace=True)
        if len(idx) < 3 or y_norm[idx].max() < 1e-6:
            continue
        b, t0, r = fit_stretched_exponential(y_norm[idx], n_lags=n_lags)
        betas.append(b); tau0s.append(t0); r2s.append(r)
    if not betas:
        return 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0
    return (np.mean(betas), np.std(betas, ddof=1),
            np.mean(tau0s), np.std(tau0s, ddof=1),
            np.mean(r2s), np.std(r2s, ddof=1),
            np.percentile(betas, 2.5), np.percentile(betas, 97.5),
            np.percentile(tau0s, 2.5), np.percentile(tau0s, 97.5))


def compute_tau_spectrum(cells, cell_slice, max_lag=100):
    grp = cells[:, cell_slice]
    s = grp.mean(axis=1)
    s = s - s.mean()
    sd = s.std()
    if sd < 1e-9:
        return np.array([]), np.array([])
    s = s / sd
    n = len(s)
    C = np.correlate(s, s, mode="full")[n - 1:]
    C0 = C[0]
    if C0 < 1e-12:
        return np.array([]), np.array([])
    C_norm = C[:max_lag] / C0
    k = np.arange(max_lag, dtype=float)
    positive = C_norm > 0
    tau_local = np.full(max_lag, np.nan)
    tau_local[positive] = -k[positive] / np.log(C_norm[positive] + 1e-12)
    return tau_local, C_norm[:max_lag]


# ════════════════════════════════════════════════════════════════
# 4. Plot — per-cellula
# ════════════════════════════════════════════════════════════════

CELL_LABELS = {
    "tatto": "Tatto",
    "chemio": "Chemio",
    "metabol": "Metabol",
    "integrat": "Integrat",
}

def serial_cells_to_array(cell_data):
    """Convert serial [(tick, t[8], c[8], m[8], i[8]), ...] to (N,32) array."""
    rows = []
    for entry in cell_data:
        rows.append(np.concatenate(entry[1:]))  # skip tick, concat 4×[8]
    return np.array(rows, dtype=np.float64)

def plot_per_cell_kww(cell_data, cell_name, n_lags, save_path):
    if cell_data is None or len(cell_data) < 10:
        print(f"[!] Insufficient data for {cell_name}")
        return
    cells = serial_cells_to_array(cell_data)
    sl = CELL_SLICES[cell_name]
    tau_local, C_norm = compute_tau_spectrum(cells, sl, max_lag=n_lags)
    if len(tau_local) == 0:
        print(f"[!] Cannot compute tau spectrum for {cell_name}")
        return

    fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(14, 5))

    k = np.arange(len(C_norm))
    ax1.semilogy(k, C_norm, "b-", lw=1.5, label=f"{CELL_LABELS[cell_name]} autocorr")
    beta_opt, tau0_opt, r2 = fit_stretched_exponential(C_norm, n_lags=n_lags)
    bm, bs, tm, ts, r2m, r2s, cib_lo, cib_hi, cit_lo, cit_hi = \
        bootstrap_kww(C_norm, n_iter=100, n_lags=n_lags)
    label = (f"KWW: beta={beta_opt:.2f} tau0={tau0_opt:.0f} R2={r2:.3f}\n"
             f"boot: beta={bm:.2f}+-{bs:.2f} [{cib_lo:.2f},{cib_hi:.2f}]\n"
             f"      tau0={tm:.0f}+-{ts:.0f} [{cit_lo:.0f},{cit_hi:.0f}]")
    ax1.semilogy(k, kww_model(k.astype(float), beta_opt, tau0_opt),
                 "r--", lw=2, alpha=0.7, label=label)
    ax1.set_xlabel("Lag (tick)")
    ax1.set_ylabel("|C(k)| / C(0)")
    ax1.set_title(f"Stretched Exponential {CELL_LABELS[cell_name]}")
    ax1.legend(fontsize=8)
    ax1.grid(True, alpha=0.3)

    valid = ~np.isnan(tau_local)
    if valid.any():
        ax2.plot(k[valid], tau_local[valid], "g.", markersize=3)
        ax2.set_ylabel("tau locale (tick)")
        ax2.set_title(f"tau spectrum: {tau_local[valid].min():.0f}-{tau_local[valid].max():.0f}")
    else:
        ax2.text(0.5, 0.5, "No valid tau", ha="center", va="center", transform=ax2.transAxes)
    ax2.set_xlabel("Lag (tick)")
    ax2.grid(True, alpha=0.3)

    plt.tight_layout()
    plt.savefig(save_path, dpi=150)
    print(f"  Saved {save_path}")
    return beta_opt, tau0_opt, r2, bm, bs, tm, ts, r2m, r2s, cib_lo, cib_hi, cit_lo, cit_hi


def plot_all_cells_overlay(cell_data, n_lags, save_path):
    """Overlay KWW fits for all 4 cell types on one plot."""
    if cell_data is None or len(cell_data) < 10:
        return
    cells = serial_cells_to_array(cell_data)
    colors = {"tatto": "#e74c3c", "chemio": "#2ecc71", "metabol": "#f39c12", "integrat": "#3498db"}
    fig, ax = plt.subplots(figsize=(10, 6))
    results = {}
    for name in ["tatto", "chemio", "metabol", "integrat"]:
        sl = CELL_SLICES[name]
        _, C_norm = compute_tau_spectrum(cells, sl, max_lag=n_lags)
        if len(C_norm) == 0:
            continue
        k = np.arange(len(C_norm))
        beta, tau0, r2 = fit_stretched_exponential(C_norm, n_lags=n_lags)
        results[name] = (beta, tau0, r2)
        ax.semilogy(k, C_norm, color=colors[name], lw=1, alpha=0.4)
        label = f"{CELL_LABELS[name]} (β={beta:.2f}, τ₀={tau0:.0f})"
        ax.semilogy(k, kww_model(k.astype(float), beta, tau0),
                    color=colors[name], lw=2, linestyle="--", label=label)
    ax.set_xlabel("Lag (tick)")
    ax.set_ylabel("|C(k)| / C(0)")
    ax.set_title("KWW fit per cellula — overlay")
    ax.legend()
    ax.grid(True, alpha=0.3)
    plt.tight_layout()
    plt.savefig(save_path, dpi=150)
    print(f"  Saved {save_path}")
    return results


def plot_pattern_activation(serial_data, save_path):
    if len(serial_data["A"]) == 0:
        print("[!] No A: lines for pattern activation")
        return
    A = np.array(serial_data["A"])
    F = np.array(serial_data["F"]) if len(serial_data["F"]) > 0 else None
    unique_patterns = sorted(set(int(t) for _, t in A))
    pattern_to_idx = {p: i for i, p in enumerate(unique_patterns)}
    min_tick = 0
    max_tick = int(A[-1, 1]) if len(A) > 0 else 500
    tick_range = max_tick - min_tick + 1
    heatmap = np.zeros((len(unique_patterns), tick_range), dtype=float)
    for sim, rec_tick in A:
        t = int(rec_tick)
        if 0 <= t - min_tick < tick_range:
            idx = pattern_to_idx[int(rec_tick)]
            heatmap[idx, t - min_tick] = sim
    fig, (ax1, ax2) = plt.subplots(2, 1, figsize=(14, 8), gridspec_kw={"height_ratios": [1, 1.5]})
    if F is not None:
        ax1.plot(F[:, 1], F[:, 0], "b-", lw=0.5, alpha=0.7)
        ax1.set_xlabel("Tick")
        ax1.set_ylabel("Familiarity")
        ax1.set_title("Familiarity over time")
        ax1.grid(True, alpha=0.3)
    extent = [min_tick, max_tick, len(unique_patterns) - 0.5, -0.5]
    im = ax2.imshow(heatmap, aspect="auto", cmap="plasma", extent=extent, vmin=0.5, vmax=1.0)
    ax2.set_yticks(range(len(unique_patterns)))
    ax2.set_yticklabels([f"P:{p}" for p in unique_patterns])
    ax2.set_xlabel("Tick")
    ax2.set_ylabel("Pattern (recall_tick)")
    ax2.set_title("Pattern activation heatmap")
    ax2.grid(False)
    plt.colorbar(im, ax=ax2, label="Similarity")
    plt.tight_layout()
    plt.savefig(save_path, dpi=150)
    print(f"  Saved {save_path}")


def plot_attractor_dynamics(serial_data, save_path):
    A = np.array(serial_data["A"]) if len(serial_data["A"]) > 0 else None
    F = np.array(serial_data["F"]) if len(serial_data["F"]) > 0 else None
    cells = serial_data.get("cells", [])
    fig, axes = plt.subplots(3, 1, figsize=(14, 9), sharex=True)

    ax = axes[0]
    if A is not None and len(A) > 0:
        ax.plot(A[:, 1], A[:, 0], "r.", markersize=2, alpha=0.5)
        ax.set_ylabel("Attractor sim")
        ax.set_title("Attractor strength per tick")
        ax.grid(True, alpha=0.3)

    ax = axes[1]
    if F is not None and len(F) > 0:
        ax.plot(F[:, 1], F[:, 0], "b-", lw=0.5, alpha=0.7)
        ax.set_ylabel("Familiarity")
        ax.grid(True, alpha=0.3)

    ax = axes[2]
    if cells:
        # Use every 10th tick to avoid overplotting
        ticks = np.array([c[0] for c in cells[::10]])
        integrat_h = np.array([c[4] for c in cells[::10]])
        for j in range(min(4, integrat_h.shape[1])):
            ax.plot(ticks, integrat_h[:, j], lw=0.5, alpha=0.7, label=f"I[{j}]")
        ax.set_xlabel("Tick")
        ax.set_ylabel("Integrat h")
        ax.legend(ncol=4, fontsize=8)
        ax.grid(True, alpha=0.3)

    plt.tight_layout()
    plt.savefig(save_path, dpi=150)
    print(f"  Saved {save_path}")


# ════════════════════════════════════════════════════════════════
# 5. Main
# ════════════════════════════════════════════════════════════════

def main():
    parser = argparse.ArgumentParser(description="Nova Exo v0.11 — Full Analysis")
    parser.add_argument("--n-lags", type=int, default=100, help="max lag per autocorrelazione")
    parser.add_argument("--cell", choices=list(CELL_SLICES.keys()) + ["all"], default="all",
                        help="cellula da analizzare")
    parser.add_argument("--no-qemu", action="store_true", help="usa file log esistente")
    parser.add_argument("--serial", help="path log seriale (con --no-qemu)")
    parser.add_argument("--debugcon", help="path log debugcon (con --no-qemu)")
    parser.add_argument("--outdir", default=PLOT_DIR, help="directory output plot")
    parser.add_argument("--timeout", type=int, default=35, help="timeout QEMU (s)")
    args = parser.parse_args()

    print("=== Nova Exo v0.11 — Full Analysis ===\n")

    if args.no_qemu:
        if not args.serial or not args.debugcon:
            print("[!] --no-qemu richiede --serial e --debugcon")
            sys.exit(1)
        print(f"[*] Lettura da {args.serial} + {args.debugcon} ...")
        with open(args.serial) as f:
            serial_data = parse_serial(f.read())
        dump_data = parse_debugcon(args.debugcon)
    else:
        print("[*] Running QEMU (long settle + DUMP)...")
        serial_data, dump_data = run_qemu([
            (1.0, "0.2,0.0,0.0,0.0"),
            (3.0, "0.0,0.5,0.0,0.0"),
            (6.0, "1.0,0.0,0.0,0.0"),
            (9.0, "DUMP"),
        ], timeout=args.timeout)

    cell_data = serial_data.get("cells", [])
    print(f"  Serial: A={len(serial_data['A'])} F={len(serial_data['F'])} cells={len(cell_data)}")
    if dump_data is not None:
        print(f"  Dump: {len(dump_data[0])} entries")
    print()

    results = {}

    # Per-cell KWW (using serial cell data — full run, not just 256-tick dump)
    cells_to_plot = [args.cell] if args.cell != "all" else list(CELL_SLICES.keys())
    for name in cells_to_plot:
        print(f"[*] KWW fit — {CELL_LABELS[name]} (n_lags={args.n_lags}, {len(cell_data)} ticks)...")
        r = plot_per_cell_kww(cell_data, name, args.n_lags,
                              os.path.join(args.outdir, f"kww_{name}.png"))
        if r:
            results[name] = r

    # Overlay if all cells
    if args.cell == "all":
        print("[*] KWW overlay — tutte le cellule...")
        overlay = plot_all_cells_overlay(cell_data, args.n_lags,
                                         os.path.join(args.outdir, "kww_overlay.png"))
        if overlay:
            results["overlay"] = overlay

    # Standard plots
    print("[*] Pattern activation...")
    plot_pattern_activation(serial_data, os.path.join(args.outdir, "pattern_activation.png"))

    print("[*] Attractor dynamics...")
    plot_attractor_dynamics(serial_data, os.path.join(args.outdir, "attractor_dynamics.png"))

# Summary
    print(f"\n{'='*50}")
    print("KWW Summary (all cells, bootstrap 100 iter):")
    print(f"{'Cell':>10} | {'beta':>6} {'+-':>3} {'CI_lo':>6} {'CI_hi':>6} | {'tau0':>5} {'+-':>3} {'CI_lo':>5} {'CI_hi':>5} | {'R2':>6}")
    print("-" * 80)
    for name in list(CELL_SLICES.keys()):
        if name not in results:
            continue
        r = results[name]
        if len(r) < 12:
            beta, tau0, r2 = r[0], r[1], r[2]
            print(f"  {CELL_LABELS.get(name, name):>8} | {beta:6.2f} {'--':>3} {'--':>6} {'--':>6} | {tau0:5.0f} {'--':>3} {'--':>5} {'--':>5} | {r2:6.3f}")
        else:
            beta, tau0, r2, bm, bs, tm, ts, r2m, r2s, cib_lo, cib_hi, cit_lo, cit_hi = r
            print(f"  {CELL_LABELS.get(name, name):>8} | {bm:6.2f} {bs:4.2f} {cib_lo:6.2f} {cib_hi:6.2f} | {tm:5.0f} {ts:4.0f} {cit_lo:5.0f} {cit_hi:5.0f} | {r2m:6.3f}")
    print("-" * 80)
    print(f"{'='*50}")
    print(f"\n[Done] Plots saved to {args.outdir}/")


if __name__ == "__main__":
    main()
