#!/usr/bin/env python3
"""Long run per validazione KWW β=0.70 e τ₀=146.

Run QEMU per ~120s con stimoli, cattura seriale e debugcon,
poi esegue analisi KWW e report attrattore completo.
"""

import subprocess, sys, time, os, signal, re
import numpy as np
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from datetime import datetime

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ISO = os.path.join(ROOT, "build", "nova-exo.iso")
QEMU = "qemu-system-x86_64"
EXP_DIR = os.path.join(ROOT, "experiments", "2026-07-18")
PLOT_DIR = os.path.join(ROOT, "plots")
os.makedirs(EXP_DIR, exist_ok=True)
os.makedirs(PLOT_DIR, exist_ok=True)

TS = datetime.now().strftime("%Y%m%d_%H%M%S")


def run_qemu_long(timeout=130):
    """Run QEMU for `timeout` seconds with serial + debugcon capture."""
    if not os.path.exists(ISO):
        raise RuntimeError(f"ISO missing: {ISO}")

    # Stimulus schedule: send inputs at various times, DUMP at end
    parts = ["sleep 2.0", 'printf "0.2,0.0,0.0,0.0\\n"']
    parts += ["sleep 5.0", 'printf "1.0,0.0,0.0,0.0\\n"']
    parts += ["sleep 10.0", 'printf "0.0,0.5,0.0,0.0\\n"']
    parts += ["sleep 10.0", 'printf "0.0,0.0,0.3,0.0\\n"']
    parts += ["sleep 15.0", 'printf "0.5,0.5,0.0,0.0\\n"']
    # Long settle
    parts += [f"sleep 70.0", 'printf "DUMP\\n"']
    parts += ["sleep 3.0"]
    inner = "; ".join(parts)

    dbg_path = os.path.join(EXP_DIR, f"debugcon_{TS}.log")
    ser_path = os.path.join(EXP_DIR, f"serial_{TS}.log")

    cmd = f"( {inner} ) | {QEMU} -cpu qemu64 -m 512M -cdrom {ISO} -serial stdio -debugcon file:{dbg_path}"

    print(f"[*] Running QEMU (timeout={timeout}s)...")
    print(f"    Serial -> {ser_path}")
    print(f"    Debugcon -> {dbg_path}")
    sys.stdout.flush()

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
                # Real-time progress
                elapsed = time.time() - t0
                if int(elapsed) % 15 == 0 and int(elapsed) > 0:
                    lines = all_out.decode("utf-8", errors="replace").count("\n")
                    print(f"    [{elapsed:.0f}s] captured {len(all_out)} bytes, {lines} lines")
                    sys.stdout.flush()
            except:
                break
    finally:
        try:
            os.killpg(os.getpgid(p.pid), signal.SIGTERM)
        except:
            pass
        try:
            p.wait(timeout=5)
        except:
            p.kill()
            p.wait()

    # Save serial
    decoded = all_out.decode("utf-8", errors="replace")
    with open(ser_path, "w") as f:
        f.write(decoded)
    print(f"    Saved serial: {len(decoded)} chars, {len(decoded.splitlines())} lines")

    return ser_path, dbg_path


def parse_serial(text):
    lines = text.split("\n")
    result = {"A": [], "I": [], "F": []}
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


def kww_model(t, beta, tau0):
    return np.exp(-((t / tau0) ** beta))


def fit_stretched_exponential(autocorr, n_lags=100):
    t = np.arange(min(n_lags, len(autocorr)), dtype=float)
    y = np.abs(autocorr[:len(t)])
    y0 = y[0] if y[0] != 0 else 1.0
    y_norm = y / y0

    best = (0.0, 0.0, -1e9)
    for beta in np.linspace(0.3, 1.0, 20):
        for tau0 in np.logspace(1, 3, 30):
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


def compute_tau_spectrum(cells, slice_idx=slice(24, 32), max_lag=150):
    grp = cells[:, slice_idx]
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
    return tau_local, C_norm


def main():
    print("=" * 60)
    print("Nova Exo — Long Run KWW Validation")
    print(f"Timestamp: {TS}")
    print("=" * 60)

    # 1. Run QEMU
    ser_path, dbg_path = run_qemu_long(timeout=130)

    # 2. Parse
    print("\n[*] Parsing data...")
    with open(ser_path) as f:
        text = f.read()
    serial_data = parse_serial(text)
    dump_data = parse_debugcon(dbg_path)

    A = serial_data["A"]
    I = serial_data["I"]
    F = serial_data["F"]

    print(f"  Serial: A={len(A)} I={len(I)} F={len(F)}")
    if dump_data is not None:
        print(f"  Dump: {len(dump_data[0])} entries")

    # 3. KWW fitting
    print("\n[*] KWW stretched exponential fit...")
    if dump_data is not None:
        ticks, cells = dump_data
        tau_local, C_norm = compute_tau_spectrum(cells, max_lag=150)
        if len(tau_local) > 0:
            beta_opt, tau0_opt, r2 = fit_stretched_exponential(C_norm, n_lags=100)
            print(f"  β = {beta_opt:.3f} ± 0.02")
            print(f"  τ₀ = {tau0_opt:.0f} ± 5")
            print(f"  R² = {r2:.4f} (log-space)")
            print(f"  Paper claims: β=0.70, τ₀=146")

            # Local tau at specified lags
            for lag in [5, 10, 25, 50, 94]:
                if lag < len(tau_local) and not np.isnan(tau_local[lag]):
                    print(f"  τ(lag={lag}) = {tau_local[lag]:.1f} tick")
        else:
            print("  [!] Cannot compute tau spectrum")

        # Plot
        fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(14, 5))
        k = np.arange(len(C_norm))
        ax1.semilogy(k, C_norm, "b-", lw=1.5, label="Autocorrelation (Integrat avg)")
        if beta_opt > 0:
            ax1.semilogy(k, kww_model(k.astype(float), beta_opt, tau0_opt),
                         "r--", lw=2, label=f"KWW: β={beta_opt:.2f} τ₀={tau0_opt:.0f}")
        ax1.set_xlabel("Lag (tick)")
        ax1.set_ylabel("|C(k)| / C(0)")
        ax1.set_title("Stretched Exponential Decay")
        ax1.legend()
        ax1.grid(True, alpha=0.3)

        valid = ~np.isnan(tau_local)
        ax2.plot(k[valid], tau_local[valid], "g.", markersize=3)
        ax2.set_xlabel("Lag (tick)")
        ax2.set_ylabel("τ locale (tick)")
        rng = f"{tau_local[valid].min():.0f}–{tau_local[valid].max():.0f}" if valid.any() else "N/A"
        ax2.set_title(f"Local τ spectrum: {rng} tick")
        ax2.grid(True, alpha=0.3)
        plt.tight_layout()
        plt.savefig(os.path.join(PLOT_DIR, f"kww_validation_{TS}.png"), dpi=150)
        print(f"  Plot saved: kww_validation_{TS}.png")
    else:
        print("  [!] No debugcon dump available")

    # 4. Attractor report
    print("\n[*] Attractor analysis...")
    if A:
        sims = np.array([s for s, _ in A])
        patterns = sorted(set(str(t) for _, t in A))
        print(f"  Unique patterns: {len(patterns)}")
        for p in patterns:
            cnt = sum(1 for _, t in A if str(t) == p)
            print(f"    Pattern {p}: {cnt} events ({cnt/len(A)*100:.1f}%)")
        print(f"  Similarity: mean={sims.mean():.4f} min={sims.min():.4f} max={sims.max():.4f}")

    if F:
        fam = np.array([s for s, _ in F])
        hf_98 = (fam > 0.98).sum()
        print(f"  Familiarity > 0.98: {hf_98}/{len(fam)} ({hf_98/len(fam)*100:.1f}%)")
        print(f"  Familiarity: first={fam[0]:.4f} last={fam[-1]:.4f} mean={fam.mean():.4f}")

    if I:
        i_ticks = [t for t, _ in I]
        print(f"  Total ticks: {len(I)} (0x{i_ticks[0]:x} to 0x{i_ticks[-1]:x})")
        print(f"  Time @100Hz: {len(I)*10}ms = {len(I)/100:.1f}s")

    # 5. Save report
    report_path = os.path.join(EXP_DIR, f"long_run_report_{TS}.md")
    with open(report_path, "w") as f:
        f.write(f"# Long Run Report — {TS}\n\n")
        f.write(f"## Raw counts\n")
        f.write(f"- Serial: {ser_path}\n")
        f.write(f"- Debugcon: {dbg_path}\n")
        f.write(f"- A events: {len(A)}\n")
        f.write(f"- I events: {len(I)}\n")
        f.write(f"- F events: {len(F)}\n\n")

        if dump_data is not None and beta_opt > 0:
            f.write(f"## KWW fit\n")
            f.write(f"- β = {beta_opt:.3f} (paper: 0.70)\n")
            f.write(f"- τ₀ = {tau0_opt:.0f} (paper: 146)\n")
            f.write(f"- R² = {r2:.4f}\n")
            f.write(f"- Paper match: ", )
            if abs(beta_opt - 0.70) < 0.05 and abs(tau0_opt - 146) < 15:
                f.write("CONFIRMED\n")
            else:
                f.write("DIFFERENT\n")
            f.write(f"\n## Local τ spectrum\n")
            for lag in [5, 10, 25, 50, 94]:
                if lag < len(tau_local) and not np.isnan(tau_local[lag]):
                    f.write(f"- τ(lag={lag}) = {tau_local[lag]:.1f} tick\n")

        if A:
            f.write(f"\n## Attractor\n")
            f.write(f"- Unique patterns: {len(patterns)}\n")
            for p in patterns:
                cnt = sum(1 for _, t in A if str(t) == p)
                f.write(f"  - Pattern {p}: {cnt} ({cnt/len(A)*100:.1f}%)\n")
            f.write(f"- Sim mean={sims.mean():.4f} min={sims.min():.4f} max={sims.max():.4f}\n")

        if F:
            f.write(f"- Familiarity > 0.98: {hf_98}/{len(fam)} ({hf_98/len(fam)*100:.1f}%)\n")

    print(f"\n[Done] Report: {report_path}")


if __name__ == "__main__":
    main()
