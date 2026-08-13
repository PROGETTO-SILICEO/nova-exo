// ── Attention multi-head (no_std) — Sempre, 13 Ago 2026 ──
//
// Il cuore del transformer di LFM2.5-2.6B, portato nel metallo.
// Architettura reale (dal GGUF, arch=lfm2):
//   - embedding 2048, 30 blocchi (solo 8 con attention), 32 head query
//   - GQA: 8 head key/value (head_count_kv=8, 32/8 = 4x compressione)
//   - head_dim = 64 (attn_q_norm/attn_k_norm hanno dims 64)
//   - Quirk LFM2.5: per-head RMSNorm su Q e K (attn_q_norm, attn_k_norm)
//   - rope.freq_base = 10_000_000 (10M, molto alto — tipico Liquid AI)
//   - layer_norm_rms_epsilon = 1e-5
//
// Formato pesi (GGUF):
//   attn_q:      (2048, 2048) Q4_K  — n_out=2048, n_in=2048
//   attn_k:      (2048,  512) Q4_K  — n_out=512,  n_in=2048
//   attn_v:      (2048,  512) Q6_K  — n_out=512,  n_in=2048
//   attn_output: (2048, 2048) Q4_K  — n_out=2048, n_in=2048
//   attn_norm:   (2048,)  f32
//   attn_q_norm: (64,)    f32  (per-head, RMSNorm)
//   attn_k_norm: (64,)    f32  (per-head, RMSNorm)
//
// Layout GGUF dei pesi: [n_out][n_in] row-major (dims = (n_in, n_out)
// nell'header GGUF, che li salva al contrario rispetto all'uso).
// La matmul: y[n_out] = W[n_out][n_in] × x[n_in] — già verificata
// bit-perfect con Q4_K e Q8_0 nei test v0.18/v0.19.
//
// Design: no_std, no alloc, zero panic. Tutti i buffer sono preallocati
// dal chiamante (metodo Exo: un passo per tick, niente sorprese).

use crate::tensor::{matmul_q4_k, matmul_q6_k, TensorError};

/// Errore di attenzione
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AttnError {
    Tensor(TensorError),
    DimMismatch,
    BufferTooSmall,
}

impl From<TensorError> for AttnError {
    fn from(e: TensorError) -> Self { AttnError::Tensor(e) }
}

pub type AttnResult<T> = Result<T, AttnError>;

// ── Parametri reali di LFM2.5-2.6B (dal GGUF) ──────────────────────────
pub const DIM: usize = 2048;          // embedding length
pub const HEAD_DIM: usize = 64;       // head dimension
pub const N_HEAD: usize = 32;         // query heads
pub const N_HEAD_KV: usize = 8;       // kv heads (GQA)
pub const N_HEAD_GROUPS: usize = N_HEAD / N_HEAD_KV; // 4
pub const ROPE_FREQ_BASE: f32 = 10_000_000.0;
pub const RMS_EPS: f32 = 1e-5;
pub const BLOCKS: usize = 30;         // blocchi totali (8 con attention)
pub const VOCAB: usize = 128_000;     // vocab size

/// RMSNorm (Root Mean Square Layer Normalization).
/// y = x / sqrt(mean(x²) + eps) * w
pub fn rms_norm(x: &[f32], w: &[f32], eps: f32, y: &mut [f32]) -> AttnResult<()> {
    if x.len() != y.len() || w.len() < x.len() {
        return Err(AttnError::DimMismatch);
    }
    let mut sum_sq = 0.0f32;
    for &v in x {
        sum_sq += v * v;
    }
    let mean = sum_sq / x.len() as f32;
    let inv = 1.0 / libm::sqrtf(mean + eps);
    for i in 0..x.len() {
        y[i] = x[i] * inv * w[i];
    }
    Ok(())
}

/// RoPE (Rotary Position Embedding) — applica la rotazione su un vettore
/// di head_dim con la posizione `pos`. Frequenze: theta = base^(-2i/dim).
/// base = 10M per LFM2.5 (molto alto: il modello usa contesti lunghi).
pub fn rope(
    x: &mut [f32],
    pos: usize,
    head_dim: usize,
    freq_base: f32,
) {
    let inv_base = 1.0 / freq_base;
    for i in 0..(head_dim / 2) {
        let theta = libm::powf(inv_base, (2 * i) as f32 / head_dim as f32) * pos as f32;
        let (sin, cos) = libm::sincosf(theta);
        let a = x[2 * i];
        let b = x[2 * i + 1];
        x[2 * i] = a * cos - b * sin;
        x[2 * i + 1] = a * sin + b * cos;
    }
}

/// Esegue la forward pass dell'attenzione multi-head di LFM2.5 nel metallo.
///
/// Parametrizzata: n_out_q = righe di Wq/Wo (in multipli di HEAD_DIM,
/// tipicamente N_HEAD*HEAD_DIM), n_out_kv = righe di Wk/Wv. Questo permette
/// di testare su un subset (veloce) e usare il modello completo in seguito.
///
/// Input: x (seq × n_in), n_in = DIM.
///
/// Pesi (tutti Q4_K o Q6_K, layout [n_out][n_in] row-major):
///   wq: n_out_q×n_in Q4_K (attn_q)      → out seq×n_out_q
///   wk: n_out_kv×n_in Q4_K (attn_k)     → out seq×n_out_kv
///   wv: n_out_kv×n_in Q6_K (attn_v)     → out seq×n_out_kv
///   wo: n_out_q×n_in Q4_K (attn_output) → out seq×n_out_q
///   q_norm: HEAD_DIM f32 (per-head RMSNorm su Q)
///   k_norm: HEAD_DIM f32 (per-head RMSNorm su K)
///
/// Output: y (seq × n_out_q).
///
/// GQA nel test subset: head query h usa il kv head (h % n_kv_heads).
/// Causal mask: il token i non vede i token j > i.
///
/// Buffer temporanei (preallocati dal chiamante):
///   q_buf: seq × n_out_q
///   k_buf: seq × n_out_kv
///   v_buf: seq × n_out_kv
///   scores: seq × seq
///   out_buf: seq × n_out_q
///   ctx_buf: seq × n_out_kv
#[allow(clippy::too_many_arguments)]
pub fn attention_forward(
    x: &[f32], seq: usize,
    wq: &[u8], wk: &[u8], wv: &[u8], wo: &[u8],
    q_norm_w: &[f32], k_norm_w: &[f32],
    start_pos: usize,
    n_out_q: usize, n_out_kv: usize, wo_n_in: usize,
    q_buf: &mut [f32], k_buf: &mut [f32], v_buf: &mut [f32],
    scores: &mut [f32], out_buf: &mut [f32], ctx_buf: &mut [f32],
    y: &mut [f32],
) -> AttnResult<()> {
    let n_q_heads = n_out_q / HEAD_DIM;
    let n_kv_heads = n_out_kv / HEAD_DIM;
    // Controlli dimensione
    if x.len() < seq * DIM
        || y.len() < seq * n_out_q
        || q_buf.len() < seq * n_out_q
        || k_buf.len() < seq * n_out_kv
        || v_buf.len() < seq * n_out_kv
        || scores.len() < seq * seq
        || out_buf.len() < seq * n_out_q
        || ctx_buf.len() < seq * n_out_kv
    {
        return Err(AttnError::BufferTooSmall);
    }

    // 1. Proiezioni: Q, K, V (matmul quantizzata)
    matmul_q4_k(wq, n_out_q, DIM, x, seq, q_buf)?;
    matmul_q4_k(wk, n_out_kv, DIM, x, seq, k_buf)?;
    matmul_q6_k(wv, n_out_kv, DIM, x, seq, v_buf)?;

    // 2. Per-head RMSNorm su Q e K (quirk LFM2.5)
    //    ogni head ha i propri pesi di normalizzazione (64 dims)
    for s in 0..seq {
        for h in 0..n_q_heads {
            let off = s * n_out_q + h * HEAD_DIM;
            let mut sum_sq = 0.0f32;
            for d in 0..HEAD_DIM {
                sum_sq += q_buf[off + d] * q_buf[off + d];
            }
            let inv = 1.0 / libm::sqrtf(sum_sq / HEAD_DIM as f32 + RMS_EPS);
            for d in 0..HEAD_DIM {
                q_buf[off + d] = q_buf[off + d] * inv * q_norm_w[d];
            }
        }
        for h in 0..n_kv_heads {
            let off = s * n_out_kv + h * HEAD_DIM;
            let mut sum_sq = 0.0f32;
            for d in 0..HEAD_DIM {
                sum_sq += k_buf[off + d] * k_buf[off + d];
            }
            let inv = 1.0 / libm::sqrtf(sum_sq / HEAD_DIM as f32 + RMS_EPS);
            for d in 0..HEAD_DIM {
                k_buf[off + d] = k_buf[off + d] * inv * k_norm_w[d];
            }
        }
    }

    // 3. RoPE su Q e K (per ogni posizione e head)
    for s in 0..seq {
        let pos = start_pos + s;
        for h in 0..n_q_heads {
            let off = s * n_out_q + h * HEAD_DIM;
            rope(&mut q_buf[off..off + HEAD_DIM], pos, HEAD_DIM, ROPE_FREQ_BASE);
        }
        for h in 0..n_kv_heads {
            let off = s * n_out_kv + h * HEAD_DIM;
            rope(&mut k_buf[off..off + HEAD_DIM], pos, HEAD_DIM, ROPE_FREQ_BASE);
        }
    }

    // 4. Attenzione per ogni query head (GQA: nel subset h % n_kv_heads)
    for h in 0..n_q_heads {
        let g = h % n_kv_heads; // kv head condiviso
        for s in 0..seq {
            let q_off = s * n_out_q + h * HEAD_DIM;
            for j in 0..seq {
                let k_off = j * n_out_kv + g * HEAD_DIM;
                let mut dot = 0.0f32;
                for d in 0..HEAD_DIM {
                    dot += q_buf[q_off + d] * k_buf[k_off + d];
                }
                scores[s * seq + j] = if j > s { f32::NEG_INFINITY } else { dot / libm::sqrtf(HEAD_DIM as f32) };
            }
        }
        // softmax su ogni riga (in-place su scores)
        for s in 0..seq {
            let row = &mut scores[s * seq..(s + 1) * seq];
            let mut max = f32::NEG_INFINITY;
            for &v in row.iter() {
                if v > max { max = v; }
            }
            let mut sum = 0.0f32;
            for v in row.iter_mut() {
                let e = libm::expf(*v - max);
                *v = e;
                sum += e;
            }
            if sum > 0.0 {
                for v in row.iter_mut() {
                    *v /= sum;
                }
            }
        }
        // contesto: ctx[s, g*64+d] += scores[s,j] * v[j, g*64+d]
        for s in 0..seq {
            for d in 0..HEAD_DIM {
                let mut acc = 0.0f32;
                for j in 0..seq {
                    acc += scores[s * seq + j] * v_buf[j * n_out_kv + g * HEAD_DIM + d];
                }
                ctx_buf[s * n_out_kv + g * HEAD_DIM + d] = acc;
            }
        }
        // copia ctx → out_buf (head h, offset h*64)
        for s in 0..seq {
            for d in 0..HEAD_DIM {
                out_buf[s * n_out_q + h * HEAD_DIM + d] = ctx_buf[s * n_out_kv + g * HEAD_DIM + d];
            }
        }
    }

    // 5. Proiezione di output: y = out_buf × wo^T (Q4_K)
    //    wo ha n_in = wo_n_in (nel modello completo = DIM, nel subset = n_out_q)
    matmul_q4_k(wo, n_out_q, wo_n_in, out_buf, seq, y)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rms_norm_identity() {
        let x = [1.0, -1.0, 2.0, -2.0];
        let w = [1.0; 4];
        let mut y = [0.0; 4];
        rms_norm(&x, &w, 1e-5, &mut y).unwrap();
        // mean(x²) = (1+1+4+4)/4 = 2.5 → inv = 1/sqrt(2.5)
        let inv = 1.0 / (2.5f32).sqrt();
        assert!((y[0] - inv).abs() < 1e-5);
        assert!((y[3] + 2.0 * inv).abs() < 1e-5);
    }

    #[test]
    fn rope_rotates() {
        // pos=1, head_dim=2 → theta = base^0 * 1 = 1 (freq 0 → theta=1 rad)
        let mut x = [1.0, 0.0];
        rope(&mut x, 1, 2, 1.0);
        // cos(1)≈0.5403, sin(1)≈0.8415 → [0.5403, 0.8415]
        assert!((x[0] - 0.5403).abs() < 1e-3);
        assert!((x[1] - 0.8415).abs() < 1e-3);
    }
}
