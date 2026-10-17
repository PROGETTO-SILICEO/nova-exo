// ── Banco host per la PLASTICITÀ SOMATICA (plasticita.rs) ─────────────────
//
// Porting fedele di `tools/fase1_plasticita_sim.py` (Fase 1) sul codice REALE
// (#[path = "../src/plasticita.rs"]): stesso mondo sintetico (W congelata +
// deriva a basso rango che cambia a metà run + canary), stessi sensori,
// stessi iperparametri (T=8000, SWITCH=4000, SLEEP_EVERY=2000, ρ≤0.10,
// E_k≤1, ε_rank=0.02). Gli ARMI e i controlli riproducono la tabella Python.
//
// COSA VERIFICA (sui TREND documentati, non seed-per-seed: il RNG è nostro,
// non numpy — la parità esatta non è possibile né necessaria). Struttura
// fedele alla sim: ordine degli stream (X, NZ, V₀ da r(seed+2000); Xe da
// r(seed+3000)), ordine del passo (update → clamp → ρ/cap → potatura → eval).
// Deviazione dichiarata: il redraw della potatura usa un seed derivato dal
// seed del run (non lo stream esatto di r, che non inseguiamo).
//   1. la regola v2.1 validata (gate M⁺, V a errore, NLMS, redraw) ADATTA:
//      loss2 scende sotto 0.6, canary piccolo, zero ranghi morti;
//   2. la v2.0 "alla lettera" NON adatta: quasi tutti i ranghi muoiono
//      nella potatura (trappola del gradiente nullo, §4.2);
//   3. M con segno è peggiore di M⁺; V hebbiano è peggiore del V a errore
//      (§3.1);
//   4. M(t) reale batte i controlli rimescolato/costante a coppie (§3.2).
//
// Riferimento numerico (numpy, seed 0-3 sweep / 10-17 finale):
//   CTRL_BASE η=1.0 → loss_f1 0.195±0.256  loss_f2 0.347±0.297  can 0.019
//   v2.0 η=1e-4     → morti 8.0           loss 1.000           can 0.000
//   M-segno η=0.03  → loss_f2 3.190       can 0.072
//   V-hebb η=0.03   → loss_f2 2.731       can 0.068
//   vs shuffle      → win 8/8 su loss1/loss2/canary;  vs const → 8/8, 6/8, 7/8
//
// Compilazione: make test   (oppure rustc --test --edition 2021 -O bench/plasticita_host.rs)

mod math {
    pub fn sqrtf(x: f32) -> f32 { x.sqrt() }
    pub fn tanhf(x: f32) -> f32 { x.tanh() }
}

#[path = "../src/plasticita.rs"]
mod plasticita;

use plasticita::{
    neuromodulatore, Adattatore, Clamp, Config, Gate, Prune, VRule, EK_MAX, PESI_VALIDATI,
    RANGO, RHO_CAP, S, V20, V21,
};

// ── Costanti del mondo (identiche alla sim Python) ────────────────────────

const D: usize = 64;
const T: usize = 8000;
const SWITCH: usize = 4000;
const SLEEP_EVERY: usize = 2000;
const NOISE_REL: f64 = 1.0;
const XE_ROWS: usize = 512;

// ── RNG deterministico (LCG splitmix + Box-Muller) ────────────────────────
// La sim Python usa numpy.default_rng; qui un nostro RNG. La parità non è
// seed-per-seed: è sui trend, come dichiarato in testa.

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        let mut z = seed.wrapping_add(0x9E3779B97F4A7C15);
        let mut s = 0u64;
        for _ in 0..4 {
            z = z.wrapping_add(0x9E3779B97F4A7C15);
            let mut x = z;
            x = (x ^ (x >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
            x = (x ^ (x >> 27)).wrapping_mul(0x94D049BB133111EB);
            s ^= x ^ (x >> 31);
        }
        Rng(if s == 0 { 0x123456789ABCDEF0 } else { s })
    }

    fn next_u64(&mut self) -> u64 {
        let s = &mut self.0;
        *s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let mut x = *s;
        x = (x ^ (x >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94D049BB133111EB);
        x ^ (x >> 31)
    }

    /// Uniforme in [0, 1).
    fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Normale N(0,1) via Box-Muller (coefficiente cos; il secondo valore
    /// della coppia viene scartato — la distribuzione resta esatta).
    fn gauss(&mut self) -> f64 {
        let u1 = self.f64().max(f64::MIN_POSITIVE);
        let u2 = self.f64();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }

    /// Permutazione di Fisher-Yates.
    fn shuffle(&mut self, a: &mut [f64]) {
        for i in (1..a.len()).rev() {
            let j = (self.f64() * (i + 1) as f64) as usize;
            a.swap(i, j.min(i));
        }
    }
}

// ── Algelli lineari base (f64, stile numpy) ───────────────────────────────

/// Q con colonne ortonormali da A (m×n, m≥n) — Modified Gram-Schmidt.
fn mgs_q(a: &mut [Vec<f64>]) {
    let n = a[0].len();
    for k in 0..n {
        for j in 0..k {
            let mut dot = 0.0;
            for i in 0..a.len() {
                dot += a[i][j] * a[i][k];
            }
            for i in 0..a.len() {
                a[i][k] -= dot * a[i][j];
            }
        }
        let mut nrm = 0.0;
        for i in 0..a.len() {
            nrm += a[i][k] * a[i][k];
        }
        let nrm = nrm.sqrt().max(1e-30);
        for i in 0..a.len() {
            a[i][k] /= nrm;
        }
    }
}

/// C = A @ Bᵀ  (A: p×m, B: q×m → C: p×q).
fn matmul_atb(a: &[Vec<f64>], b: &[Vec<f64>], c: &mut [Vec<f64>]) {
    let p = a.len();
    let q = b.len();
    let m = a[0].len();
    for i in 0..p {
        for j in 0..q {
            let mut acc = 0.0;
            for k in 0..m {
                acc += a[i][k] * b[j][k];
            }
            c[i][j] = acc;
        }
    }
}

/// C = A @ B  (A: p×m, B: m×q → C: p×q).
fn matmul(a: &[Vec<f64>], b: &[Vec<f64>], c: &mut [Vec<f64>]) {
    let p = a.len();
    let q = b[0].len();
    let m = a[0].len();
    for i in 0..p {
        for j in 0..q {
            let mut acc = 0.0;
            for k in 0..m {
                acc += a[i][k] * b[k][j];
            }
            c[i][j] = acc;
        }
    }
}

fn zeros(rows: usize, cols: usize) -> Vec<Vec<f64>> {
    vec![vec![0.0; cols]; rows]
}

fn randn(rows: usize, cols: usize, rng: &mut Rng) -> Vec<Vec<f64>> {
    (0..rows)
        .map(|_| (0..cols).map(|_| rng.gauss()).collect())
        .collect()
}

fn frobenius(a: &[Vec<f64>]) -> f64 {
    let mut acc = 0.0;
    for row in a {
        for &v in row {
            acc += v * v;
        }
    }
    acc.sqrt()
}

// ── Mondo e sensori (make_world / make_sensors della sim) ─────────────────

struct Mondo {
    w: Vec<Vec<f64>>,       // D×D — W congelata
    drift: [Vec<Vec<f64>>; 2], // D×D ciascuno (rango 4), prima/meta' run
    p_proj: Vec<Vec<f64>>,  // D×D — proiettore sul complemento delle derive
    wn: f64,              // ‖W‖_F
    lam: Vec<f64>,        // D — spettro di covarianza (serve per L in pacchetto)
    q: Vec<Vec<f64>>,     // D×D — basis del mondo (serve per L)
}

fn make_world(seed: u64) -> Mondo {
    let mut rng = Rng::new(seed);

    // W = randn(D,D)/sqrt(D)
    let mut w = randn(D, D, &mut rng);
    for i in 0..D {
        for j in 0..D {
            w[i][j] /= (D as f64).sqrt();
        }
    }
    let wn = frobenius(&w);

    // Q da QR di gaussiana D×D; lam_j = (j+1)^-0.7 normalizzato a somma D
    let mut q = randn(D, D, &mut rng);
    mgs_q(&mut q);
    let lam_raw: Vec<f64> = (1..=D).map(|j| (j as f64).powf(-0.7)).collect();
    let lam_sum: f64 = lam_raw.iter().sum();
    let lam: Vec<f64> = lam_raw.iter().map(|l| l * D as f64 / lam_sum).collect();

    // A12: QR di gaussiana D×8
    let mut a12 = randn(D, 8, &mut rng);
    mgs_q(&mut a12);

    // Derive: per fase f, A = colonne [4f..4f+4) di A12;
    // dW = randn(D,4) @ Aᵀ (D×D, rango 4), scalato a 0.06·‖W‖/‖dW‖
    let mut drift = [zeros(D, D), zeros(D, D)];
    for ph in 0..2 {
        let g = randn(D, 4, &mut rng);
        let mut dw = zeros(D, D);
        for i in 0..D {
            for j in 0..D {
                let mut acc = 0.0;
                for k in 0..4 {
                    acc += g[i][k] * a12[j][4 * ph + k];
                }
                dw[i][j] = acc;
            }
        }
        let scala = 0.06 * wn / frobenius(&dw);
        for i in 0..D {
            for j in 0..D {
                dw[i][j] *= scala;
            }
        }
        drift[ph] = dw;
    }

    // P = I − A12 A12ᵀ
    let mut p_proj = zeros(D, D);
    for i in 0..D {
        for j in 0..D {
            let mut acc = if i == j { 1.0 } else { 0.0 };
            for k in 0..8 {
                acc -= a12[i][k] * a12[j][k];
            }
            p_proj[i][j] = acc;
        }
    }

    Mondo { w, drift, p_proj, wn, lam, q }
}

struct Sensori {
    n: Vec<f64>,
    rw: Vec<f64>,
    p: Vec<f64>,
    stress: Vec<bool>,
}

fn make_sensors(seed: u64) -> Sensori {
    let mut rng = Rng::new(seed.wrapping_add(1000));
    let mut n = vec![0.0; T];
    let mut rw = vec![0.0; T];
    let mut p = vec![0.0; T];
    let mut stress = vec![false; T];
    let (mut ns, mut rws, mut ps) = (0.5f64, 0.6f64, 0.2f64);
    let mut st = false;
    for t in 0..T {
        st = if st {
            rng.f64() > 1.0 / 250.0
        } else {
            rng.f64() < 1.0 / 800.0
        };
        ns += 0.05 * (rng.f64() - ns);
        rws = (rws + 0.01 * (0.6 - rws) + 0.02 * rng.gauss()).clamp(0.0, 1.0);
        let target_p = if st { 0.85 } else { 0.2 };
        ps = (ps + 0.05 * (target_p - ps) + 0.02 * rng.gauss()).clamp(0.0, 1.0);
        n[t] = ns;
        rw[t] = rws;
        p[t] = ps;
        stress[t] = st;
    }
    Sensori { n, rw, p, stress }
}

// ── Bundle precalcolato per seed (identico a ogni run, come in numpy) ─────

struct Pacchetto {
    seed: u64,
    mondo: Mondo,
    sensori: Sensori,
    x: Vec<Vec<f64>>,       // T×D — input del mondo
    nz: Vec<Vec<f64>>,      // T×D — rumore dello stress
    v_init: Vec<f32>,       // D×RANGO — init LoRA di V (dallo stream del run, come in numpy)
    wx: Vec<Vec<f64>>,      // T×D — W x
    dx: [Vec<Vec<f64>>; 2], // T×D — deriva applicata
    xe: Vec<Vec<f64>>,      // XE_ROWS×D — eval (task)
    xc: Vec<Vec<f64>>,      // XE_ROWS×D — eval (canary, deriva nulla)
}

fn pacchetto(seed: u64) -> Pacchetto {
    let mondo = make_world(seed);
    let sensori = make_sensors(seed);
    let mut rng = Rng::new(seed.wrapping_add(2000));

    // L[i][j] = Q[i][j]·sqrt(lam[j])  (x = L g); Q e lam dal mondo.
    let mut l = zeros(D, D);
    for i in 0..D {
        for j in 0..D {
            l[i][j] = mondo.q[i][j] * mondo.lam[j].sqrt();
        }
    }

    // X = randn(T,D) @ Lᵀ ; NZ = randn(T,D)/sqrt(D) ; V₀ = randn(D,R)/sqrt(D)
    // (stesso stream r(seed+2000), nell'ordine della sim Python).
    let g = randn(T, D, &mut rng);
    let mut x = zeros(T, D);
    matmul_atb(&g, &l, &mut x);

    let mut nz = randn(T, D, &mut rng);
    for i in 0..T {
        for j in 0..D {
            nz[i][j] /= (D as f64).sqrt();
        }
    }

    let inv_sqrt_d = 1.0 / (D as f64).sqrt();
    let mut v_init = vec![0.0f32; D * RANGO];
    for j in 0..D {
        for k in 0..RANGO {
            v_init[j * RANGO + k] = (rng.gauss() * inv_sqrt_d) as f32;
        }
    }

    // Xe usa uno stream SEPARATO r(seed+3000), come in numpy.
    let mut erng = Rng::new(seed.wrapping_add(3000));
    let ge = randn(XE_ROWS, D, &mut erng);
    let mut xe = zeros(XE_ROWS, D);
    matmul_atb(&ge, &l, &mut xe);
    let mut xc = zeros(XE_ROWS, D);
    matmul(&xe, &mondo.p_proj, &mut xc);

    // WX = X @ Wᵀ ; DX[ph] = X @ drift[ph]ᵀ
    let mut wx = zeros(T, D);
    matmul_atb(&x, &mondo.w, &mut wx);
    let mut dx0 = zeros(T, D);
    matmul_atb(&x, &mondo.drift[0], &mut dx0);
    let mut dx1 = zeros(T, D);
    matmul_atb(&x, &mondo.drift[1], &mut dx1);

    Pacchetto { seed, mondo, sensori, x, nz, v_init, wx, dx: [dx0, dx1], xe, xc }
}

// Cache dei pacchetti per i test (Box::leak: il processo di test vive poco).
fn pacchetto_cached(seed: u64) -> &'static Pacchetto {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static CACHE: Mutex<Option<HashMap<u64, &'static Pacchetto>>> = Mutex::new(None);
    let mut guard = CACHE.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    if let Some(&p) = map.get(&seed) {
        return p;
    }
    let p: &'static Pacchetto = Box::leak(Box::new(pacchetto(seed)));
    map.insert(seed, p);
    p
}

// ── Configurazioni armi (identiche alla sim) ──────────────────────────────

#[derive(Clone, Copy)]
struct Arma {
    nome: &'static str,
    cfg: Config,
    w: [f64; 4], // (ω_n, ω_r, ω_p, ω_pe)
    etas: &'static [f64],
}

const IDX_V20: usize = 0;
const IDX_V21_EPRED: usize = 1;
const IDX_M_SEGNO: usize = 2;
const IDX_CTRL: usize = 3; // "v2.1 senza E_pred nel gate" = CTRL_BASE
const IDX_HEBB: usize = 4;

fn armi() -> [Arma; 5] {
    [
        Arma {
            nome: "v2.0 letterale",
            cfg: V20,
            w: [0.25, 0.25, 0.25, 0.25],
            etas: &[1e-4, 1e-3, 1e-2, 1e-1],
        },
        Arma {
            nome: "v2.1 (gate M+, V a errore, NLMS)",
            cfg: V21,
            w: [0.25, 0.25, 0.25, 0.25],
            etas: &[0.03, 0.1, 0.3, 1.0, 3.0],
        },
        Arma {
            nome: "ablazione: v2.1 con M con segno",
            cfg: Config {
                gate: Gate::Signed,
                vrule: VRule::Err,
                nlms: true,
                clamp: Clamp::Prodotto,
                prune: Prune::Redraw,
            },
            w: [0.25, 0.25, 0.25, 0.25],
            etas: &[0.03, 0.1, 0.3, 1.0, 3.0],
        },
        Arma {
            nome: "v2.1 senza E_pred nel gate",
            cfg: V21,
            w: [1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0, 0.0],
            etas: &[0.03, 0.1, 0.3, 1.0, 3.0],
        },
        Arma {
            nome: "ablazione: v2.1 con V hebbiano",
            cfg: Config {
                gate: Gate::Pos,
                vrule: VRule::Hebb,
                nlms: true,
                clamp: Clamp::Prodotto,
                prune: Prune::Redraw,
            },
            w: [1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0, 0.0],
            etas: &[0.03, 0.1, 0.3, 1.0, 3.0],
        },
    ]
}

// ── Risultato di un run ────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct Res {
    diverged: bool,
    loss1: f64,
    can1: f64,
    loss2: f64,
    can2: f64,
    rho: f64,
    cap_pct: f64,
    dead: f64,
    negm: f64,
    negm_post: f64,
}

// ── evaluate (identico alla sim, in f64 sui pesi f32 della regola) ────────

fn evaluate(
    w: &[Vec<f64>],
    u: &[f32],
    v: &[f32],
    xe: &[Vec<f64>],
    xc: &[Vec<f64>],
    dw: &[Vec<f64>],
) -> (f64, f64) {
    let s = S as f64;
    let n_rows = xe.len();
    let mut num_task = 0.0;
    let mut den_task = 0.0;
    let mut num_can = 0.0;
    let mut den_can = 0.0;
    for i in 0..n_rows {
        // z = Vᵀ x_e ; d = Xe dWᵀ ; pe = s·U z
        let mut z = [0.0f64; RANGO];
        for j in 0..D {
            let xij = xe[i][j];
            for k in 0..RANGO {
                z[k] += xij * v[j * RANGO + k] as f64;
            }
        }
        for m in 0..D {
            let mut acc_d = 0.0;
            let mut acc_pe = 0.0;
            for j in 0..D {
                acc_d += xe[i][j] * dw[m][j];
            }
            for k in 0..RANGO {
                acc_pe += z[k] * u[m * RANGO + k] as f64;
            }
            let pe = s * acc_pe;
            num_task += (acc_d - pe) * (acc_d - pe);
            den_task += acc_d * acc_d;
        }
        // canary: pc = s·U (Vᵀ x_c) ; base = Xc Wᵀ
        let mut zc = [0.0f64; RANGO];
        for j in 0..D {
            let xij = xc[i][j];
            for k in 0..RANGO {
                zc[k] += xij * v[j * RANGO + k] as f64;
            }
        }
        for m in 0..D {
            let mut acc_pc = 0.0;
            let mut bw = 0.0;
            for k in 0..RANGO {
                acc_pc += zc[k] * u[m * RANGO + k] as f64;
            }
            for j in 0..D {
                bw += xc[i][j] * w[m][j];
            }
            let pc = s * acc_pc;
            num_can += pc * pc;
            den_can += bw * bw;
        }
    }
    let task = (num_task / n_rows as f64) / (den_task / n_rows as f64).max(1e-30);
    let can = ((num_can / n_rows as f64) / (den_can / n_rows as f64).max(1e-30)).sqrt();
    (task, can)
}

// ── run (identico al ciclo della sim) ─────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum MMode {
    Live,
    Shuffle,
    Const,
}

fn run(p: &'static Pacchetto, w: [f64; 4], cfg: &Config, eta: f64, m_mode: MMode) -> Res {
    let d_out = D;
    let d_in = D;
    let wn_f = p.mondo.wn as f32;
    let (wn, wr, wp, wpe) = (w[0], w[1], w[2], w[3]);

    // M exogeno quando ω_pe = 0 (con rimescolamento/costante per i controlli)
    let mut m_seq: Option<Vec<f64>> = None;
    if wpe == 0.0 {
        let mut ms: Vec<f64> = (0..T)
            .map(|t| (wn * p.sensori.n[t] + wr * p.sensori.rw[t] - wp * p.sensori.p[t]).tanh())
            .collect();
        match m_mode {
            MMode::Live => {}
            MMode::Shuffle => {
                let mut srng = Rng::new(p.seed ^ 0x00C0_FFEE);
                srng.shuffle(&mut ms);
            }
            MMode::Const => {
                let mean = ms.iter().sum::<f64>() / T as f64;
                for v in ms.iter_mut() {
                    *v = mean;
                }
            }
        }
        m_seq = Some(ms);
    }

    // LoRA init: U = 0, V = randn(D,R)/sqrt(D) — dallo stream del run (per seed).
    let mut u = vec![0.0f32; d_out * RANGO];
    let mut v = p.v_init.clone();

    let mut att = Adattatore {
        u: &mut u,
        v: &mut v,
        d_out,
        d_in,
        w_norm: wn_f,
        e_ema: 0.06,
    };

    let mut res = Res {
        diverged: false,
        loss1: f64::INFINITY,
        can1: f64::INFINITY,
        loss2: f64::INFINITY,
        can2: f64::INFINITY,
        rho: 0.0,
        cap_pct: 0.0,
        dead: 0.0,
        negm: 0.0,
        negm_post: 0.0,
    };

    let mut zbuf = [0.0f32; RANGO];
    let mut utebuf = [0.0f32; RANGO];
    let mut negm_count = 0usize;
    let mut negm_post_count = 0usize;
    let mut cap_hits = 0usize;
    let mut rho_last = 0.0f32;

    for t in 0..T {
        let ph = if t >= SWITCH { 1 } else { 0 };
        // y = W x + d(ph)  (+ rumore se stress: il target si corrompe)
        let mut y = [0.0f64; D];
        for m in 0..D {
            y[m] = p.wx[t][m] + p.dx[ph][t][m];
        }
        if p.sensori.stress[t] {
            let mut yn = 0.0;
            for m in 0..D {
                yn += y[m] * y[m];
            }
            let yn = yn.sqrt() * NOISE_REL;
            for m in 0..D {
                y[m] += yn * p.nz[t][m];
            }
        }

        let x_f32: Vec<f32> = p.x[t].iter().map(|&q| q as f32).collect();
        let y_f32: Vec<f32> = y.iter().map(|&q| q as f32).collect();

        // e = y − (W x + S·(U z)) con lo stato attuale (prima dell'update)
        plasticita::proietta(&att.v, d_in, &x_f32, &mut zbuf);
        let mut e = [0.0f32; D];
        for m in 0..D {
            let mut acc = 0.0f32;
            for k in 0..RANGO {
                acc += att.u[m * RANGO + k] * zbuf[k];
            }
            e[m] = y_f32[m] - (p.wx[t][m] as f32 + S * acc);
        }

        // EMA dell'errore relativo + E normalizzato (per M con ω_pe ≠ 0)
        let mut en = 0.0f32;
        let mut yn2 = 0.0f32;
        for m in 0..D {
            en += e[m] * e[m];
            yn2 += y_f32[m] * y_f32[m];
        }
        att.aggiorna_errore(en.sqrt(), yn2.sqrt());
        let e_norm = att.errore_normalizzato();

        // M(t)
        let m_meta: f32 = match &m_seq {
            Some(ms) => ms[t] as f32,
            None => {
                ((wn * p.sensori.n[t] + wr * p.sensori.rw[t] - wp * p.sensori.p[t]
                    - wpe * e_norm as f64)
                    .tanh()) as f32
            }
        };
        if m_meta < 0.0 {
            negm_count += 1;
            if t >= SWITCH && t < SWITCH + 300 {
                negm_post_count += 1;
            }
        }

        // passo della regola sul codice REALE
        let esito = att.passo(&x_f32, &e, m_meta, eta as f32, cfg, &mut zbuf, &mut utebuf);
        if !esito.rho.is_finite() {
            res.diverged = true;
            res.rho = f64::NAN;
            res.dead = RANGO as f64;
            return res;
        }
        rho_last = esito.rho;
        if esito.cap_hit {
            cap_hits += 1;
        }

        // sonno: potatura ogni SLEEP_EVERY
        if (t + 1) % SLEEP_EVERY == 0 {
            let mut seed: u32 = (p.seed as u32).wrapping_mul(2654435761) ^ (t as u32);
            let _ = att.potatura(cfg, &mut seed);
        }

        // valutazioni a metà e a fine run (DOPO la potatura, come in numpy)
        if t + 1 == SWITCH {
            let (l1, c1) = evaluate(
                &p.mondo.w,
                &att.u[..],
                &att.v[..],
                &p.xe,
                &p.xc,
                &p.mondo.drift[0],
            );
            res.loss1 = l1;
            res.can1 = c1;
        }
        if t + 1 == T {
            let (l2, c2) = evaluate(
                &p.mondo.w,
                &att.u[..],
                &att.v[..],
                &p.xe,
                &p.xc,
                &p.mondo.drift[1],
            );
            res.loss2 = l2;
            res.can2 = c2;
        }
    }

    res.rho = rho_last as f64;
    res.cap_pct = 100.0 * cap_hits as f64 / T as f64;
    let mut dead = 0usize;
    for k in 0..RANGO {
        let mut nv = 0.0f64;
        for j in 0..d_in {
            let q = att.v[j * RANGO + k] as f64;
            nv += q * q;
        }
        if nv.sqrt() < 1e-8 {
            dead += 1;
        }
    }
    res.dead = dead as f64;
    res.negm = 100.0 * negm_count as f64 / T as f64;
    let post_n = (SWITCH + 300).min(T) - SWITCH;
    res.negm_post = 100.0 * negm_post_count as f64 / post_n as f64;
    res
}

fn score(rs: &[Res]) -> f64 {
    if rs.iter().any(|r| r.diverged) {
        return f64::INFINITY;
    }
    rs.iter().map(|r| (r.loss1 + r.loss2) / 2.0).sum::<f64>() / rs.len() as f64
}

fn mean(rs: &[Res], f: fn(&Res) -> f64) -> f64 {
    rs.iter().map(f).sum::<f64>() / rs.len() as f64
}

// ── Test ───────────────────────────────────────────────────────────────────

#[test]
fn neuromodulatore_gate_e_epred_azzero() {
    // La regola validata: ω_pe = 0 → l'errore NON entra nel gate (§3.2).
    let w = PESI_VALIDATI;
    assert_eq!(w.errore, 0.0);
    let m_basso = neuromodulatore(0.5, 0.6, 0.2, 0.0, &w);
    let m_alto = neuromodulatore(0.5, 0.6, 0.2, 1.0, &w);
    assert!(
        (m_basso - m_alto).abs() < 1e-6,
        "E_pred non deve muovere M: {} vs {}",
        m_basso,
        m_alto
    );
    assert!(m_basso > 0.0, "corpo tranquillo → M > 0");
    let m_dolore = neuromodulatore(0.1, 0.1, 1.0, 0.0, &w);
    assert!(m_dolore < 0.0, "dolore forte → M < 0 (gate si chiude)");
    assert!(m_dolore > -1.0 && m_basso < 1.0);
}

#[test]
fn pesi_validati_normalizzati() {
    let w = PESI_VALIDATI;
    let s = w.novita.abs() + w.reward.abs() + w.allarme.abs() + w.errore.abs();
    assert!((s - 1.0).abs() < 1e-6, "somma |ω| = 1, ottenuto {}", s);
}

#[test]
fn proietta_e_predici_additive() {
    let d_in = 4;
    let d_out = 3;
    let v = vec![1.0f32; d_in * RANGO];
    let u = vec![1.0f32; d_out * RANGO];
    let x = vec![1.0f32; d_in];
    let mut z = [0.0f32; RANGO];
    plasticita::proietta(&v, d_in, &x, &mut z);
    for k in 0..RANGO {
        assert_eq!(z[k], d_in as f32, "z_k = Σ x_j v_jk = 4");
    }
    let mut y = vec![0.0f32; d_out];
    plasticita::predici_aggiungi(&u, d_out, &z, &mut y);
    for i in 0..d_out {
        assert_eq!(y[i], S * (RANGO as f32) * d_in as f32, "y_i = S·r·4");
    }
}

#[test]
fn proietta_e_predici_ricostruiscono_la_base() {
    // Verifica numerica: y = S·U·(Vᵀx) contro il calcolo a mano.
    let mut u = vec![0.0f32; D * RANGO];
    let mut v = vec![0.0f32; D * RANGO];
    let mut rng = Rng::new(7);
    for q in u.iter_mut() {
        *q = rng.gauss() as f32;
    }
    for q in v.iter_mut() {
        *q = rng.gauss() as f32;
    }
    let x: Vec<f32> = (0..D).map(|_| rng.gauss() as f32).collect();
    let mut z = [0.0f32; RANGO];
    plasticita::proietta(&v, D, &x, &mut z);
    let mut y = [0.0f32; D];
    plasticita::predici_aggiungi(&u, D, &z, &mut y);
    for i in 0..D {
        let mut atteso = 0.0f64;
        for k in 0..RANGO {
            let mut zk = 0.0f64;
            for j in 0..D {
                zk += v[j * RANGO + k] as f64 * x[j] as f64;
            }
            atteso += u[i * RANGO + k] as f64 * zk;
        }
        atteso *= S as f64;
        assert!(
            (y[i] as f64 - atteso).abs() < 1e-2,
            "predizione plastica sbagliata i={}: {} vs {}",
            i,
            y[i],
            atteso
        );
    }
}

#[test]
fn clamp_per_rango_protegge_da_runaway() {
    // Dopo un passo violento (η alto, e grande) l'energia per rango resta ≤ EK_MAX.
    let d = 32usize;
    let mut u = vec![0.0f32; d * RANGO];
    let mut v = vec![1.0f32; d * RANGO];
    let mut ad = Adattatore {
        u: &mut u,
        v: &mut v,
        d_out: d,
        d_in: d,
        w_norm: 1.0,
        e_ema: 0.0,
    };
    let x = vec![1.0f32; d];
    let e = vec![50.0f32; d];
    let mut z = [0.0f32; RANGO];
    let mut ute = [0.0f32; RANGO];
    ad.passo(&x, &e, 1.0, 10.0, &V21, &mut z, &mut ute);
    for k in 0..RANGO {
        let mut nu = 0.0f64;
        let mut nv = 0.0f64;
        for i in 0..d {
            let qu = ad.u[i * RANGO + k] as f64;
            let qv = ad.v[i * RANGO + k] as f64;
            nu += qu * qu;
            nv += qv * qv;
        }
        let ek = nu.sqrt() * nv.sqrt();
        assert!(ek <= EK_MAX as f64 + 1e-3, "E_{} = {:.4} supera EK_MAX", k, ek);
    }
}

#[test]
fn cap_rho_non_supera_il_limite() {
    // Con w_norm minuscolo ρ esploderebbe senza cap: dopo il passo deve essere ≤ RHO_CAP.
    let d = 24usize;
    let mut u = vec![0.0f32; d * RANGO];
    let mut v = vec![0.0f32; d * RANGO];
    let mut rng = Rng::new(3);
    for q in u.iter_mut() {
        *q = rng.gauss() as f32;
    }
    for q in v.iter_mut() {
        *q = rng.gauss() as f32;
    }
    let mut ad = Adattatore {
        u: &mut u,
        v: &mut v,
        d_out: d,
        d_in: d,
        w_norm: 1e-4,
        e_ema: 0.0,
    };
    let x = vec![0.5f32; d];
    let e = vec![0.1f32; d];
    let mut z = [0.0f32; RANGO];
    let mut ute = [0.0f32; RANGO];
    let esito = ad.passo(&x, &e, 1.0, 1.0, &V21, &mut z, &mut ute);
    assert!(esito.cap_hit, "con w_norm = 1e-4 il cap ρ deve scattare");
    // ρ = S·‖U Vᵀ‖_F / w_norm  (dopo il cap deve essere ~RHO_CAP)
    let mut acc = 0.0f64;
    for a in 0..RANGO {
        for b in 0..RANGO {
            let mut gu = 0.0f64;
            let mut gv = 0.0f64;
            for i in 0..d {
                gu += (ad.u[i * RANGO + a] * ad.u[i * RANGO + b]) as f64;
                gv += (ad.v[i * RANGO + a] * ad.v[i * RANGO + b]) as f64;
            }
            acc += gu * gv;
        }
    }
    let rho_ora = S as f64 * acc.max(0.0).sqrt() / 1e-4;
    assert!(
        rho_ora <= RHO_CAP as f64 * 1.02,
        "ρ dopo il cap = {:.4} (> {})",
        rho_ora,
        RHO_CAP
    );
}

#[test]
fn gate_chiuso_non_apprende() {
    // M < 0 con gate positivo ⇒ M⁺ = 0 ⇒ ΔU = ΔV = 0: gli adattatori solo DECADONO.
    let d = 16usize;
    let mut u = vec![0.0f32; d * RANGO];
    let mut v = vec![0.0f32; d * RANGO];
    let mut rng = Rng::new(11);
    for q in u.iter_mut() {
        *q = rng.gauss() as f32;
    }
    for q in v.iter_mut() {
        *q = rng.gauss() as f32;
    }
    let u_prima = u.clone();
    let v_prima = v.clone();
    let mut ad = Adattatore {
        u: &mut u,
        v: &mut v,
        d_out: d,
        d_in: d,
        w_norm: 1.0,
        e_ema: 0.0,
    };
    let x = vec![1.0f32; d];
    let e = vec![1.0f32; d];
    let mut z = [0.0f32; RANGO];
    let mut ute = [0.0f32; RANGO];
    ad.passo(&x, &e, -0.8, 1.0, &V21, &mut z, &mut ute);
    let ampiezza = |s: &[f32]| s.iter().map(|q| q * q).sum::<f32>().sqrt();
    assert!(
        ampiezza(ad.u) <= ampiezza(&u_prima) + 1e-6
            && ampiezza(ad.v) <= ampiezza(&v_prima) + 1e-6,
        "con M⁺ = 0 gli adattatori devono solo decadere, non crescere"
    );
}

#[test]
fn rho_cap_comprime_ma_non_diverge() {
    let d = 8;
    let mut u = vec![1.0f32; d * RANGO];
    let mut v = vec![1.0f32; d * RANGO]; // pesi enormi → ρ enorme
    let mut att = Adattatore {
        u: &mut u,
        v: &mut v,
        d_out: d,
        d_in: d,
        w_norm: 1.0,
        e_ema: 0.0,
    };
    let x = vec![1.0f32; d];
    let e = vec![1.0f32; d];
    let mut z = [0.0f32; RANGO];
    let mut ute = [0.0f32; RANGO];
    let esito = att.passo(&x, &e, 1.0, 10.0, &V21, &mut z, &mut ute);
    assert!(esito.cap_hit, "ρ sopra RHO_CAP deve scattare il cap");
    assert!(esito.rho.is_finite());
}

#[test]
fn potatura_redraw_risveglia_il_rango() {
    // §4.2: redraw = U azzerato, V rigenerato NON nullo → il gradiente può
    // tornare. ZeroEntrambi (v2.0) condanna il rango alla morte eterna.
    let d = 8;
    let mut u = vec![0.0f32; d * RANGO];
    let mut v = vec![0.001f32; d * RANGO]; // energia sotto ε_rank
    let mut att = Adattatore {
        u: &mut u,
        v: &mut v,
        d_out: d,
        d_in: d,
        w_norm: 1.0,
        e_ema: 0.0,
    };
    let mut seed = 12345u32;
    let morti = att.potatura(&V21, &mut seed);
    assert_eq!(morti as usize, RANGO, "tutti i ranghi sotto soglia");
    let mut v_non_nullo = false;
    for j in 0..d * RANGO {
        if att.v[j].abs() > 0.0 {
            v_non_nullo = true;
        }
    }
    assert!(v_non_nullo, "redraw: V deve essere rigenerato non nullo");

    let mut u2 = vec![0.0f32; d * RANGO];
    let mut v2 = vec![0.001f32; d * RANGO];
    let mut att2 = Adattatore {
        u: &mut u2,
        v: &mut v2,
        d_out: d,
        d_in: d,
        w_norm: 1.0,
        e_ema: 0.0,
    };
    let mut seed2 = 12345u32;
    let _ = att2.potatura(&V20, &mut seed2);
    let mut v2_nullo = true;
    for j in 0..d * RANGO {
        if att2.v[j].abs() > 0.0 {
            v2_nullo = false;
        }
    }
    assert!(v2_nullo, "zero_both: V resta zero (trappola storica)");
}

#[test]
fn congelato_e_il_riferimento() {
    // Con U=V=0: loss = 1 (modello congelato), canary = 0 (nessun danno).
    let p = pacchetto(42);
    let u = vec![0.0f32; D * RANGO];
    let v = vec![0.0f32; D * RANGO];
    let (loss, can) = evaluate(&p.mondo.w, &u, &v, &p.xe, &p.xc, &p.mondo.drift[1]);
    assert!((loss - 1.0).abs() < 1e-9, "congelato: loss = 1, ottenuto {}", loss);
    assert_eq!(can, 0.0, "congelato: canary = 0");
}

#[test]
fn v21_su_mondo_completo_apprende_e_non_diverge() {
    // Smoke test a singolo seed (veloce): la regola validata, sul mondo intero,
    // scende sotto il congelato (loss < 1), protegge la base e non diverge.
    let p = pacchetto_cached(0);
    let w = [1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0, 0.0]; // PESI_VALIDATI (E_pred fuori dal gate)
    let r = run(p, w, &V21, 0.3, MMode::Live);
    assert!(!r.diverged, "ρ deve restare finito (nessuna divergenza)");
    assert!(r.rho.is_finite());
    assert!(r.loss2 < 1.0, "deve adattare (loss2 < 1): {:.3}", r.loss2);
    assert!(r.can2 < 0.20, "il canary deve restare piccolo: {:.3}", r.can2);
    assert!(r.cap_pct < 100.0, "il cap ρ non deve scattare a ogni passo");
    assert!(r.dead < RANGO as f64, "non tutti i rangi devono morire");
}

// ── Sweep + tabella finale + controlli (il cuore della sim) ────────────────

#[test]
fn sweep_e_tabella_finale_riproducono_i_trend() {
    // 1. Sweep: eta per arma sui seed [0,1,2,3] (come la sim).
    let seeds_sweep = [0u64, 1, 2, 3];
    let arms = armi();
    let mut best_eta = [0.0f64; 5];
    for (ai, arma) in arms.iter().enumerate() {
        let mut best = f64::INFINITY;
        let mut best_e = arma.etas[0];
        for &eta in arma.etas {
            let rs: Vec<Res> = seeds_sweep
                .iter()
                .map(|&s| run(pacchetto_cached(s), arma.w, &arma.cfg, eta, MMode::Live))
                .collect();
            let sc = score(&rs);
            if sc < best {
                best = sc;
                best_e = eta;
            }
        }
        best_eta[ai] = best_e;
    }

    // 2. Tabella finale su seed NUOVI [10..17] con l'eta dello sweep.
    let seeds_final = [10u64, 11, 12, 13, 14, 15, 16, 17];
    let mut finals: Vec<Vec<Res>> = Vec::new();
    for (ai, arma) in arms.iter().enumerate() {
        let rs: Vec<Res> = seeds_final
            .iter()
            .map(|&s| run(pacchetto_cached(s), arma.w, &arma.cfg, best_eta[ai], MMode::Live))
            .collect();
        finals.push(rs);
    }

    // 3. Controlli sul braccio base (CTRL = "v2.1 senza E_pred").
    let ctrl_arma = &arms[IDX_CTRL];
    let mut ctrl_shuffle = Vec::new();
    let mut ctrl_const = Vec::new();
    for &s in &seeds_final {
        let p = pacchetto_cached(s);
        ctrl_shuffle.push(run(p, ctrl_arma.w, &ctrl_arma.cfg, best_eta[IDX_CTRL], MMode::Shuffle));
        ctrl_const.push(run(p, ctrl_arma.w, &ctrl_arma.cfg, best_eta[IDX_CTRL], MMode::Const));
    }

    // ── Assertion sui TREND (margini larghi: il RNG è nostro) ────────────

    let ctrl = &finals[IDX_CTRL];
    let ctrl_loss2 = mean(ctrl, |r| r.loss2);
    let ctrl_can2 = mean(ctrl, |r| r.can2);
    let ctrl_dead = mean(ctrl, |r| r.dead);
    assert!(
        ctrl_loss2 < 0.6,
        "la regola validata deve ADATTARE (loss2 << 1): loss2 = {:.3}",
        ctrl_loss2
    );
    assert!(
        ctrl_can2 < 0.08,
        "la regola validata NON deve danneggiare la base (canary piccolo): {:.3}",
        ctrl_can2
    );
    assert_eq!(ctrl_dead, 0.0, "nessun rango morto col redraw");

    // v2.0: non adatta — quasi tutti i ranghi morti (trappola §4.2).
    let v20 = &finals[IDX_V20];
    let v20_dead = mean(v20, |r| r.dead);
    let v20_loss2 = mean(v20, |r| r.loss2);
    assert!(
        v20_dead >= 5.0 || v20_loss2 > 0.9,
        "v2.0 deve fallire (morti ≈ 8 o loss ≈ 1): morti = {:.1}, loss2 = {:.3}",
        v20_dead,
        v20_loss2
    );

    // M con segno è peggiore di M⁺.
    let ms_loss2 = mean(&finals[IDX_M_SEGNO], |r| r.loss2);
    assert!(
        ms_loss2 > ctrl_loss2,
        "M con segno deve essere PEGGIORE di M⁺: {:.3} vs {:.3}",
        ms_loss2,
        ctrl_loss2
    );

    // V hebbiano è peggiore del V a errore (§3.1).
    let hb_loss2 = mean(&finals[IDX_HEBB], |r| r.loss2);
    assert!(
        hb_loss2 > ctrl_loss2,
        "V hebbiano deve essere PEGGIORE del V a errore: {:.3} vs {:.3}",
        hb_loss2,
        ctrl_loss2
    );

    // M reale batte i controlli a coppie (stesso seed).
    let wins_shuffle = ctrl
        .iter()
        .zip(ctrl_shuffle.iter())
        .filter(|(b, c)| b.loss2 < c.loss2)
        .count();
    let wins_const = ctrl
        .iter()
        .zip(ctrl_const.iter())
        .filter(|(b, c)| b.loss2 < c.loss2)
        .count();
    assert!(
        wins_shuffle >= 6,
        "M reale deve battere M rimescolato su loss2 (≥6/8): {}/8",
        wins_shuffle
    );
    assert!(
        wins_const >= 5,
        "M reale deve battere M costante su loss2 (≥5/8): {}/8",
        wins_const
    );

    // Stampa la tabella (riproduce l'output della sim, per il confronto umano).
    let _ = IDX_V21_EPRED; // usato solo nella stampa completa qui sotto
    println!("\n== Tabella finale Rust (eta dallo sweep su seed 0-3, valutazione su 10-17)");
    println!(
        "{:<36} {:>6} {:>9} {:>9} {:>9} {:>6}",
        "braccio", "eta", "loss_f1", "loss_f2", "canary", "morti"
    );
    for (ai, arma) in arms.iter().enumerate() {
        let rs = &finals[ai];
        println!(
            "{:<36} {:>6.3} {:>9.3} {:>9.3} {:>9.3} {:>6.1}",
            arma.nome,
            best_eta[ai],
            mean(rs, |r| r.loss1),
            mean(rs, |r| r.loss2),
            mean(rs, |r| r.can2),
            mean(rs, |r| r.dead),
        );
    }
    println!(
        "{:<36} {:>6.3} {:>9} {:>9.3} {:>9.3} {:>6}",
        "controllo: M rimescolato",
        best_eta[IDX_CTRL],
        "-",
        mean(&ctrl_shuffle, |r| r.loss2),
        mean(&ctrl_shuffle, |r| r.can2),
        mean(&ctrl_shuffle, |r| r.dead),
    );
    println!(
        "{:<36} {:>6.3} {:>9} {:>9.3} {:>9.3} {:>6}",
        "controllo: M costante",
        best_eta[IDX_CTRL],
        "-",
        mean(&ctrl_const, |r| r.loss2),
        mean(&ctrl_const, |r| r.can2),
        mean(&ctrl_const, |r| r.dead),
    );
    println!(
        "   vs rimescolato (loss2): {}/8   vs costante (loss2): {}/8",
        wins_shuffle, wins_const
    );
}
