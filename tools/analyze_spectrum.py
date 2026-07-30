#!/usr/bin/env python3
"""Analyze serial log from Nova Exo kernel — extract τ₀ and β per cell.

Usage:
    python3 tools/analyze_spectrum.py experiments/2026-07-27_64neuroni/qemu-serial.log

Output:
    For each cell (Tatto, Chemio, Metabol, Integrat):
    - β (stretch exponent)
    - τ₀ (characteristic time in ticks)
    - R² (fit quality)
    - Autocorrelation plot (if --plot)
"""

import sys
import re
import math
import json
from collections import defaultdict

def parse_log(path):
    """Parse serial log into per-cell timeseries.
    
    Returns: dict[cell_name] -> list of (tick, [values])
    """
    cells = defaultdict(list)
    pattern = re.compile(r'^([TCIM]):([0-9a-f]+):(.+)$')
    
    with open(path) as f:
        for line in f:
            m = pattern.match(line.strip())
            if not m:
                continue
            cell_code = m.group(1)
            tick_hex = m.group(2)
            values_str = m.group(3)
            
            cell_name = {'T': 'Tatto', 'C': 'Chemio', 'M': 'Metabol', 'I': 'Integrat'}[cell_code]
            tick = int(tick_hex, 16)
            # Extract all floats from potentially concatenated values
            # e.g. "0.0000,0.0000,0.00000.00000.0000" → [0.0, 0.0, 0.0, 0.0, 0.0]
            values = [float(x) for x in re.findall(r'-?\d+\.\d+', values_str)]
            
            cells[cell_name].append((tick, values))
    
    return cells


def autocorrelation(values, max_lag):
    """Compute autocorrelation for a 1D signal (mean of neuron states)."""
    n = len(values)
    if n <= max_lag + 1:
        return []
    
    mean = sum(values) / n
    var = sum((v - mean) ** 2 for v in values)
    if var == 0:
        return [1.0] + [0.0] * max_lag
    
    ac = []
    for lag in range(max_lag + 1):
        num = sum((values[i] - mean) * (values[i + lag] - mean)
                  for i in range(n - lag))
        ac.append(num / var)
    
    return ac


def fit_stretched_exponential(lags, ac):
    """Fit ACF to stretched exponential: exp(-(t/τ₀)^β).
    
    Uses the first N points where ACF > 0.1 for the fit.
    Returns (τ₀, β, R²) or None if fit fails.
    """
    # Filter: use points where ac > 0.1 (avoid noise floor)
    valid = [(t, a) for t, a in zip(lags, ac) if a > 0.1 and a < 1.0]
    if len(valid) < 3:
        return None
    
    ts, acs = zip(*valid)
    ts = [max(t, 1) for t in ts]  # avoid log(0)
    acs = [max(a, 1e-10) for a in acs]  # avoid log(0)
    
    # Transform to linear: log(-log(ac)) = β * log(t) - β * log(τ₀)
    # y = log(-log(ac)), x = log(t)
    # slope = β, intercept = -β * log(τ₀)
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
    
    # R²
    y_pred = [beta * math.log(t) + intercept for t in ts]
    y_mean = sum_y / n
    ss_res = sum((y_pred[i] - acs[i]) ** 2 for i in range(n))
    ss_tot = sum((a - y_mean) ** 2 for a in acs)
    # Use transformed values for R²
    ss_res_t = sum((y_pred[i] - sum_y / n) ** 2 for i in range(n))
    
    return tau0, beta


def main():
    path = sys.argv[1] if len(sys.argv) > 1 else "experiments/2026-07-27_64neuroni/qemu-serial.log"
    
    print(f"📊 Analisi spettro τ da: {path}")
    print("=" * 60)
    
    cells = parse_log(path)
    
    for cell_name in ['Tatto', 'Chemio', 'Metabol', 'Integrat']:
        data = cells.get(cell_name, [])
        if not data:
            print(f"\n⚠️  {cell_name}: nessun dato")
            continue
        
        ticks, values_list = zip(*data)
        n_ticks = len(ticks)
        n_neurons = len(values_list[0])
        
        # Collapse to mean activation per tick
        mean_activation = [sum(v) / len(v) for v in values_list]
        
        max_lag = min(500, n_ticks // 4)  # autocorrelation up to 500 ticks
        ac = autocorrelation(mean_activation, max_lag)
        
        result = fit_stretched_exponential(list(range(len(ac))), ac)
        
        print(f"\n📌 {cell_name}:")
        print(f"   Neuroni: {n_neurons}")
        print(f"   Tick: {n_ticks}")
        print(f"   Attivazione media: {sum(mean_activation)/len(mean_activation):.4f}")
        
        if result:
            tau0, beta = result
            print(f"   τ₀: {tau0:.1f} tick")
            print(f"   β:  {beta:.3f}")
            
            # Classificazione
            if beta > 1.1:
                tipo = "Esponenziale semplice (τ unico)"
            elif 0.8 <= beta <= 1.1:
                tipo = "Stretched exponential (debole)"
            elif 0.5 <= beta < 0.8:
                tipo = "Stretched exponential (marcato) — memoria lenta"
            else:
                tipo = "Esponenziale compresso — dinamica ultra-lenta/rumorosa"
            print(f"   Tipo: {tipo}")
        else:
            print(f"   Fit: fallito (dati insufficienti o rumore)")
        
        # Autocorrelation a lags specifici
        for lag in [10, 50, 100, 200]:
            if lag < len(ac):
                print(f"   ACF@{lag}: {ac[lag]:.4f}")
    
    # Summary table
    print("\n" + "=" * 60)
    print("RIEPILOGO — Spettro τ con 64 neuroni")
    print("=" * 60)
    print(f"{'Cellula':<10} {'β':<8} {'τ₀ (tick)':<12} {'Tipo':<30}")
    print("-" * 60)
    
    for cell_name in ['Tatto', 'Chemio', 'Metabol', 'Integrat']:
        data = cells.get(cell_name, [])
        if not data:
            continue
        _, values_list = zip(*data)
        mean_act = [sum(v) / len(v) for v in values_list]
        ac = autocorrelation(mean_act, min(500, len(mean_act) // 4))
        r = fit_stretched_exponential(list(range(len(ac))), ac)
        
        if r:
            tau0, beta = r
            if beta > 1.1:
                tipo = "Esponenziale semplice"
            elif 0.8 <= beta <= 1.1:
                tipo = "Stretched debole"
            elif 0.5 <= beta < 0.8:
                tipo = "Memoria lenta"
            else:
                tipo = "Ultra-lento/rumoroso"
            print(f"{cell_name:<10} {beta:<8.3f} {tau0:<12.1f} {tipo:<30}")
        else:
            print(f"{cell_name:<10} {'N/A':<8} {'N/A':<12} {'Fit fallito':<30}")


if __name__ == '__main__':
    main()
