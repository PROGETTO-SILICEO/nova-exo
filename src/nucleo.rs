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
        }
    }

    pub fn total_think_ticks(&self) -> u64 {
        self.total_think_ticks
    }

    /// Esegue il forward di un singolo layer (budget ~2 ms su AMD A8).
    fn forward_layer(&mut self, l: usize) -> Result<(), &'static str> {
        let layer = &self.model.layers[l];

        // 1. RMSNorm pre-attenzione
        rms_norm(&self.scratch.x, layer.attn_norm, 1e-5, &mut self.scratch.x_norm)
            .map_err(|_| "rms_norm attn error")?;

        // Dummy q_norm_w e k_norm_w (ones se non usate dal layer)
        static DUMMY_NORM: [f32; HEAD_DIM] = [1.0; HEAD_DIM];

        // 2. GQA Attention Causal
        attention_forward_dim(
            &self.scratch.x_norm,
            1, // seq = 1 per token inference
            DIM,
            layer.wq,
            layer.wk,
            layer.wv,
            layer.wo,
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

        // 4. RMSNorm pre-FFN
        rms_norm(&self.scratch.x, layer.ffn_norm, 1e-5, &mut self.scratch.x_norm)
            .map_err(|_| "rms_norm ffn error")?;

        // 5. SwiGLU FFN: y = ffn_swiglu(x_norm)
        ffn_swiglu(
            layer.w_gate,
            layer.w_up,
            layer.w_down,
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

        // 6. Connessione residuale FFN: x = x + y
        for i in 0..DIM {
            self.scratch.x[i] += self.scratch.y[i];
        }

        Ok(())
    }
}

impl<'a> InferenceEngine for NucleoBrain<'a> {
    fn submit(&mut self, input: &[f32; 4]) -> bool {
        if self.state != BrainState::Idle {
            return false;
        }
        self.input = *input;

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
}

// ══════════════════════════════════════════════════════════════════════════
// PLASTICITÀ — Fase 1 (regola a tre fattori). 17/10/2026.
//
//   La MATEMATICA vive in `plasticita.rs` (testata su host in
//   bench/plasticita_host.rs, che include quel file REALE via #[path]).
//   Qui c'è il COLLEGAMENTO con Il Nucleo: il layout degli adattatori a
//   basso rango delle 4 proiezioni plastiche, e l'API per un passo di
//   apprendimento.
//
//   NOTA — cosa manca e perché NON è qui (onestà, non pigrizia):
//   la regola serve `e = y_target − ŷ`, l'errore locale del layer. La sorgente
//   di `e` (il "maestro") è ancora da scegliere secondo la teoria §3.3/§3.5:
//     (a) errore di next-token (richiede un flusso testuale di ascolto);
//     (b) auto-encoding in SwiGLU (§3.3);
//     (c) errore somatico del PFM proiettato (§3.5).
//   Finché il maestro non esiste, ALLOCARE e ARMARE gli adattatori servirebbe
//   solo a modificarli con un errore fasullo — e una base appresa che viene
//   corrotta da un rumore di servizio non è "apprendimento", è danno.
//   Questo commit dà la legge + il suo alloggio; il maestro è la Fase 2.
// ══════════════════════════════════════════════════════════════════════════

/// Le proiezioni plastiche di un layer (§2.3 della teoria).
#[allow(dead_code)] // Fase 1: alloggio pronto, maestro (la sorgente di `e`) non ancora scelto
#[derive(Clone, Copy, PartialEq, Eq)]
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
#[allow(dead_code)] // Fase 1: alloggio pronto, maestro non ancora scelto
pub const DIM_PROIEZIONE: [(usize, usize); 4] = [
    (N_FF, DIM), // Gate
    (N_FF, DIM), // Up
    (DIM, N_FF), // Down
    (DIM, DIM),  // Wo
];

/// Floats di adattatori per layer = 57.344 (U+V delle 4 proiezioni, r=8):
///   3 × (1536+512)·8 + (512+512)·8 = 49.152 + 8.192 = 57.344
#[allow(dead_code)] // Fase 1: alloggio pronto, maestro non ancora scelto
pub const ADATTATORI_LAYER_FLOATS: usize =
    3 * (N_FF + DIM) * plasticita::RANGO + 2 * DIM * plasticita::RANGO;

/// Byte per layer (f32): 229.376 B ≈ 224 KiB (§2.3).
#[allow(dead_code)] // Fase 1: alloggio pronto, maestro non ancora scelto
pub const ADATTATORI_LAYER_BYTES: usize = ADATTATORI_LAYER_FLOATS * core::mem::size_of::<f32>();

/// Totale 12 layer: 2.752.512 B ≈ 2,625 MiB (§2.3).
#[allow(dead_code)] // Fase 1: alloggio pronto, maestro non ancora scelto
pub const ADATTATORI_TOTALI_BYTES: usize = ADATTATORI_LAYER_BYTES * N_LAYERS;

/// Adattatori plastici di UN layer, costruiti sopra un buffer fornito
/// dal chiamante (zero alloc: il buffer è in .bss o su disco, come NucleoModel).
#[allow(dead_code)] // Fase 1: alloggio pronto, maestro (la sorgente di `e`) non ancora scelto
pub struct AdattatoriLayer<'a> {
    /// U delle 4 proiezioni, nell'ordine [Gate, Up, Down, Wo]
    u: [&'a mut [f32]; 4],
    /// V delle 4 proiezioni
    v: [&'a mut [f32]; 4],
}

#[allow(dead_code)] // Fase 1: alloggio pronto, maestro (la sorgente di `e`) non ancora scelto
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
