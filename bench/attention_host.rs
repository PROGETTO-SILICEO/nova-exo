// ── Banco host per attention.rs: verifica di attention_forward_dim ─────────
//
// Creato il 15/10/2026 per convalidare il Passo 1 del Nucleo:
// la parametrizzazione di attention_forward_dim per supportare sia
// LFM2.5 (dim=2048) che il Nucleo (dim=512) nel metallo, con GQA causale.
//
// Compilazione ed esecuzione: make test (oppure rustc --test bench/attention_host.rs)

mod math {
    pub fn sqrtf(x: f32) -> f32 { x.sqrt() }
    pub fn expf(x: f32) -> f32 { x.exp() }
    pub fn powf(x: f32, y: f32) -> f32 { x.powf(y) }
    pub fn sincosf(x: f32) -> (f32, f32) { (x.sin(), x.cos()) }
}

#[path = "../src/tensor.rs"]
mod tensor;

#[path = "../src/attention.rs"]
mod attention;

use attention::{rms_norm, rope, attention_forward_dim, HEAD_DIM};

#[test]
fn rms_norm_test_unitario() {
    let x = [1.0, -1.0, 2.0, -2.0];
    let w = [1.0; 4];
    let mut y = [0.0; 4];
    rms_norm(&x, &w, 1e-5, &mut y).unwrap();
    let inv = 1.0 / (2.5f32).sqrt();
    assert!((y[0] - inv).abs() < 1e-5);
    assert!((y[3] + 2.0 * inv).abs() < 1e-5);
}

#[test]
fn rope_rotazione_ortogonale() {
    let mut x = [1.0, 0.0];
    rope(&mut x, 1, 2, 1.0);
    assert!((x[0] - 0.5403).abs() < 1e-3);
    assert!((x[1] - 0.8415).abs() < 1e-3);
}

#[test]
fn attention_forward_dim_512_nucleo_funziona() {
    // Dimensioni del Nucleo:
    // dim = 512, 8 Q-heads (n_out_q = 512), 2 KV-heads (n_out_kv = 128)
    let dim = 512;
    let n_out_q = 8 * HEAD_DIM;   // 512
    let n_out_kv = 2 * HEAD_DIM;  // 128
    let seq = 2;

    let x = vec![0.5f32; seq * dim];
    let qn = vec![1.0f32; HEAD_DIM];
    let kn = vec![1.0f32; HEAD_DIM];

    // Dummy quantized weights:
    // Q4_K block: 256 weights -> 144 bytes
    // Per n_out * dim elementi: (n_out * dim / 256) * 144 bytes
    let q4_len_q = (n_out_q * dim / 256) * 144;
    let q4_len_k = (n_out_kv * dim / 256) * 144;
    // Q6_K block: 256 weights -> 210 bytes
    let q6_len_v = (n_out_kv * dim / 256) * 210;
    let q4_len_o = (n_out_q * n_out_q / 256) * 144;

    let wq = vec![0u8; q4_len_q];
    let wk = vec![0u8; q4_len_k];
    let wv = vec![0u8; q6_len_v];
    let wo = vec![0u8; q4_len_o];

    let mut q_buf = vec![0.0f32; seq * n_out_q];
    let mut k_buf = vec![0.0f32; seq * n_out_kv];
    let mut v_buf = vec![0.0f32; seq * n_out_kv];
    let mut scores = vec![0.0f32; seq * seq];
    let mut out_buf = vec![0.0f32; seq * n_out_q];
    let mut ctx_buf = vec![0.0f32; seq * n_out_kv];
    let mut y = vec![0.0f32; seq * n_out_q];

    let res = attention_forward_dim(
        &x, seq, dim,
        &wq, &wk, &wv, &wo,
        &qn, &kn,
        0, n_out_q, n_out_kv, n_out_q,
        4, // window
        &mut q_buf, &mut k_buf, &mut v_buf,
        &mut scores, &mut out_buf, &mut ctx_buf,
        &mut y,
    );

    assert!(res.is_ok(), "attention_forward_dim con dim=512 deve completarsi senza errori");
    assert_eq!(y.len(), seq * n_out_q);
}

#[test]
fn attention_forward_dim_buffer_checks() {
    let dim = 512;
    let n_out_q = 512;
    let n_out_kv = 128;
    let seq = 2;

    let x = vec![0.5f32; seq * dim - 1]; // Buffer intenzionalmente corto
    let qn = vec![1.0f32; HEAD_DIM];
    let kn = vec![1.0f32; HEAD_DIM];
    let wq = vec![0u8; 144];
    let wk = vec![0u8; 144];
    let wv = vec![0u8; 210];
    let wo = vec![0u8; 144];

    let mut q_buf = vec![0.0f32; seq * n_out_q];
    let mut k_buf = vec![0.0f32; seq * n_out_kv];
    let mut v_buf = vec![0.0f32; seq * n_out_kv];
    let mut scores = vec![0.0f32; seq * seq];
    let mut out_buf = vec![0.0f32; seq * n_out_q];
    let mut ctx_buf = vec![0.0f32; seq * n_out_kv];
    let mut y = vec![0.0f32; seq * n_out_q];

    let res = attention_forward_dim(
        &x, seq, dim,
        &wq, &wk, &wv, &wo,
        &qn, &kn,
        0, n_out_q, n_out_kv, n_out_q,
        4,
        &mut q_buf, &mut k_buf, &mut v_buf,
        &mut scores, &mut out_buf, &mut ctx_buf,
        &mut y,
    );

    assert!(matches!(res, Err(attention::AttnError::BufferTooSmall)));
}
