"""
Baselines per validazione Nova Exo.
Confronto sistematico: RNN vanilla, AR(1), Exo-random, Exo reale.

Tutte producono matrici (N, 32) con la stessa struttura:
  [0..7]   Tatto h
  [8..15]  Chemio h
  [16..23] Metabol h
  [24..31] Integrat h

Usage:
  from baselines import run_vanilla_rnn, run_ar1, run_exo_random
  cells = run_vanilla_rnn(n_ticks=5000)
"""

import numpy as np

SEED = 42

# ── Shared helpers ───────────────────────────────────────────────────────

def sigmoid(x):
    return 0.5 * x / (1.0 + np.abs(x)) + 0.5

def tanh_approx(x):
    return 2.0 * sigmoid(2.0 * x) - 1.0


# ── Stimulus schedule (same as real Exo analysis) ────────────────────────

def default_stimuli(n_ticks):
    """Return (N, 4) input matrix matching analyze_v010 schedule."""
    rng = np.random.default_rng(SEED)
    I = np.zeros((n_ticks, 4), dtype=np.float64)
    # sparse impulses at same intervals as analyze_v010
    I[50:55, 0] = 0.2      # ~1s settle + 0.2
    I[150:155, 0] = 0.0
    I[250:255, 1] = 0.5    # chemio impulse
    I[350:355, 0] = 1.0    # strong tattoo impulse
    I[500:505, 0] = 0.5
    I[500:505, 1] = 0.5
    # low-level noise
    I += rng.normal(0, 0.01, I.shape)
    return I


# ═══════════════════════════════════════════════════════════════════════════
# 1. Vanilla RNN (32 neuroni, tanh, no gating)
# ═══════════════════════════════════════════════════════════════════════════

class VanillaRNN:
    """Simple RNN: h(t) = tanh(W·h(t-1) + W_in·input(t) + b)."""

    def __init__(self, seed=SEED):
        rng = np.random.default_rng(seed)
        scale = 0.1
        # 32×32 recurrent
        self.W = rng.normal(0, scale, (32, 32)).astype(np.float64)
        # 32×4 input
        self.W_in = rng.normal(0, scale, (32, 4)).astype(np.float64)
        self.b = rng.normal(0, scale, 32).astype(np.float64)
        self.h = np.zeros(32, dtype=np.float64)

    def step(self, inp, dt):
        """One tick. inp = 4-dim array. dt ignored (no continuous-time in vanilla RNN)."""
        pre = self.W @ self.h + self.W_in @ inp + self.b
        self.h = np.tanh(pre)
        return self.h


# ═══════════════════════════════════════════════════════════════════════════
# 2. AR(1) per cellula
# ═══════════════════════════════════════════════════════════════════════════

class AR1:
    """Each cell group follows AR(1) with different τ."""

    # τ values: Tatto fast, Chemio medium, Metabol slow, Integrat medium-slow
    TAU = {"tatto": 5.0, "chemio": 10.0, "metabol": 20.0, "integrat": 30.0}
    # cell mapping: [0..7]=tatto, [8..15]=chemio, [16..23]=metabol, [24..31]=integrat
    SLICES = {"tatto": (0, 8), "chemio": (8, 16), "metabol": (16, 24), "integrat": (24, 32)}

    def __init__(self, seed=SEED):
        self.rng = np.random.default_rng(seed)
        self.h = np.zeros(32, dtype=np.float64)
        self.alpha = {}
        for name, tau in self.TAU.items():
            self.alpha[name] = np.exp(-1.0 / tau)

    def step(self, inp, dt):
        """One tick at 100Hz."""
        for name, (lo, hi) in self.SLICES.items():
            a = self.alpha[name]
            noise = self.rng.normal(0, 0.1, hi - lo)
            self.h[lo:hi] = a * self.h[lo:hi] + (1.0 - a) * noise
        return self.h


# ═══════════════════════════════════════════════════════════════════════════
# 3. Exo-random (stessa architettura CfC, pesi fissi, no sedimentazione)
# ═══════════════════════════════════════════════════════════════════════════

class CfcCell:
    """Single CfC cell (8 neurons), same equation as nova-exo."""

    def __init__(self, rng, n_inputs):
        scale = 0.5
        self.h = np.zeros(8, dtype=np.float64)
        self.w_f = rng.normal(0, scale, (8, 8)).astype(np.float64)
        self.w_f_in = rng.normal(0, scale, (8, n_inputs)).astype(np.float64)
        self.b_f = rng.normal(0, scale, 8).astype(np.float64)
        self.w_g = rng.normal(0, scale, (8, 8)).astype(np.float64)
        self.w_g_in = rng.normal(0, scale, (8, n_inputs)).astype(np.float64)
        self.b_g = rng.normal(0, scale, 8).astype(np.float64)

    def step(self, inp, dt):
        f = self.b_f.copy()
        for i in range(8):
            f[i] += self.w_f[i] @ self.h + self.w_f_in[i] @ inp
        g = self.b_g.copy()
        for i in range(8):
            g[i] += self.w_g[i] @ self.h + self.w_g_in[i] @ inp
        for i in range(8):
            gate = sigmoid(-f[i] * dt)
            self.h[i] = gate * tanh_approx(g[i]) + (1.0 - gate) * self.h[i]
        return self.h


class ExoRandom:
    """Same LTC/CfC architecture as real Exo. Fixed random weights.
    4 cells × 8 neurons, axon routing, NO attractor, NO sedimentation."""

    def __init__(self, seed=SEED):
        rng = np.random.default_rng(seed)
        self.tatto = CfcCell(rng, 4)
        self.chemio = CfcCell(rng, 4)
        self.metabol = CfcCell(rng, 4)
        self.integrat = CfcCell(rng, 4)

    def step(self, inp, dt_vals):
        """One tick. dt_vals = (dt_tatto, dt_rest).
        Axon routing mirrors real Exo: Tatto[0..1]+Chemio[0..1] → Integrat input."""
        inp_tatto = np.array([inp[0], inp[1], 0.0, 0.0], dtype=np.float64)
        inp_chemio = np.array([inp[2], inp[3], 0.0, 0.0], dtype=np.float64)
        inp_metabol = np.array([inp[0], inp[1], inp[2], inp[3]], dtype=np.float64)

        self.tatto.step(inp_tatto, dt_vals[0])
        self.chemio.step(inp_chemio, dt_vals[1])
        self.metabol.step(inp_metabol, dt_vals[1])

        # Axon bundles: Tatto[0..1] + Chemio[0..1] → Integrat[0..3]
        integrat_inp = np.array([
            self.tatto.h[0], self.tatto.h[1],
            self.chemio.h[0], self.chemio.h[1],
        ], dtype=np.float64)
        # Sense injection: Tatto[2] from first input element, Chemio[2] from second
        integrat_inp_sense = integrat_inp.copy()
        integrat_inp_sense[0] += inp[0] * 0.5
        integrat_inp_sense[2] += inp[1] * 0.5
        self.integrat.step(integrat_inp_sense, dt_vals[1])

        return np.concatenate([
            self.tatto.h, self.chemio.h, self.metabol.h, self.integrat.h,
        ])


# ═══════════════════════════════════════════════════════════════════════════
# 4. Stress test: sedimentazione 100k tick
# ═══════════════════════════════════════════════════════════════════════════

def sediment_stress_test(n_ticks=100000, sample_every=1000):
    """Run synthetic Exo with attractor+sedimentation for n_ticks.
    Uses a FIXED attractor pattern (not random noise) to simulate realistic
    convergence: weights approach the attractor's input signature.
    Returns (ticks, weight_norms) arrays.
    weight_norms[:, 0] = Frobenius norm of W_f_in
    weight_norms[:, 1] = max |W_ij|
    """
    rng = np.random.default_rng(SEED)
    scale = 0.5
    w_f_in = rng.normal(0, scale, (8, 4)).astype(np.float64)
    w0_norm = np.linalg.norm(w_f_in, 'fro')

    # Fixed attractor pattern: Integrat input from Tatto[0..1]+Chemio[0..1]
    attractor_input = np.array([0.5, -0.3, 0.8, 0.1], dtype=np.float64)

    ticks = []
    norms = []

    for t in range(n_ticks):
        if t % sample_every == 0:
            ticks.append(t)
            norms.append([
                np.linalg.norm(w_f_in, 'fro'),
                np.abs(w_f_in).max(),
                np.linalg.norm(w_f_in, 'fro') / w0_norm,
            ])

        # Simulate attractor recall every tick with high similarity
        sim = 0.95
        alpha_sed = 0.0001
        for i in range(8):
            for j in range(4):
                w_f_in[i][j] += alpha_sed * sim * (attractor_input[j] - w_f_in[i][j])

    return np.array(ticks), np.array(norms)


# ═══════════════════════════════════════════════════════════════════════════
# 5. Convenience: run all baselines
# ═══════════════════════════════════════════════════════════════════════════

CELL_SLICES = {
    "tatto": slice(0, 8),
    "chemio": slice(8, 16),
    "metabol": slice(16, 24),
    "integrat": slice(24, 32),
}

def run_baseline(model_cls, n_ticks=5000, seed=SEED):
    """Run a baseline model for n_ticks. Returns (N, 32) array."""
    import numpy as np
    stimuli = default_stimuli(n_ticks)
    model = model_cls(seed=seed)
    history = np.zeros((n_ticks, 32), dtype=np.float64)
    dt_tatto = 0.001
    dt_rest = 0.01
    for t in range(n_ticks):
        inp = stimuli[t]
        if model_cls == ExoRandom:
            h = model.step(inp, (dt_tatto, dt_rest))
        else:
            h = model.step(inp, dt_rest)
        history[t] = h
    return history


# ═══════════════════════════════════════════════════════════════════════════
# 6. PD index per baseline
# ═══════════════════════════════════════════════════════════════════════════

def compute_pd(cells, impulse_tick, window=48):
    """Compute PD index from a single run with single+double impulses.
    cells: (N, 32) array
    impulse_tick: the tick where the first impulse fires
    Returns (pd_index, rms_control, rms_test, gap_ticks).
    """
    # Find impulse peaks in Chemio[0] (= cells[:, 8])
    chemio0 = np.abs(cells[:, 8])
    peaks = []
    for i in range(1, len(chemio0) - 1):
        if chemio0[i] >= 0.1 and chemio0[i] >= chemio0[i - 1] and chemio0[i] >= chemio0[i + 1]:
            j = i
            while j + 1 < len(chemio0) and chemio0[j + 1] == chemio0[i]:
                j += 1
            peaks.append((i + j) // 2)

    if len(peaks) < 1:
        return 0.0, 0.0, 0.0, 0

    # Control: response to first impulse
    imp0 = peaks[0]
    lo = max(0, imp0 - 8)
    baseline = cells[lo:imp0, 24:28].mean(axis=0)
    hi = min(len(cells), imp0 + window)
    ctrl_resp = cells[imp0:hi, 24:28] - baseline

    if len(peaks) < 2:
        # Single impulse only, PD is theoretical 1.0
        n = len(ctrl_resp)
        rms_c = np.sqrt((ctrl_resp[:n] ** 2).mean()) + 1e-9
        return 1.0, rms_c, rms_c, 0

    # Test: response to second impulse
    imp1 = peaks[1]
    gap = imp1 - imp0
    lo2 = max(0, imp1 - 8)
    baseline2 = cells[lo2:imp1, 24:28].mean(axis=0)
    hi2 = min(len(cells), imp1 + window)
    test_resp = cells[imp1:hi2, 24:28] - baseline2

    n = min(len(ctrl_resp), len(test_resp))
    if n < 4:
        return 0.0, 0.0, 0.0, gap

    diff = test_resp[:n] - ctrl_resp[:n]
    rms_c = np.sqrt((ctrl_resp[:n] ** 2).mean()) + 1e-9
    rms_t = np.sqrt((test_resp[:n] ** 2).mean()) + 1e-9
    pd = float(np.sqrt((diff ** 2).mean()) / rms_c)
    return pd, rms_c, rms_t, gap
