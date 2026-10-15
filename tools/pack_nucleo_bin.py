#!/usr/bin/env python3
"""
tools/pack_nucleo_bin.py — Generatore e Impacchettatore binario per Il Nucleo (~41M)
Progetto Siliceo — Exokernel Nova Exo (Ottobre 2026)

Genera il file binario `nucleo_41m.bin` da caricare in RAM tramite modulo Limine
(`boot():/boot/nucleo_41m.bin`), compatibile con `src/nucleo.rs`.

Specifiche Formato Binario:
  Header (64 byte):
    [0..4]   magic: b"NUCL"
    [4..8]   version: u32 = 1
    [8..12]  dim: u32 = 512
    [12..16] n_layers: u32 = 12
    [16..20] n_ff: u32 = 1536
    [20..24] n_q_heads: u32 = 8
    [24..28] n_kv_heads: u32 = 2
    [28..32] head_dim: u32 = 64
    [32..36] chemio_dim: u32 = 4
    [36..64] riservato: 28 byte a zero

  Sezione Embed (8,192 byte):
    chemio (4) x dim (512) f32

  12 Layer (1,716,736 byte ciascuno):
    - attn_norm:   512 f32 = 2,048 B
    - wq (Q4_K):   (512 * 512 / 256) * 144 B = 147,456 B
    - wk (Q4_K):   (128 * 512 / 256) * 144 B =  36,864 B
    - wv (Q6_K):   (128 * 512 / 256) * 210 B =  53,760 B
    - wo (Q4_K):   (512 * 512 / 256) * 144 B = 147,456 B
    - ffn_norm:    512 f32 = 2,048 B
    - w_gate (Q4_K): (1536 * 512 / 256) * 144 B = 442,368 B
    - w_up (Q4_K):   (1536 * 512 / 256) * 144 B = 442,368 B
    - w_down (Q4_K): (512 * 1536 / 256) * 144 B = 442,368 B

  Final Norm (2,048 byte):
    512 f32

  Head Projection (8,192 byte):
    dim (512) x chemio (4) f32

Totale: 20,416,576 byte (~19.47 MB)
"""

import struct
import sys
import os

DIM = 512
N_LAYERS = 12
N_Q_HEADS = 8
N_KV_HEADS = 2
HEAD_DIM = 64
N_OUT_Q = N_Q_HEADS * HEAD_DIM   # 512
N_OUT_KV = N_KV_HEADS * HEAD_DIM # 128
N_FF = 1536
CHEMIO_DIM = 4

Q4_K_WQ_BYTES = (N_OUT_Q * DIM // 256) * 144
Q4_K_WK_BYTES = (N_OUT_KV * DIM // 256) * 144
Q6_K_WV_BYTES = (N_OUT_KV * DIM // 256) * 210
Q4_K_WO_BYTES = (DIM * N_OUT_Q // 256) * 144
Q4_K_WGATE_BYTES = (N_FF * DIM // 256) * 144
Q4_K_WUP_BYTES = (N_FF * DIM // 256) * 144
Q4_K_WDOWN_BYTES = (DIM * N_FF // 256) * 144

NORM_BYTES = DIM * 4
LAYER_BYTES = (
    NORM_BYTES
    + Q4_K_WQ_BYTES
    + Q4_K_WK_BYTES
    + Q6_K_WV_BYTES
    + Q4_K_WO_BYTES
    + NORM_BYTES
    + Q4_K_WGATE_BYTES
    + Q4_K_WUP_BYTES
    + Q4_K_WDOWN_BYTES
)

HEADER_BYTES = 64
EMBED_BYTES = CHEMIO_DIM * DIM * 4
HEAD_BYTES = DIM * CHEMIO_DIM * 4

TOTAL_BYTES = HEADER_BYTES + EMBED_BYTES + (LAYER_BYTES * N_LAYERS) + NORM_BYTES + HEAD_BYTES

def make_header():
    header = bytearray(64)
    header[0:4] = b"NUCL"
    struct.pack_into("<I", header, 4, 1)          # version
    struct.pack_into("<I", header, 8, DIM)        # dim
    struct.pack_into("<I", header, 12, N_LAYERS)  # n_layers
    struct.pack_into("<I", header, 16, N_FF)      # n_ff
    struct.pack_into("<I", header, 20, N_Q_HEADS) # n_q_heads
    struct.pack_into("<I", header, 24, N_KV_HEADS)# n_kv_heads
    struct.pack_into("<I", header, 28, HEAD_DIM)  # head_dim
    struct.pack_into("<I", header, 32, CHEMIO_DIM)# chemio_dim
    return bytes(header)

def make_synthetic_bin(out_path):
    print(f"── Generazione binario sintetico per Il Nucleo 41M: {out_path}")
    print(f"   Dimensione target: {TOTAL_BYTES:,} byte (~{TOTAL_BYTES / 1024 / 1024:.2f} MB)")
    
    with open(out_path, "wb") as f:
        # 1. Header
        f.write(make_header())
        
        # 2. Embed weights (chemio -> dim): 4 x 512 float
        # Inizializziamo con valori identità/sensoriali ragionevoli (es. 0.05)
        embed_data = struct.pack(f"<{CHEMIO_DIM * DIM}f", *([0.05] * (CHEMIO_DIM * DIM)))
        f.write(embed_data)
        
        # 3. 12 Layer
        norm_ones = struct.pack(f"<{DIM}f", *([1.0] * DIM))
        for l in range(N_LAYERS):
            f.write(norm_ones)               # attn_norm
            f.write(b"\x00" * Q4_K_WQ_BYTES) # wq
            f.write(b"\x00" * Q4_K_WK_BYTES) # wk
            f.write(b"\x00" * Q6_K_WV_BYTES) # wv
            f.write(b"\x00" * Q4_K_WO_BYTES) # wo
            f.write(norm_ones)               # ffn_norm
            f.write(b"\x00" * Q4_K_WGATE_BYTES) # w_gate
            f.write(b"\x00" * Q4_K_WUP_BYTES)   # w_up
            f.write(b"\x00" * Q4_K_WDOWN_BYTES) # w_down
            
        # 4. Final Norm
        f.write(norm_ones)
        
        # 5. Head projection (dim -> chemio): 512 x 4 float
        head_data = struct.pack(f"<{DIM * CHEMIO_DIM}f", *([0.02] * (DIM * CHEMIO_DIM)))
        f.write(head_data)
        
    actual_sz = os.path.getsize(out_path)
    assert actual_sz == TOTAL_BYTES, f"Errore dimensione: {actual_sz} != {TOTAL_BYTES}"
    print(f"✅ Creato {out_path} ({actual_sz:,} byte) con magic b'NUCL' e 12 layer!")

if __name__ == "__main__":
    out_file = sys.argv[1] if len(sys.argv) > 1 else "nucleo_41m.bin"
    make_synthetic_bin(out_file)
