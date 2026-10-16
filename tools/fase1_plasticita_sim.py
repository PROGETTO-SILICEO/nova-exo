#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Fase 1 - simulazione della regola a tre fattori
(docs/TEORIA_PLASTICITA_SOMATICA_NUCLEO.md, par. 3 e 7)

Confronta la regola v2.0 "alla lettera" con una v2.1 corretta e con due controlli
su M(t) (rimescolato nel tempo, costante).

Mondo sintetico
  - layer lineare congelato W (64x64) + adattatore a basso rango, r=8, alpha/r=2
  - il mondo applica una deriva a basso rango (rango 4, ~6% di ||W||_F) che CAMBIA a meta' run
  - canary: input ortogonali ai sottospazi di deriva. Il mondo li lascia invariati,
    quindi qualsiasi modifica dell'uscita su di essi e' danno alla base
  - episodi di stress (sensore p alto) in cui il target e' corrotto da rumore

LIMITI DICHIARATI
  1. x_target e' un oracolo (e = y_mondo - y_modello). La simulazione verifica la
     dinamica della regola, NON da dove nasca x_target nel kernel vero.
  2. Il legame "stress alto -> dati corrotti" e' un'ipotesi costruita nel mondo
     sintetico. Va verificata sul ferro (Fase 4), non e' dimostrata qui.
  3. Inizializzazione alla LoRA per tutti i bracci (U=0, V casuale): il documento
     non la specifica.
"""
import numpy as np

D, R, ALPHA = 64, 8, 16.0
S = ALPHA / R                       # alpha / r
T, SWITCH = 8000, 4000              # passi totali, passo in cui cambia il mondo
GAMMA, GNEG = 1e-4, 1e-2            # decadimento omeostatico; extra-decadimento per M<0 (v2.1)
RHO_CAP, EK_MAX, EPS_RANK = 0.10, 1.0, 0.02
SLEEP_EVERY = 2000                  # potatura "nel sonno"
NLMS_EPS, E_SCALE, NOISE_REL = 1.0, 0.10, 1.0
SEEDS = [0, 1, 2, 3]                # sweep di eta
FINAL_SEEDS = list(range(10, 18))   # seed nuovi per la tabella finale (niente bias di selezione)

# w = (omega_n, omega_r, omega_p, omega_pe), con somma dei valori assoluti = 1
ARMS = {
    "v2.0 letterale": dict(
        gate="signed", vrule="hebb", nlms=False, prune="zero_both", clamp="U",
        w=(.25, .25, .25, .25), etas=[1e-4, 1e-3, 1e-2, 1e-1]),
    "v2.1 (gate M+, V a errore, NLMS)": dict(
        gate="pos", vrule="err", nlms=True, prune="redraw", clamp="E",
        w=(.25, .25, .25, .25), etas=[.03, .1, .3, 1.0, 3.0]),
    # Isola il segno di M: identico al braccio precedente, ma M resta con segno
    # (nessun M+, nessun extra-decadimento) e con E_pred nel gate.
    "ablazione: v2.1 con M con segno": dict(
        gate="signed", vrule="err", nlms=True, prune="redraw", clamp="E",
        w=(.25, .25, .25, .25), etas=[.03, .1, .3, 1.0, 3.0]),
    "v2.1 senza E_pred nel gate": dict(
        gate="pos", vrule="err", nlms=True, prune="redraw", clamp="E",
        w=(1 / 3, 1 / 3, 1 / 3, 0.0), etas=[.03, .1, .3, 1.0, 3.0]),
    "ablazione: v2.1 con V hebbiano": dict(
        gate="pos", vrule="hebb", nlms=True, prune="redraw", clamp="E",
        w=(1 / 3, 1 / 3, 1 / 3, 0.0), etas=[.03, .1, .3, 1.0, 3.0]),
}
CTRL_BASE = "v2.1 senza E_pred nel gate"


def make_world(seed):
    r = np.random.default_rng(seed)
    W = r.standard_normal((D, D)) / np.sqrt(D)
    Q, _ = np.linalg.qr(r.standard_normal((D, D)))
    lam = np.arange(1, D + 1) ** -0.7          # covarianza input anisotropa
    lam *= D / lam.sum()
    L = Q * np.sqrt(lam)                       # x = L g
    A12, _ = np.linalg.qr(r.standard_normal((D, 8)))
    drifts = []
    for ph in range(2):
        A = A12[:, 4 * ph:4 * ph + 4]
        dW = r.standard_normal((D, 4)) @ A.T
        dW *= 0.06 * np.linalg.norm(W) / np.linalg.norm(dW)
        drifts.append(dW)
    P = np.eye(D) - A12 @ A12.T                # proiettore sul complemento delle derive
    return W, L, drifts, P


def make_sensors(seed):
    r = np.random.default_rng(seed + 1000)
    n, rw, p = np.empty(T), np.empty(T), np.empty(T)
    stress = np.zeros(T, bool)
    n_s, r_s, p_s, st = 0.5, 0.6, 0.2, False
    for t in range(T):
        st = (r.random() > 1 / 250) if st else (r.random() < 1 / 800)
        n_s += 0.05 * (r.random() - n_s)
        r_s = min(max(r_s + 0.01 * (0.6 - r_s) + 0.02 * r.standard_normal(), 0.0), 1.0)
        p_s = min(max(p_s + 0.05 * ((0.85 if st else 0.2) - p_s) + 0.02 * r.standard_normal(), 0.0), 1.0)
        n[t], rw[t], p[t], stress[t] = n_s, r_s, p_s, st
    return n, rw, p, stress


def evaluate(W, U, V, Xe, Xc, dW):
    pe = S * (Xe @ V) @ U.T
    pc = S * (Xc @ V) @ U.T
    d = Xe @ dW.T
    task = np.mean(np.sum((d - pe) ** 2, 1)) / np.mean(np.sum(d ** 2, 1))   # 1 = congelato, 0 = perfetto
    can = np.sqrt(np.mean(np.sum(pc ** 2, 1)) / np.mean(np.sum((Xc @ W.T) ** 2, 1)))
    return task, can


def run(seed, cfg, eta, m_mode="live"):
    W, L, drifts, P = make_world(seed)
    n, rw, p, stress = make_sensors(seed)
    r = np.random.default_rng(seed + 2000)
    X = r.standard_normal((T, D)) @ L.T
    NZ = r.standard_normal((T, D)) / np.sqrt(D)
    Xe = np.random.default_rng(seed + 3000).standard_normal((512, D)) @ L.T
    Xc = Xe @ P
    WX = X @ W.T
    DX = [X @ d.T for d in drifts]
    WN = np.linalg.norm(W)
    wn, wr, wp, wpe = cfg["w"]

    m_seq = None                                # M esogeno quando non dipende dall'errore
    if wpe == 0.0:
        m_seq = np.tanh(wn * n + wr * rw - wp * p)
        if m_mode == "shuffle":
            m_seq = r.permutation(m_seq)
        elif m_mode == "const":
            m_seq = np.full(T, m_seq.mean())

    U = np.zeros((D, R))
    V = r.standard_normal((D, R)) / np.sqrt(D)
    Eema, cap_hits = 0.06, 0
    negM = np.zeros(T, bool)
    res = dict(diverged=False, loss1=np.inf, can1=np.inf, loss2=np.inf, can2=np.inf)

    for t in range(T):
        ph = int(t >= SWITCH)
        y = WX[t] + DX[ph][t]
        if stress[t]:
            y = y + NOISE_REL * np.linalg.norm(y) * NZ[t]   # target corrotto
        x = X[t]
        z = V.T @ x
        e = y - (WX[t] + S * (U @ z))
        Eema += 0.01 * (np.linalg.norm(e) / np.linalg.norm(y) - Eema)
        E = min(Eema / E_SCALE, 1.0)
        M = m_seq[t] if m_seq is not None else np.tanh(wn * n[t] + wr * rw[t] - wp * p[t] - wpe * E)
        negM[t] = M < 0
        m = M if cfg["gate"] == "signed" else max(M, 0.0)
        mneg = max(-M, 0.0) if cfg["gate"] == "pos" else 0.0

        if cfg["nlms"]:
            dU = eta * m * S * np.outer(e, z) / (NLMS_EPS + S * S * (z @ z))
            if cfg["vrule"] == "err":
                dV = eta * m * S * np.outer(x, U.T @ e) / (NLMS_EPS + S * S * (x @ x) * np.sum(U * U))
            else:
                dV = eta * m * np.outer(x, z) / (NLMS_EPS + x @ x)
        else:
            dU = eta * m * np.outer(e, z)
            dV = eta * m * np.outer(x, z)
        g = GAMMA + GNEG * mneg                 # decadimento disaccoppiato dal segno di M
        U = U + dU - g * U
        V = V + dV - g * V

        if cfg["clamp"] == "U":                 # v2.0: clamp solo sulle colonne di U
            U = U / np.maximum(1.0, np.linalg.norm(U, axis=0))
        else:                                   # v2.1: clamp sul prodotto ||u_k||*||v_k||
            ek = np.linalg.norm(U, axis=0) * np.linalg.norm(V, axis=0)
            f = np.sqrt(np.minimum(1.0, EK_MAX / np.maximum(ek, 1e-12)))
            U, V = U * f, V * f
        rho = S * np.sqrt(max(np.sum((U.T @ U) * (V.T @ V)), 0.0)) / WN
        if not np.isfinite(rho):
            res["diverged"] = True
            break
        if rho > RHO_CAP:
            cap_hits += 1
            f = np.sqrt(RHO_CAP / rho)
            U, V = U * f, V * f

        if (t + 1) % SLEEP_EVERY == 0:          # sonno: potatura per energia di rango
            ek = np.linalg.norm(U, axis=0) * np.linalg.norm(V, axis=0)
            dead = ek < EPS_RANK
            U[:, dead] = 0.0
            if cfg["prune"] == "zero_both":
                V[:, dead] = 0.0
            else:
                V[:, dead] = r.standard_normal((D, int(dead.sum()))) / np.sqrt(D)
        if t + 1 == SWITCH:
            res["loss1"], res["can1"] = evaluate(W, U, V, Xe, Xc, drifts[0])
        if t + 1 == T:
            res["loss2"], res["can2"] = evaluate(W, U, V, Xe, Xc, drifts[1])

    res["rho"] = rho if not res["diverged"] else np.nan
    res["cap_pct"] = 100.0 * cap_hits / T
    res["dead"] = int(np.sum(np.linalg.norm(V, axis=0) < 1e-8)) if not res["diverged"] else R
    res["negM"] = 100.0 * negM.mean()
    res["negM_post"] = 100.0 * negM[SWITCH:SWITCH + 300].mean()
    return res


def agg(rs, k):
    return float(np.mean([r[k] for r in rs]))


def score(rs):
    if any(r["diverged"] for r in rs):
        return np.inf
    return np.mean([(r["loss1"] + r["loss2"]) / 2 for r in rs])


def row(name, eta, rs):
    nd = sum(r["diverged"] for r in rs)
    return (f"{name:34s} {eta:>7} {agg(rs, 'loss1'):8.3f} {agg(rs, 'loss2'):8.3f} "
            f"{agg(rs, 'can2'):8.3f} {agg(rs, 'rho'):7.3f} {agg(rs, 'cap_pct'):6.1f} "
            f"{agg(rs, 'dead'):5.1f} {agg(rs, 'negM'):6.1f} {agg(rs, 'negM_post'):7.1f} {nd:>4}")


def row_ms(name, eta, rs):
    nd = sum(r["diverged"] for r in rs)

    def a(k):
        v = [r[k] for r in rs]
        return f"{np.mean(v):6.3f}±{np.std(v):5.3f}"
    return (f"{name:34s} {eta:>5} {a('loss1')} {a('loss2')} {a('can2')} "
            f"{agg(rs, 'dead'):5.1f} {agg(rs, 'negM'):6.1f} {nd:>4}")


HEAD_MS = (f"{'braccio':34s} {'eta':>5} {'loss_f1':>13} {'loss_f2':>13} {'canary':>13} "
           f"{'morti':>5} {'M<0%':>6} {'div':>4}")

HEAD = (f"{'braccio':34s} {'eta':>7} {'loss_f1':>8} {'loss_f2':>8} {'canary':>8} "
        f"{'rho':>7} {'cap%':>6} {'morti':>5} {'M<0%':>6} {'M<0post':>7} {'div':>4}")


def main():
    results = {}
    for name, cfg in ARMS.items():
        for eta in cfg["etas"]:
            results[(name, eta)] = [run(s, cfg, eta) for s in SEEDS]

    print("== Sweep di eta (media su", len(SEEDS), "seed). loss: 1 = modello congelato, 0 = perfetto")
    print("   canary = variazione relativa RMS dell'uscita su input a deriva nulla (0 = nessun danno)")
    print("   M<0post = % passi con M<0 nei 300 passi dopo il cambio di mondo; div = seed divergenti\n")
    print(HEAD)
    for (name, eta), rs in results.items():
        print(row(name, eta, rs))

    print(f"\n== Tabella finale: eta scelto sullo sweep (seed {SEEDS}), valutato su seed NUOVI {FINAL_SEEDS}")
    print("   media±std sui seed; morti = rank con v_k=0 a fine run; M<0% = % passi con M<0\n")
    print(HEAD_MS)
    print(f"{'congelato':34s} {'-':>5} {'1.000':>13} {'1.000':>13} {'0.000':>13}")
    best_eta, finals = {}, {}
    for name, cfg in ARMS.items():
        eta = min(cfg["etas"], key=lambda e: score(results[(name, e)]))
        best_eta[name] = eta
        finals[name] = [run(s, cfg, eta) for s in FINAL_SEEDS]
        print(row_ms(name, eta, finals[name]))
    eta = best_eta[CTRL_BASE]
    ctrl = {}
    for mode, label in (("shuffle", "controllo: M rimescolato"), ("const", "controllo: M costante")):
        ctrl[label] = [run(s, ARMS[CTRL_BASE], eta, m_mode=mode) for s in FINAL_SEEDS]
        print(row_ms(label, eta, ctrl[label]))

    print(f"\n== Confronto a coppie (stesso seed): su quanti dei {len(FINAL_SEEDS)} seed M reale batte il controllo")
    base = finals[CTRL_BASE]
    for label, rs in ctrl.items():
        wins = {k: sum(b[k] < c[k] for b, c in zip(base, rs)) for k in ("loss1", "loss2", "can2")}
        print(f"   vs {label:26s} loss_f1: {wins['loss1']}/{len(rs)}   loss_f2: {wins['loss2']}/{len(rs)}   "
              f"canary: {wins['can2']}/{len(rs)}")
    a, b = finals["v2.1 (gate M+, V a errore, NLMS)"], finals["ablazione: v2.1 con M con segno"]
    print("   M+ batte M con segno (stessi pesi, E_pred nel gate): "
          + "   ".join(f"{k}: {sum(x[k] < y[k] for x, y in zip(a, b))}/{len(a)}" for k in ("loss1", "loss2", "can2")))


if __name__ == "__main__":
    main()
