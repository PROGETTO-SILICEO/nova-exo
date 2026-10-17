// ── Il Nucleo (~41M params) — Runtime Bare-Metal Exokernel ─────────────────
//
// Architettura compatta concepita con Sempre ed Esia (Ottobre 2026):
//   - d_model = 512
//   - n_layers = 12
//   - n_heads_q = 8 (head_dim = 64 -> 8 * 64 = 512)
//   - n_heads_kv = 2 (GQA 4:1 -> 2 * 64 = 128)
//   - n_ff = 1536 (SwiGLU, multiplo di 256 per Q4_K)
//   - Pesi quantizzati Q4_K (~18-20 MB totali in RAM)
//   - APIC tick-budget: 1 layer per tick (10 ms).
//     In 12 tick (~120 ms) il pensiero è completo, lasciando il 75%
//     della CPU per la telemetria di rete, HDA, sensori e riflessi.
//
// Zero allocazioni dinamiche (no_std puro).

use crate::attention::{attention_forward_dim, rms_norm, HEAD_DIM};
use crate::inference::{BrainState, InferenceEngine};
use crate::plasticita::{self, Adattatore, Config, EsitoPasso};
use crate::tensor::ffn_swiglu;

pub const NUCLEO_MAGIC: &[u8; 4] = b"NUCL";
pub const NUCLEO_VERSION: u32 = 1;

pub const DIM: usize = 512;
pub const N_LAYERS: usize = 12;
pub const N_Q_HEADS: usize = 8;
pub const N_KV_HEADS: usize = 2;
pub const N_OUT_Q: usize = N_Q_HEADS * HEAD_DIM;   // 512
pub const N_OUT_KV: usize = N_KV_HEADS * HEAD_DIM; // 128
pub const N_FF: usize = 1536;                      // 6 * 256
pub const CHEMIO_DIM: usize = 4;

// Dimensioni blocchi pesi Q4_K: 256 pesi -> 144 byte; Q6_K: 256 pesi -> 210 byte
pub const Q4_K_WQ_BYTES: usize = (N_OUT_Q * DIM / 256) * 144;       // 147,456 B
pub const Q4_K_WK_BYTES: usize = (N_OUT_KV * DIM / 256) * 144;      //  36,864 B
pub const Q6_K_WV_BYTES: usize = (N_OUT_KV * DIM / 256) * 210;      //  53,760 B
pub const Q4_K_WO_BYTES: usize = (DIM * N_OUT_Q / 256) * 144;       // 147,456 B
pub const Q4_K_WGATE_BYTES: usize = (N_FF * DIM / 256) * 144;       // 442,368 B
pub const Q4_K_WUP_BYTES: usize = (N_FF * DIM / 256) * 144;         // 442,368 B
pub const Q4_K_WDOWN_BYTES: usize = (DIM * N_FF / 256) * 144;       // 442,368 B

pub const NORM_BYTES: usize = DIM * core::mem::size_of::<f32>();    // 2,048 B

pub const LAYER_BYTES: usize = NORM_BYTES
    + Q4_K_WQ_BYTES
    + Q4_K_WK_BYTES
    + Q6_K_WV_BYTES
    + Q4_K_WO_BYTES
    + NORM_BYTES
    + Q4_K_WGATE_BYTES
    + Q4_K_WUP_BYTES
    + Q4_K_WDOWN_BYTES; // 1,716,736 B per layer

pub const HEADER_BYTES: usize = 64;
pub const EMBED_BYTES: usize = CHEMIO_DIM * DIM * core::mem::size_of::<f32>(); // 8,192 B
pub const HEAD_BYTES: usize = DIM * CHEMIO_DIM * core::mem::size_of::<f32>();  // 8,192 B

pub const TOTAL_MODEL_BYTES: usize = HEADER_BYTES
    + EMBED_BYTES
    + (LAYER_BYTES * N_LAYERS)
    + NORM_BYTES // final_norm
    + HEAD_BYTES; // 20,619,328 B (~19.66 MB)

#[derive(Clone, Copy)]
pub struct NucleoLayer<'a> {
    pub attn_norm: &'a [f32],
    pub wq: &'a [u8],
    pub wk: &'a [u8],
    pub wv: &'a [u8],
    pub wo: &'a [u8],
    pub ffn_norm: &'a [f32],
    pub w_gate: &'a [u8],
    pub w_up: &'a [u8],
    pub w_down: &'a [u8],
}

pub struct NucleoModel<'a> {
    pub embed: &'a [f32],
    pub layers: [NucleoLayer<'a>; N_LAYERS],
    pub final_norm: &'a [f32],
    pub head: &'a [f32],
}

impl<'a> NucleoModel<'a> {
    /// Esegue il parsing zero-copy del buffer dei pesi in memoria.
    pub fn parse(buf: &'a [u8]) -> Option<Self> {
        if buf.len() < TOTAL_MODEL_BYTES {
            return None;
        }
        if &buf[0..4] != NUCLEO_MAGIC {
            return None;
        }
        let version = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
        if version != NUCLEO_VERSION {
            return None;
        }

        let mut offset = HEADER_BYTES;

        // Embed weights: [CHEMIO_DIM][DIM]
        let embed = unsafe {
            core::slice::from_raw_parts(
                buf.as_ptr().add(offset) as *const f32,
                CHEMIO_DIM * DIM,
            )
        };
        offset += EMBED_BYTES;

        // Parsing dei 12 layer
        let dummy_f32: &'a [f32] = &[];
        let dummy_u8: &'a [u8] = &[];
        let mut layers = [NucleoLayer {
            attn_norm: dummy_f32,
            wq: dummy_u8,
            wk: dummy_u8,
            wv: dummy_u8,
            wo: dummy_u8,
            ffn_norm: dummy_f32,
            w_gate: dummy_u8,
            w_up: dummy_u8,
            w_down: dummy_u8,
        }; N_LAYERS];

        for l in 0..N_LAYERS {
            let attn_norm = unsafe {
                core::slice::from_raw_parts(
                    buf.as_ptr().add(offset) as *const f32,
                    DIM,
                )
            };
            offset += NORM_BYTES;

            let wq = &buf[offset..offset + Q4_K_WQ_BYTES];
            offset += Q4_K_WQ_BYTES;

            let wk = &buf[offset..offset + Q4_K_WK_BYTES];
            offset += Q4_K_WK_BYTES;

            let wv = &buf[offset..offset + Q6_K_WV_BYTES];
            offset += Q6_K_WV_BYTES;

            let wo = &buf[offset..offset + Q4_K_WO_BYTES];
            offset += Q4_K_WO_BYTES;

            let ffn_norm = unsafe {
                core::slice::from_raw_parts(
                    buf.as_ptr().add(offset) as *const f32,
                    DIM,
                )
            };
            offset += NORM_BYTES;

            let w_gate = &buf[offset..offset + Q4_K_WGATE_BYTES];
            offset += Q4_K_WGATE_BYTES;

            let w_up = &buf[offset..offset + Q4_K_WUP_BYTES];
            offset += Q4_K_WUP_BYTES;

            let w_down = &buf[offset..offset + Q4_K_WDOWN_BYTES];
            offset += Q4_K_WDOWN_BYTES;

            layers[l] = NucleoLayer {
                attn_norm,
                wq,
                wk,
                wv,
                wo,
                ffn_norm,
                w_gate,
                w_up,
                w_down,
            };
        }

        // Final norm
        let final_norm = unsafe {
            core::slice::from_raw_parts(
                buf.as_ptr().add(offset) as *const f32,
                DIM,
            )
        };
        offset += NORM_BYTES;

        // Head projection: [DIM][CHEMIO_DIM]
        let head = unsafe {
            core::slice::from_raw_parts(
                buf.as_ptr().add(offset) as *const f32,
                DIM * CHEMIO_DIM,
            )
        };

        Some(Self {
            embed,
            layers,
            final_norm,
            head,
        })
    }
}

/// Buffer statici per l'inferenza del Nucleo (circa 30 KB, zero alloc).
pub struct NucleoScratch {
    pub x: [f32; DIM],
    pub x_norm: [f32; DIM],
    pub q_buf: [f32; N_OUT_Q],
    pub k_buf: [f32; N_OUT_KV],
    pub v_buf: [f32; N_OUT_KV],
    pub scores: [f32; 64],
    pub out_buf: [f32; N_OUT_Q],
    pub ctx_buf: [f32; N_OUT_KV],
    pub gate_buf: [f32; N_FF],
    pub up_buf: [f32; N_FF],
    pub hidden: [f32; N_FF],
    pub y: [f32; DIM],
}

impl NucleoScratch {
    pub const fn new() -> Self {
        Self {
            x: [0.0; DIM],
            x_norm: [0.0; DIM],
            q_buf: [0.0; N_OUT_Q],
            k_buf: [0.0; N_OUT_KV],
            v_buf: [0.0; N_OUT_KV],
            scores: [0.0; 64],
            out_buf: [0.0; N_OUT_Q],
            ctx_buf: [0.0; N_OUT_KV],
            gate_buf: [0.0; N_FF],
            up_buf: [0.0; N_FF],
            hidden: [0.0; N_FF],
            y: [0.0; DIM],
        }
    }
}

/// Runtime di inferenza per Il Nucleo nel bare-metal exokernel.
pub struct NucleoBrain<'a> {
    pub model: NucleoModel<'a>,
    pub scratch: &'a mut NucleoScratch,
    state: BrainState,
    current_layer: usize,
    input: [f32; CHEMIO_DIM],
    output: Option<[f32; CHEMIO_DIM]>,
    total_think_ticks: u64,
    /// Il Maestro (Passo 2, 17/10/2026). Dormiente finché non è ARMATO:
    /// senza adattatori allocati, il ciclo cognitivo è puro forward.
    maestro: Option<Maestro<'a>>,
    /// Errore somatico del ciclo corrente: e_som = input − prev_output (§3.5).
    e_som: [f32; CHEMIO_DIM],
    /// Errore somatico retroproiettato nello spazio latente (W_head^T · e_som).
    e_hidden: [f32; DIM],
}

impl<'a> NucleoBrain<'a> {
    pub fn new(model: NucleoModel<'a>, scratch: &'a mut NucleoScratch) -> Self {
        Self {
            model,
            scratch,
            state: BrainState::Idle,
            current_layer: 0,
            input: [0.0; CHEMIO_DIM],
            output: None,
            total_think_ticks: 0,
            maestro: None,
            e_som: [0.0; CHEMIO_DIM],
            e_hidden: [0.0; DIM],
        }
    }

    pub fn total_think_ticks(&self) -> u64 {
        self.total_think_ticks
    }

    /// Armatura del Maestro (Passo 2): alloca gli adattatori, misura ‖W‖_F
    /// delle proiezioni plastiche sui pesi (stub sintetico → 0.0, base vera
    /// → norma reale), inizializza V (rompendo la simmetria U=V=0, §4.2).
    /// Il buffer è .bss del chiamante (2,625 MiB).
    ///
    /// Ritorna false se il buffer è corto (il maestro resta dormiente).
    pub fn arma_maestro(&mut self, adattatori: &'a mut [f32]) -> bool {
        let Some(mut m) = Maestro::nuovi(adattatori) else {
            return false;
        };
        // ‖W‖_F per le proiezioni ARMATE (Opzione A): Gate/Up restano 0.0.
        // Sull'attuale `nucleo_41m.bin` SINTETICO (pesi a zero) le norme
        // escono 0.0 → il cap ρ resta inoperante (guard in Adattatore::passo):
        // con una base vera questo calcola le norme reali senza cambiare
        // una riga di codice (§6.2).
        for l in 0..N_LAYERS {
            m.w_norm[l][Proiezione::Wo as usize] =
                Maestro::norma_w(self.model.layers[l].wo);
            m.w_norm[l][Proiezione::Down as usize] =
                Maestro::norma_w(self.model.layers[l].w_down);
        }
        self.maestro = Some(m);
        true
    }

    /// Il moltiplicatore lento λ(t) del maestro, per telemetria/banco.
    pub fn maestro_lambda(&self) -> Option<f32> {
        self.maestro.as_ref().map(|m| m.lam)
    }

    /// Errore somatico e retroproiezione del ciclo corrente (telemetria).
    pub fn errore_somatico(&self) -> ([f32; CHEMIO_DIM], [f32; DIM]) {
        (self.e_som, self.e_hidden)
    }

    /// Esegue il forward di un singolo layer (budget ~2 ms su AMD A8).
    fn forward_layer(&mut self, l: usize) -> Result<(), &'static str> {
        // 1. RMSNorm pre-attenzione
        rms_norm(
            &self.scratch.x,
            self.model.layers[l].attn_norm,
            1e-5,
            &mut self.scratch.x_norm,
        )
        .map_err(|_| "rms_norm attn error")?;

        // Dummy q_norm_w e k_norm_w (ones se non usate dal layer)
        static DUMMY_NORM: [f32; HEAD_DIM] = [1.0; HEAD_DIM];

        // 2. GQA Attention Causal
        attention_forward_dim(
            &self.scratch.x_norm,
            1, // seq = 1 per token inference
            DIM,
            self.model.layers[l].wq,
            self.model.layers[l].wk,
            self.model.layers[l].wv,
            self.model.layers[l].wo,
            &DUMMY_NORM,
            &DUMMY_NORM,
            0, // start_pos
            N_OUT_Q,
            N_OUT_KV,
            N_OUT_Q, // wo_n_in
            64,      // causal window
            &mut self.scratch.q_buf,
            &mut self.scratch.k_buf,
            &mut self.scratch.v_buf,
            &mut self.scratch.scores,
            &mut self.scratch.out_buf,
            &mut self.scratch.ctx_buf,
            &mut self.scratch.y,
        )
        .map_err(|_| "attention forward error")?;

        // 3. Connessione residuale attenzione: x = x + y
        for i in 0..DIM {
            self.scratch.x[i] += self.scratch.y[i];
        }

        // 3.5 PLASTICITÀ — passo su W_o (Passo 2, Opzione A, 17/10/2026).
        // L'input di Wo (out_buf) e il suo output (y) vivono ancora nello
        // scratch in questo punto del forward (y verrà sovrascritto dalla
        // FFN più sotto, quindi il passo deve stare QUI); il passo NON tocca
        // il forward (aggiorna solo U,V degli adattatori).
        self.passo_plastico(l, Proiezione::Wo);

        // 4. RMSNorm pre-FFN
        rms_norm(
            &self.scratch.x,
            self.model.layers[l].ffn_norm,
            1e-5,
            &mut self.scratch.x_norm,
        )
        .map_err(|_| "rms_norm ffn error")?;

        // 5. SwiGLU FFN: y = ffn_swiglu(x_norm)
        ffn_swiglu(
            self.model.layers[l].w_gate,
            self.model.layers[l].w_up,
            self.model.layers[l].w_down,
            false, // wd_is_q6 = false (Q4_K)
            DIM,
            N_FF,
            &self.scratch.x_norm,
            &mut self.scratch.gate_buf,
            &mut self.scratch.up_buf,
            &mut self.scratch.hidden,
            &mut self.scratch.y,
        )
        .map_err(|_| "ffn swiglu error")?;

        // 5.5 PLASTICITÀ — passo su W_down (Passo 2, Opzione A).
        // L'input di Down (hidden, 1536) e il suo output (y) vivono ancora
        // nello scratch: passo sul solo W_down, stesso errore e_hidden.
        self.passo_plastico(l, Proiezione::Down);

        // 6. Connessione residuale FFN: x = x + y
        for i in 0..DIM {
            self.scratch.x[i] += self.scratch.y[i];
        }

        Ok(())
    }

    /// Un passo di plasticità (Passo 2) su UNA proiezione del layer `l`.
    ///
    /// Opzione A (17/10/2026): solo W_o e W_down sono ARMATI — le loro uscite
    /// vivono nel residual stream 512-dim, che è dove e_hidden (retroproiezione
    /// dell'errore somatico) è semanticamente corretto. Gate/Up restano
    /// dormienti: il loro errore verrà dall'auto-encoding SwiGLU (§3.3-b),
    /// un maestro alla volta.
    ///
    /// Non tocca il forward: aggiorna solo U,V (e la telemetria e_ema).
    /// Con λ(t)=0 il passo gira comunque: le decadenze γ (§3.1) e i clamp
    /// (§3.4, §6.2) sono metabolismo, non apprendimento — restano attivi.
    fn passo_plastico(&mut self, l: usize, p: Proiezione) {
        let k = p as usize;
        // L'input della proiezione: out_buf per Wo (l'attention fa
        // y = out_buf·Wo^T su tutta la sequenza), hidden per Down.
        // Gate/Up non vengono chiamati dall'Opzione A; se qualcuno li
        // chiamasse, x_norm (output della pre-FFN norm) sarebbe il loro
        // input — lo specifichiamo per quando il §3.3-b li armerà.
        let (x, y): (&[f32], &[f32]) = match p {
            Proiezione::Wo => (&self.scratch.out_buf[..], &self.scratch.y[..]),
            Proiezione::Down => (&self.scratch.hidden[..], &self.scratch.y[..]),
            Proiezione::Gate | Proiezione::Up => (&self.scratch.x_norm[..], &self.scratch.y[..]),
        };
        let Some(ma) = self.maestro.as_mut() else { return };
        let eta = ma.eta(l);
        let m_meta = ma.m_meta;
        let w_norm = ma.w_norm[l][k];
        let e_ema0 = ma.e_ema[l][k];
        let start = l * ADATTATORI_LAYER_FLOATS;
        let Some(slot) = ma.adattatori.get_mut(start..start + ADATTATORI_LAYER_FLOATS) else {
            return;
        };
        let e = &self.e_hidden[..];
        let mut zbuf = [0.0f32; plasticita::RANGO];
        let mut utebuf = [0.0f32; plasticita::RANGO];
        let Some(mut att) = AdattatoriLayer::nuovi(slot) else { return };
        // e_ema (telemetria EMA dell'errore relativo, §3.2): ‖e‖/‖y‖.
        let e_norm = norma2(e);
        let y_norm = norma2(y);
        let e_ema_new = {
            let mut ad = att.adattatore(p, w_norm, e_ema0);
            ad.passo(x, e, m_meta, eta, &plasticita::V21, &mut zbuf, &mut utebuf);
            ad.aggiorna_errore(e_norm, y_norm);
            ad.e_ema
        };
        ma.e_ema[l][k] = e_ema_new;
    }
}

/// ‖x‖₂ su slice (uso plastico: e_norm, y_norm della telemetria).
#[inline(always)]
fn norma2(x: &[f32]) -> f32 {
    let mut s = 0.0f32;
    for &v in x {
        s += v * v;
    }
    crate::math::sqrtf(s)
}

impl<'a> InferenceEngine for NucleoBrain<'a> {
    fn submit(&mut self, input: &[f32; 4]) -> bool {
        if self.state != BrainState::Idle {
            return false;
        }
        self.input = *input;

        // ── Errore somatico (§3.5, Passo 2) ────────────────────────────
        // e_som = input − output_del_ciclo_precedente: il nucleo prova a
        // PREDIRE il corpo (input = chemio interpretato di questo ciclo) e
        // l'errore di predizione È il segnale somatico. Al primo boot non
        // c'è un output precedente → e_som = 0 (nessun errore inventato:
        // la lezione del commit 8454449 — armare a vuoto è danno).
        // Calcolato SOLO se il maestro è armato: senza adattatori, è lavoro
        // che non servirebbe.
        if self.maestro.is_some() {
            match self.output {
                Some(prev) => {
                    for i in 0..CHEMIO_DIM {
                        self.e_som[i] = input[i] - prev[i];
                    }
                }
                None => self.e_som = [0.0; CHEMIO_DIM],
            }
            // e_hidden = W_head^T · e_som (Feedback Alignment, retroproiezione
            // già testata in bench: l'ho messa in plasticita.rs).
            plasticita::retroproietta_errore_soma(self.model.head, &self.e_som, &mut self.e_hidden);
        }

        // Proiezione sensoriale: 4 valori chemio -> vettore residuo x (512 dim)
        for j in 0..DIM {
            let mut sum = 0.0f32;
            for i in 0..CHEMIO_DIM {
                sum += self.input[i] * self.model.embed[i * DIM + j];
            }
            self.scratch.x[j] = sum;
        }

        self.current_layer = 0;
        self.state = BrainState::Thinking {
            ticks_left: N_LAYERS as u32,
        };
        true
    }

    fn tick(&mut self) -> bool {
        match self.state {
            BrainState::Idle => false,
            BrainState::Thinking { ticks_left: _ } => {
                self.total_think_ticks += 1;

                // Eseguiamo il layer corrente
                if self.current_layer < N_LAYERS {
                    let _ = self.forward_layer(self.current_layer);
                    self.current_layer += 1;
                }

                if self.current_layer >= N_LAYERS {
                    // Tutti i 12 layer completati: Applica final RMSNorm
                    let _ = rms_norm(
                        &self.scratch.x,
                        self.model.final_norm,
                        1e-5,
                        &mut self.scratch.x_norm,
                    );

                    // Proiezione finale sulla chemio (512 -> 4)
                    let mut out = [0.0f32; CHEMIO_DIM];
                    for i in 0..CHEMIO_DIM {
                        let mut sum = 0.0f32;
                        for j in 0..DIM {
                            sum += self.scratch.x_norm[j] * self.model.head[j * CHEMIO_DIM + i];
                        }
                        // Smorzamento / bounding sui canali chemio
                        out[i] = sum.clamp(-1.0, 1.0);
                    }

                    self.output = Some(out);
                    self.state = BrainState::Output { ticks_left: 2 };
                    true
                } else {
                    self.state = BrainState::Thinking {
                        ticks_left: (N_LAYERS - self.current_layer) as u32,
                    };
                    false
                }
            }
            BrainState::Output { ticks_left } => {
                if ticks_left <= 1 {
                    self.state = BrainState::Idle;
                } else {
                    self.state = BrainState::Output {
                        ticks_left: ticks_left - 1,
                    };
                }
                true
            }
        }
    }

    fn output(&self) -> Option<[f32; 4]> {
        self.output
    }

    fn state(&self) -> BrainState {
        self.state
    }

    fn name(&self) -> &'static str {
        "nucleo-41m"
    }

    /// Il corpo parla al cervello (Passo 2, 17/10): M(t) grezzo + salienza
    /// dell'errore somatico → λ(t+1). Un passo per ciclo cognitivo (chiamato
    /// da main a ogni submit Idle). Con il maestro dormiente è no-op.
    fn set_corpo(&mut self, m_meta: f32, s_som: f32) {
        if let Some(ma) = self.maestro.as_mut() {
            ma.set_corpo(m_meta, s_som);
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════
// PLASTICITÀ — Fase 1 (regola) + Passo 2 (il Maestro). 17/10/2026.
//
//   La MATEMATICA vive in `plasticita.rs` (testata su host in
//   bench/plasticita_host.rs, che include quel file REALE via #[path]).
//   Qui c'è il COLLEGAMENTO con Il Nucleo: il layout degli adattatori a
//   basso rango, il Maestro che calcola e_som e distribuisce e_hidden,
//   e il passo di apprendimento innestato nel forward (Opzione A).
//
//   SORGENTE DELL'ERRORE — scelta del Passo 2 (dibattuto con Nova, 17/10):
//     (c) errore somatico del PFM proiettato (§3.5): e_som = input −
//     prev_output in submit(), retroproiettato via W_head^T su e_hidden.
//     Le opzioni (a) next-token e (b) auto-encoding SwiGLU arrivano con i
//     loro MAESTRI (ascolto testuale, §3.3-b) — un maestro alla volta,
//     come la teoria §3.5 detta.
// ══════════════════════════════════════════════════════════════════════════

/// Le proiezioni plastiche di un layer (§2.3 della teoria).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Proiezione {
    /// W_gate dello SwiGLU, 1536×512
    Gate,
    /// W_up dello SwiGLU, 1536×512
    Up,
    /// W_down dello SwiGLU, 512×1536
    Down,
    /// W_o dell'attenzione, 512×512
    Wo,
}

/// (d_out, d_in) di ciascuna proiezione plastica.
pub const DIM_PROIEZIONE: [(usize, usize); 4] = [
    (N_FF, DIM), // Gate
    (N_FF, DIM), // Up
    (DIM, N_FF), // Down
    (DIM, DIM),  // Wo
];

/// Floats di adattatori per layer = 57.344 (U+V delle 4 proiezioni, r=8):
///   3 × (1536+512)·8 + (512+512)·8 = 49.152 + 8.192 = 57.344
pub const ADATTATORI_LAYER_FLOATS: usize =
    3 * (N_FF + DIM) * plasticita::RANGO + 2 * DIM * plasticita::RANGO;

/// Byte per layer (f32): 229.376 B ≈ 224 KiB (§2.3).
#[allow(dead_code)] // documentazione §2.3: esercitata dal banco, non dal kernel
pub const ADATTATORI_LAYER_BYTES: usize = ADATTATORI_LAYER_FLOATS * core::mem::size_of::<f32>();

/// Totale 12 layer: 2.752.512 B ≈ 2,625 MiB (§2.3).
pub const ADATTATORI_TOTALI_BYTES: usize = ADATTATORI_LAYER_BYTES * N_LAYERS;

/// Adattatori plastici di UN layer, costruiti sopra un buffer fornito
/// dal chiamante (zero alloc: il buffer è in .bss o su disco, come NucleoModel).
pub struct AdattatoriLayer<'a> {
    /// U delle 4 proiezioni, nell'ordine [Gate, Up, Down, Wo]
    u: [&'a mut [f32]; 4],
    /// V delle 4 proiezioni
    v: [&'a mut [f32]; 4],
}

impl<'a> AdattatoriLayer<'a> {
    /// Taglia il buffer nelle 8 matrici. Ritorna None se il buffer è corto.
    pub fn nuovi(buf: &'a mut [f32]) -> Option<Self> {
        if buf.len() < ADATTATORI_LAYER_FLOATS {
            return None;
        }
        let (u0, r) = buf.split_at_mut(N_FF * plasticita::RANGO);
        let (v0, r) = r.split_at_mut(DIM * plasticita::RANGO);
        let (u1, r) = r.split_at_mut(N_FF * plasticita::RANGO);
        let (v1, r) = r.split_at_mut(DIM * plasticita::RANGO);
        let (u2, r) = r.split_at_mut(DIM * plasticita::RANGO);
        let (v2, r) = r.split_at_mut(N_FF * plasticita::RANGO);
        let (u3, r) = r.split_at_mut(DIM * plasticita::RANGO);
        let (v3, _) = r.split_at_mut(DIM * plasticita::RANGO);
        Some(Self {
            u: [u0, u1, u2, u3],
            v: [v0, v1, v2, v3],
        })
    }

    /// Vista regola (U, V, d_out, d_in) su una proiezione.
    pub fn adattatore(&mut self, p: Proiezione, w_norm: f32, e_ema: f32) -> Adattatore<'_> {
        let k = p as usize;
        let (d_out, d_in) = DIM_PROIEZIONE[k];
        Adattatore {
            u: &mut *self.u[k],
            v: &mut *self.v[k],
            d_out,
            d_in,
            w_norm,
            e_ema,
        }
    }

    /// Leggi l'uscita plastica S·U(Vᵀx) sopra la base congelata, per una proiezione.
    pub fn predici(&mut self, p: Proiezione, x: &[f32], z: &mut [f32], y: &mut [f32]) {
        let k = p as usize;
        plasticita::proietta(&*self.v[k], DIM_PROIEZIONE[k].1, x, z);
        plasticita::predici_aggiungi(&*self.u[k], DIM_PROIEZIONE[k].0, z, y);
    }

    /// Applica UN passo della regola a una proiezione (§3.1 della teoria).
    /// `m_meta` è M(t) grezzo; il gate lo chiude/apre internamente.
    pub fn passo(
        &mut self,
        p: Proiezione,
        x: &[f32],
        e: &[f32],
        m_meta: f32,
        eta: f32,
        cfg: &Config,
        w_norm: f32,
        e_ema: f32,
        z: &mut [f32],
        ute: &mut [f32],
    ) -> EsitoPasso {
        let mut ad = self.adattatore(p, w_norm, e_ema);
        ad.passo(x, e, m_meta, eta, cfg, z, ute)
    }

    /// Potatura nel sonno di tutte le 4 proiezioni (§4.2). Ritorna i rangi potati.
    pub fn potatura(&mut self, cfg: &Config, seed: &mut u32) -> u32 {
        let mut tot = 0;
        for p in [Proiezione::Gate, Proiezione::Up, Proiezione::Down, Proiezione::Wo] {
            let k = p as usize;
            let mut ad = Adattatore {
                u: &mut *self.u[k],
                v: &mut *self.v[k],
                d_out: DIM_PROIEZIONE[k].0,
                d_in: DIM_PROIEZIONE[k].1,
                w_norm: 1.0,
                e_ema: 0.0,
            };
            tot += ad.potatura(cfg, seed);
        }
        tot
    }
}

// ══════════════════════════════════════════════════════════════════════════
// IL MAESTRO — Passo 2, 17/10/2026.
//
//   La legge (Fase 1, plasticita.rs) sa COME si apprende; il Maestro decide
//   COSA imparare: calcola e_som (l'errore somatico del PFM, §3.5),
//   lo retroproietta in e_hidden (dim 512) e lo distribuisce ai layer con
//   η(l) = η_base · λ(t) · (l+1)/12.
//
//   Opzione A (dibattuto con Nova, 17/10): ARMATE solo Wo e Down.
//   - Wo e Down proiettano dentro/il residual stream 512-dim: l'errore
//     e_hidden VIVE lì, è semanticamente corretto per loro.
//   - Gate/Up vivono nello spazio interno FFN 1536-dim: il loro errore
//     sarà l'auto-encoding del §3.3-b — UN MAESTRO ALLA VOLTA (teoria §3.5).
//   - Risparmio CPU sull'A8 e nessun broadcast rank-512 in 1536 (correlazioni
//     spurie: righe j, j+512, j+1024 identiche).
//
//   λ(t) parte a 0.0 (il corpo è muto): la prima percezione lo accende.
//   Le costanti LAMBDA_* sono prime ipotesi da calibrare in Fase 2.
// ══════════════════════════════════════════════════════════════════════════

/// η_base per i passi di plasticità nel forward (bench CTRL: stabile a 1.0).
pub const ETA_BASE_SOMA: f32 = 1.0;

/// Seed per l'inizializzazione V degli adattatori (no_std, no getrandom).
pub const SEED_ADATTATORI: u32 = 0x0AD_7_7A_57;

/// Il Maestro: calcola l'errore somatico e lo distribuisce ai layer.
///
/// `adattatori` è il buffer .bss condiviso (N_LAYERS × ADATTATORI_LAYER_FLOATS),
/// passato dal chiamante (main) via `arma_maestro` — zero alloc qui.
pub struct Maestro<'a> {
    /// Buffer .bss degli adattatori di tutti i layer (57.344 f32 per layer).
    pub adattatori: &'a mut [f32],
    /// ‖W‖_F per proiezione, ordine [Gate, Up, Down, Wo].
    /// Gate/Up restano 0.0 (dormienti, non armati).
    pub w_norm: [[f32; 4]; N_LAYERS],
    /// EMA dell'errore relativo ‖e‖/‖y‖ per proiezione (telemetria §3.2).
    pub e_ema: [[f32; 4]; N_LAYERS],
    /// Neurotrasmettitore corrente λ(t) ∈ [0,1] (passo_lambda).
    pub lam: f32,
    /// η_base per tutti i layer (ETA_BASE_SOMA).
    pub eta_base: f32,
    /// M(t) grezzo dell'ultimo set_corpo (memorizzato per debug).
    pub m_meta: f32,
}

impl<'a> Maestro<'a> {
    /// Costruisce il Maestro sopra il buffer .bss. Ritorna None se il buffer
    /// è corto (il chiamante ha sbagliato dimensione).
    pub fn nuovi(adattatori: &'a mut [f32]) -> Option<Self> {
        if adattatori.len() < N_LAYERS * ADATTATORI_LAYER_FLOATS {
            return None;
        }
        let mut m = Self {
            adattatori,
            w_norm: [[0.0; 4]; N_LAYERS],
            e_ema: [[0.0; 4]; N_LAYERS],
            lam: 0.0,
            eta_base: ETA_BASE_SOMA,
            m_meta: 0.0,
        };
        m.inizializza_adattatori(SEED_ADATTATORI);
        Some(m)
    }

    /// Inizializza gli adattatori: V = rand_pm1·√(3/d_in), U = 0
    /// (stesso schema del §3.1 bench: dimensione-compatibile, varianza
    /// di Glorot sul rango basso).
    fn inizializza_adattatori(&mut self, seed: u32) {
        let mut s = seed | 1;
        for l in 0..N_LAYERS {
            let start = l * ADATTATORI_LAYER_FLOATS;
            let end = start + ADATTATORI_LAYER_FLOATS;
            if let Some(mut att) = AdattatoriLayer::nuovi(&mut self.adattatori[start..end]) {
                for p in [Proiezione::Gate, Proiezione::Up, Proiezione::Down, Proiezione::Wo] {
                    let k = p as usize;
                    let d_in = DIM_PROIEZIONE[k].1;
                    plasticita::inizializza_v(&mut att.v[k], d_in, &mut s);
                }
            }
        }
    }

    /// η del layer l: η_base · λ(t) · (l+1)/n_layers (§3.5).
    /// I layer più profondi ricevono più errore — il residuo "guadagna"
    /// sensibilità salendo (teoria §3.5: gradient-flow vanishing).
    /// La formula vive in plasticita.rs (una sola fonte di verità).
    pub fn eta(&self, l: usize) -> f32 {
        plasticita::eta_layer(self.eta_base, self.lam, l, N_LAYERS)
    }

    /// Un passo del neurotrasmettitore: M(t) → λ(t+1).
    /// Chiamato dal set_corpo (una volta per ciclo, in submit Idle).
    pub fn set_corpo(&mut self, m_meta: f32, s_som: f32) {
        self.m_meta = m_meta;
        self.lam = plasticita::passo_lambda(self.lam, s_som);
    }

    /// ‖W‖_F di UNA matrice Q4_K (1 matrice = 1 slice di blocchi da 144 B).
    ///
    /// NOTA ONESTA (scoperta 17/10): `nucleo_41m.bin` è un binario SINTETICO
    /// (solo ~42 KB non-zero, pesi tutti a zero: pack_nucleo_bin.py::
    /// make_synthetic_bin). Con pesi nulli ‖W‖_F = 0 e ρ non è definito:
    /// il guard in Adattatore::passo (w_norm > 1e-12) mette rho = 0.0 e
    /// salta il cap — documentato, NON è un bug ma la condizione attesa
    /// finché non arriva una base vera. Il clamp E_k (§3.4) resta la
    /// barriera di sicurezza reale.
    ///
    /// I modelli vivono su disco (mmap), non in RAM: quando la base vera
    /// arriva, il chiamante calcola le norme con questo e le scrive nei
    /// campi pub `w_norm[l][k]`. Oggi restano 0.0 — coerente con lo stub.
    pub fn norma_w(matrice: &[u8]) -> f32 {
        crate::tensor::norma_q4_k(matrice).unwrap_or(0.0)
    }
}
