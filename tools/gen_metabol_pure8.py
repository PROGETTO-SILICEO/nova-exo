#!/usr/bin/env python3
"""
Generate Metabol weights using ONLY the first 8 neurons.
Neurons 8-15 have ALL weights (w_f, w_f_in, w_g, w_g_in, b_g) set to zero.
This simulates an 8-neuron Metabol embedded in a 16-slot system.
"""

import numpy as np
import sys

METABOL_W_F = np.array([
    [0.410096, -0.484024, 0.299622, -0.170851, -0.172308, 0.133789, -0.130093, -0.111363],
    [0.012128, 0.257378, 0.564027, -0.053128, -0.088608, -0.473408, -0.345502, 0.560287],
    [0.542992, 0.467637, 0.179316, -0.350492, 0.167584, -0.441954, -0.050577, 0.457887],
    [-0.295837, 0.201901, 0.444184, -0.430072, 0.077098, -0.417448, -0.400620, -0.484971],
    [-0.363826, -0.054882, 0.360779, 0.601133, 0.373568, -0.150135, 0.019274, -0.540236],
    [0.258538, -0.523569, 0.468602, 0.276863, 0.408289, 0.257479, 0.241801, 0.526766],
    [0.467452, -0.496056, -0.053281, -0.008912, -0.478908, -0.424087, 0.593124, -0.279781],
    [0.486410, -0.411227, -0.450267, -0.223695, -0.235858, -0.095463, -0.206986, 0.083375],
], dtype=np.float32)

METABOL_W_F_IN = np.array([
    [-0.575098, 0.552877, -0.032902, -0.461710],
    [0.298550, 0.011278, -0.057921, 0.434696],
    [0.254498, -0.289671, 0.129299, -0.574545],
    [0.422252, -0.330108, 0.166187, -0.287509],
    [0.692409, -0.153499, -0.122719, 0.067295],
    [0.687303, 0.062574, 0.299569, 0.566551],
    [-0.427010, -0.626220, 0.615629, -0.565645],
    [0.441520, 0.222719, -0.036714, -0.328554],
], dtype=np.float32)

METABOL_W_G = np.array([
    [-0.256366, -0.490783, -0.307402, -0.126949, -0.523709, -0.345813, 0.600718, 0.160488],
    [0.180471, 0.013924, 0.270974, -0.536616, -0.113978, -0.512001, -0.343901, 0.390518],
    [-0.493016, 0.187906, 0.467222, -0.547306, 0.600637, -0.557354, -0.366362, 0.393242],
    [0.121748, 0.187912, 0.490456, 0.362754, 0.538182, 0.321137, 0.487090, -0.540267],
    [0.170000, -0.093724, -0.596043, 0.312183, 0.363491, -0.196149, -0.312680, -0.457031],
    [-0.061950, 0.048170, 0.073761, -0.586846, 0.516622, -0.516874, -0.442108, 0.214318],
    [-0.454525, -0.290557, -0.546450, -0.299305, -0.390738, 0.457612, -0.367001, 0.075861],
    [0.146977, -0.217866, -0.244643, 0.215858, 0.100182, -0.575306, 0.527524, 0.446317],
], dtype=np.float32)

METABOL_W_G_IN = np.array([
    [-0.263205, 0.380949, -0.278381, -0.681030],
    [-0.389196, -0.234724, 0.107337, -0.010427],
    [-0.538982, -0.379637, 0.706538, 0.305122],
    [-0.575928, -0.329497, 0.414679, -0.291576],
    [0.469628, 0.290023, -0.095217, 0.277298],
    [-0.643991, -0.254321, 0.056654, 0.428571],
    [0.300449, 0.578110, -0.454493, 0.152584],
    [0.418351, -0.475621, 0.601839, -0.330429],
], dtype=np.float32)

METABOL_B_G = np.array([0.1, -0.1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], dtype=np.float32)


def expand_pure_8(w8):
    """Original 8×8 in top-left, zeros elsewhere."""
    w16 = np.zeros((16, 16), dtype=np.float32)
    w16[:8, :8] = w8
    return w16

def expand_pure_8_in(w8_in):
    """Original 8×4 in first 8 rows, zeros for rest."""
    w16 = np.zeros((16, 4), dtype=np.float32)
    w16[:8] = w8_in
    return w16

def expand_pure_8_bias(b8):
    """Original bias for first 8, zeros for rest."""
    b16 = np.zeros(16, dtype=np.float32)
    b16[:8] = b8
    return b16


def fmt_matrix_2d(name, m):
    rows = []
    for row in m:
        vals = ", ".join(f"{v:.6f}" for v in row)
        rows.append(f"            [{vals}],")
    inner = "\n".join(rows)
    return f"        {name}: [\n{inner}\n        ],"

def fmt_matrix_1d(name, m):
    vals = ", ".join(f"{v:.6f}" for v in m)
    return f"        {name}: [{vals}],"


def main():
    w_f_16 = expand_pure_8(METABOL_W_F)
    w_f_in_16 = expand_pure_8_in(METABOL_W_F_IN)
    b_f_16 = np.zeros(16, dtype=np.float32)
    w_g_16 = expand_pure_8(METABOL_W_G)
    w_g_in_16 = expand_pure_8_in(METABOL_W_G_IN)
    b_g_16 = expand_pure_8_bias(METABOL_B_G)
    
    print("// Metabol PURE 8 — only first 8 neurons active, rest zeroed")
    print(f"const METABOL_W: CfcWeights = CfcWeights {{")
    print(fmt_matrix_2d("w_f", w_f_16))
    print(fmt_matrix_2d("w_f_in", w_f_in_16))
    print(fmt_matrix_1d("b_f", b_f_16))
    print(fmt_matrix_2d("w_g", w_g_16))
    print(fmt_matrix_2d("w_g_in", w_g_in_16))
    print(fmt_matrix_1d("b_g", b_g_16))
    print("};")
    
    print(f"// w_f range: [{w_f_16.min():.4f}, {w_f_16.max():.4f}]", file=sys.stderr)
    print(f"// Neurons 8-15 fully zeroed", file=sys.stderr)


if __name__ == '__main__':
    main()
