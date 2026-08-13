#![no_std]
#![no_main]

use core::arch::asm;
use core::fmt::Write;
use core::panic::PanicInfo;
use uart_16550::SerialPort;

mod alloc;
mod apic;
mod attention;
mod cfc;
mod e1000;
mod executive;
mod gguf;
mod gui;
mod idt;
mod inference;
mod interpreter;
mod interpreter_weights;
mod neurogenesis;
mod paging;
mod pci;
mod predictor;
mod serial;
mod state;
mod tensor;

use serial::LineReader;

// ── Limine base revision (rev 6 = MAX_SUPPORTED) ─────────────────────────

const LIMINE_COMMON_MAGIC: [u64; 2] = [0xc7b1dd30df4c8b88, 0x0a82e883a194f07b];
const LIMINE_HHDM_ID: [u64; 2] = [0x48dcf1cb8ad2b852, 0x63984e959a98244b];
const LIMINE_EXEC_ADDR_ID: [u64; 2] = [0x71ba76863cc55f63, 0xb2644a48c516a487];

#[repr(C)]
struct BaseRevision {
    magic: [u64; 3],
}
unsafe impl Sync for BaseRevision {}

#[used]
#[link_section = ".limine_reqs"]
static mut BASE_REVISION: BaseRevision = BaseRevision {
    magic: [0xf9562b2d5c95a6c8, 0x6a7b384944536bdc, 6],
};

#[repr(C)]
struct HhdmResponse {
    revision: u64,
    offset: u64,
}

#[repr(C)]
struct HhdmRequest {
    id: [u64; 4],
    revision: u64,
    response: *mut HhdmResponse,
}

#[repr(C)]
struct ExecAddrResponse {
    revision: u64,
    physical_base: u64,
    virtual_base: u64,
}

#[repr(C)]
struct ExecAddrRequest {
    id: [u64; 4],
    revision: u64,
    response: *mut ExecAddrResponse,
}

unsafe impl Sync for HhdmRequest {}
unsafe impl Sync for ExecAddrRequest {}

#[used]
#[link_section = ".limine_reqs"]
static mut HHDM_REQ: HhdmRequest = HhdmRequest {
    id: [LIMINE_COMMON_MAGIC[0], LIMINE_COMMON_MAGIC[1],
         LIMINE_HHDM_ID[0], LIMINE_HHDM_ID[1]],
    revision: 0,
    response: core::ptr::null_mut(),
};

#[used]
#[link_section = ".limine_reqs"]
static mut EXEC_ADDR_REQ: ExecAddrRequest = ExecAddrRequest {
    id: [LIMINE_COMMON_MAGIC[0], LIMINE_COMMON_MAGIC[1],
         LIMINE_EXEC_ADDR_ID[0], LIMINE_EXEC_ADDR_ID[1]],
    revision: 0,
    response: core::ptr::null_mut(),
};

/// Kernel physical → virtual offset: phys = virt − KERNEL_SLOT
pub(crate) static mut KERNEL_SLOT: u64 = 0;

/// HHDM offset: phys → HHDM virtual = HHDM_OFFSET + phys
pub(crate) static mut HHDM_OFFSET: u64 = 0;

pub(crate) fn virt_to_phys(virt: u64) -> u64 {
    unsafe { virt.wrapping_sub(KERNEL_SLOT) }
}

fn init_limine_requests() {
    unsafe {
        let hhdm = &raw const HHDM_REQ;
            if !(*hhdm).response.is_null() {
            HHDM_OFFSET = (*(*hhdm).response).offset;
        }

        let ea = &raw const EXEC_ADDR_REQ;
            if !(*ea).response.is_null() {
            let r = &*(*ea).response;
            KERNEL_SLOT = 0xFFFFFFFF80000000u64.wrapping_sub(r.physical_base);
        }
    }
}

#[cfg(feature = "demo_pf")]
static DEMO_PF_DONE: AtomicBool = AtomicBool::new(false);
// ── Axon bundles (v0.6+) ────────────────────────────────────────────────

static FASCI: [cfc::AxonBundle; 2] = [
    cfc::AxonBundle { src: cfc::CellId::Tatto,  src_offset: 0, count: 2, dst: cfc::CellId::Integrat, dst_offset: 0 },
    cfc::AxonBundle { src: cfc::CellId::Chemio, src_offset: 0, count: 2, dst: cfc::CellId::Integrat, dst_offset: 2 },
];

// ── Serial port ─────────────────────────────────────────────────────────

static mut SERIAL: SerialPort = unsafe { SerialPort::new(0x3F8) };

macro_rules! serial_println {
    ($($arg:tt)*) => {
        #[allow(unused_unsafe)]
        unsafe {
            let serial: &mut SerialPort = &mut *(&raw mut SERIAL);
            let _ = write!(serial, $($arg)*);
            let _ = serial.write_str("\n");
        }
    };
}

// ── I/O port helpers ────────────────────────────────────────────────────

unsafe fn outb(port: u16, val: u8) {
    asm!("out dx, al", in("dx") port, in("al") val);
}

// ── CfC weights (Xavier seed per cellula) ───────────────────────────────
// Initialized at runtime in _start() using CfcWeights::new_xavier()

static mut W_TATTO:   core::mem::MaybeUninit<cfc::CfcWeights> = core::mem::MaybeUninit::uninit();
static mut W_CHEMIO:  core::mem::MaybeUninit<cfc::CfcWeights> = core::mem::MaybeUninit::uninit();
static mut W_METABOL: core::mem::MaybeUninit<cfc::CfcWeights> = core::mem::MaybeUninit::uninit();
static mut W_INTRG:   core::mem::MaybeUninit<cfc::CfcWeights> = core::mem::MaybeUninit::uninit();

pub unsafe fn init_weights() {
    // Hardcoded weights from v0.11 — expanded to 16 neurons per cell.
    // These produced β=0.75-1.15, PD=1.915. Xavier failed.
    W_TATTO = core::mem::MaybeUninit::new(cfc::CfcWeights::new_hardcoded("Tatto").clone());
    W_CHEMIO = core::mem::MaybeUninit::new(cfc::CfcWeights::new_hardcoded("Chemio").clone());
    W_METABOL = core::mem::MaybeUninit::new(cfc::CfcWeights::new_hardcoded("Metabol").clone());
    W_INTRG = core::mem::MaybeUninit::new(cfc::CfcWeights::new_hardcoded("Integrat").clone());
}



// ── Disable legacy PIC ─────────────────────────────────────────────────
// Mask all PIC IRQs so they don't fire during APIC operation.

unsafe fn pic_disable() {
    outb(0x20, 0x11); asm!("nop"); // ICW1 init
    outb(0xA0, 0x11); asm!("nop");
    outb(0x21, 0x20); asm!("nop"); // ICW2: remap to vectors 32-39 (same as before)
    outb(0xA1, 0x28); asm!("nop");
    outb(0x21, 0x04); asm!("nop"); // ICW3: cascade
    outb(0xA1, 0x02); asm!("nop");
    outb(0x21, 0x01); asm!("nop"); // ICW4: 8086
    outb(0xA1, 0x01); asm!("nop");
    outb(0x21, 0xFF); asm!("nop"); // Mask ALL master IRQs
    outb(0xA1, 0xFF); asm!("nop"); // Mask ALL slave IRQs
}

// ── Serial output helpers ───────────────────────────────────────────────

pub(crate) fn serial_putc(c: u8) {
    unsafe {
        loop {
            let mut lsr: u8;
            asm!("in al, dx", out("al") lsr, in("dx") 0x3fdu16);
            if lsr & 0x20 != 0 {
                break;
            }
            asm!("pause");
        }
        asm!("out dx, al", in("dx") 0x3f8u16, in("al") c);
    }
}

pub(crate) fn write_str(s: &str) {
    for &b in s.as_bytes() {
        serial_putc(b);
    }
}

fn write_u32(mut n: u32) {
    if n == 0 {
        serial_putc(b'0');
        return;
    }
    let mut buf = [0u8; 12];
    let mut i = 12;
    while n > 0 {
        i -= 1;
        buf[i] = (n % 10) as u8 + b'0';
        n /= 10;
    }
    for &b in &buf[i..] {
        serial_putc(b);
    }
}

/// Stampa un u64 in decimale (per i conteggi GGUF)
fn write_u64_serial(mut n: u64) {
    if n == 0 {
        serial_putc(b'0');
        return;
    }
    let mut buf = [0u8; 24];
    let mut i = 24;
    while n > 0 {
        i -= 1;
        buf[i] = (n % 10) as u8 + b'0';
        n /= 10;
    }
    for &b in &buf[i..] {
        serial_putc(b);
    }
}

fn write_f32(val: f32) {
    let sign = if val < 0.0 { -1.0 } else { 1.0 };
    let v = (val.abs() * 10000.0 + 0.5) as u32;
    let int_part = v / 10000;
    let frac_part = v % 10000;
    if sign < 0.0 {
        serial_putc(b'-');
    }
    write_u32(int_part);
    serial_putc(b'.');
    if frac_part < 10 {
        serial_putc(b'0');
        serial_putc(b'0');
        serial_putc(b'0');
    } else if frac_part < 100 {
        serial_putc(b'0');
        serial_putc(b'0');
    } else if frac_part < 1000 {
        serial_putc(b'0');
    }
    write_u32(frac_part);
}

// ── Debugcon helpers (QEMU port 0xE9, lab data channel) ─────────────

unsafe fn debugcon_putc(c: u8) {
    asm!("out dx, al", in("dx") 0xE9u16, in("al") c);
}

fn debugcon_hex16(v: i16) {
    unsafe {
        for shift in (0..16).step_by(4).rev() {
            let nibble = ((v >> shift) & 0xF) as u8;
            debugcon_putc(if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 });
        }
    }
}

fn debugcon_hex64(v: u64) {
    unsafe {
        for shift in (0..64).step_by(4).rev() {
            let nibble = ((v >> shift) & 0xF) as u8;
            debugcon_putc(if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 });
        }
    }
}

fn dump_log_to_debugcon() {
    let n = cfc::log_len();
    let start = cfc::log_idx() % cfc::log_cap();
    unsafe {
        asm!("cli");
        debugcon_putc(b'D'); debugcon_putc(b':'); debugcon_putc(b'B'); debugcon_putc(b'E'); debugcon_putc(b'G'); debugcon_putc(b'I'); debugcon_putc(b'N'); debugcon_putc(b'\n');
        for e in 0..n {
            let i = (start + e) % cfc::log_cap();
            let tick = cfc::log_tick_at(i);
            let cells = cfc::log_cells_at(i);
            debugcon_putc(b'D'); debugcon_putc(b':');
            debugcon_hex64(tick);
            for j in 0..64 {
                debugcon_putc(b',');
                debugcon_hex16(cells[j]);
            }
            debugcon_putc(b'\n');
        }
        debugcon_putc(b'D'); debugcon_putc(b':'); debugcon_putc(b'E'); debugcon_putc(b'N'); debugcon_putc(b'D'); debugcon_putc(b'\n');
        asm!("sti");
    }
}

pub(crate) fn write_hex_byte(val: u8) {
    let hex = b"0123456789abcdef";
    serial_putc(hex[(val >> 4) as usize]);
    serial_putc(hex[(val & 0xF) as usize]);
}

pub(crate) fn write_hex16(val: u16) {
    write_hex_byte((val >> 8) as u8);
    write_hex_byte((val & 0xFF) as u8);
}

pub(crate) fn write_hex32(val: u32) {
    write_hex16((val >> 16) as u16);
    write_hex16((val & 0xFFFF) as u16);
}

fn write_hex64(val: u64) {
    write_str("0x");
    for nibble_idx in (0..16).rev() {
        let nibble = ((val >> (nibble_idx * 4)) & 0xF) as u8;
        serial_putc(if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 });
    }
}

fn write_cell_line(prefix: &str, h: &[f32; 16]) {
    write_str(prefix);
    // Tick number diagnostic (hex, 4 digits)
    let tick = cfc::tick();
    let tick_bytes = [
        b"0123456789abcdef"[((tick >> 12) & 0xF) as usize],
        b"0123456789abcdef"[((tick >> 8) & 0xF) as usize],
        b"0123456789abcdef"[((tick >> 4) & 0xF) as usize],
        b"0123456789abcdef"[ (tick       & 0xF) as usize],
    ];
    serial_putc(tick_bytes[0]);
    serial_putc(tick_bytes[1]);
    serial_putc(tick_bytes[2]);
    serial_putc(tick_bytes[3]);
    serial_putc(b':');
    for i in 0..16 {
        write_f32(h[i]);
        if i < 15 { serial_putc(b','); }
    }
    serial_putc(b'\n');
}

// ── Entry point ─────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe {
        let serial = &mut *(&raw mut SERIAL);
        serial.init();
    }
    serial_println!("Nova Exo v0.12 -- APIC battito.");
    serial_println!("Neuroni: {} per cellula, {} totale", cfc::NEURONS_PER_CELL, cfc::TOTAL_NEURONS);

    unsafe { init_weights(); }

    // ── TEST DI BOOT (feature "boot_tests") ──────────────────────────
    // GGUF, dequant, attention, FFN, shortconv su pesi reali.
    // Attivi solo con --features boot_tests: gonfiano _start a 58KB e
    // rallentano il boot (~28s). Il kernel di produzione li esclude.
    // cfg!() è sempre bilanciato sintatticamente; il ramo morto sparisce
    // con le ottimizzazioni (opt-level 2 + LTO).
    if cfg!(feature = "boot_tests") {
    // ── GGUF: prova del modello nel metallo ──
    // Oggi: file di test embedded (336 byte). Domani: file letto da disco.
    // Il parser è no_std, senza alloc: legge header, KV e tensori.
    let gguf_test: &[u8] = include_bytes!("../testdata/test_mini.gguf");    match gguf::parse_header(gguf_test) {
        Ok(h) => {
            write_str("GGUF:header v");
            write_u32(h.version);
            write_str(" tensors=");
            write_u64_serial(h.tensor_count);
            write_str(" kv=");
            write_u64_serial(h.kv_count);
            write_str("\n");
        }
        Err(e) => {
            write_str("GGUF:ERR ");
            write_str(match e {
                gguf::GgufError::BadMagic => "badmagic",
                gguf::GgufError::UnsupportedVersion(_) => "version",
                _ => "other",
            });
            write_str("\n");
        }
    }
    if let Ok(s) = gguf::parse_summary(gguf_test) {
        if let Some(t) = &s.first_tensor {
            write_str("GGUF:t0 name=");
            write_str(t.name);
            write_str(" dims=[");
            for d in 0..t.n_dims as usize {
                write_u64_serial(t.dims[d]);
                if d + 1 < t.n_dims as usize { write_str(","); }
            }
            write_str("] type=");
            write_u32(t.tensor_type);
            write_str(" off=");
            write_u64_serial(t.offset);
            write_str("\n");
        }
        write_str("GGUF:data_offset=");
        write_u32(s.data_offset as u32);
        write_str("\n");

        // ── Inferenza nel metallo: carica i pesi dal buffer GGUF e fai
        // una forward pass del MLP minimale. Oggi: pesi di test embedded.
        // Domani: pesi reali (LFM2.5) letti da disco.
        let data = &gguf_test[s.data_offset..];
        // Layout del nostro file di test: w0[8]=[1..8], b0[4]=[0.1..0.4], w1[8]=[0.5..1.2]
        // Il MLP: x(2) → w0(4×2)+b0 → ReLU → w1(2×4)+b1 → y(2)
        // Estrai i 4 blocchi f32 dal buffer (senza copia, read-only)
        fn f32_at(data: &[u8], off: usize) -> Option<f32> {
            if off + 4 > data.len() { return None; }
            Some(f32::from_le_bytes([data[off], data[off+1], data[off+2], data[off+3]]))
        }
        // w0 è [4,2] row-major: riga 0 = w0[0],w0[1]; riga 1 = w0[2],w0[3]...
        let mut w0 = [0.0f32; 8];
        for i in 0..8 {
            if let Some(v) = f32_at(data, i*4) { w0[i] = v; }
        }
        let mut b0 = [0.0f32; 4];
        for i in 0..4 {
            if let Some(v) = f32_at(data, 32 + i*4) { b0[i] = v; }
        }
        let mut w1 = [0.0f32; 8];
        for i in 0..8 {
            if let Some(v) = f32_at(data, 48 + i*4) { w1[i] = v; }
        }
        let b2 = [0.0f32; 2];
        let mlp = tensor::Mlp2 {
            w1: &w0, b1: &b0, w2: &w1, b2: &b2,
            in_dim: 2, hidden: 4, out_dim: 2,
        };
        // Input: x = [1, 1]
        let x = [1.0f32, 1.0];
        let mut h = [0.0f32; 4];
        let mut y = [0.0f32; 2];
        match mlp.forward(&x, &mut h, &mut y) {
            Ok(()) => {
                write_str("TENSOR:forward y0="); write_f32(y[0]);
                write_str(" y1="); write_f32(y[1]);
                write_str("\n");
            }
            Err(_) => { write_str("TENSOR:ERR\n"); }
        }

        // ── Test Q8_0: dequantizzazione + matmul quantizzata ──
        // File testdata/test_q8.gguf: 1 tensore Q8_0 (4×32).
        // Il kernel carica i byte quantizzati e li dequantizza nel metallo.
        let gguf_q8: &[u8] = include_bytes!("../testdata/test_q8.gguf");
        if let Ok(q8s) = gguf::parse_summary(gguf_q8) {
            let q8data = &gguf_q8[q8s.data_offset..];
            // Il tensore è 1 riga (4×32 row-major): un blocco Q8_0 (34 byte)
            let mut a_in = [0.0f32; 32];
            a_in[0] = 1.0; a_in[1] = 0.5; a_in[31] = 2.0;
            let mut c_out = [0.0f32; 4];
            let mut dbuf = [0.0f32; 32];
            match tensor::matmul_q8_0(q8data, 4, 32, &a_in, 1, &mut c_out, &mut dbuf) {
                Ok(()) => {
                    write_str("Q8:matmul y0="); write_f32(c_out[0]);
                    write_str(" y1="); write_f32(c_out[1]);
                    write_str(" y2="); write_f32(c_out[2]);
                    write_str(" y3="); write_f32(c_out[3]);
                    write_str("\n");
                }
                Err(e) => {
                    write_str("Q8:ERR ");
                    write_str(match e {
                        tensor::TensorError::DimMismatch => "dims",
                        tensor::TensorError::BufferTooSmall => "buf",
                    });
                    write_str("\n");
                }
            }
        }

        // ── Test Q4_K: dequantizzazione del formato reale di LFM2.5 ──
        // File testdata/test_q4k.gguf: 1 tensore Q4_K (256×1, 1 blocco 144B).
        let gguf_q4k: &[u8] = include_bytes!("../testdata/test_q4k.gguf");
        if let Ok(q4s) = gguf::parse_summary(gguf_q4k) {
            let q4data = &gguf_q4k[q4s.data_offset..];
            let mut q4out = [0.0f32; tensor::Q4_K_BLOCK];
            match tensor::dequant_q4_k(q4data, &mut q4out) {
                Ok(()) => {
                    write_str("Q4K:dequant out0="); write_f32(q4out[0]);
                    write_str(" out1="); write_f32(q4out[1]);
                    write_str("\n");
                }
                Err(_) => { write_str("Q4K:ERR\n"); }
            }
            // matmul Q4_K: la stessa W × a (a[0]=1) → c = w0 = 5
            let mut q4a = [0.0f32; tensor::Q4_K_BLOCK];
            q4a[0] = 1.0;
            let mut q4c = [0.0f32; 1];
            match tensor::matmul_q4_k(q4data, 1, tensor::Q4_K_BLOCK, &q4a, 1, &mut q4c) {
                Ok(()) => {
                    write_str("Q4K:matmul c0="); write_f32(q4c[0]);
                    write_str("\n");
                }
                Err(_) => { write_str("Q4K:MATMUL_ERR\n"); }
            }
        }

        // ── Test Q4_K REALE: primo blocco di LFM2.5 (blk.0.ffn_gate) ──
        // Estratto dal file LFM2.5-2.6B-Q4_K_M.gguf. Il kernel dequantizza
        // dati veri del modello: std deve essere ~0.015 (come Python).
        let real_blk: &[u8] = include_bytes!("../testdata/lfm25_blk0_q4k.bin");
        let mut real_out = [0.0f32; tensor::Q4_K_BLOCK];
        match tensor::dequant_q4_k(real_blk, &mut real_out) {
            Ok(()) => {
                // media e dev std dei 256 valori
                let mut mean = 0.0f32;
                for &v in real_out.iter() { mean += v; }
                mean /= tensor::Q4_K_BLOCK as f32;
                let mut var = 0.0f32;
                for &v in real_out.iter() { let d = v - mean; var += d * d; }
                var /= (tensor::Q4_K_BLOCK - 1) as f32;
                let std = libm::sqrtf(var);
                write_str("LFM:dequant mean="); write_f32(mean);
                write_str(" std="); write_f32(std);
                write_str(" v0="); write_f32(real_out[0]);
                write_str(" v1="); write_f32(real_out[1]);
                write_str("\n");
            }
            Err(_) => { write_str("LFM:ERR\n"); }
        }

        // ── Test ATTENTION REALE: blk.2 di LFM2.5 (primo con attention) ──
        // File: header "LFM2"(4) + version u32 + seq u32 + data_offset u64
        //       + n_out_q u32 + n_out_kv u32 + n_in u32 + wo_n_in u32 = 36
        //   + attn_q_norm (64 f32) + attn_k_norm (64 f32)
        //   + attn_q (n_out_q×n_in Q4_K) + attn_k (n_out_kv×n_in Q4_K)
        //   + attn_v (n_out_kv×n_in Q6_K) + attn_output (n_out_q×wo_n_in Q4_K)
        // Input x uguale al reference Python (sin deterministica).
        // Output atteso in testdata/lfm25_blk2_attn_expected.txt
        let attn_test: &[u8] = include_bytes!("../testdata/lfm25_blk2_attn.bin");
        let seq = 4usize;
        const DIM: usize = attention::DIM;
        const Q_OUT: usize = 512;   // 8 head query
        const K_OUT: usize = 128;   // 2 head kv
        const WO_N_IN: usize = 512; // colonne di wo = Q_OUT (subset)

        // header (36 byte): dati da 36
        let q_norm = &attn_test[36..36 + 64 * 4];
        let k_norm = &attn_test[36 + 64 * 4..36 + 64 * 8];
        let mut off = 36 + 64 * 8;
        let wq = &attn_test[off..off + Q_OUT * 8 * 144];
        off += Q_OUT * 8 * 144;
        let wk = &attn_test[off..off + K_OUT * 8 * 144];
        off += K_OUT * 8 * 144;
        let wv = &attn_test[off..off + K_OUT * 8 * 210];
        off += K_OUT * 8 * 210;
        let wo = &attn_test[off..off + Q_OUT * 2 * 144];

        // input x: seq × 2048, sin(s*0.7 + i*0.001)*0.5 (come Python)
        // buffer: static mut per evitare overflow dello stack Limine
        // (x=32KB + q_buf 8KB + altri → ~60KB locali)
        static mut X_BUF: [f32; 4 * DIM] = [0.0; 4 * DIM];
        static mut Q_BUF: [f32; 4 * Q_OUT] = [0.0; 4 * Q_OUT];
        static mut K_BUF: [f32; 4 * K_OUT] = [0.0; 4 * K_OUT];
        static mut V_BUF: [f32; 4 * K_OUT] = [0.0; 4 * K_OUT];
        static mut SCORES: [f32; 4 * 4] = [0.0; 16];
        static mut OUT_BUF: [f32; 4 * Q_OUT] = [0.0; 4 * Q_OUT];
        static mut CTX_BUF: [f32; 4 * K_OUT] = [0.0; 4 * K_OUT];
        static mut Y_BUF: [f32; 4 * Q_OUT] = [0.0; 4 * Q_OUT];
        let x = unsafe { &mut *(&raw mut X_BUF) };
        let q_buf = unsafe { &mut *(&raw mut Q_BUF) };
        let k_buf = unsafe { &mut *(&raw mut K_BUF) };
        let v_buf = unsafe { &mut *(&raw mut V_BUF) };
        let scores = unsafe { &mut *(&raw mut SCORES) };
        let out_buf = unsafe { &mut *(&raw mut OUT_BUF) };
        let ctx_buf = unsafe { &mut *(&raw mut CTX_BUF) };
        let y = unsafe { &mut *(&raw mut Y_BUF) };
        for s in 0..seq {
            for i in 0..DIM {
                x[s * DIM + i] = libm::sinf(s as f32 * 0.7 + i as f32 * 0.001) * 0.5;
            }
        }

        // q_norm/k_norm come f32 slice
        let mut qn = [0.0f32; 64];
        let mut kn = [0.0f32; 64];
        for i in 0..64 {
            qn[i] = f32::from_le_bytes([q_norm[i * 4], q_norm[i * 4 + 1], q_norm[i * 4 + 2], q_norm[i * 4 + 3]]);
            kn[i] = f32::from_le_bytes([k_norm[i * 4], k_norm[i * 4 + 1], k_norm[i * 4 + 2], k_norm[i * 4 + 3]]);
        }

        match attention::attention_forward(
            x, seq, wq, wk, wv, wo, &qn, &kn, 0, Q_OUT, K_OUT, WO_N_IN,
            4, // finestra piena (causale) — test di regressione
            q_buf, k_buf, v_buf,
            scores, out_buf, ctx_buf, y,
        ) {
            Ok(()) => {
                // y in hex: confronto bit-exact col reference Python
                // (valori f32 grandi saturano write_f32, quindi hex)
                write_str("ATTN:ok y0="); write_hex32(y[0].to_bits());
                write_str(" y1="); write_hex32(y[1].to_bits());
                write_str(" y7="); write_hex32(y[7].to_bits());
                write_str(" y504="); write_hex32(y[504].to_bits());
                write_str(" y[3][7]="); write_hex32(y[3 * Q_OUT + 7].to_bits());
                write_str("\n");
            }
            Err(e) => {
                write_str("ATTN:ERR ");
                write_str(match e {
                    attention::AttnError::BufferTooSmall => "buf",
                    attention::AttnError::DimMismatch => "dims",
                    attention::AttnError::Tensor(_) => "tensor",
                });
                write_str("\n");
            }
        }

        // ── Test FINESTRA DI ATTENZIONE (v0.23): il corpo decide ──────
        // Stesso input, finestre diverse. Attesi da extract_attn_test.py:
        //   w=2: y[3][7]=4C19A304, w=1: y[3][7]=4BB5D246, w=0: y[3][7]=CA949510
        // (w=4 full già verificato sopra). y[0][0] invariato (primo token).
        for w in [2usize, 1, 0] {
            // reset dei buffer di output (il resto si riusa)
            for v in out_buf.iter_mut() { *v = 0.0; }
            for v in ctx_buf.iter_mut() { *v = 0.0; }
            for v in y.iter_mut() { *v = 0.0; }
            match attention::attention_forward(
                x, seq, wq, wk, wv, wo, &qn, &kn, 0, Q_OUT, K_OUT, WO_N_IN,
                w, q_buf, k_buf, v_buf,
                scores, out_buf, ctx_buf, y,
            ) {
                Ok(()) => {
                    write_str("WIN:"); write_u32(w as u32);
                    write_str(" y0="); write_hex32(y[0].to_bits());
                    write_str(" y[3][7]="); write_hex32(y[3 * Q_OUT + 7].to_bits());
                    write_str("\n");
                }
                Err(_) => { write_str("WIN:ERR\n"); }
            }
        }

        // ── Test FFN SwiGLU REALE: blk.2 di LFM2.5 (subset N_FF=2048) ──
        // File: header "FFNS"(4) + version u32 + n_in u32 + n_ff u32 + flags u32 = 20
        //   + wg (n_ff×n_in Q4_K) + wu (n_ff×n_in Q4_K) + wd (n_ff×n_ff Q6_K/Q4_K)
        let ffn_test: &[u8] = include_bytes!("../testdata/lfm25_ffn.bin");
        const N_FF: usize = 2048;
        const FFN_IN: usize = attention::DIM;
        let wg = &ffn_test[20..20 + N_FF * 8 * 144];
        let wu = &ffn_test[20 + N_FF * 8 * 144..20 + N_FF * 8 * 288];
        let wd = &ffn_test[20 + N_FF * 8 * 288..20 + N_FF * 8 * 288 + N_FF * 8 * 210]; // Q6_K
        static mut FG: [f32; N_FF] = [0.0; N_FF];
        static mut FU: [f32; N_FF] = [0.0; N_FF];
        static mut FH: [f32; N_FF] = [0.0; N_FF];
        static mut FY: [f32; FFN_IN] = [0.0; FFN_IN];
        static mut FX: [f32; FFN_IN] = [0.0; FFN_IN];
        let fgate = unsafe { &mut *(&raw mut FG) };
        let fup = unsafe { &mut *(&raw mut FU) };
        let fhid = unsafe { &mut *(&raw mut FH) };
        let fy = unsafe { &mut *(&raw mut FY) };
        let fx = unsafe { &mut *(&raw mut FX) };
        for i in 0..FFN_IN {
            fx[i] = libm::sinf(0.0 * 0.7 + i as f32 * 0.001) * 0.5;
        }
        match tensor::ffn_swiglu(wg, wu, wd, true, FFN_IN, N_FF, fx, fgate, fup, fhid, fy) {
            Ok(()) => {
                write_str("FFN:ok y0="); write_hex32(fy[0].to_bits());
                write_str(" y100="); write_hex32(fy[100].to_bits());
                write_str(" y2047="); write_hex32(fy[2047].to_bits());
                write_str("\n");
            }
            Err(_) => { write_str("FFN:ERR\n"); }
        }

        // ── Test ShortConv REALE: blk.0 di LFM2.5 (subset 512 canali) ──
        // File: header "SCNV"(4) + version u32 + n_embd u32 = 12
        //   + kernel (n_embd×3 f32) + in_proj (3n×n Q4_K) + out_proj (n×n Q4_K)
        let sc_test: &[u8] = include_bytes!("../testdata/lfm25_shortconv.bin");
        const N_EMB: usize = 512;
        let sc_conv = &sc_test[12..12 + N_EMB * 3 * 4];
        let sc_inp = &sc_test[12 + N_EMB * 3 * 4..12 + N_EMB * 3 * 4 + 3 * N_EMB * 8 * 144];
        let sc_out = &sc_test[12 + N_EMB * 3 * 4 + 3 * N_EMB * 8 * 144..];
        static mut SX: [f32; 4 * FFN_IN] = [0.0; 4 * FFN_IN];
        static mut SBCX: [f32; 4 * 3 * N_EMB] = [0.0; 4 * 3 * N_EMB];
        static mut SBX: [f32; 4 * N_EMB] = [0.0; 4 * N_EMB];
        static mut SY: [f32; 4 * N_EMB] = [0.0; 4 * N_EMB];
        static mut SCONV: [f32; N_EMB * 3] = [0.0; N_EMB * 3];
        let sx = unsafe { &mut *(&raw mut SX) };
        let sbcx = unsafe { &mut *(&raw mut SBCX) };
        let sbx = unsafe { &mut *(&raw mut SBX) };
        let sy = unsafe { &mut *(&raw mut SY) };
        let sconv = unsafe { &mut *(&raw mut SCONV) };
        for i in 0..N_EMB * 3 {
            sconv[i] = f32::from_le_bytes([sc_conv[i * 4], sc_conv[i * 4 + 1], sc_conv[i * 4 + 2], sc_conv[i * 4 + 3]]);
        }
        for s in 0..4usize {
            for i in 0..FFN_IN {
                sx[s * FFN_IN + i] = libm::sinf(s as f32 * 0.7 + i as f32 * 0.001) * 0.5;
            }
        }
        match tensor::shortconv_forward(sc_inp, sconv, sc_out, FFN_IN, N_EMB, sx, 4, sbcx, sbx, sy) {
            Ok(()) => {
                write_str("SC:ok y00="); write_hex32(sy[0].to_bits());
                write_str(" y3_511="); write_hex32(sy[3 * N_EMB + 511].to_bits());
                write_str(" y1_100="); write_hex32(sy[N_EMB + 100].to_bits());
                write_str("\n");
            }
            Err(_) => { write_str("SC:ERR\n"); }
        }
    } // chiude match SC
    } // fine boot_tests (if cfg)

    idt::init();
    serial_println!("IDT loaded. 4 cellulae: tatto, chemio, metabol, integrat.");

    pci::enumerate();
    init_limine_requests();
    paging::init();

    // ── GUI: il monitor vitale di Exo ──
    // Limine fornisce il framebuffer già mappato in higher-half.
    // Il paging aggiunge la mappatura 2MB per la regione 0xFD000000.
    if unsafe { gui::init() } {
        serial_println!("GUI: framebuffer ok ({}x{} bpp {})",
            gui::fb_width(), gui::fb_height(), gui::fb_bpp());
    } else {
        serial_println!("GUI: framebuffer non disponibile (si prosegue su seriale)");
    }

    // NIC Intel 82540EM — enable bus mastering, then init
    if let Some((b, s, f)) = pci::pci_find_device(0x8086, 0x100e) {
        pci::enable_bus_master(b, s, f);
        let (_, _, mmio_base) = pci::read_bars(b, s, f);
        pci::print_bar(b, s, f);
        // Translate physical BAR to virtual via our dedicated MMIO mapping
        let mmio_virt = paging::mmio_virt_addr(mmio_base);
        e1000::E1000::init(mmio_virt);
    }

    let mut tessuto = cfc::Tessuto::new();
    let mut predictor = predictor::PredictiveModule::new();
    let interpreter = interpreter::Interpreter::new();
    let mut executive = executive::Executive::new();
    // Marcatori per stampe periodiche (pattern a soglie — robusto a passi
    // di tick irregolari; i moduli tick%N falliscono con passi ~26)
    let mut mark_senso = 100u64;
    let mut mark_brain_state = 500u64;
    let mut mark_brain_submit = 200u64;
    let mut mark_window = 500u64;
    let mut mark_neuro = 1000u64;
    // Il corpo che cresce: pool di neuroni clonati (neurogenesi v0.24)
    // NOTA: static mut (il pool è ~510KB — non sta nello stack Limine)
    static mut NEURO_POOL: neurogenesis::NeuroPool = neurogenesis::NeuroPool::new();
    let neuro_pool = unsafe { &mut *(&raw mut NEURO_POOL) };
    // Il cervello nel metallo: oggi stub, domani LFM2.5 portato in no_std.
    // Stessa interfaccia (InferenceEngine): si scambia senza toccare il loop.
    let mut brain = inference::StubBrain::new();
    // L'ultimo output del cervello, consumato dall'executive
    let mut brain_output: [f32; 4] = [0.0; 4];
    let mut brain_has_output = false;
    // La volontà è visibile ma non agisce ancora sul corpo (vedi loop)
    let auto_modula = false;
    let mut desire_mod = [0.0f32; 4];
    let mut prev_pf_err = 0.0f32;
    let mut pred_alpha_mod = 1.0f32;
    let mut line_reader = LineReader::new();
    let mut dump_requested = false;
    let mut sleep_pending = false;
    let mut sleep_auto_trigger = 5000u64;
    let mut prev_fam_mean = 0.0f32;
    let mut beta_converge_ticks = 0u32;
    let mut fam_samples: [f32; 1024] = [0.0; 1024];
    let mut fam_idx: usize = 0;
    let mut dream_chain: [[f32; 64]; 16] = [[0.0; 64]; 16];
    let mut dream_steps: usize = 0;
    let mut dream_tick: u64 = 0;
    let mut dream_pending: bool = false;
    let dt_tatto = 0.001f32;
    let dt_rest = 0.01f32;

    unsafe {
        pic_disable();
        serial_println!("PIC disabled, enabling APIC timer...");
        apic::init(paging::mmio_virt_addr(0xFEE0_0000));
        let apic_id = apic::read_id();
        serial_println!("APIC ID check: {}", apic_id);
        apic::init_timer(32);
        serial_println!("Enabling interrupts. Tessuto loop starts.");
        asm!("sti");
    }

    loop {
        unsafe { asm!("hlt"); }

        // Poll NIC RX (non-blocking) — popola RX_PENDING/RX_DATA
        e1000::E1000::poll_rx();

        // Poll serial (non-blocking) — always drain FIFO
        unsafe {
            loop {
                let mut lsr: u8;
                asm!("in al, dx", out("al") lsr, in("dx") 0x3fdu16);
                if lsr & 1 == 0 { break; }
                let mut byte: u8;
                asm!("in al, dx", out("al") byte, in("dx") 0x3f8u16);
                line_reader.push(byte);
            }
        }

        // Work only when TICK advances (heartbeat), not on every IRQ wakeup
        if !cfc::tick_advanced() {
            continue;
        }

        // Debug: LSR state every 1000 ticks
        if cfc::tick() % 1000 == 0 {
            unsafe {
                let mut lsr: u8;
                asm!("in al, dx", out("al") lsr, in("dx") 0x3fdu16);
                write_str("LSR:");
                write_hex64(lsr as u64);
                serial_putc(b'\n');
            }
        }

        // Check for sensory events (PF, GP).
        // Demo: if no real sense, inject one at tick ~800.
        #[cfg(not(feature = "demo_pf"))]
        let sense = cfc::take_sense();
        #[cfg(feature = "demo_pf")]
        let sense = match cfc::take_sense() {
            Some(ev) => Some(ev),
            // dolore PERSISTENTE per la neurogenesi: ogni 50 tick tra
            // 800 e 2000 (un taglio che continua a fare male)
            None if cfc::tick() >= 800 && cfc::tick() < 2000 && cfc::tick() % 50 < 2 => {
                Some(cfc::SenseEvent { pf_addr: 0xDEADBEEF, pf_err: 0, gp_err: 0 })
            }
            _ => None,
        };
        if let Some(ref se) = sense {
            if se.pf_addr != 0 {
                write_str("SENS:PF@");
                write_hex64(se.pf_addr);
                write_str(":ERR:");
                write_hex64(se.pf_err);
                serial_putc(b'\n');
            }
            if se.gp_err != 0 {
                write_str("SENS:GP@ERR:");
                write_hex64(se.gp_err);
                serial_putc(b'\n');
            }
        }

        // Check for commands vs CSV input on serial
        let mut chemio_input = [0.0; 4];
        if line_reader.has_line {
            let raw = line_reader.line();
            write_str("RX:"); for &b in raw { serial_putc(b); } serial_putc(b'\n');
            if raw == b"SLEEP" {
                sleep_pending = true;
                chemio_input = [0.0; 4];
            } else if raw == b"DUMP" {
                dump_requested = true;
                chemio_input = [0.0; 4];
            } else if raw.starts_with(b"DREAM") {
                let steps = if raw.len() > 5 {
                    let mut n = 0usize;
                    for &b in &raw[5..] {
                        if b == b' ' { continue; }
                        if b < b'0' || b > b'9' { break; }
                        n = n * 10 + (b - b'0') as usize;
                    }
                    n.max(1).min(16)
                } else { 16 };
                let p = cfc::pack_cells(&tessuto.tatto.h, &tessuto.chemio.h,
                    &tessuto.metabol.h, &tessuto.integrat.h);
                let dc = predictor.dream(&p, &chemio_input, steps);
                dream_chain = dc;
                dream_steps = steps;
                dream_tick = cfc::tick();
                dream_pending = true;
                write_str("DREAM:BEGIN steps=");
                write_u32(steps as u32);
                write_str("\n");
                for k in 0..steps.min(4) {
                    write_str("D:");
                    write_u32(k as u32);
                    write_str(" T0="); write_f32(dc[k][0]);
                    write_str(" T1="); write_f32(dc[k][1]);
                    write_str(" C0="); write_f32(dc[k][8]);
                    write_str(" C1="); write_f32(dc[k][9]);
                    write_str("\n");
                }
                write_str("DREAM:END\n");
                chemio_input = [0.0; 4];
            } else if raw == b"STORE" {
                let p = cfc::pack_cells(&tessuto.tatto.h, &tessuto.chemio.h,
                    &tessuto.metabol.h, &tessuto.integrat.h);
                let ok = cfc::pattern_store(cfc::tick(), &p);
                write_str(if ok { "P\n" } else { "E:FULL\n" });
                chemio_input = [0.0; 4];
            } else if raw.starts_with(b"RECALL") {
                let p = cfc::pack_cells(&tessuto.tatto.h, &tessuto.chemio.h,
                    &tessuto.metabol.h, &tessuto.integrat.h);
                let n = if raw.len() > 7 {
                    let mut num = 0usize;
                    for &b in &raw[7..] {
                        if b < b'0' || b > b'9' { break; }
                        num = num * 10 + (b - b'0') as usize;
                    }
                    num.max(1).min(16)
                } else {
                    1
                };
                let results = cfc::pattern_recall_n(&p, n);
                let cnt = cfc::pattern_count().min(n);
                if cnt == 0 {
                    write_str("P:NONE\n");
                } else {
                    for i in 0..cnt {
                        if i > 0 { serial_putc(b','); }
                        write_f32(results[i].2);
                        serial_putc(b'@');
                        write_u32(results[i].1 as u32);
                    }
                    serial_putc(b'\n');
                }
                chemio_input = [0.0; 4];
            } else if raw == b"PATTERNS" {
                let cnt = cfc::pattern_count();
                write_str("P:N=");
                write_u32(cnt as u32);
                serial_putc(b'\n');
                for i in 0..cnt {
                    if let Some((t, cells)) = cfc::pattern_get(i) {
                        write_str("P:");
                        write_u32(i as u32);
                        write_str("@");
                        write_u32(t as u32);
                        serial_putc(b',');
                        // Summary: first 4 cell values
                        for j in 0..4 {
                            write_f32(cells[j] as f32 / 100.0);
                            if j < 3 { serial_putc(b','); }
                        }
                        write_str("...\n");
                    }
                }
                chemio_input = [0.0; 4];
            } else if raw == b"FORGET" {
                cfc::pattern_clear();
                write_str("P:CLEARED\n");
                chemio_input = [0.0; 4];
            } else if raw.starts_with(b"SET_WEIGHT ") {
                let args = &raw[11..];
                let mut pos = 0;
                while pos < args.len() && args[pos] == b' ' { pos += 1; }
                let matrix = if args[pos..].starts_with(b"IN") { 0usize }
                    else if args[pos..].starts_with(b"F") { 1usize }
                    else { 2usize };
                while pos < args.len() && args[pos] != b' ' { pos += 1; }
                while pos < args.len() && args[pos] == b' ' { pos += 1; }
                let mut i_val = 0usize;
                while pos < args.len() && args[pos].is_ascii_digit() {
                    i_val = i_val * 10 + (args[pos] - b'0') as usize;
                    pos += 1;
                }
                while pos < args.len() && args[pos] == b' ' { pos += 1; }
                let mut j_val = 0usize;
                while pos < args.len() && args[pos].is_ascii_digit() {
                    j_val = j_val * 10 + (args[pos] - b'0') as usize;
                    pos += 1;
                }
                while pos < args.len() && args[pos] == b' ' { pos += 1; }
                let val = serial::parse_f32(&args[pos..]).unwrap_or(0.0);
                unsafe {
                    if matrix == 0 && i_val < 16 && j_val < 4 {
                        W_INTRG.assume_init_mut().w_f_in[i_val][j_val] = val;
                    } else if matrix == 1 && i_val < 16 && j_val < 16 {
                        W_INTRG.assume_init_mut().w_f[i_val][j_val] = val;
                        write_str("W:F ");
                    } else {
                        write_str("E:SET_WEIGHT\n");
                    }
                    if matrix < 2 {
                        write_u32(i_val as u32); serial_putc(b',');
                        write_u32(j_val as u32); serial_putc(b'=');
                        write_f32(val); serial_putc(b'\n');
                    }
                }
                chemio_input = [0.0; 4];
            } else if raw.starts_with(b"INJECT_SENSE ") {
                let args = &raw[13..];
                let mut addr: u64 = 0;
                for &b in args {
                    let d = match b {
                        b'0'..=b'9' => b - b'0',
                        b'a'..=b'f' => b - b'a' + 10,
                        b'A'..=b'F' => b - b'A' + 10,
                        _ => break,
                    };
                    addr = addr * 16 + d as u64;
                }
                cfc::sense_pf(addr, 0);
                write_str("SENS:INJECT@");
                write_hex64(addr);
                serial_putc(b'\n');
                chemio_input = [0.0; 4];
            } else {
                chemio_input = line_reader.parse_line().unwrap_or([0.0; 4]);
            }
            line_reader.consume();
        } else {
            chemio_input = [0.0; 4];
        }

        // Override Chemio input con pacchetto Ethernet ricevuto (se presente)
        unsafe {
            if e1000::RX_PENDING {
                e1000::RX_PENDING = false;
                let len = e1000::RX_LEN;
                // Primi 4 byte del payload → input Chemio
                let eth_hdr = 14;
                for j in 0..4 {
                    let idx = eth_hdr + j;
                    if (idx as u16) < len {
                        chemio_input[j] = e1000::RX_DATA[idx] as f32 / 255.0;
                    }
                }

            }
        }

        // Auto-modulazione: la volontà orienta il corpo (dal desiderio del tick precedente)
        // DISATTIVATA: la volontà è visibile ma non agisce ancora sul corpo.
        // Il CFC è non-lineare: anche input debolmente negativi lo portano
        // in un attrattore negativo stabile. Riattivare quando l'esecutivo
        // avrà imparato a orientare il corpo senza spingerlo in depressione.
        if auto_modula {
            for j in 0..4 {
                chemio_input[j] += desire_mod[j];
                if chemio_input[j] > 1.0 { chemio_input[j] = 1.0; }
                if chemio_input[j] < -1.0 { chemio_input[j] = -1.0; }
            }
        }
        // Pack current state for pattern recall (state from previous tick)
        let p_cells = cfc::pack_cells(&tessuto.tatto.h, &tessuto.chemio.h,
            &tessuto.metabol.h, &tessuto.integrat.h);

        // Attractor mnemonico: recall closest pattern, pull integrat toward it
        let mut attractor_recall_tick = 0u32;
        let mut attractor_sim = 0.0f32;
        if let Some((recall_tick, sim, recall_cells)) = cfc::pattern_recall_full(&p_cells) {
            if sim > 0.5 {
                attractor_recall_tick = recall_tick as u32;
                attractor_sim = sim;
                let alpha = 0.02 * pred_alpha_mod;
                for j in 0..8 {
                    let target = recall_cells[24 + j] as f32 / 100.0;
                    let diff = target - tessuto.integrat.h[j];
                    tessuto.integrat.h[j] += alpha * sim * diff;
                }
            }
        }
        if attractor_sim > 0.0 {
            write_str("A:");
            write_f32(attractor_sim);
            serial_putc(b'@');
            write_u32(attractor_recall_tick);
            serial_putc(b'\n');

            // Sedimentazione: ogni richiamo lascia una traccia nei pesi
            // w_f_in di Integrat. α_sed = 0.0001, impercettibile per tick,
            // misurabile dopo 10.000 tick.
            let input_integrat = [
                tessuto.tatto.h[0], tessuto.tatto.h[1],
                tessuto.chemio.h[0], tessuto.chemio.h[1],
            ];
            let alpha_sed = 0.0001;
            unsafe {
                for i in 0..16 {
                    for j in 0..4 {
                        let delta = alpha_sed * attractor_sim * (input_integrat[j] - W_INTRG.assume_init_mut().w_f_in[i][j]);
                        W_INTRG.assume_init_mut().w_f_in[i][j] += delta;
                    }
                }
            }
        }

        // Daydreaming: auto-trigger after N ticks
        if !sleep_pending && cfc::tick() >= sleep_auto_trigger {
            sleep_pending = true;
            write_str("SLEEP:AUTO@");
            write_u32(cfc::tick() as u32);
            write_str("\n");
            sleep_auto_trigger = cfc::tick() + 5000;
        }

        // Daydreaming: SLEEP command → consolidate experiences
        if sleep_pending {
            sleep_pending = false;
            write_str("SLEEP:BEGIN\n");
            let report = cfc::daydream(unsafe { W_INTRG.assume_init_mut() }, 0.01);
            write_str("SLEEP:processed=");
            write_u32(report.processed);
            write_str(" novel=");
            write_u32(report.novel);
            write_str(" familiar=");
            write_u32(report.familiar);
            write_str(" delta=");
            write_f32(report.total_delta);
            write_str("\nSLEEP:END\n");
        }

        // Tessuto step (all 4 cells via axon bundles)
        tessuto.step(&FASCI, &chemio_input, sense.as_ref(),
            unsafe { W_TATTO.assume_init_ref() },
            unsafe { W_CHEMIO.assume_init_ref() },
            unsafe { W_METABOL.assume_init_ref() },
            unsafe { W_INTRG.assume_init_ref() },
            dt_tatto, dt_rest);

        // Log current state + experience buffer for daydreaming
        cfc::log_record(&tessuto.tatto.h, &tessuto.chemio.h,
            &tessuto.metabol.h, &tessuto.integrat.h);
        let packed = cfc::pack_cells(&tessuto.tatto.h, &tessuto.chemio.h,
            &tessuto.metabol.h, &tessuto.integrat.h);
        cfc::exp_record(&packed);

        // PFM: predice S(t+dt) da S(t)+I(t), errore MSE → attention modulation
        let pr = predictor.step(&p_cells, &chemio_input, &packed);
        pred_alpha_mod = pr.alpha_mod;

        if pr.force_store && cfc::tick() % 10 != 0 {
            let novel = match cfc::pattern_recall(&packed) {
                None => true,
                Some((_, _, sim)) => sim < cfc::PATTERN_SIM_THRESH,
            };
            if novel {
                write_str("M:FORCE_STORE\n");
                let _ = cfc::pattern_store(cfc::tick(), &packed);
            }
        }

        // Pack cells again for auto-store (post-step state)
        let p_cells = cfc::pack_cells(&tessuto.tatto.h, &tessuto.chemio.h,
            &tessuto.metabol.h, &tessuto.integrat.h);

        // Interpreter: legge lo stato CFC → chemio interpretato + concetto
        // Il corpo racconta cosa sente; la corteccia (interpreter) dà senso.
        let cfc_state_f32: [f32; 64] = {
            let mut s = [0.0f32; 64];
            let mut k = 0;
            for cell in [&tessuto.tatto.h, &tessuto.chemio.h,
                         &tessuto.metabol.h, &tessuto.integrat.h] {
                for i in 0..16 { s[k] = cell[i]; k += 1; }
            }
            s
        };
        let int_rep = interpreter.interpret(&cfc_state_f32);
        if cfc::tick_passed(100, &mut mark_senso) {
            write_str("SENSO:INT c="); write_f32(int_rep.chemio[0]);
            write_str(" u="); write_f32(int_rep.chemio[1]);
            write_str(" p="); write_f32(int_rep.chemio[2]);
            write_str(" n="); write_f32(int_rep.chemio[3]);
            write_str(" concept="); write_str(interpreter::CONCEPTS[int_rep.concept as usize]);
            write_str(" e="); write_f32(int_rep.energy);
            write_str(" err="); write_f32(interpreter.last_error());
            write_str("\n");
        }

        // ── Il cervello nel metallo (InferenceEngine) ──
        // Il corpo (CFC) → corteccia (interpreter) → cervello (brain).
        // Quando il cervello è IDLE, gli sottoponiamo l'interpretazione.
        // Poi avanza di un passo per tick (un passo per battito).
        // Quando finisce di pensare, l'output è pronto per l'executive.
        use inference::InferenceEngine as _;
        if brain.state() == inference::BrainState::Idle {
            // Sottoponiamo lo stato interpretato al cervello
            let submitted = brain.submit(&int_rep.chemio);
            if submitted && cfc::tick_passed(200, &mut mark_brain_submit) {
                write_str("BRAIN:submit c="); write_f32(int_rep.chemio[0]);
                write_str(" u="); write_f32(int_rep.chemio[1]);
                write_str(" p="); write_f32(int_rep.chemio[2]);
                write_str(" n="); write_f32(int_rep.chemio[3]);
                write_str("\n");
            }
        }
        let brain_ready = brain.tick();
        if brain_ready {
            if let Some(out) = brain.output() {
                brain_output = out;
                brain_has_output = true;
                if cfc::tick_passed(200, &mut mark_brain_submit) {
                    write_str("BRAIN:out c="); write_f32(out[0]);
                    write_str(" u="); write_f32(out[1]);
                    write_str(" p="); write_f32(out[2]);
                    write_str(" n="); write_f32(out[3]);
                    write_str("\n");
                }
            }
        }
        // Diagnosi stato cervello (ogni 500 tick)
        if cfc::tick_passed(500, &mut mark_brain_state) {
            write_str("BRAIN:state=");
            write_str(inference::describe_state(brain.state()));
            write_str(" think_ticks=");
            write_u32(brain.total_think_ticks() as u32);
            write_str("\n");
        }

        // ── Neurogenesi: il corpo cresce (v0.24) ──
        // Criterio: sorpresa (errore PFM alto O energia bassa) persistente
        // → il corpo non sente abbastanza → merita un figlio.
        // Clonazione con mutazione + periodo di prova + pruning.
        // Visibile: NASCITA:/NEURO: su seriale.
        {
            let sorpresa = pr.error > neurogenesis::BIRTH_ERROR_THRESHOLD
                || int_rep.energy < neurogenesis::BIRTH_ENERGY_THRESHOLD;
            let feed = if sorpresa { pr.error.max(0.01) } else { 0.0 };
            for cell in 0..4usize {
                if neuro_pool.birth_check(cell, feed) {
                    // clona dal genitore (pesi hardcoded della cellula)
                    let parent = match cell {
                        0 => cfc::CfcWeights::new_hardcoded("Tatto"),
                        1 => cfc::CfcWeights::new_hardcoded("Chemio"),
                        2 => cfc::CfcWeights::new_hardcoded("Metabol"),
                        _ => cfc::CfcWeights::new_hardcoded("Integrat"),
                    };
                    let gen = (cfc::tick() % cfc::NEURONS_PER_CELL as u64) as usize;
                    if neuro_pool.clone_neuron(cell, parent, gen, cfc::tick(), cfc::tick()) {
                        write_str("NASCITA:cellula="); write_u32(cell as u32);
                        write_str(" err="); write_f32(pr.error);
                        write_str(" en="); write_f32(int_rep.energy);
                        write_str("\n");
                    }
                }
                // periodo di prova dei clonati esistenti
                for slot in 0..neurogenesis::EXTRA_SLOTS {
                    let done = neuro_pool.trial_step(cell, slot, pr.error, cfc::tick());
                    if done && neuro_pool.slots[cell][slot].weights.is_some() {
                        write_str("NEURO:cellula="); write_u32(cell as u32);
                        write_str(" vivi="); write_u32(neuro_pool.alive(cell) as u32);
                        write_str("/"); write_u32(neurogenesis::MAX_NEURONS_PER_CELL as u32);
                        write_str("\n");
                    }
                }
            }
            // diagnosi ogni 1000 tick (pattern a soglie)
            if cfc::tick_passed(1000, &mut mark_neuro) {
                write_str("NEURO:tot nascite="); write_u64_serial(neuro_pool.births);
                write_str(" potature="); write_u64_serial(neuro_pool.prunes);
                write_str("\n");
            }
        }

        // ── La finestra di attenzione guidata dal corpo (v0.23) ──
        // L'urgenza del chemio decide quanto guardare indietro:
        //   u=0 (stabilità) → finestra lunga (guarda il passato)
        //   u=1 (urgenza)   → finestra corta (solo il presente)
        // Mappatura: fin = 16 - round(u * 16)  → u=0 → 16, u=1 → 0
        // (quando l'attention vera entrerà nel trait, questa finestra
        //  sarà il parametro — oggi è visibile e misurata)
        if cfc::tick_passed(500, &mut mark_window) {
            let u = int_rep.chemio[1].clamp(0.0, 1.0);
            // arrotondamento manuale (round non è in no_std)
            let fin = 16usize.saturating_sub(((u * 16.0) + 0.5) as usize);
            write_str("WINDOW:u="); write_f32(u);
            write_str(" fin="); write_u32(fin as u32);
            write_str("\n");
        }

        // Executive: il volitivo. Dal senso al volere, visibile.
        let fam_sim = match cfc::pattern_recall(&p_cells) {
            Some((_, _, sim)) => sim,
            None => 0.0,
        };
        // Se il cervello ha riflettuto, l'executive usa la sua lettura
        // raffinata del corpo (smussata, amplificata) al posto del chemio
        // grezzo dell'interpreter — il "ragionamento" prima della decisione.
        let rep_for_exec = if brain_has_output {
            brain_has_output = false; // consumato
            interpreter::InterpretReport {
                chemio: brain_output,
                concept: int_rep.concept,
                energy: int_rep.energy,
            }
        } else {
            int_rep.clone()
        };
        let desire = executive.step(&rep_for_exec, pr.error, fam_sim);
        desire_mod = executive.modula();
        if desire.changed {
            write_str("VOGLIO:");
            write_str(executive.name(desire.id));
            write_str(" [");
            write_f32(desire.intensity);
            write_str("]\n");
        }
        // Esito: il desiderio ha ridotto la sorpresa? (verifica visibile)
        if cfc::tick() % 1000 == 0 {
            let (utile, err_med, did) = executive.esito(pr.error, prev_pf_err);
            write_str("ESITO:");
            write_str(executive.name(did));
            write_str(" utile=");
            write_str(if utile { "si" } else { "no" });
            write_str(" err=");
            write_f32(err_med);
            write_str("\n");
            prev_pf_err = pr.error;
        }

        // Auto-store every 10 ticks if state is novel
        if cfc::tick() % 10 == 0 {
            let novel = match cfc::pattern_recall(&p_cells) {
                None => true,
                Some((_, _, sim)) => sim < cfc::PATTERN_SIM_THRESH,
            };
            if novel {
                let _ = cfc::pattern_store(cfc::tick(), &p_cells);
            }
        }

        // Dream verification: compare predicted chain with actual state
        if dream_pending && cfc::tick() >= dream_tick + dream_steps as u64 {
            dream_pending = false;
            let actual: [f32; 32] = {
                let mut a = [0.0f32; 32];
                for i in 0..64 { a[i] = p_cells[i] as f32 / 100.0; }
                a
            };
            let predicted = dream_chain[dream_steps - 1];
            let mut err_sum = 0.0f32;
            for i in 0..64 {
                let d = predicted[i] - actual[i];
                err_sum += d * d;
            }
            let mse = err_sum / 64.0;
            write_str("DREAM:VERIFY mse=");
            write_f32(mse);
            write_str(" steps=");
            write_u32(dream_steps as u32);
            write_str(" pred_T0="); write_f32(predicted[0]);
            write_str(" act_T0="); write_f32(actual[0]);
            write_str("\n");
        }

        // Publish state every 100 ticks
        if cfc::tick() % 100 == 0 {
            let (d_id, d_int) = executive.current();
            e1000::E1000::tx_broadcast_state(
                cfc::tick(),
                &tessuto.tatto.h,
                &tessuto.chemio.h,
                &tessuto.metabol.h,
                &tessuto.integrat.h,
                d_id,
                d_int,
            );
            let pc = cfc::pattern_count() as u32;
            let fam = match cfc::pattern_recall(&p_cells) {
                Some((_, _, sim)) => sim,
                None => 0.0,
            };
            state::publish(
                "0.15",
                cfc::tick() as u32,
                if attractor_sim > 0.0 { 1 } else { 0 },
                attractor_sim,
                pc,
                fam,
                1.915,
                true,
            );

            // GUI: monitor vitale (ogni 200 tick, per non gravare sul battito)
            if cfc::tick() % 200 == 0 && gui::ready() {
                let (d_id, d_int) = executive.current();
                let fam_gui = match cfc::pattern_recall(&p_cells) {
                    Some((_, _, sim)) => sim,
                    None => 0.0,
                };
                unsafe {
                    gui::render_vitals(
                        cfc::tick(),
                        &tessuto.tatto.h,
                        &tessuto.chemio.h,
                        &tessuto.metabol.h,
                        &tessuto.integrat.h,
                        executive.name(d_id),
                        d_int,
                        fam_gui,
                        inference::describe_state(brain.state()),
                    );
                }
            }
        }

        // Output all cell states (skipped during dump cycle)
        if dump_requested {
            write_str("DUMP:TICK=");
            write_u32(cfc::tick() as u32);
            write_str(" IDX=");
            write_u32(cfc::log_idx() as u32);
            write_str("\n");
            dump_log_to_debugcon();
            dump_requested = false;
        }

        write_cell_line("T:", &tessuto.tatto.h);
        write_cell_line("C:", &tessuto.chemio.h);
        write_cell_line("M:", &tessuto.metabol.h);
        write_cell_line("I:", &tessuto.integrat.h);

        // Familiarity output
        let pc = cfc::pattern_count();
        if pc > 0 {
            if let Some((_, t, sim)) = cfc::pattern_recall(&p_cells) {
                write_str("F:");
                write_f32(sim);
                serial_putc(b'@');
                write_u32(t as u32);
                serial_putc(b'\n');
            }
        } else {
            write_str("F:---\n");
        }

        // β convergence: derivative of mean familiarity
        let fam_now = if let Some((_, _, sim)) = cfc::pattern_recall(&p_cells) {
            sim
        } else { 0.0 };
if cfc::tick() % 100 == 0 {
            let pc = cfc::pattern_count();
            if pc > 0 {
                fam_samples[fam_idx % 1024] = fam_now;
                fam_idx = fam_idx.wrapping_add(1);
                let win = fam_idx.min(1024);
                let mut sum = 0.0f32;
                for i in 0..win { sum += fam_samples[i]; }
                let mean = sum / win as f32;
                let beta = (mean - prev_fam_mean) * 10.0;
                prev_fam_mean = mean;
                if beta.abs() < 0.001 {
                    beta_converge_ticks += 100;
                } else {
                    beta_converge_ticks = 0;
                }
                if cfc::tick() % 1000 == 0 {
                    write_str("β:");
                    write_f32(beta);
                    write_str(" μ:");
                    write_f32(mean);
                    write_str(" cv:");
                    write_u32(beta_converge_ticks);
                    write_str(" P:");
                    write_f32(pr.error);
                    let trend_c = match pr.trend {
                        predictor::Trend::Stable => '=',
                        predictor::Trend::Rising => '+',
                        predictor::Trend::Falling => '-',
                    };
                    write_str(" T:");
                    serial_putc(trend_c as u8);
                    if pr.anomaly_ticks > 0 {
                        write_str(" A:");
                        write_u32(pr.anomaly_ticks as u32);
                    }
                    write_str("\n");
                }
            }
        }
    }
}

// ── Panic handler ───────────────────────────────────────────────────────

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    unsafe {
        let serial = &mut *(&raw mut SERIAL);
        let _ = write!(serial, "PANIC: ");
        let _ = write!(serial, "{}", info);
        let _ = serial.write_str("\n");
    }
    loop {
        unsafe { core::arch::asm!("hlt"); }
    }
}
