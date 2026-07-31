#!/usr/bin/env python3
"""
train_interpreter.py — Interprete dello stato CFC (prototipo)
==============================================================
Verifica se lo stato del CFC (64 valori) è interpretabile:
  1. Regressione 64 → chemio_input (leggere il corpo)
  2. Classificazione 64 → fase/concept (dare senso)

Se il modello lineare basta, la distillazione nel kernel è una matrice
64×4 (delta rule, addestrabile online come PFM).
Se serve una MLP, si esporta un piccolo percettrone a uno strato.

Uso:
  python3 tools/train_interpreter.py --dataset dataset/interpreter/state_chemio_dataset.json
"""

import argparse
import json
import sys
from pathlib import Path

import numpy as np

TOOLS = Path(__file__).resolve().parent


def load_dataset(path: str):
    with open(path) as f:
        data = json.load(f)
    X = np.array([s["state"] for s in data["samples"]], dtype=np.float32)
    Y_chemio = np.array([s["chemio_input"] for s in data["samples"]], dtype=np.float32)
    Y_phase = np.array([s["phase"] for s in data["samples"]], dtype=np.int64)
    return X, Y_chemio, Y_phase, data


def linear_regression(X, Y, lam: float = 1e-3, with_bias: bool = True):
    """Minimi quadrati con regolarizzazione ridge.
    with_bias=True: aggiunge colonna di 1 (bias esplicito).
    """
    n, d = X.shape
    if with_bias:
        Xb = np.hstack([X, np.ones((n, 1), dtype=np.float32)])
        d += 1
    else:
        Xb = X
    A = Xb.T @ Xb + lam * np.eye(d, dtype=np.float32)
    B = Xb.T @ Y
    W = np.linalg.solve(A, B)
    return W  # (d+1) × k se with_bias, altrimenti d × k


def evaluate_regression(X, Y, W):
    pred = predict_with_bias(X, W)
    ss_res = np.sum((Y - pred) ** 2)
    ss_tot = np.sum((Y - Y.mean(axis=0)) ** 2)
    r2 = 1.0 - ss_res / ss_tot
    mae = np.mean(np.abs(Y - pred))
    return r2, mae, pred


def predict_with_bias(X, W):
    """Predizione con W che include il bias come ultima riga."""
    n = X.shape[0]
    Xb = np.hstack([X, np.ones((n, 1), dtype=np.float32)])
    return Xb @ W


def train_mlp(X, Y, hidden=16, epochs=500, lr=0.01, seed=42):
    """MLP 64→hidden→4 con ReLU, addestrata con SGD (per distillazione)."""
    rng = np.random.default_rng(seed)
    d, k = X.shape[1], Y.shape[1]
    W1 = rng.normal(0, 0.1, (d, hidden)).astype(np.float32)
    b1 = np.zeros(hidden, dtype=np.float32)
    W2 = rng.normal(0, 0.1, (hidden, k)).astype(np.float32)
    b2 = np.zeros(k, dtype=np.float32)

    n = X.shape[0]
    for epoch in range(epochs):
        # Mini-batch
        idx = rng.permutation(n)[:256]
        xb = X[idx]
        yb = Y[idx]
        # Forward
        z1 = xb @ W1 + b1
        a1 = np.maximum(z1, 0)
        pred = a1 @ W2 + b2
        # Backward (MSE)
        err = pred - yb
        gW2 = a1.T @ err
        gb2 = err.sum(axis=0)
        ga1 = err @ W2.T
        gz1 = ga1 * (z1 > 0)
        gW1 = xb.T @ gz1
        gb1 = gz1.sum(axis=0)
        # Update
        W2 -= lr * gW2 / n
        b2 -= lr * gb2 / n
        W1 -= lr * gW1 / n
        b1 -= lr * gb1 / n

    return W1, b1, W2, b2


def mlp_predict(X, W1, b1, W2, b2):
    z1 = X @ W1 + b1
    a1 = np.maximum(z1, 0)
    return a1 @ W2 + b2


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dataset", default="dataset/interpreter/state_chemio_dataset.json")
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--mlp", action="store_true", help="Prova anche MLP")
    args = parser.parse_args()

    X, Y_chemio, Y_phase, data = load_dataset(args.dataset)
    n = X.shape[0]
    print(f"Dataset: {n} campioni, stato {X.shape[1]} dim, chemio 4 dim")

    # Split
    idx = np.random.default_rng(args.seed).permutation(n)
    n_tr = int(0.8 * n)
    tr, te = idx[:n_tr], idx[n_tr:]

    # ── 1. Regressione lineare 64 → chemio ──────────────────────────
    print("\n=== Regressione lineare: stato → chemio_input ===")
    W = linear_regression(X[tr], Y_chemio[tr], lam=0.1)
    r2_tr, mae_tr, _ = evaluate_regression(X[tr], Y_chemio[tr], W)
    r2_te, mae_te, _ = evaluate_regression(X[te], Y_chemio[te], W)
    print(f"  Train: R²={r2_tr:.4f} MAE={mae_tr:.4f}")
    print(f"  Test : R²={r2_te:.4f} MAE={mae_te:.4f}")
    print(f"  W shape: {W.shape}  (65×4 con bias — distillabile come matrice [4][65])")

    # ── 2. Classificazione lineare 64 → fase (concept) ──────────────
    print("\n=== Classificazione lineare: stato → concept (fase) ===")
    # One-hot
    K = 4
    Y1 = np.eye(K, dtype=np.float32)[Y_phase]
    Wc = linear_regression(X[tr], Y1[tr], lam=0.1)
    pred_tr = predict_with_bias(X[tr], Wc)
    pred_te = predict_with_bias(X[te], Wc)
    acc_tr = np.mean(pred_tr.argmax(axis=1) == Y_phase[tr])
    acc_te = np.mean(pred_te.argmax(axis=1) == Y_phase[te])
    print(f"  Train: acc={acc_tr:.4f}")
    print(f"  Test : acc={acc_te:.4f}")

    # ── Verifica stati limite: zero e iniziale ──────────────────────
    print("\n=== Verifica stati limite ===")
    zero_state = np.zeros((1, 64), dtype=np.float32)
    _, _, cz = evaluate_regression(zero_state, np.zeros((1, 4), dtype=np.float32), W)
    pz = predict_with_bias(zero_state, Wc)
    print(f"  stato zero  → chemio=[{cz[0][0]:+.3f},{cz[0][1]:+.3f},{cz[0][2]:+.3f},{cz[0][3]:+.3f}] "
          f"concept={int(pz.argmax(axis=1)[0])} (atteso: riposo=2 o neutro)")

    # ── 3. MLP (opzionale) ──────────────────────────────────────────
    if args.mlp:
        print("\n=== MLP 64→16→4 (solo chemio) ===")
        W1, b1, W2, b2 = train_mlp(X[tr], Y_chemio[tr], hidden=16, epochs=1000)
        pred_tr = mlp_predict(X[tr], W1, b1, W2, b2)
        pred_te = mlp_predict(X[te], W1, b1, W2, b2)
        r2_tr = 1 - np.sum((Y_chemio[tr] - pred_tr)**2) / np.sum((Y_chemio[tr] - Y_chemio[tr].mean(0))**2)
        r2_te = 1 - np.sum((Y_chemio[te] - pred_te)**2) / np.sum((Y_chemio[te] - Y_chemio[te].mean(0))**2)
        mae_te = np.mean(np.abs(Y_chemio[te] - pred_te))
        print(f"  Train: R²={r2_tr:.4f}")
        print(f"  Test : R²={r2_te:.4f} MAE={mae_te:.4f}")
        print(f"  W1 {W1.shape}, b1 {b1.shape}, W2 {W2.shape}, b2 {b2.shape}")

    # ── Salva pesi lineari per il kernel ────────────────────────────
    out = Path("models/interpreter/interpreter_weights.npz")
    out.parent.mkdir(parents=True, exist_ok=True)
    np.savez(out, W_chemio=W, W_concept=Wc, lam=0.1, with_bias=True)
    print(f"\nPesi salvati: {out} (65×4 con bias)")


if __name__ == "__main__":
    main()
