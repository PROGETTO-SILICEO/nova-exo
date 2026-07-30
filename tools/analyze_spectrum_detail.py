#!/usr/bin/env python3
"""Analyze serial log — per-neuron spectrum, not mean activation.

Usage:
    python3 tools/analyze_spectrum_detail.py qemu-serial-fixed.log

Output:
    For each cell + neuron: β, τ₀
    Distribution of β across neurons within each cell.
"""

import sys
import re
import math
from collections import defaultdict

def parse_log(path):
    cells = defaultdict(list)
    pattern = re.compile(r'^([TCIM]):([0-9a-f]+):(.+)$')
    with open(path) as f:
        for line in f:
            m = pattern.match(line.strip())
            if not m:
                continue
            cell_code = m.group(1)
            tick = int(m.group(2), 16)
            values_str = m.group(3)
            values = [float(x) for x in re.findall(r'-?\d+\.\d+', values_str)]
            cell_name = {'T': 'Tatto', 'C': 'Chemio', 'M': 'Metabol', 'I': 'Integrat'}[cell_code]
            cells[cell_name].append((tick, values))
    return cells


def autocorrelation(signal, max_lag):
    n = len(signal)
    if n <= max_lag + 1:
        return []
    mean = sum(signal) / n
    var = sum((v - mean) ** 2 for v in signal)
    if var == 0:
        return [1.0] + [0.0] * max_lag
    ac = []
    for lag in range(max_lag + 1):
        num = sum((signal[i] - mean) * (signal[i + lag] - mean) for i in range(n - lag))
        ac.append(num / var)
    return ac


def fit_stretched(lags, ac):
    """Fit ACF to stretched exponential: exp(-(t/τ₀)^β). Returns (τ₀, β, R²) or None."""
    valid = [(t, a) for t, a in zip(lags, ac) if abs(a) > 0.05 and t > 0]
    if len(valid) < 5:
        return None
    ts, acs = zip(*valid)
    ts = [max(t, 1) for t in ts]
    acs = [max(a, 1e-10) for a in acs]
    
    n = len(ts)
    sum_x = sum(math.log(t) for t in ts)
    sum_y = sum(math.log(-math.log(a)) for a in acs)
    sum_xy = sum(math.log(t) * math.log(-math.log(a)) for t, a in zip(ts, acs))
    sum_x2 = sum(math.log(t) ** 2 for t in ts)
    
    denom = n * sum_x2 - sum_x ** 2
    if abs(denom) < 1e-10:
        return None
    beta = (n * sum_xy - sum_x * sum_y) / denom
    intercept = (sum_y - beta * sum_x) / n
    tau0 = math.exp(-intercept / beta) if beta != 0 else 0
    if tau0 <= 0 or beta <= 0:
        return None
    
    # R² in transformed space
    y_pred = [beta * math.log(t) + intercept for t in ts]
    ss_res = sum((y_pred[i] - sum_y / n) ** 2 for i in range(n))
    ss_tot = sum((acs[i] - sum_y / n) ** 2 for i in range(n))
    r2 = ss_res / ss_tot if ss_tot > 0 else 0
    return tau0, beta, r2


def main():
    path = sys.argv[1] if len(sys.argv) > 1 else "qemu-serial-fixed.log"
    print(f"📊 Analisi PER-NEURONE da: {path}")
    print("=" * 60)
    
    cells = parse_log(path)
    
    for cell_name in ['Tatto', 'Chemio', 'Metabol', 'Integrat']:
        data = cells.get(cell_name, [])
        if not data:
            continue
        ticks, values_list = zip(*data)
        n_ticks = len(ticks)
        n_neurons = len(values_list[0])
        
        print(f"\n📌 {cell_name}: {n_neurons} neuroni, {n_ticks} tick")
        print(f"   {'Neurone':<8} {'β':<8} {'τ₀':<12} {'R²':<8} {'Att.media':<10}")
        print(f"   {'-'*46}")
        
        betas = []
        tau0s = []
        
        for ni in range(n_neurons):
            signal = [v[ni] for v in values_list]
            mean_act = sum(signal) / len(signal)
            
            max_lag = min(500, n_ticks // 4)
            ac = autocorrelation(signal, max_lag)
            result = fit_stretched(list(range(len(ac))), ac)
            
            if result:
                tau0, beta, r2 = result
                betas.append(beta)
                tau0s.append(tau0)
                print(f"   N{ni:<6} {beta:<8.3f} {tau0:<12.1f} {r2:<8.4f} {mean_act:<10.4f}")
            else:
                print(f"   N{ni:<6} {'N/A':<8} {'N/A':<12} {'N/A':<8} {mean_act:<10.4f} (fit fallito, segnale piatto)")
        
        if betas:
            print(f"\n   📊 Statistiche β: media={sum(betas)/len(betas):.3f}, min={min(betas):.3f}, max={max(betas):.3f}")
        
        # Autocorrelation classificaion
        classes = {'semplice': 0, 'stretched': 0, 'lento': 0, 'rumore': 0}
        for b in betas:
            if b > 1.1: classes['semplice'] += 1
            elif 0.8 <= b <= 1.1: classes['stretched'] += 1
            elif 0.4 <= b < 0.8: classes['lento'] += 1
            else: classes['rumore'] += 1
        
        print(f"   Classificazione: β>1.1={classes['semplice']}, β0.8-1.1={classes['stretched']}, β0.4-0.8={classes['lento']}, β<0.4={classes['rumore']}")

    # Energy per cell
    print("\n" + "=" * 60)
    print("ENERGIA TOTALE PER CELLA (somma dei quadrati delle attivazioni)")
    print("=" * 60)
    for cell_name in ['Tatto', 'Chemio', 'Metabol', 'Integrat']:
        data = cells.get(cell_name, [])
        if not data:
            continue
        _, values_list = zip(*data)
        energies = [sum(v*v for v in vals) for vals in values_list]
        max_lag = min(500, len(energies) // 4)
        ac = autocorrelation(energies, max_lag)
        result = fit_stretched(list(range(len(ac))), ac)
        
        print(f"\n{cell_name}: energia media={sum(energies)/len(energies):.4f}")
        if result:
            print(f"   β energia={result[1]:.3f}, τ₀ energia={result[0]:.1f} tick, R²={result[2]:.4f}")


if __name__ == '__main__':
    main()
