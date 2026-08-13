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

/// Converte fp16 (bit pattern) in f32 (implementazione manuale no_std)
pub fn fp16_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) & 1) as u32;
    let exp = ((h >> 10) & 0x1F) as u32;
    let frac = (h & 0x3FF) as u32;

    let (e, f): (u32, u32) = if exp == 0 {
        if frac == 0 { (0, 0) } else {
            // subnormale: normalizza
            let mut e = 1u32;
            let mut f = frac;
            while f & 0x400 == 0 {
                f <<= 1;
                e -= 1;
            }
            f &= 0x3FF;
            (127 - 15 - e + 1, f)
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

/// C = A × B, con A (m×k) row-major, B (k×n) row-major, C (m×n).
/// Tutti i buffer sono preallocati dal chiamante (no alloc).
pub fn matmul(
    a: &[f32], m: usize, k: usize,
    b: &[f32], n: usize,
    c: &mut [f32],
) -> TensorResult<()> {
    if a.len() < m * k || b.len() < k * n || c.len() < m * n {
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
}
