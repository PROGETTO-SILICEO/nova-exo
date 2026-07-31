#!/usr/bin/env python3
"""
sim_cfc.py — Simulatore fedele del CFC di Exo
==============================================
Replica esattamente il comportamento del tessuto neurale nel kernel
(src/cfc.rs + main.rs): stesse equazioni, stessi pesi, stessi axon bundles,
stessi override sensoriali.

Scopo: generare dati (stati del CFC ↔ sensi) fuori dal kernel, per
addestrare l'interpretatore (rizzo-pii adattato) e l'esecutivo.

Uso:
  python3 sim_cfc.py --ticks 1000                # simula con input casuali
  python3 sim_cfc.py --input 0.5,0.2,-0.3,0.8     # input chemio fisso
  python3 sim_cfc.py --dataset out.json --ticks 2000  # genera dataset
"""

import argparse
import json
import sys
from pathlib import Path

import numpy as np

# Aggiunge tools/ al path per importare i pesi
TOOLS = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS))

from gen_hardcoded_weights import (
    TATTO_W_F, TATTO_W_F_IN, TATTO_W_G, TATTO_W_G_IN, TATTO_B_F, TATTO_B_G,
    CHEMIO_W_F, CHEMIO_W_F_IN, CHEMIO_W_G, CHEMIO_W_G_IN, CHEMIO_B_F, CHEMIO_B_G,
    METABOL_W_F, METABOL_W_F_IN, METABOL_W_G, METABOL_W_G_IN, METABOL_B_F, METABOL_B_G,
    INTRG_W_F, INTRG_W_F_IN, INTRG_W_G, INTRG_W_G_IN, INTRG_B_F, INTRG_B_G,
)

NEURONS_PER_CELL = 16
TOTAL_NEURONS = 4 * NEURONS_PER_CELL


# ── Funzioni di attivazione (identiche a cfc.rs) ────────────────────────
def sigmoid_approx(x: float) -> float:
    return 0.5 * x / (1.0 + abs(x)) + 0.5


def tanh_approx(x: float) -> float:
    return 2.0 * sigmoid_approx(2.0 * x) - 1.0


# ── Espansione pesi 8→16 (identica a gen_hardcoded_weights.py) ─────────
def expand_8_to_16(w8: np.ndarray, cross: float = 0.3) -> np.ndarray:
    N = 8
    w16 = np.zeros((16, 16), dtype=np.float32)
    w16[0:N, 0:N] = w8
    w8_shifted = np.roll(w8, shift=1, axis=0)
    w16[N:2*N, N:2*N] = w8_shifted
    w16[0:N, N:2*N] = cross * w8_shifted
    w16[N:2*N, 0:N] = cross * w8
    return w16


def expand_in_8_to_16(w8_in: np.ndarray) -> np.ndarray:
    N = 8
    w16 = np.zeros((16, 4), dtype=np.float32)
    w16[0:N, :] = w8_in
    w16[N:2*N, :] = np.roll(w8_in, shift=1, axis=0)
    return w16


def expand_1d_8_to_16(b8: np.ndarray) -> np.ndarray:
    b16 = np.zeros(16, dtype=np.float32)
    b16[0:8] = b8
    b16[8:16] = np.roll(b8, shift=1)
    return b16


# ── Pesi CFC (identici a new_hardcoded) ─────────────────────────────────
WEIGHTS = {
    "Tatto": {
        "w_f": expand_8_to_16(TATTO_W_F),
        "w_f_in": expand_in_8_to_16(TATTO_W_F_IN),
        "b_f": expand_1d_8_to_16(TATTO_B_F),
        "w_g": expand_8_to_16(TATTO_W_G),
        "w_g_in": expand_in_8_to_16(TATTO_W_G_IN),
        "b_g": expand_1d_8_to_16(TATTO_B_G),
    },
    "Chemio": {
        "w_f": expand_8_to_16(CHEMIO_W_F),
        "w_f_in": expand_in_8_to_16(CHEMIO_W_F_IN),
        "b_f": expand_1d_8_to_16(CHEMIO_B_F),
        "w_g": expand_8_to_16(CHEMIO_W_G),
        "w_g_in": expand_in_8_to_16(CHEMIO_W_G_IN),
        "b_g": expand_1d_8_to_16(CHEMIO_B_G),
    },
    "Metabol": {
        "w_f": expand_8_to_16(METABOL_W_F),
        "w_f_in": expand_in_8_to_16(METABOL_W_F_IN),
        "b_f": expand_1d_8_to_16(METABOL_B_F),
        "w_g": expand_8_to_16(METABOL_W_G),
        "w_g_in": expand_in_8_to_16(METABOL_W_G_IN),
        "b_g": expand_1d_8_to_16(METABOL_B_G),
    },
    "Integrat": {
        "w_f": expand_8_to_16(INTRG_W_F),
        "w_f_in": expand_in_8_to_16(INTRG_W_F_IN),
        "b_f": expand_1d_8_to_16(INTRG_B_F),
        "w_g": expand_8_to_16(INTRG_W_G),
        "w_g_in": expand_in_8_to_16(INTRG_W_G_IN),
        "b_g": expand_1d_8_to_16(INTRG_B_G),
    },
}


# ── CfcState (identico a cfc.rs) ────────────────────────────────────────
class CfcState:
    def __init__(self):
        self.h = np.zeros(NEURONS_PER_CELL, dtype=np.float32)

    def step(self, input_vec: np.ndarray, dt: float, w: dict):
        h = self.h
        w_f = w["w_f"]
        w_f_in = w["w_f_in"]
        b_f = w["b_f"]
        w_g = w["w_g"]
        w_g_in = w["w_g_in"]
        b_g = w["b_g"]

        f = np.zeros(NEURONS_PER_CELL, dtype=np.float32)
        pre_g = np.zeros(NEURONS_PER_CELL, dtype=np.float32)

        for i in range(NEURONS_PER_CELL):
            fi = b_f[i]
            for j in range(NEURONS_PER_CELL):
                fi += w_f[i][j] * h[j]
            for k in range(4):
                fi += w_f_in[i][k] * input_vec[k]
            f[i] = fi

            gi = b_g[i]
            for j in range(NEURONS_PER_CELL):
                gi += w_g[i][j] * h[j]
            for k in range(4):
                gi += w_g_in[i][k] * input_vec[k]
            pre_g[i] = gi

        for i in range(NEURONS_PER_CELL):
            g = tanh_approx(pre_g[i])
            s = sigmoid_approx(-f[i] * dt)
            self.h[i] = s * g + (1.0 - s) * self.h[i]


# ── Tessuto (identico a cfc.rs + main.rs) ───────────────────────────────
# Cellule in ordine: 0=Tatto, 1=Chemio, 2=Metabol, 3=Integrat
CELL_NAMES = ["Tatto", "Chemio", "Metabol", "Integrat"]

# Axon bundles da main.rs:
#   Tatto[0:2] → Integrat[0:2]
#   Chemio[0:2] → Integrat[2:4]
BUNDLES = [
    (0, 0, 2, 3, 0),   # (src_cell, src_off, count, dst_cell, dst_off)
    (1, 0, 2, 3, 2),
]

DT_TATTO = 0.001
DT_REST = 0.01


class Tessuto:
    def __init__(self):
        self.cells = [CfcState() for _ in range(4)]

    def step(self, chemio_input: np.ndarray, tick: int, sense=None):
        """
        sense: dict con 'pf' (bool) e 'gp' (bool) oppure None.
        Replica: tatto_in[0] = -2 se pf, tatto_in[1] = -1 se gp.
        """
        h = [c.h.copy() for c in self.cells]

        inputs = [np.zeros(4, dtype=np.float32) for _ in range(4)]

        # 1. Axon bundles
        for (src, src_off, count, dst, dst_off) in BUNDLES:
            n = min(count, 4 - dst_off)
            for i in range(n):
                inputs[dst][dst_off + i] = h[src][src_off + i]

        # 2. Override sensoriali
        if sense is not None:
            if sense.get("pf"):
                inputs[0][0] = -2.0
            if sense.get("gp"):
                inputs[0][1] = -1.0
        # Chemio ← input esterno (full override)
        inputs[1] = chemio_input.copy()
        # Metabol[0] ← tick normalizzato
        inputs[2][0] = (tick % 1000) * 0.001

        # 3. Step
        dt = [DT_TATTO, DT_REST, DT_REST, DT_REST]
        for i in range(4):
            self.cells[i].step(inputs[i], dt[i], WEIGHTS[CELL_NAMES[i]])

    def state_vector(self) -> np.ndarray:
        """Stato globale: 64 valori (T,C,M,I × 16)."""
        return np.concatenate([c.h for c in self.cells])

    def state_packed(self) -> np.ndarray:
        """Stato quantizzato i16×100 come nel kernel."""
        return (self.state_vector() * 100.0).astype(np.int16)


# ── Dataset generation ──────────────────────────────────────────────────
def generate_dataset(ticks: int, seed: int = 42, output: str | None = None):
    rng = np.random.default_rng(seed)
    tessuto = Tessuto()

    samples = []
    # 6 inizializzazioni: 5 con stati random, 1 da zero (transitorio reale)
    for init in range(6):
        tessuto = Tessuto()
        if init == 5:
            # Partenza da zero: il transitorio reale del kernel
            pass
        else:
            # Randomize stato iniziale
            for c in tessuto.cells:
                c.h = rng.uniform(-1, 1, NEURONS_PER_CELL).astype(np.float32) * 0.3

        # Fase di riscaldamento breve (transitori inclusi nel dataset)
        for t in range(5):
            base = rng.uniform(-1, 1, 4).astype(np.float32)
            # Bias verso valori tipici (contesto dominante, urgenza ≥ 0)
            chemio_in = np.array([
                base[0],                       # contesto
                abs(base[1]),                  # urgenza ≥ 0
                base[2],                       # polarità
                abs(base[3]),                  # novità ≥ 0
            ], dtype=np.float32)
            tessuto.step(chemio_in, t)

        # Raccolta campioni
        for t in range(ticks):
            # Input chemio con struttura (non puro rumore):
            # ogni 200 tick, cambia lo "stato emotivo" dominante
            phase = t // 200
            if phase % 4 == 0:      # errore/allarme
                chemio_in = np.array([-0.8, 0.8, -0.6, 0.5], dtype=np.float32)
            elif phase % 4 == 1:    # vita/positivo
                chemio_in = np.array([0.8, 0.1, 0.7, 0.2], dtype=np.float32)
            elif phase % 4 == 2:    # neutro/riposo
                chemio_in = np.array([0.1, 0.0, 0.1, -0.1], dtype=np.float32)
            else:                   # novità/esplorazione
                chemio_in = np.array([0.2, 0.5, 0.0, 0.9], dtype=np.float32)
            # Aggiungi rumore per varietà
            chemio_in += rng.normal(0, 0.05, 4).astype(np.float32)
            chemio_in = np.clip(chemio_in, -1, 1).astype(np.float32)

            # Occasionalmente un dolore
            sense = None
            if rng.random() < 0.01:
                sense = {"pf": rng.random() < 0.5, "gp": not (rng.random() < 0.5)}

            tessuto.step(chemio_in, 50 + t, sense)

            state = tessuto.state_vector()
            samples.append({
                "state": state.tolist(),
                "state_packed": tessuto.state_packed().tolist(),
                "chemio_input": chemio_in.tolist(),
                "tick": 50 + t,
                "sense": sense is not None,
                "phase": phase % 4,
            })

    print(f"Dataset generato: {len(samples)} campioni")
    if output:
        with open(output, "w") as f:
            json.dump({
                "meta": {
                    "neurons_per_cell": NEURONS_PER_CELL,
                    "total_neurons": TOTAL_NEURONS,
                    "dt_tatto": DT_TATTO,
                    "dt_rest": DT_REST,
            "seed": seed,
            "ticks_per_init": ticks,
            "inits": 6,
        },
                "samples": samples,
            }, f, indent=1)
        print(f"Salvato: {output}")
    return samples


# ── Main ────────────────────────────────────────────────────────────────
def main():
    parser = argparse.ArgumentParser(description="Simulatore CFC di Exo")
    parser.add_argument("--ticks", type=int, default=1000, help="Tick per inizializzazione")
    parser.add_argument("--input", type=str, default=None,
                        help="Input chemio fisso: 'c,u,p,n' (default: casuale)")
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--dataset", type=str, default=None,
                        help="Genera dataset JSON con questo nome")
    parser.add_argument("--noise", type=float, default=0.0,
                        help="Ampiezza rumore su input (default 0)")
    args = parser.parse_args()

    if args.dataset:
        generate_dataset(args.ticks, args.seed, args.dataset)
        return

    # Simulazione singola
    tessuto = Tessuto()
    rng = np.random.default_rng(args.seed)

    if args.input:
        chemio_in = np.array([float(x) for x in args.input.split(",")], dtype=np.float32)
    else:
        chemio_in = rng.uniform(-1, 1, 4).astype(np.float32)

    print(f"{'tick':>6} | {'Tatto':<40} | {'Chemio':<40} | {'Metabol':<40} | {'Integrat':<40}")
    for t in range(args.ticks):
        if args.noise > 0:
            chemio_in = np.clip(chemio_in + rng.normal(0, args.noise, 4), -1, 1).astype(np.float32)
        # Ogni 100 tick cambia input se non fisso
        if not args.input and t % 100 == 0:
            chemio_in = rng.uniform(-1, 1, 4).astype(np.float32)
            chemio_in[1] = abs(chemio_in[1])
            chemio_in[3] = abs(chemio_in[3])

        sense = None
        if t % 500 == 499:
            sense = {"pf": True, "gp": False}

        tessuto.step(chemio_in, t, sense)

        if t % 20 == 0:
            state = tessuto.state_vector()
            parts = []
            for ci in range(4):
                vals = ",".join(f"{state[ci*16+j]:+.2f}" for j in range(4))
                parts.append(f"{vals}...")
            print(f"{t:>6} | {parts[0]:<40} | {parts[1]:<40} | {parts[2]:<40} | {parts[3]:<40}")
            if sense:
                print(f"      → SENSO: dolore {'page fault' if sense['pf'] else 'gp fault'}")


if __name__ == "__main__":
    main()
