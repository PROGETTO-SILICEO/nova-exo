#!/usr/bin/env python3
"""
Estrae i pesi REALI di attenzione del blocco 2 di LFM2.5-2.6B
(primo blocco con attention) e calcola l'output atteso della
forward pass di attention multi-head (GQA 32/8, per-head norm,
RoPE base 10M) per il test nel kernel (v0.21).

Output:
  - testdata/lfm25_blk2_attn.bin     → pesi per il kernel
  - testdata/lfm25_blk2_attn_expected.txt → valori attesi per confronto
"""
import struct
import numpy as np
from gguf import GGUFReader

MODEL = '/home/guardiano/Documenti/models/liquid/LFM2.5-2.6B-Q4_K_M.gguf'
OUT_BIN = '/home/guardiano/Documenti/GitHub/nova-exo/testdata/lfm25_blk2_attn.bin'
OUT_EXP = '/home/guardiano/Documenti/GitHub/nova-exo/testdata/lfm25_blk2_attn_expected.txt'

DIM = 2048
HEAD_DIM = 64
N_HEAD = 32
N_HEAD_KV = 8
N_HEAD_GROUPS = N_HEAD // N_HEAD_KV
ROPE_BASE = 10_000_000.0
RMS_EPS = 1e-5
SEQ = 4

# ── dequant (stessa formula del kernel) ──────────────────────────────

def fp16_to_f32(h):
    h = int(h)
    s = (h >> 15) & 1
    e = (h >> 10) & 0x1F
    f = h & 0x3FF
    if e == 0:
        if f == 0:
            val = 0.0
        else:
            # subnormale: valore = f * 2^-24  (mantissa esplicita)
            val = np.float32(f) * np.float32(2.0 ** -24)
    elif e == 0x1F:
        val = np.inf if f == 0 else np.nan
    else:
        val = np.float32(2.0 ** (e - 15)) * np.float32(1.0 + f / 1024.0)
    if s:
        val = -val
    return np.float32(val)

def dequant_q4_k_block(blk):
    """blk: 144 byte → 256 f32 (formula esatta ggml)"""
    blk = [int(b) for b in blk]
    d = fp16_to_f32(blk[0] | (blk[1] << 8))
    mn = fp16_to_f32(blk[2] | (blk[3] << 8))
    scales = blk[4:16]
    qs = blk[16:144]
    out = np.zeros(256, dtype=np.float32)
    is_ = 0
    idx = 0
    qoff = 0
    for _ in range(4):
        def get_scale_min(j):
            if j < 4:
                return scales[j] & 63, scales[j + 4] & 63
            else:
                ds = (scales[j + 4] & 0x0F) | ((scales[j - 4] >> 6) << 4)
                ms = (scales[j + 4] >> 4) | ((scales[j] >> 6) << 4)
                return ds, ms
        ds1, ms1 = get_scale_min(is_)
        d1 = np.float32(d * np.float32(ds1))
        m1 = np.float32(mn * np.float32(ms1))
        ds2, ms2 = get_scale_min(is_ + 1)
        d2 = np.float32(d * np.float32(ds2))
        m2 = np.float32(mn * np.float32(ms2))
        for l in range(32):
            out[idx + l] = np.float32(d1 * np.float32(qs[qoff + l] & 0x0F)) - m1
        for l in range(32):
            out[idx + 32 + l] = np.float32(d2 * np.float32(qs[qoff + l] >> 4)) - m2
        qoff += 32
        idx += 64
        is_ += 2
    return out

def dequant_q6_k_block(blk):
    """blk: 210 byte → 256 f32 (formula esatta ggml)"""
    blk = [int(b) for b in blk]
    d = fp16_to_f32(blk[0] | (blk[1] << 8))
    # righe morte: d NaN/inf → contributo zero (come nel kernel)
    if not np.isfinite(d):
        return np.zeros(256, dtype=np.float32)
    ql = blk[2:130]
    qh = blk[130:194]
    sc = np.array([int(b) if int(b) < 128 else int(b) - 256 for b in blk[194:210]], dtype=np.int8)
    out = np.zeros(256, dtype=np.float32)
    yoff = 0
    for n in range(2):
        qloff = n * 64
        qhoff = n * 32
        scoff = n * 8
        for l in range(32):
            is_ = l // 16
            q1 = ((ql[qloff + l] & 0x0F) | (((qh[qhoff + l] >> 0) & 3) << 4)) - 32
            q2 = ((ql[qloff + l + 32] & 0x0F) | (((qh[qhoff + l] >> 2) & 3) << 4)) - 32
            q3 = ((ql[qloff + l] >> 4) | (((qh[qhoff + l] >> 4) & 3) << 4)) - 32
            q4 = ((ql[qloff + l + 32] >> 4) | (((qh[qhoff + l] >> 6) & 3) << 4)) - 32
            out[yoff + l] = np.float32(d * np.float32(sc[scoff + is_ + 0]) * np.float32(q1))
            out[yoff + l + 32] = np.float32(d * np.float32(sc[scoff + is_ + 2]) * np.float32(q2))
            out[yoff + l + 64] = np.float32(d * np.float32(sc[scoff + is_ + 4]) * np.float32(q3))
            out[yoff + l + 96] = np.float32(d * np.float32(sc[scoff + is_ + 6]) * np.float32(q4))
        yoff += 128
    return out

def dequant_matrix(data, n_out, n_in, block_bytes, fn):
    """data: bytes del tensore, layout [n_out][n_in] row-major quantizzato"""
    blocks_per_row = n_in // 256
    m = np.zeros((n_out, n_in), dtype=np.float32)
    for r in range(n_out):
        for b in range(blocks_per_row):
            blk = data[(r * blocks_per_row + b) * block_bytes:(r * blocks_per_row + b + 1) * block_bytes]
            vals = fn(blk)
            m[r, b * 256:(b + 1) * 256] = vals
    return m

# ── estrazione dal GGUF ───────────────────────────────────────────────
#
# NOTA: usiamo t.data.tobytes() (GGUFReader espone i dati già con layout
# corretto per righe). NON usiamo seek raw con data_offset: il reader
# gestisce già l'offset interno.
#
# Il layout GGUF dei tensori 2D è [rows][cols] dove rows = prima dim
# (n_out) e cols = seconda dim (n_in), row-major con blocchi da 256.

# ── subset per il test (per velocità su QEMU TCG) ────────────────────
# Usiamo SOLO i primi N_HEAD_TEST head di query e N_KV_TEST head di kv.
# La matematica resta identica: stesse proiezioni, stessi pesi, ma n_out
# ridotto (righe in meno di Wq/Wo e Wk/Wv). L'output atteso si calcola
# con lo stesso subset → confronto esatto kernel vs Python.
N_HEAD_TEST = 8     # head query usate (di 32)
N_KV_TEST = 2       # head kv usate (di 8)
Q_OUT = N_HEAD_TEST * HEAD_DIM   # 512
K_OUT = N_KV_TEST * HEAD_DIM     # 128

def main():
    print("Lettura GGUF...")
    r = GGUFReader(MODEL)
    tensors = {t.name: t for t in r.tensors}

    def get_raw(name):
        t = tensors[name]
        return t.data.tobytes(), list(t.shape)

    # Verifica dims attesi
    for name, exp in [("blk.2.attn_q.weight", (2048, 2048)),
                      ("blk.2.attn_k.weight", (2048, 512)),
                      ("blk.2.attn_v.weight", (2048, 512)),
                      ("blk.2.attn_output.weight", (2048, 2048))]:
        t = tensors[name]
        shape = tuple(int(x) for x in t.shape)
        assert shape == exp, f"{name}: atteso {exp}, got {shape}"

    # raw bytes (interi tensori)
    wq_raw, _ = get_raw("blk.2.attn_q.weight")      # 2048×2048 Q4_K
    wk_raw, _ = get_raw("blk.2.attn_k.weight")      # 2048×512  Q4_K
    wv_raw, _ = get_raw("blk.2.attn_v.weight")      # 2048×512  Q6_K
    wo_raw, _ = get_raw("blk.2.attn_output.weight") # 2048×2048 Q4_K
    q_norm = tensors["blk.2.attn_q_norm.weight"].data.astype(np.float32).tobytes()
    k_norm = tensors["blk.2.attn_k_norm.weight"].data.astype(np.float32).tobytes()

    # subset: prime Q_OUT righe di wq, prime K_OUT righe di wk/wv.
    # Per wo: prime Q_OUT righe E prime Q_OUT colonne (il contesto del
    # subset ha Q_OUT dims, non DIM) → 2 blocchi per riga (512/256).
    # layout riga = n_in/256 blocchi × block_bytes
    wq_rows = Q_OUT * (2048 // 256) * 144
    wk_rows = K_OUT * (2048 // 256) * 144
    wv_rows = K_OUT * (2048 // 256) * 210
    wq_sub = wq_raw[:wq_rows]
    wk_sub = wk_raw[:wk_rows]
    wv_sub = wv_raw[:wv_rows]
    # wo: per ogni riga (144*8 byte), prendo solo i primi 2 blocchi (288 byte)
    wo_row_bytes = 144 * 8
    wo_sub = b"".join(wo_raw[r*wo_row_bytes : r*wo_row_bytes + 144*2] for r in range(Q_OUT))

    # ── file binario di test per il kernel ────────────────────────────
    # header: magic "LFM2"(4) + version u32(4) + seq u32(4) + data_offset u64(8)
    #         + n_out_q u32 + n_out_kv u32 + n_in u32 + wo_n_in u32 = 36
    header = b"LFM2" + struct.pack("<II", 1, SEQ) + struct.pack("<Q", 36)
    header += struct.pack("<IIII", Q_OUT, K_OUT, DIM, Q_OUT)
    with open(OUT_BIN, "wb") as f:
        f.write(header)
        f.write(q_norm)   # 64 f32  (attn_q_norm)
        f.write(k_norm)   # 64 f32  (attn_k_norm)
        f.write(wq_sub)   # Q_OUT×2048 Q4_K
        f.write(wk_sub)   # K_OUT×2048 Q4_K
        f.write(wv_sub)   # K_OUT×2048 Q6_K
        f.write(wo_sub)   # Q_OUT×Q_OUT Q4_K
    total = len(header) + 64*8 + len(wq_sub) + len(wk_sub) + len(wv_sub) + len(wo_sub)
    print(f"Binario di test: {OUT_BIN} ({total} bytes, subset {Q_OUT}/{K_OUT})")

    # ── forward pass attesa (stessa matematica del kernel) ────────────
    print("Calcolo atteso...")

    # input sintetico deterministico: seq × DIM, valori piccoli e vari
    x = np.zeros((SEQ, DIM), dtype=np.float32)
    for s in range(SEQ):
        for i in range(DIM):
            x[s, i] = np.float32(np.sin(s * 0.7 + i * 0.001) * 0.5)

    # 1. proiezioni (subset: Q_OUT righe q, K_OUT righe k/v)
    Wq = dequant_matrix(wq_sub, Q_OUT, 2048, 144, dequant_q4_k_block)
    Wk = dequant_matrix(wk_sub, K_OUT, 2048, 144, dequant_q4_k_block)
    Wv = dequant_matrix(wv_sub, K_OUT, 2048, 210, dequant_q6_k_block)
    Wo = dequant_matrix(wo_sub, Q_OUT, Q_OUT, 144, dequant_q4_k_block)  # 512×512

    q = x @ Wq.T   # (SEQ, Q_OUT)
    k = x @ Wk.T   # (SEQ, K_OUT)
    v = x @ Wv.T   # (SEQ, K_OUT)

    q_norm_w = np.frombuffer(q_norm, dtype=np.float32)
    k_norm_w = np.frombuffer(k_norm, dtype=np.float32)

    # 2. per-head RMSNorm (f32)
    def rms_norm_vec(vec, w, eps):
        m = np.mean(vec.astype(np.float32) ** 2)
        inv = np.float32(1.0 / np.sqrt(np.float32(m + eps)))
        return (vec.astype(np.float32) * inv * w).astype(np.float32)

    for s in range(SEQ):
        for h in range(N_HEAD_TEST):
            off = h * HEAD_DIM
            q[s, off:off + HEAD_DIM] = rms_norm_vec(q[s, off:off + HEAD_DIM], q_norm_w, RMS_EPS)
        for h in range(N_KV_TEST):
            off = h * HEAD_DIM
            k[s, off:off + HEAD_DIM] = rms_norm_vec(k[s, off:off + HEAD_DIM], k_norm_w, RMS_EPS)

    # 3. RoPE
    def rope_vec(vec, pos):
        v = vec.astype(np.float32).copy()
        for i in range(HEAD_DIM // 2):
            theta = np.float32(np.float32(ROPE_BASE ** np.float32(-2.0 * i / HEAD_DIM)) * np.float32(pos))
            sin = np.float32(np.sin(theta))
            cos = np.float32(np.cos(theta))
            a = v[2 * i]
            b = v[2 * i + 1]
            v[2 * i] = np.float32(a * cos - b * sin)
            v[2 * i + 1] = np.float32(a * sin + b * cos)
        return v

    for s in range(SEQ):
        for h in range(N_HEAD_TEST):
            off = h * HEAD_DIM
            q[s, off:off + HEAD_DIM] = rope_vec(q[s, off:off + HEAD_DIM], s)
        for h in range(N_KV_TEST):
            off = h * HEAD_DIM
            k[s, off:off + HEAD_DIM] = rope_vec(k[s, off:off + HEAD_DIM], s)

    # 4-5. attention GQA + mask causale a finestra (come il kernel v0.23)
    # window: il token s guarda j con s-window <= j <= s
    def attn_window(window):
        out = np.zeros((SEQ, Q_OUT), dtype=np.float32)
        scale = np.float32(1.0 / np.sqrt(HEAD_DIM))
        for h in range(N_HEAD_TEST):
            g = h % N_KV_TEST
            for s in range(SEQ):
                qh = q[s, h * HEAD_DIM:(h + 1) * HEAD_DIM]
                lo = max(0, s - window)
                scores = np.zeros(SEQ, dtype=np.float32)
                for j in range(SEQ):
                    if j > s or j < lo:
                        scores[j] = np.float32(-1e30)
                    else:
                        kh = k[j, g * HEAD_DIM:(g + 1) * HEAD_DIM]
                        scores[j] = np.float32(np.dot(qh, kh) * scale)
                # softmax stabile
                mx = np.float32(np.max(scores))
                e = np.exp((scores - mx).astype(np.float32)).astype(np.float32)
                e = e / np.float32(np.sum(e))
                for j in range(SEQ):
                    out[s, h * HEAD_DIM:(h + 1) * HEAD_DIM] += np.float32(e[j] * v[j, g * HEAD_DIM:(g + 1) * HEAD_DIM])
        return out @ Wo.T   # (SEQ, Q_OUT)

    # attesi per finestre multiple: 4 (full), 2, 1, 0
    with open(OUT_EXP, "w") as f:
        f.write(f"SEQ={SEQ} DIM={DIM} Q_OUT={Q_OUT} K_OUT={K_OUT}\n")
        for w in [4, 2, 1, 0]:
            y = attn_window(w)
            f.write(f"WINDOW={w}\n")
            for s in range(SEQ):
                vals = " ".join(f"{v:.6f}" for v in y[s, :8])
                f.write(f"y[{s}][0:8] {vals}\n")
            for s in range(SEQ):
                vals = " ".join(f"{v:.6f}" for v in y[s, Q_OUT-8:Q_OUT])
                f.write(f"y[{s}][{Q_OUT-8}:{Q_OUT}] {vals}\n")
            f.write(f"y[0][0]={y[0][0]:.6f} y[3][7]={y[3][7]:.6f} bits00={y[0][0].view(np.uint32):08X} bits37={y[3][7].view(np.uint32):08X}\n")
    print(f"Atteso multi-window: {OUT_EXP}")
    for w in [4, 2, 1, 0]:
        y = attn_window(w)
        print(f"  w={w}: y[0][0]={y[0][0]:.6f} ({y[0][0].view(np.uint32):08X}) y[3][7]={y[3][7]:.6f} ({y[3][7].view(np.uint32):08X})")
    print("OK")

if __name__ == "__main__":
    main()

