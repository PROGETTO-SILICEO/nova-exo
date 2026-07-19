#!/usr/bin/env python3
"""
Comparazione sistematica: Exo reale vs baseline.
Genera tabella beta/tau0/PD/R2 per ogni modello e cellula.

Run:
  python3 compare_baselines.py [--n-ticks 5000] [--outdir ../plots/]
"""

import sys, os, argparse, json
import numpy as np
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from baselines import (
    VanillaRNN, AR1, ExoRandom, run_baseline, compute_pd, CELL_SLICES,
    sediment_stress_test
)
from analyze_v010 import (
    compute_tau_spectrum, fit_stretched_exponential, bootstrap_kww
)

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PLOT_DIR = os.path.join(ROOT, "plots")
os.makedirs(PLOT_DIR, exist_ok=True)


def compute_kww_report(cells):
    """Compute KWW beta, tau0, R2 for each cell group.
    Returns dict: {cell_name: {beta, tau0, r2, beta_std, tau0_std, ...}}"""
    report = {}
    for name, sl in CELL_SLICES.items():
        _, C_norm = compute_tau_spectrum(cells, sl, max_lag=100)
        if len(C_norm) == 0:
            report[name] = {"beta": 0.0, "tau0": 0.0, "r2": 0.0,
                            "beta_std": 0.0, "tau0_std": 0.0, "r2_std": 0.0,
                            "ci_beta": (0,0), "ci_tau0": (0,0)}
            continue
        beta, tau0, r2 = fit_stretched_exponential(C_norm, n_lags=100)
        bm, bs, tm, ts, r2m, r2s, cib_lo, cib_hi, cit_lo, cit_hi = \
            bootstrap_kww(C_norm, n_iter=100, n_lags=100)
        report[name] = {
            "beta": float(beta), "tau0": float(tau0), "r2": float(r2),
            "beta_mean": float(bm), "beta_std": float(bs),
            "tau0_mean": float(tm), "tau0_std": float(ts),
            "r2_mean": float(r2m), "r2_std": float(r2s),
            "ci_beta": (float(cib_lo), float(cib_hi)),
            "ci_tau0": (float(cit_lo), float(cit_hi)),
        }
    return report


def run_comparison(n_ticks=5000):
    """Run all baselines and return comparison data."""
    print(f"=== Baseline Comparison (n_ticks={n_ticks}) ===\n")

    results = {}

    # 1. Vanilla RNN
    print("[*] Vanilla RNN...")
    cells = run_baseline(VanillaRNN, n_ticks=n_ticks)
    pd_val, rms_c, rms_t, gap = compute_pd(cells, 50)
    kww = compute_kww_report(cells)
    results["vanilla_rnn"] = {"pd": pd_val, "gap": gap, "kww": kww}
    print(f"    PD={pd_val:.3f} gap={gap} tick")
    for name, v in kww.items():
        print(f"    {name:>8}: beta={v['beta']:.2f} tau0={v['tau0']:.0f} R2={v['r2']:.3f}")

    # 2. AR(1)
    print("[*] AR(1)...")
    cells = run_baseline(AR1, n_ticks=n_ticks)
    pd_val, rms_c, rms_t, gap = compute_pd(cells, 50)
    kww = compute_kww_report(cells)
    results["ar1"] = {"pd": pd_val, "gap": gap, "kww": kww}
    print(f"    PD={pd_val:.3f} gap={gap} tick")
    for name, v in kww.items():
        print(f"    {name:>8}: beta={v['beta']:.2f} tau0={v['tau0']:.0f} R2={v['r2']:.3f}")

    # 3. Exo random (CfC, no sedimentazione)
    print("[*] Exo-random...")
    cells = run_baseline(ExoRandom, n_ticks=n_ticks)
    pd_val, rms_c, rms_t, gap = compute_pd(cells, 50)
    kww = compute_kww_report(cells)
    results["exo_random"] = {"pd": pd_val, "gap": gap, "kww": kww}
    print(f"    PD={pd_val:.3f} gap={gap} tick")
    for name, v in kww.items():
        print(f"    {name:>8}: beta={v['beta']:.2f} tau0={v['tau0']:.0f} R2={v['r2']:.3f}")

    # 4. Stress test sedimentazione
    print("[*] Sediment stress test (100k ticks)...")
    ticks_s, norms_s = sediment_stress_test(100000, sample_every=1000)
    final_ratio = norms_s[-1, 2]
    norm_init = norms_s[0, 0]
    norm_final = norms_s[-1, 0]
    results["sediment_stress"] = {
        "n_ticks": 100000,
        "norm_initial": float(norm_init),
        "norm_final": float(norm_final),
        "ratio_final": float(final_ratio),
        "max_weight": float(norms_s[-1, 1]),
    }
    print(f"    Frobenius norm: {norm_init:.4f} -> {norm_final:.4f}")
    print(f"    Ratio final/initial: {final_ratio:.3f}")
    print(f"    Max |W_ij|: {norms_s[-1, 1]:.4f}")
    if final_ratio > 2.0:
        print("    [WARNING] Weight growth > 2x - saturation risk")
    else:
        print("    [OK] Weight growth within 2x bound")

    return results


def print_table(results):
    """Print comparison table."""
    print("\n" + "=" * 100)
    print("COMPARISON TABLE: Exo vs Baselines")
    print("=" * 100)

    models = ["vanilla_rnn", "ar1", "exo_random"]
    cells = ["tatto", "chemio", "metabol", "integrat"]

    # Header
    header = f"{'Model':<14} | {'PD':>6} | "
    for c in cells:
        header += f"{c[:3]:>10} "  # beta
    print(header)
    print("-" * len(header))

    for m in models:
        r = results.get(m, {})
        pd = r.get("pd", 0)
        kww = r.get("kww", {})
        row = f"{m:<14} | {pd:>6.3f} | "
        for c in cells:
            v = kww.get(c, {})
            b = v.get("beta", 0)
            t0 = v.get("tau0", 0)
            r2 = v.get("r2", 0)
            row += f"{b:>6.2f}/{r2:>5.2f}  "
        print(row)

    # Sediment stress
    ss = results.get("sediment_stress", {})
    if ss:
        print(f"\nSediment stress (100k tick):")
        print(f"  Frobenius: {ss['norm_initial']:.4f} -> {ss['norm_final']:.4f}")
        print(f"  Ratio: {ss['ratio_final']:.3f}x")
        status = "OK" if ss['ratio_final'] < 2.0 else "SATURATION RISK"
        print(f"  Status: {status}")


def main():
    parser = argparse.ArgumentParser(description="Compare Exo vs baselines")
    parser.add_argument("--n-ticks", type=int, default=5000, help="run length")
    parser.add_argument("--outdir", default=PLOT_DIR)
    args = parser.parse_args()

    results = run_comparison(n_ticks=args.n_ticks)
    print_table(results)

    # Save JSON
    outpath = os.path.join(args.outdir, "baseline_comparison.json")
    # Convert numpy types
    class NpEncoder(json.JSONEncoder):
        def default(self, obj):
            if isinstance(obj, (np.floating,)):
                return float(obj)
            if isinstance(obj, (np.integer,)):
                return int(obj)
            if isinstance(obj, tuple):
                return list(obj)
            return super().default(obj)
    with open(outpath, "w") as f:
        json.dump(results, f, indent=2, cls=NpEncoder)
    print(f"\n[Done] Results saved to {outpath}")


if __name__ == "__main__":
    main()
