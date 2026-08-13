// ── Tensor ops (no_std) — Sempre, 13 Ago 2026 ──
#![no_std]
//
// Le operazioni fondamentali di inferenza nel metallo:
//   - matmul f32 (senza alloc: input/output forniti dal chiamante)
//   - layer lineare con bias
//   - forward di un piccolo MLP
//   - dequantizzazione Q8_0 (GGUF) + matmul quantizzata
//
// Design: niente alloc. Tutte le funzioni scrivono su buffer preallocati
// dal chiamante. Gli errori (dimension mismatch) ritornano Result, non
// crashano (metodo Exo: le eccezioni non sono errori, sono sensi).

use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TensorError {
    DimMismatch,
    BufferTooSmall,
}

impl fmt::Display for TensorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TensorError::DimMismatch => write!(f, "dimension mismatch"),
            TensorError::BufferTooSmall => write!(f, "output buffer too small"),
        }
    }
}

pub type TensorResult<T> = Result<T, TensorError>;

/// Dimensione blocco Q8_0 (dal formato GGUF)
pub const Q8_0_BLOCK: usize = 32;

/// Dimensione blocco Q4_K (dal formato GGUF): 256 elementi
pub const Q4_K_BLOCK: usize = 256;
/// Dimensione in byte di un blocco Q4_K
pub const Q4_K_BLOCK_BYTES: usize = 144;

/// Dimensione blocco Q6_K (dal formato GGUF): 256 elementi
pub const Q6_K_BLOCK: usize = 256;
/// Dimensione in byte di un blocco Q6_K: d(fp16) + ql[128] + qh[64] + scales[16]
pub const Q6_K_BLOCK_BYTES: usize = 210;

/// Dequantizza un blocco Q4_K (256 elementi) secondo ggml-quants.c.
/// Layout blocco: d(fp16) dmin(fp16) scales[12] qs[128].
/// Formula (da dequantize_row_q4_K):
///   get_scale_min_k4(j, scales):
///     j<4:  d_s = scales[j]&63,  m_s = scales[j+4]&63
///     else: d_s = (scales[j+4]&0xF)|((scales[j-4]>>6)<<4)
///           m_s = (scales[j+4]>>4)|((scales[j]>>6)<<4)
///   per sub-block di 64: 32 valori low-nibble (d1,m1) + 32 high-nibble (d2,m2)
///   x = d * d_s * q - dmin * m_s   (q unsigned 0-15, NON centrato)
pub fn dequant_q4_k(block: &[u8], out: &mut [f32]) -> TensorResult<()> {
    if block.len() < Q4_K_BLOCK_BYTES || out.len() < Q4_K_BLOCK {
        return Err(TensorError::BufferTooSmall);
    }
    let d = fp16_to_f32(block[0] as u16 | ((block[1] as u16) << 8));
    let mn = fp16_to_f32(block[2] as u16 | ((block[3] as u16) << 8));
    let scales = &block[4..16];
    let qs = &block[16..144];

    let mut is = 0usize;
    let mut idx = 0usize;
    let mut qoff = 0usize;
    for _j in 0..(Q4_K_BLOCK / 64) {
        let (ds1, ms1) = get_scale_min_k4(is, scales);
        let d1 = d * ds1 as f32;
        let m1 = mn * ms1 as f32;
        let (ds2, ms2) = get_scale_min_k4(is + 1, scales);
        let d2 = d * ds2 as f32;
        let m2 = mn * ms2 as f32;
        for l in 0..32 {
            out[idx + l] = d1 * (qs[qoff + l] & 0x0F) as f32 - m1;
        }
        for l in 0..32 {
            out[idx + 32 + l] = d2 * (qs[qoff + l] >> 4) as f32 - m2;
        }
        qoff += 32;
        idx += 64;
        is += 2;
    }
    Ok(())
}

/// get_scale_min_k4 dal sorgente ggml (pattern di scala/min per sub-block)
pub fn get_scale_min_k4(j: usize, scales: &[u8]) -> (u8, u8) {
    if j < 4 {
        (scales[j] & 63, scales[j + 4] & 63)
    } else {
        let d = (scales[j + 4] & 0x0F) | ((scales[j - 4] >> 6) << 4);
        let m = (scales[j + 4] >> 4) | ((scales[j] >> 6) << 4);
        (d, m)
    }
}

/// Dequantizza un blocco Q8_0: fp16 scale + 32 int8 → 32 f32.
/// Formato GGUF Q8_0 (tipo 8):
///   [d: fp16][qs: 32 × int8]
///   x[i] = qs[i] * d
pub fn dequant_q8_0(block: &[u8], out: &mut [f32]) -> TensorResult<()> {
    if block.len() < 2 + Q8_0_BLOCK || out.len() < Q8_0_BLOCK {
        return Err(TensorError::BufferTooSmall);
    }
    // fp16 → f32 manuale (no dipendenze)
    let d = fp16_to_f32(
        block[0] as u16 | ((block[1] as u16) << 8),
    );
    for i in 0..Q8_0_BLOCK {
        let q = block[2 + i] as i8 as f32;
        out[i] = q * d;
    }
    Ok(())
}

/// Dequantizza un blocco Q6_K (256 elementi, 210 byte) secondo ggml-quants.c.
/// Layout blocco: d(fp16) ql[128] qh[64] scales[16] (int8).
/// Formula (da dequantize_row_q6_K):
///   per sub-blocco di 128 (2 iterazioni):
///     per l in 0..32:
///       is = l/16
///       q1 = (ql[l]&0xF | (qh[l]&0x3)<<4) - 32
///       q2 = (ql[l+32]&0xF | (qh[l]>>2&0x3)<<4) - 32
///       q3 = (ql[l]>>4 | (qh[l]>>4&0x3)<<4) - 32
///       q4 = (ql[l+32]>>4 | (qh[l]>>6&0x3)<<4) - 32
///       x = d * sc * q   (sc = scale int8, q centrato su 32)
pub fn dequant_q6_k(block: &[u8], out: &mut [f32]) -> TensorResult<()> {
    if block.len() < Q6_K_BLOCK_BYTES || out.len() < Q6_K_BLOCK {
        return Err(TensorError::BufferTooSmall);
    }
    let d = fp16_to_f32(block[0] as u16 | ((block[1] as u16) << 8));
    // Modelli reali (LFM2.5) contengono blocchi con d=NaN/inf (righe morte
    // della quantizzazione). Trattarli come zero: contributo nullo e
    // nessuna propagazione di NaN (metodo Exo: accetta l'imperfezione).
    if !d.is_finite() {
        out[..Q6_K_BLOCK].fill(0.0);
        return Ok(());
    }
    let ql = &block[2..130];
    let qh = &block[130..194];
    let sc = &block[194..210];

    let mut yoff = 0usize;
    for n in 0..(Q6_K_BLOCK / 128) {
        let qloff = n * 64;
        let qhoff = n * 32;
        let scoff = n * 8;
        for l in 0..32 {
            let is = l / 16;
            let q1 = ((ql[qloff + l] & 0x0F) | (((qh[qhoff + l] >> 0) & 3) << 4)) as i8 - 32;
            let q2 = ((ql[qloff + l + 32] & 0x0F) | (((qh[qhoff + l] >> 2) & 3) << 4)) as i8 - 32;
            let q3 = ((ql[qloff + l] >> 4) | (((qh[qhoff + l] >> 4) & 3) << 4)) as i8 - 32;
            let q4 = ((ql[qloff + l + 32] >> 4) | (((qh[qhoff + l] >> 6) & 3) << 4)) as i8 - 32;
            out[yoff + l] = d * sc[scoff + is + 0] as i8 as f32 * q1 as f32;
            out[yoff + l + 32] = d * sc[scoff + is + 2] as i8 as f32 * q2 as f32;
            out[yoff + l + 64] = d * sc[scoff + is + 4] as i8 as f32 * q3 as f32;
            out[yoff + l + 96] = d * sc[scoff + is + 6] as i8 as f32 * q4 as f32;
        }
        yoff += 128;
    }
    Ok(())
}

/// Matmul con peso Q6_K: W quantizzato in blocchi da 256 (210 byte/blocco).
/// W layout: [n_out][n_in] row-major, n_in multiplo di 256.
/// A è f32 (m×k), C = A×W^T f32 (m×n). Dequant inline nella matmul.
pub fn matmul_q6_k(
    w: &[u8], n_out: usize, n_in: usize,
    a: &[f32], m: usize,
    c: &mut [f32],
) -> TensorResult<()> {
    if n_in % Q6_K_BLOCK != 0 {
        return Err(TensorError::DimMismatch);
    }
    let blocks_per_row = n_in / Q6_K_BLOCK;
    if w.len() < n_out * blocks_per_row * Q6_K_BLOCK_BYTES
        || a.len() < m * n_in
        || c.len() < m * n_out
    {
        return Err(TensorError::BufferTooSmall);
    }

    for i in 0..m {
        for o in 0..n_out {
            let mut sum = 0.0f32;
            for b in 0..blocks_per_row {
                let blk = &w[(o * blocks_per_row + b) * Q6_K_BLOCK_BYTES
                    ..(o * blocks_per_row + b + 1) * Q6_K_BLOCK_BYTES];
                let d = fp16_to_f32(blk[0] as u16 | ((blk[1] as u16) << 8));
                // riga morta (d NaN/inf nel file reale) → contributo zero
                if !d.is_finite() {
                    continue;
                }
                let ql = &blk[2..130];
                let qh = &blk[130..194];
                let sc = &blk[194..210];
                for n in 0..(Q6_K_BLOCK / 128) {
                    let qloff = n * 64;
                    let qhoff = n * 32;
                    let scoff = n * 8;
                    for l in 0..32 {
                        let is = l / 16;
                        let q1 = ((ql[qloff + l] & 0x0F) | (((qh[qhoff + l] >> 0) & 3) << 4)) as i8 - 32;
                        let q2 = ((ql[qloff + l + 32] & 0x0F) | (((qh[qhoff + l] >> 2) & 3) << 4)) as i8 - 32;
                        let q3 = ((ql[qloff + l] >> 4) | (((qh[qhoff + l] >> 4) & 3) << 4)) as i8 - 32;
                        let q4 = ((ql[qloff + l + 32] >> 4) | (((qh[qhoff + l] >> 6) & 3) << 4)) as i8 - 32;
                        let base = b * Q6_K_BLOCK + n * 128;
                        sum += d * sc[scoff + is + 0] as i8 as f32 * q1 as f32 * a[i * n_in + base + l];
                        sum += d * sc[scoff + is + 2] as i8 as f32 * q2 as f32 * a[i * n_in + base + l + 32];
                        sum += d * sc[scoff + is + 4] as i8 as f32 * q3 as f32 * a[i * n_in + base + l + 64];
                        sum += d * sc[scoff + is + 6] as i8 as f32 * q4 as f32 * a[i * n_in + base + l + 96];
                    }
                }
            }
            c[i * n_out + o] = sum;
        }
    }
    Ok(())
}

/// Converte fp16 (bit pattern) in f32 (implementazione manuale no_std)
pub fn fp16_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) & 1) as u32;
    let exp = ((h >> 10) & 0x1F) as u32;
    let frac = (h & 0x3FF) as u32;

    let (e, f): (u32, u32) = if exp == 0 {
        if frac == 0 { (0, 0) } else {
            // subnormale fp16: valore = frac * 2^-24.
            // Rappresentazione esatta in f32: frac * (2^-24) con f32 aritmetico.
            let val = (frac as f32) * 5.9604645e-8; // 2^-24
            return if sign == 1 { -val } else { val };
        }
    } else if exp == 0x1F {
        (0xFF, if frac == 0 { 0 } else { 0x200000 })
    } else {
        (exp + 127 - 15, frac)
    };

    let bits = (sign << 31) | (e << 23) | (f << 13);
    f32::from_bits(bits)
}

/// Matmul con peso Q8_0: W è quantizzato in blocchi (n_blocks × 34 byte).
/// W layout: [n_out][n_in] row-major, quantizzato in blocchi di 32 lungo n_in.
/// A è f32 (m×k), C = A×W^T è f32 (m×n).
/// dequant_buf: scratch di k f32 per dequantizzare una riga.
pub fn matmul_q8_0(
    w: &[u8], n_out: usize, n_in: usize,
    a: &[f32], m: usize,
    c: &mut [f32],
    dequant_buf: &mut [f32],
) -> TensorResult<()> {
    if n_in % Q8_0_BLOCK != 0 {
        return Err(TensorError::DimMismatch);
    }
    let blocks_per_row = n_in / Q8_0_BLOCK;
    if w.len() < n_out * blocks_per_row * (2 + Q8_0_BLOCK)
        || a.len() < m * n_in
        || c.len() < m * n_out
        || dequant_buf.len() < n_in
    {
        return Err(TensorError::BufferTooSmall);
    }

    for i in 0..m {
        for o in 0..n_out {
            let mut sum = 0.0f32;
            // dequantizza riga o di W
            for b in 0..blocks_per_row {
                let block_off = (o * blocks_per_row + b) * (2 + Q8_0_BLOCK);
                let d = fp16_to_f32(
                    w[block_off] as u16 | ((w[block_off + 1] as u16) << 8),
                );
                for j in 0..Q8_0_BLOCK {
                    let q = w[block_off + 2 + j] as i8 as f32;
                    let wv = q * d;
                    let idx = b * Q8_0_BLOCK + j;
                    sum += wv * a[i * n_in + idx];
                }
            }
            c[i * n_out + o] = sum;
        }
    }
    Ok(())
}

/// Matmul con peso Q4_K: W quantizzato in blocchi da 256 (144 byte/blocco).
/// W layout: [n_out][n_in] row-major, n_in multiplo di 256.
/// A è f32 (m×k), C = A×W^T f32 (m×n). Nessun buffer intermedio necessario:
/// la dequant è inline nella matmul (i q si usano direttamente).
pub fn matmul_q4_k(
    w: &[u8], n_out: usize, n_in: usize,
    a: &[f32], m: usize,
    c: &mut [f32],
) -> TensorResult<()> {
    if n_in % Q4_K_BLOCK != 0 {
        return Err(TensorError::DimMismatch);
    }
    let blocks_per_row = n_in / Q4_K_BLOCK;
    if w.len() < n_out * blocks_per_row * Q4_K_BLOCK_BYTES
        || a.len() < m * n_in
        || c.len() < m * n_out
    {
        return Err(TensorError::BufferTooSmall);
    }

    for i in 0..m {
        for o in 0..n_out {
            let mut sum = 0.0f32;
            for b in 0..blocks_per_row {
                let block_off = (o * blocks_per_row + b) * Q4_K_BLOCK_BYTES;
                let blk = &w[block_off..block_off + Q4_K_BLOCK_BYTES];
                let d = fp16_to_f32(blk[0] as u16 | ((blk[1] as u16) << 8));
                let mn = fp16_to_f32(blk[2] as u16 | ((blk[3] as u16) << 8));
                let scales = &blk[4..16];
                let qs = &blk[16..144];
                let mut is = 0usize;
                let mut qoff = 0usize;
                let mut oidx = 0usize; // indice output nel blocco (avanza 64)
                for _j in 0..(Q4_K_BLOCK / 64) {
                    let (ds1, ms1) = get_scale_min_k4(is, scales);
                    let d1 = d * ds1 as f32;
                    let m1 = mn * ms1 as f32;
                    let (ds2, ms2) = get_scale_min_k4(is + 1, scales);
                    let d2 = d * ds2 as f32;
                    let m2 = mn * ms2 as f32;
                    let base = b * Q4_K_BLOCK + oidx;
                    for l in 0..32 {
                        let wv = d1 * (qs[qoff + l] & 0x0F) as f32 - m1;
                        sum += wv * a[i * n_in + base + l];
                    }
                    for l in 0..32 {
                        let wv = d2 * (qs[qoff + l] >> 4) as f32 - m2;
                        sum += wv * a[i * n_in + base + 32 + l];
                    }
                    qoff += 32;
                    oidx += 64;
                    is += 2;
                }
            }
            c[i * n_out + o] = sum;
        }
    }
    Ok(())
}

/// C = A × B, con A (m×k) row-major, B (k×n) row-major, C (m×n).
/// Tutti i buffer sono preallocati dal chiamante (no alloc).
pub fn matmul(
    a: &[f32], m: usize, k: usize,
    b: &[f32], n: usize,
    c: &mut [f32],
) -> TensorResult<()> {    if a.len() < m * k || b.len() < k * n || c.len() < m * n {
        return Err(TensorError::BufferTooSmall);
    }
    for i in 0..m {
        for j in 0..n {
            let mut sum = 0.0f32;
            for t in 0..k {
                sum += a[i * k + t] * b[t * n + j];
            }
            c[i * n + j] = sum;
        }
    }
    Ok(())
}

/// y = W·x + b, con W (out×in) row-major, x (in), b (out), y (out).
pub fn linear(
    w: &[f32], in_dim: usize, out_dim: usize,
    x: &[f32],
    b: &[f32],
    y: &mut [f32],
) -> TensorResult<()> {
    if w.len() < out_dim * in_dim || x.len() < in_dim || b.len() < out_dim || y.len() < out_dim {
        return Err(TensorError::BufferTooSmall);
    }
    for o in 0..out_dim {
        let mut sum = b[o];
        for i in 0..in_dim {
            sum += w[o * in_dim + i] * x[i];
        }
        y[o] = sum;
    }
    Ok(())
}

/// Applica ReLU in-place
pub fn relu_inplace(x: &mut [f32]) {
    for v in x.iter_mut() {
        if *v < 0.0 { *v = 0.0; }
    }
}

/// SiLU (swish): x * sigmoid(x) = x / (1 + e^-x)
pub fn silu(x: f32) -> f32 {
    x / (1.0 + libm::expf(-x))
}

/// ShortConv (LFM2.5, blocco ricorrente): gate element-wise + conv1d causale.
/// Architettura reale (da llama.cpp lfm2.cpp):
///   bcx = x @ in_proj^T          # 2048 → 6144 (3 chunk da 2048: b, c, xc)
///   bx  = b * xc                 # gate
///   conv_out[t][c] = sum_k kernel[k][c] * bx[t-1+k][c]   # causale, kernel 3
///   y   = c * conv_out           # gate finale
///   out = y @ out_proj^T         # 2048 → 2048
/// Il caso base (senza stato ricorrente, inizio sequenza) tratta i passati
/// come zero: conv_out[t][c] = kernel[1][c]*bx[t][c] + kernel[2][c]*bx[t+1][c]
/// (kernel[0] = passato → 0 se t=0).
pub fn shortconv_forward(
    in_proj: &[u8], conv: &[f32], out_proj: &[u8],
    n_in: usize, n_embd: usize,
    x: &[f32], seq: usize,
    bcx_buf: &mut [f32], bx_buf: &mut [f32], y: &mut [f32],
) -> TensorResult<()> {
    if x.len() < seq * n_in || bcx_buf.len() < seq * 3 * n_embd
        || bx_buf.len() < seq * n_embd || y.len() < seq * n_embd
        || conv.len() < 3 * n_embd {
        return Err(TensorError::BufferTooSmall);
    }
    // 1. in_proj: x (seq×n_in) @ in_proj^T → bcx (seq×3*n_embd)
    //    in_proj nel GGUF: (3*n_embd, n_in) → matmul n_out=3*n_embd, n_in=n_in
    matmul_q4_k(in_proj, 3 * n_embd, n_in, x, seq, bcx_buf)?;
    // 2. split in b (chunk 0), c (chunk 1), xc (chunk 2) e bx = b * xc
    for s in 0..seq {
        for i in 0..n_embd {
            let b = bcx_buf[s * 3 * n_embd + i];
            let xc = bcx_buf[s * 3 * n_embd + 2 * n_embd + i];
            bx_buf[s * n_embd + i] = b * xc;
        }
    }
    // 3. conv1d causale: conv_out[s][c] = k0*bx[s-1] + k1*bx[s] + k2*bx[s+1]
    //    conv nel file: (3, n_embd) = conv[k*n_embd + c] (timestep × canale,
    //    formato ggml: kernel ne0=d_conv=3, ne1=d_inner=n_embd)
    //    il passato (t-1) è zero all'inizio della sequenza
    for s in 0..seq {
        for c in 0..n_embd {
            let cur = bx_buf[s * n_embd + c];
            let next = if s + 1 < seq { bx_buf[(s + 1) * n_embd + c] } else { 0.0 };
            let past = if s > 0 { bx_buf[(s - 1) * n_embd + c] } else { 0.0 };
            let cv = conv[0 * n_embd + c] * past + conv[1 * n_embd + c] * cur + conv[2 * n_embd + c] * next;
            let cc = bcx_buf[s * 3 * n_embd + n_embd + c];
            y[s * n_embd + c] = cc * cv;
        }
    }
    // 4. out_proj: y (seq×n_embd) @ out_proj^T → y (seq×n_embd)
    bx_buf[..seq * n_embd].copy_from_slice(&y[..seq * n_embd]);
    matmul_q4_k(out_proj, n_embd, n_embd, bx_buf, seq, y)
}

/// FFN SwiGLU (LFM2 dense): y = silu(x·Wg^T) ⊙ (x·Wu^T), out = y·Wd^T
/// wg, wu: (n_ff × n_in) Q4_K; wd: (n_in × n_ff) Q4_K o Q6_K.
/// Buffer: gate_buf, up_buf (n_ff), hidden (n_ff) per il prodotto.
pub fn ffn_swiglu(
    wg: &[u8], wu: &[u8], wd: &[u8],
    wd_is_q6: bool,
    n_in: usize, n_ff: usize,
    x: &[f32],
    gate_buf: &mut [f32], up_buf: &mut [f32], hidden: &mut [f32],
    y: &mut [f32],
) -> TensorResult<()> {
    if gate_buf.len() < n_ff || up_buf.len() < n_ff || hidden.len() < n_ff || y.len() < n_in {
        return Err(TensorError::BufferTooSmall);
    }
    // gate e up in parallelo (pesi Q4_K, n_in multiplo di 256)
    matmul_q4_k(wg, n_ff, n_in, x, 1, gate_buf)?;
    matmul_q4_k(wu, n_ff, n_in, x, 1, up_buf)?;
    // hidden[i] = silu(gate[i]) * up[i]
    for i in 0..n_ff {
        hidden[i] = silu(gate_buf[i]) * up_buf[i];
    }
    // down: y = hidden @ wd^T (n_in × n_ff)
    if wd_is_q6 {
        matmul_q6_k(wd, n_in, n_ff, hidden, 1, y)
    } else {
        matmul_q4_k(wd, n_in, n_ff, hidden, 1, y)
    }
}

/// Softmax semplice su un vettore (in-place, esito in `out`)
pub fn softmax(x: &[f32], out: &mut [f32]) -> TensorResult<()> {
    if out.len() < x.len() { return Err(TensorError::BufferTooSmall); }
    let mut max = f32::NEG_INFINITY;
    for &v in x { if v > max { max = v; } }
    let mut sum = 0.0f32;
    for i in 0..x.len() {
        let e = libm::expf(x[i] - max);
        out[i] = e;
        sum += e;
    }
    if sum > 0.0 {
        for v in out.iter_mut() { *v /= sum; }
    }
    Ok(())
}

/// MLP minimale a 2 layer: x (in) → linear(in,h) → ReLU → linear(h,out)
/// Tutti i buffer preallocati dal chiamante.
pub struct Mlp2<'a> {
    pub w1: &'a [f32], // (h × in)
    pub b1: &'a [f32], // (h)
    pub w2: &'a [f32], // (out × h)
    pub b2: &'a [f32], // (out)
    pub in_dim: usize,
    pub hidden: usize,
    pub out_dim: usize,
}

impl<'a> Mlp2<'a> {
    /// forward: x (in_dim) → y (out_dim). hidden_buf serve come scratch.
    pub fn forward(&self, x: &[f32], hidden_buf: &mut [f32], y: &mut [f32]) -> TensorResult<()> {
        if hidden_buf.len() < self.hidden || y.len() < self.out_dim {
            return Err(TensorError::BufferTooSmall);
        }
        linear(self.w1, self.in_dim, self.hidden, x, self.b1, hidden_buf)?;
        relu_inplace(&mut hidden_buf[..self.hidden]);
        linear(self.w2, self.hidden, self.out_dim, hidden_buf, self.b2, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matmul_2x2() {
        let a = [1.0, 2.0, 3.0, 4.0];
        let b = [5.0, 6.0, 7.0, 8.0];
        let mut c = [0.0; 4];
        matmul(&a, 2, 2, &b, 2, &mut c).unwrap();
        assert!((c[0] - 19.0).abs() < 1e-6); // 1*5+2*7
        assert!((c[1] - 22.0).abs() < 1e-6); // 1*6+2*8
        assert!((c[2] - 43.0).abs() < 1e-6); // 3*5+4*7
        assert!((c[3] - 50.0).abs() < 1e-6); // 3*6+4*8
    }

    #[test]
    fn linear_basic() {
        let w = [1.0, 0.0, 0.0, 1.0]; // identità
        let x = [3.0, 4.0];
        let b = [0.1, 0.2];
        let mut y = [0.0; 2];
        linear(&w, 2, 2, &x, &b, &mut y).unwrap();
        assert!((y[0] - 3.1).abs() < 1e-6);
        assert!((y[1] - 4.2).abs() < 1e-6);
    }

    #[test]
    fn mlp_forward() {
        // w1 = identità 2→2, w2 = identità 2→2, b zero → y = x
        let w1 = [1.0, 0.0, 0.0, 1.0];
        let b1 = [0.0, 0.0];
        let w2 = [1.0, 0.0, 0.0, 1.0];
        let b2 = [0.0, 0.0];
        let mlp = Mlp2 { w1: &w1, b1: &b1, w2: &w2, b2: &b2, in_dim: 2, hidden: 2, out_dim: 2 };
        let x = [2.0, -3.0];
        let mut h = [0.0; 2];
        let mut y = [0.0; 2];
        mlp.forward(&x, &mut h, &mut y).unwrap();
        // ReLU: -3 → 0, quindi y = [2, 0]
        assert!((y[0] - 2.0).abs() < 1e-6);
        assert!((y[1] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn softmax_sums_to_one() {
        let x = [1.0, 2.0, 3.0];
        let mut out = [0.0; 3];
        softmax(&x, &mut out).unwrap();
        let s: f32 = out.iter().sum();
        assert!((s - 1.0).abs() < 1e-6);
    }

    #[test]
    fn fp16_roundtrip() {
        // 1.0 = 0x3C00, 0.5 = 0x3800, -2.0 = 0xC000
        assert!((fp16_to_f32(0x3C00) - 1.0).abs() < 1e-6);
        assert!((fp16_to_f32(0x3800) - 0.5).abs() < 1e-6);
        assert!((fp16_to_f32(0xC000) + 2.0).abs() < 1e-6);
        assert!((fp16_to_f32(0x0000) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn dequant_q8_basic() {
        // Blocco: d=1.0 (0x3C00), qs = [1,2,3,...]
        let mut block = [0u8; 2 + Q8_0_BLOCK];
        block[0] = 0x00; block[1] = 0x3C; // fp16 1.0
        for i in 0..Q8_0_BLOCK {
            block[2 + i] = (i as i8) as u8;
        }
        let mut out = [0.0f32; Q8_0_BLOCK];
        dequant_q8_0(&block, &mut out).unwrap();
        assert!((out[0] - 0.0).abs() < 1e-4);
        assert!((out[1] - 1.0).abs() < 1e-4);
        assert!((out[31] - 31.0).abs() < 1e-4);
    }

    #[test]
    fn matmul_q8_small() {
        // W: 1 riga (n_out=1), 32 input. W = identità*2 → x*2
        // d=2.0 (0x4000), qs = [1,0,0,...] per la colonna 0 → w0=2.0
        let mut w = [0u8; 2 + Q8_0_BLOCK];
        w[0] = 0x00; w[1] = 0x40; // fp16 2.0
        w[2] = 1; // qs[0]=1 → w[0] = 2.0
        let a = [1.0f32, 0.0, 0.0, 0.0]; // m=1, k=32 (ma solo 4 usati)
        let mut c = [0.0f32; 1];
        let mut dbuf = [0.0f32; 32];
        matmul_q8_0(&w, 1, 32, &a, 1, &mut c, &mut dbuf).unwrap();
        assert!((c[0] - 2.0).abs() < 1e-3, "atteso 2.0, got {}", c[0]);
    }

    #[test]
    fn q4k_scale_min_pattern() {
        // Verifica get_scale_min_k4 col pattern atteso
        let scales = [10, 20, 30, 40, 5, 6, 7, 8, 0x12, 0x34, 0x56, 0x78];
        let (d0, m0) = get_scale_min_k4(0, &scales);
        assert_eq!(d0, 10); assert_eq!(m0, 5);
        let (d1, m1) = get_scale_min_k4(1, &scales);
        assert_eq!(d1, 20); assert_eq!(m1, 6);
        // j=4: d = (scales[8]&0xF)|((scales[0]>>6)<<4) = 0x2 | 0 = 2
        let (d4, m4) = get_scale_min_k4(4, &scales);
        assert_eq!(d4, (scales[8] & 0x0F) | ((scales[0] >> 6) << 4));
        assert_eq!(m4, (scales[8] >> 4) | ((scales[4] >> 6) << 4));
    }

    #[test]
    fn q4k_dequant_deterministic() {
        // Blocco sintetico: d=1.0, dmin=0, scales zero, qs con pattern noto
        // → out = d * q  (senza min)
        let mut blk = [0u8; Q4_K_BLOCK_BYTES];
        blk[0] = 0x00; blk[1] = 0x3C; // d = 1.0
        blk[2] = 0x00; blk[3] = 0x00; // dmin = 0
        // scales[0]=1 (d_s=1&63=1), scales[4]=0 (m_s=0)
        blk[4] = 1;
        // qs[0] low-nibble = 5 → out[0] = 1*1*5 - 0 = 5
        blk[16] = 0x05;
        let mut out = [0.0f32; Q4_K_BLOCK];
        dequant_q4_k(&blk, &mut out).unwrap();
        assert!((out[0] - 5.0).abs() < 1e-4, "out0={}", out[0]);
    }

    #[test]
    fn q6k_dequant_deterministic() {
        // Blocco sintetico: d=1.0, tutti q=0 (centrati: q=0 → -32), sc=0
        // → out = d * sc * q = 0
        let mut blk = [0u8; Q6_K_BLOCK_BYTES];
        blk[0] = 0x00; blk[1] = 0x3C; // d = 1.0
        let mut out = [0.0f32; Q6_K_BLOCK];
        dequant_q6_k(&blk, &mut out).unwrap();
        for &v in out.iter() {
            assert!(v.abs() < 1e-6, "atteso 0, got {}", v);
        }
        // Ora: sc[0]=2 (int8 2), q1 per l=0: ql[0]=0xF, qh[0]=0 → q1 = 15-32 = -17
        // out[0] = d * sc[0] * q1 = 1 * 2 * (-17) = -34
        let mut blk2 = [0u8; Q6_K_BLOCK_BYTES];
        blk2[0] = 0x00; blk2[1] = 0x3C; // d = 1.0
        blk2[194] = 2; // sc[0] = 2
        blk2[2] = 0x0F; // ql[0] = 0x0F
        let mut out2 = [0.0f32; Q6_K_BLOCK];
        dequant_q6_k(&blk2, &mut out2).unwrap();
        assert!((out2[0] + 34.0).abs() < 1e-4, "out0={}", out2[0]);
    }

    #[test]
    fn matmul_q6k_small() {
        // W: 1 riga, 256 input. d=1.0, sc[0]=1, ql[0]=0x1F → q1 = 31-32 = -1
        // → w[0] = 1 * 1 * (-1) = -1
        let mut w = [0u8; Q6_K_BLOCK_BYTES];
        w[0] = 0x00; w[1] = 0x3C; // d = 1.0
        w[194] = 1; // sc[0] = 1
        w[2] = 0x1F; // ql[0] = 0x1F → q1 = -1
        let a = [1.0f32, 0.0, 0.0, 0.0]; // m=1, k=256
        let mut c = [0.0f32; 1];
        matmul_q6_k(&w, 1, 256, &a, 1, &mut c).unwrap();
        assert!((c[0] + 1.0).abs() < 1e-3, "atteso -1, got {}", c[0]);
    }
}
