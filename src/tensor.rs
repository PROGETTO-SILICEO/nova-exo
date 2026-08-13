// ── Tensor ops (no_std) — Sempre, 13 Ago 2026 ──
#![no_std]
//
// Le operazioni fondamentali di inferenza nel metallo:
//   - matmul f32 (senza alloc: input/output forniti dal chiamante)
//   - layer lineare con bias
//   - forward di un piccolo MLP
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
}
