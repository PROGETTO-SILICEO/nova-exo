// ── plasticita.rs — Regola di plasticità somatica a tre fattori (locale) ──
//
// Porting Rust di `tools/fase1_plasticita_sim.py` — Fase 1.
// Teoria: docs/TEORIA_PLASTICITA_SOMATICA_NUCLEO.md §2–§4.
// Simulazione validata: 16–17/10/2026 (Nova / Antigravity).
// Porting: Sempre (OpenCode), 17/10/2026. Direzione: Alfonso Riva.
//
// COSA È
//   La legge con cui il Nucleo modifica i propri adattatori a basso rango
//   (U, V) a runtime, SENZA backpropagation globale: solo l'errore di
//   predizione locale `e` del singolo layer + il neuromodulatore somatico M(t).
//
//   W_effettivo(t) = W_lenti + S·U(t)·V(t)ᵀ,   S = α/r  (§2)
//
//   z    = Vᵀ x                                   (collo di bottiglia, r)
//   ŷ    = W x + S·U z
//   e    = y − ŷ                                  (errore locale, d_out)
//   M    = tanh(ω_n·n + ω_r·Reward − ω_p·p)       (§3.2, E_pred RIMOSSO)
//   M⁺   = max(M, 0)
//   ΔU   = η·M⁺·S·(e zᵀ)/(ε + S²(zᵀz))            − γ·U     (§3.1 NLMS)
//   ΔV   = η·M⁺·S·(x (Uᵀe)ᵀ)/(ε + S²(xᵀx)·‖U‖²_F) − γ·V
//   poi: clamp per rango E_k = ‖u_k‖‖v_k‖ ≤ 1 (§3.4) e cap ρ ≤ 0.10 (§6.2)
//
// SCOPE DI QUESTO FILE
//   Solo la REGOLA (matematica pura, no_std, niente allocazioni). Testata su
//   host da `bench/plasticita_host.rs`, che include questo codice REALE.
//   L'allocazione dei 224 KiB/layer di adattatori in .bss e la persistenza
//   AHCI sono i passi successivi (§2.3, §5) — non sono qui.
//
// NON-FEDELTÀ DICHIARATE (onestà)
//   - Il redraw dopo la potatura usa un uniforme a varianza 1/d_in (via
//     LCG), non un gaussiano: serve solo a rompere la simmetria U=V=0
//     (§4.2). La varianza del canale è la stessa del gaussiano della sim.
//   - `Adattatore::passo` riceve l'errore `e` già calcolato: il kernel dovrà
//     fornirlo (next-token §3.3, o errore somatico dal PFM §3.5).

// Perché nessun avviso `dead_code`: nel kernel binario questa legge è ancora
// solo ALLOGGIATA (nucleo.rs la richiama per AdattatoriLayer) ma non ARMATA —
// il maestro che fornisce l'errore locale `e` è la Fase 2. L'intera superficie
// (neuromodulatore, V20/V21, clamp, potatura) è invece esercitata a pieno da
// bench/plasticita_host.rs e bench/nucleo_host.rs, che la compilano davvero.
#![allow(dead_code)]

use crate::math::{sqrtf, tanhf};

/// Rango dell'adattatore r (§2). Fissato a 8 dalla teoria.
pub const RANGO: usize = 8;

/// Fattore di scala LoRA α (§2: α = 16).
pub const ALPHA: f32 = 16.0;

/// S = α / r = 2.0 (sim: ALPHA/R).
pub const S: f32 = ALPHA / (RANGO as f32);

/// Decadimento omeostatico continuo γ, disaccoppiato dal segno di M (§3.1).
pub const GAMMA: f32 = 1e-4;

/// Extra-decadimento quando M < 0 — ABLAZIONE storica (v2.1 pre-fix), non la
/// regola validata. Tenuta solo perché il banco host possa riprodurre i bracci.
pub const GNEG: f32 = 1e-2;

/// Cap del rapporto sinaptico ρ = S·‖UVᵀ‖_F / ‖W_lenti‖_F (§6.2).
pub const RHO_CAP: f32 = 0.10;

/// Energia massima per rango E_k = ‖u_k‖·‖v_k‖ (§3.4).
pub const EK_MAX: f32 = 1.0;

/// Soglia di rumore di rango per la potatura nel sonno (§4.2).
pub const EPS_RANK: f32 = 0.02;

/// ε di regolarizzazione NLMS (§3.1).
pub const NLMS_EPS: f32 = 1.0;

/// Scala dell'errore relativo per la telemetria E ∈ [0,1] (§3.2, storico).
pub const E_SCALE: f32 = 0.10;

// ── Neuromodulatore ──────────────────────────────────────────────────────

/// Pesi del neuromodulatore. La regola VALIDATA (§3.2): (1/3, 1/3, 1/3, 0).
/// `errore` resta a 0: E_pred è stato rimosso dal gate (paradosso bimodale).
#[derive(Clone, Copy)]
pub struct PesiNeuromodulatore {
    pub novita: f32,
    pub reward: f32,
    pub allarme: f32,
    pub errore: f32,
}

/// Pesi bilanciati normalizzati del gate validato (§3.2).
pub const PESI_VALIDATI: PesiNeuromodulatore = PesiNeuromodulatore {
    novita: 1.0 / 3.0,
    reward: 1.0 / 3.0,
    allarme: 1.0 / 3.0,
    errore: 0.0,
};

/// Neuromodulatore somatico M(t) ∈ (−1, 1) — §3.2.
/// n = novità sensoriale, reward = benessere/interazione, p = allarme.
/// `e_pred` è l'errore relativo E∈[0,1]: coefficiente 0 nella regola validata.
pub fn neuromodulatore(n: f32, reward: f32, allarme: f32, e_pred: f32, w: &PesiNeuromodulatore) -> f32 {
    let a = w.novita * n + w.reward * reward - w.allarme * allarme - w.errore * e_pred;
    tanhf(a)
}

// ── Configurazione (per riprodurre i bracci della sim su host) ────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// M con segno (v2.0).
    Signed,
    /// M⁺ = max(M, 0) (v2.1 validata).
    Pos,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VRule {
    /// ΔV Hebbiano x zᵀ (v2.0 — deriva distruttiva, §3.1).
    Hebb,
    /// ΔV guidato dall'errore x (Uᵀe)ᵀ (v2.1 validata).
    Err,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Clamp {
    /// clamp solo sulle colonne di U (v2.0).
    SoloU,
    /// clamp sul prodotto ‖u_k‖·‖v_k‖ (v2.1 validata, §3.4).
    Prodotto,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Prune {
    /// azzera U e V del rango morto (v2.0 — trappola del gradiente nullo, §4.2).
    ZeroEntrambi,
    /// azzera U e RIGENERA V (v2.1 validata, §4.2).
    Redraw,
}

#[derive(Clone, Copy)]
pub struct Config {
    pub gate: Gate,
    pub vrule: VRule,
    pub nlms: bool,
    pub clamp: Clamp,
    pub prune: Prune,
}

/// v2.1 — la regola validata in Fase 1.
pub const V21: Config = Config {
    gate: Gate::Pos,
    vrule: VRule::Err,
    nlms: true,
    clamp: Clamp::Prodotto,
    prune: Prune::Redraw,
};

/// v2.0 — "alla lettera", solo per il confronto nel banco host.
pub const V20: Config = Config {
    gate: Gate::Signed,
    vrule: VRule::Hebb,
    nlms: false,
    clamp: Clamp::SoloU,
    prune: Prune::ZeroEntrambi,
};

// ── Adattatore (vista su memoria del chiamante) ───────────────────────────

/// Vista su un adattatore a basso rango di UNA proiezione.
/// `u` e `v` sono forniti dal chiamante (riga-major): vivranno in .bss quando
/// i layer li allocheranno (§2.3). Qui la regola lavora su slice, senza
/// possedere memoria — così è testabile e innestabile ovunque.
pub struct Adattatore<'a> {
    /// d_out × RANGO, riga-major: u[i*RANGO + k].
    pub u: &'a mut [f32],
    /// d_in × RANGO, riga-major: v[j*RANGO + k].
    pub v: &'a mut [f32],
    pub d_out: usize,
    pub d_in: usize,
    /// ‖W_lenti‖_F del layer (costante, §6.2: denominatore di ρ).
    pub w_norm: f32,
    /// Errore relativo EMA (telemetria / sonno).
    pub e_ema: f32,
}

/// Esito di un passo di plasticità.
pub struct EsitoPasso {
    /// ρ PRIMA del cap (§6.2). Può superare RHO_CAP se `cap_hit`.
    pub rho: f32,
    /// vero se il cap ρ è scattato in questo passo.
    pub cap_hit: bool,
}

/// z = Vᵀ x  (§3.1). `z` ha lunghezza RANGO.
pub fn proietta(v: &[f32], d_in: usize, x: &[f32], z: &mut [f32]) {
    for k in 0..RANGO {
        z[k] = 0.0;
    }
    for j in 0..d_in {
        let xj = x[j];
        let base = j * RANGO;
        for k in 0..RANGO {
            z[k] += v[base + k] * xj;
        }
    }
}

/// y += S·(U z)  — leggi l'uscita plastica SOPRA la base congelata W x.
pub fn predici_aggiungi(u: &[f32], d_out: usize, z: &[f32], y: &mut [f32]) {
    for i in 0..d_out {
        let base = i * RANGO;
        let mut acc = 0.0;
        for k in 0..RANGO {
            acc += u[base + k] * z[k];
        }
        y[i] += S * acc;
    }
}

/// Gram matrix r×r di una matrice d×r (riga-major). MᵀM.
fn gram(m: &[f32], d: usize) -> [[f32; RANGO]; RANGO] {
    let mut g = [[0.0f32; RANGO]; RANGO];
    for i in 0..d {
        let row = &m[i * RANGO..i * RANGO + RANGO];
        for a in 0..RANGO {
            let ra = row[a];
            for b in 0..RANGO {
                g[a][b] += ra * row[b];
            }
        }
    }
    g
}

/// Intero uniforme in [−1, 1) da un LCG (deterministico, no_std).
fn rand_pm1(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
    ((*seed >> 16) as f32 / 65536.0) * 2.0 - 1.0
}

impl<'a> Adattatore<'a> {
    /// Applica un passo della regola a tre fattori.
    ///
    /// - `x`  : attivazione in ingresso al layer (d_in)
    /// - `e`  : errore locale y − ŷ (d_out)
    /// - `m_meta` : M(t) GREZZO (con segno); il gate lo trasforma secondo `cfg`
    /// - `eta`: tasso di apprendimento (scala temporale)
    /// - `z`, `ute`: buffer di lavoro di lunghezza RANGO
    pub fn passo(
        &mut self,
        x: &[f32],
        e: &[f32],
        m_meta: f32,
        eta: f32,
        cfg: &Config,
        z: &mut [f32],
        ute: &mut [f32],
    ) -> EsitoPasso {
        // z = Vᵀ x ; ute = Uᵀ e  (§3.1)
        proietta(self.v, self.d_in, x, z);
        for k in 0..RANGO {
            ute[k] = 0.0;
        }
        for i in 0..self.d_out {
            let ei = e[i];
            let base = i * RANGO;
            for k in 0..RANGO {
                ute[k] += self.u[base + k] * ei;
            }
        }

        // normalizzatori NLMS
        let mut zz = 0.0;
        for k in 0..RANGO {
            zz += z[k] * z[k];
        }
        let mut xx = 0.0;
        for j in 0..self.d_in {
            xx += x[j] * x[j];
        }
        let mut uu = 0.0;
        for i in 0..self.d_out {
            let base = i * RANGO;
            for k in 0..RANGO {
                let q = self.u[base + k];
                uu += q * q;
            }
        }
        let den_u = NLMS_EPS + S * S * zz;
        let den_v = NLMS_EPS + S * S * xx * uu;

        // gate M⁺ e decadimento disaccoppiato dal segno (§3.1, §3.2)
        let (gate, mneg) = match cfg.gate {
            Gate::Pos => (if m_meta > 0.0 { m_meta } else { 0.0 }, if m_meta < 0.0 { -m_meta } else { 0.0 }),
            Gate::Signed => (m_meta, 0.0),
        };
        let g = GAMMA + GNEG * mneg;
        let ks = eta * gate * S;
        let k0 = eta * gate;

        // ΔU ; U ← U + ΔU − γU
        for i in 0..self.d_out {
            let ei = e[i];
            let base = i * RANGO;
            for k in 0..RANGO {
                let du = if cfg.nlms { ks * ei * z[k] / den_u } else { k0 * ei * z[k] };
                let cur = self.u[base + k];
                self.u[base + k] = cur + du - g * cur;
            }
        }
        // ΔV ; V ← V + ΔV − γV
        for j in 0..self.d_in {
            let xj = x[j];
            let base = j * RANGO;
            for k in 0..RANGO {
                let dv = if cfg.nlms {
                    if cfg.vrule == VRule::Err {
                        ks * xj * ute[k] / den_v
                    } else {
                        k0 * xj * z[k] / (NLMS_EPS + xx)
                    }
                } else {
                    k0 * xj * z[k]
                };
                let cur = self.v[base + k];
                self.v[base + k] = cur + dv - g * cur;
            }
        }

        // clamp per rango (§3.4 per la v2.1; solo-U per la v2.0)
        match cfg.clamp {
            Clamp::SoloU => {
                for k in 0..RANGO {
                    let mut nu = 0.0;
                    for i in 0..self.d_out {
                        let q = self.u[i * RANGO + k];
                        nu += q * q;
                    }
                    let nrm = sqrtf(nu);
                    if nrm > 1.0 {
                        let f = 1.0 / nrm;
                        for i in 0..self.d_out {
                            self.u[i * RANGO + k] *= f;
                        }
                    }
                }
            }
            Clamp::Prodotto => {
                for k in 0..RANGO {
                    let mut nu = 0.0;
                    for i in 0..self.d_out {
                        let q = self.u[i * RANGO + k];
                        nu += q * q;
                    }
                    let mut nv = 0.0;
                    for j in 0..self.d_in {
                        let q = self.v[j * RANGO + k];
                        nv += q * q;
                    }
                    let ek = sqrtf(nu) * sqrtf(nv);
                    let ek_safe = if ek > 1e-12 { ek } else { 1e-12 };
                    let ratio = EK_MAX / ek_safe;
                    let f = sqrtf(if ratio < 1.0 { ratio } else { 1.0 });
                    if f < 1.0 {
                        for i in 0..self.d_out {
                            self.u[i * RANGO + k] *= f;
                        }
                        for j in 0..self.d_in {
                            self.v[j * RANGO + k] *= f;
                        }
                    }
                }
            }
        }

        // ρ = S·‖UVᵀ‖_F / ‖W‖_F, poi cap scalando U e V di f (A scala di f²) — §6.2
        let gu = gram(self.u, self.d_out);
        let gv = gram(self.v, self.d_in);
        let mut acc = 0.0;
        for a in 0..RANGO {
            for b in 0..RANGO {
                acc += gu[a][b] * gv[a][b];
            }
        }
        let acc = if acc > 0.0 { acc } else { 0.0 };
        let rho = S * sqrtf(acc) / self.w_norm;
        let mut cap_hit = false;
        if rho > RHO_CAP {
            cap_hit = true;
            let f = sqrtf(RHO_CAP / rho);
            for q in self.u.iter_mut() {
                *q *= f;
            }
            for q in self.v.iter_mut() {
                *q *= f;
            }
        }
        EsitoPasso { rho, cap_hit }
    }

    /// Aggiorna l'errore relativo EMA (telemetria, §3.2 storico).
    /// `e_norm` = ‖e‖, `y_norm` = ‖y‖.
    pub fn aggiorna_errore(&mut self, e_norm: f32, y_norm: f32) {
        let rel = if y_norm > 1e-12 { e_norm / y_norm } else { 0.0 };
        self.e_ema += 0.01 * (rel - self.e_ema);
    }

    /// Errore relativo normalizzato E = min(e_ema/E_SCALE, 1) (§3.2).
    pub fn errore_normalizzato(&self) -> f32 {
        let e = self.e_ema / E_SCALE;
        if e < 1.0 { e } else { 1.0 }
    }

    /// Potatura sinaptica nel sonno (§4.2). Ritorna il numero di rangi potati.
    /// `seed` è mutato: il redraw è deterministico e riproducibile.
    pub fn potatura(&mut self, cfg: &Config, seed: &mut u32) -> u32 {
        let mut morti = 0;
        for k in 0..RANGO {
            let mut nu = 0.0;
            for i in 0..self.d_out {
                let q = self.u[i * RANGO + k];
                nu += q * q;
            }
            let mut nv = 0.0;
            for j in 0..self.d_in {
                let q = self.v[j * RANGO + k];
                nv += q * q;
            }
            if sqrtf(nu) * sqrtf(nv) < EPS_RANK {
                morti += 1;
                for i in 0..self.d_out {
                    self.u[i * RANGO + k] = 0.0;
                }
                match cfg.prune {
                    Prune::ZeroEntrambi => {
                        for j in 0..self.d_in {
                            self.v[j * RANGO + k] = 0.0;
                        }
                    }
                    Prune::Redraw => {
                        // uniforme a varianza 1/d_in (il gaussiano della sim ha
                        // la stessa varianza; qui basta rompere U=V=0).
                        let scala = sqrtf(3.0 / (self.d_in as f32));
                        for j in 0..self.d_in {
                            self.v[j * RANGO + k] = rand_pm1(seed) * scala;
                        }
                    }
                }
            }
        }
        morti
    }
}
