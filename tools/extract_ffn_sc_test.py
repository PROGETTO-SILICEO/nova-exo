#!/usr/bin/env python3
"""
Estrae i pesi REALI di FFN SwiGLU (blk.2) e ShortConv (blk.0) di LFM2.5-2.6B
e calcola gli output attesi per il test nel kernel (v0.22).

Output:
  - testdata/lfm25_ffn.bin       → pesi FFN per il kernel
  - testdata/lfm25_ffn_expected.txt
  - testdata/lfm25_shortconv.bin → pesi shortconv per il kernel
  - testdata/lfm25_shortconv_expected.txt
"""
import struct
import numpy as np
from gguf import GGUFReader

MODEL = '/home/guardiano/Documenti/models/liquid/LFM2.5-2.6B-Q4_K_M.gguf'
FFN_BIN = '/home/guardiano/Documenti/GitHub/nova-exo/testdata/lfm25_ffn.bin'
FFN_EXP = '/home/guardiano/Documenti/GitHub/nova-exo/testdata/lfm25_ffn_expected.txt'
SC_BIN = '/home/guardiano/Documenti/GitHub/nova-exo/testdata/lfm25_shortconv.bin'
SC_EXP = '/home/guardiano/Documenti/GitHub/nova-exo/testdata/lfm25_shortconv_expected.txt'

DIM = 2048
N_FF = 10752
SEQ = 4
# ── subset per il test (velocità QEMU TCG) ───────────────────────────
# FFN: usiamo solo le prime N_FF_TEST colonne di gate/up e le prime
# righe×colonne di down — la matematica resta identica.
N_FF_TEST = 2048
# ShortConv: solo i primi N_EMB_TEST canali (in_proj 3×N_EMB_TEST righe,
# out_proj N_EMB_TEST×N_EMB_TEST, kernel N_EMB_TEST×3).
N_EMB_TEST = 512

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
            val = np.float32(f) * np.float32(2.0 ** -24)
    elif e == 0x1F:
        val = np.inf if f == 0 else np.nan
    else:
        val = np.float32(2.0 ** (e - 15)) * np.float32(1.0 + f / 1024.0)
    if s:
        val = -val
    return np.float32(val)

def dequant_q4_k_block(blk):
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
    blk = [int(b) for b in blk]
    d = fp16_to_f32(blk[0] | (blk[1] << 8))
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
    blocks_per_row = n_in // 256
    m = np.zeros((n_out, n_in), dtype=np.float32)
    for r in range(n_out):
        for b in range(blocks_per_row):
            blk = data[(r * blocks_per_row + b) * block_bytes:(r * blocks_per_row + b + 1) * block_bytes]
            m[r, b * 256:(b + 1) * 256] = fn(blk)
    return m

def gen_input(seq, dim):
    x = np.zeros((seq, dim), dtype=np.float32)
    for s in range(seq):
        for i in range(dim):
            x[s, i] = np.float32(np.sin(s * 0.7 + i * 0.001) * 0.5)
    return x

def main():
    print("Lettura GGUF...")
    r = GGUFReader(MODEL)
    tensors = {t.name: t for t in r.tensors}

    def get_raw(name):
        t = tensors[name]
        return t.data.tobytes()

    # ── FFN blk.2 (dense, subset) ────────────────────────────────────
    # dims attesi: ffn_gate/up (2048, 10752), ffn_down (10752, 2048)
    # subset: prime N_FF_TEST colonne di gate/up (righe intere di wd)
    wg = get_raw("blk.2.ffn_gate.weight")   # Q4_K (2048, 10752)
    wu = get_raw("blk.2.ffn_up.weight")     # Q4_K (2048, 10752)
    wd = get_raw("blk.2.ffn_down.weight")   # tipo? blk.2 ffn_down type=14 (Q6_K)
    wd_type = tensors["blk.2.ffn_down.weight"].tensor_type
    wd_bpb = 210 if wd_type == 14 else 144
    print(f"ffn_down blk.2 type: {wd_type} (12=Q4_K, 14=Q6_K)")
    assert len(wg) == 2048 * (10752 // 256) * 144, f"wg len {len(wg)}"
    assert len(wu) == 2048 * (10752 // 256) * 144, f"wu len {len(wu)}"
    assert len(wd) == 2048 * (10752 // 256) * wd_bpb, f"wd len {len(wd)}"

    # subset: per gate/up, il layout GGUF è [n_out][n_in] con n_out=10752
    # (righe) e n_in=2048 (8 blocchi da 256 per riga). Prendiamo le prime
    # N_FF_TEST righe (n_out ridotto) — la matematica resta identica.
    # wg/wu: 10752 righe × 8 blocchi × 144 byte = 12386304
    sub_rows = N_FF_TEST
    wg_sub = wg[:sub_rows * 8 * 144]
    wu_sub = wu[:sub_rows * 8 * 144]
    # per down: (n_in=2048 righe × n_ff=10752 col) → layout GGUF
    # [n_out=10752][n_in=2048]? NO: ffn_down è (10752, 2048) nel file →
    # GGUFReader darà (2048, ...) = n_out=2048, n_in=10752 → 42 blocchi/riga.
    # Ma per il subset dobbiamo produrre Wd (n_ff_sub × n_ff_sub) = (2048, 2048).
    # Il file GGUF espone wd come (2048, 6048): 2048 righe, ognuna 42 blocchi.
    # Per il subset: prime 2048 righe × primi 8 blocchi (2048 colonne).
    wd_sub = b"".join(wd[r*42*wd_bpb : r*42*wd_bpb + 8*wd_bpb] for r in range(N_FF_TEST))

    # file kernel: header 16 (magic "FFNS" + version + n_in + n_ff + flags + pad)
    flags = 1 if wd_type == 14 else 0  # bit 0: wd è Q6_K
    header = b"FFNS" + struct.pack("<IIII", 1, DIM, N_FF_TEST, flags)
    with open(FFN_BIN, "wb") as f:
        f.write(header)
        f.write(wg_sub)
        f.write(wu_sub)
        f.write(wd_sub)
    print(f"FFN binario: {FFN_BIN} ({16 + len(wg_sub) + len(wu_sub) + len(wd_sub)} bytes)")

    # atteso
    Wg = dequant_matrix(wg_sub, N_FF_TEST, DIM, 144, dequant_q4_k_block)
    Wu = dequant_matrix(wu_sub, N_FF_TEST, DIM, 144, dequant_q4_k_block)
    if wd_type == 14:
        Wd = dequant_matrix(wd_sub, N_FF_TEST, N_FF_TEST, 210, dequant_q6_k_block)
    else:
        Wd = dequant_matrix(wd_sub, N_FF_TEST, N_FF_TEST, 144, dequant_q4_k_block)
    x = gen_input(1, DIM)[0]  # singolo token
    gate = x @ Wg.T
    up = x @ Wu.T
    h = np.array([silu_1(g) * u for g, u in zip(gate, up)], dtype=np.float32)
    y = h @ Wd.T
    with open(FFN_EXP, "w") as f:
        f.write(f"DIM={DIM} N_FF={N_FF_TEST} SEQ=1\n")
        for i in range(8):
            f.write(f"y[{i}] {y[i]:.6f}\n")
        f.write(f"y[100] {y[100]:.6f}\n")
        f.write(f"y[{N_FF_TEST-1}] {y[N_FF_TEST-1]:.6f}\n")
        f.write(f"y bits0 {y[0].view(np.uint32):08X}\n")
        f.write(f"y bits{N_FF_TEST-1} {y[N_FF_TEST-1].view(np.uint32):08X}\n")
    print(f"FFN atteso: {FFN_EXP}")
    print(f"  y[0]={y[0]:.6f} y[100]={y[100]:.6f} y[{N_FF_TEST-1}]={y[N_FF_TEST-1]:.6f}")

    # ── ShortConv blk.0 (subset: N_EMB_TEST canali) ──────────────────
    # conv nel GGUF: shape (2048, 3) — kernel[c][k], canali × timestep
    sc_conv = tensors["blk.0.shortconv.conv.weight"].data.astype(np.float32)
    sc_inp = get_raw("blk.0.shortconv.in_proj.weight")   # Q4_K (2048, 6144)
    sc_out = get_raw("blk.0.shortconv.out_proj.weight")  # Q4_K (2048, 2048)
    print(f"conv shape: {sc_conv.shape}")
    assert len(sc_inp) == 2048 * (6144 // 256) * 144, f"inp len {len(sc_inp)}"
    assert len(sc_out) == 2048 * (2048 // 256) * 144, f"out len {len(sc_out)}"

    # subset: in_proj prime 3*N_EMB_TEST righe (3×512=1536), out_proj
    # prime N_EMB_TEST righe × prime N_EMB_TEST colonne (2 blocchi/riga)
    e = N_EMB_TEST
    inp_sub = sc_inp[: 3*e * (2048//256) * 144]  # prime 1536 righe complete
    out_sub = b"".join(sc_out[r*8*144 : r*8*144 + (e//256)*144] for r in range(e))

    header = b"SCNV" + struct.pack("<II", 1, e)
    with open(SC_BIN, "wb") as f:
        f.write(header)
        # kernel nel formato ggml: (d_conv=3, d_inner=e) = [k][c].
        # GGUFReader espone (e, 3) ma il dato è [k][c] → usiamo .T
        f.write(np.ascontiguousarray(sc_conv[:e].T).tobytes())  # (3, e) → k-major
        f.write(inp_sub)
        f.write(out_sub)
    print(f"SC binario: {SC_BIN} ({8 + e*3*4 + len(inp_sub) + len(out_sub)} bytes)")

    # atteso
    W_inp = dequant_matrix(inp_sub, 3 * e, DIM, 144, dequant_q4_k_block)
    W_out = dequant_matrix(out_sub, e, e, 144, dequant_q4_k_block)
    kernel = sc_conv[:e]  # shape (e, 3) esposta = [c][k]; in ggml è [k][c]
    x = gen_input(SEQ, DIM)
    bcx = x @ W_inp.T   # (SEQ, 3*e)
    b = bcx[:, :e]
    c = bcx[:, e:2*e]
    xc = bcx[:, 2*e:]
    bx = b * xc
    # conv causale: out[s][ch] = k0*bx[s-1] + k1*bx[s] + k2*bx[s+1], passato=0
    # kernel ggml [k][c] = kernel.T in numpy
    kg = kernel.T  # (3, e)
    conv_out = np.zeros((SEQ, e), dtype=np.float32)
    for s in range(SEQ):
        for ch in range(e):
            past = bx[s-1, ch] if s > 0 else 0.0
            cur = bx[s, ch]
            nxt = bx[s+1, ch] if s+1 < SEQ else 0.0
            conv_out[s, ch] = kg[0, ch] * past + kg[1, ch] * cur + kg[2, ch] * nxt
    y = c * conv_out
    y = y @ W_out.T
    with open(SC_EXP, "w") as f:
        f.write(f"DIM={DIM} N_EMB={e} SEQ={SEQ}\n")
        for s in range(SEQ):
            f.write(f"y[{s}][0] {y[s,0]:.6f} y[{s}][100] {y[s,100]:.6f} y[{s}][{e-1}] {y[s,e-1]:.6f}\n")
        f.write(f"y[0][0] bits {y[0,0].view(np.uint32):08X}\n")
        f.write(f"y[3][{e-1}] bits {y[3,e-1].view(np.uint32):08X}\n")
    print(f"SC atteso: {SC_EXP}")
    print(f"  y[0][0]={y[0,0]:.6f} y[3][{e-1}]={y[3,e-1]:.6f}")
    print("OK")

def silu_1(x):
    return float(np.float32(x) / np.float32(1.0 + np.exp(np.float32(-x))))

if __name__ == "__main__":
    main()
