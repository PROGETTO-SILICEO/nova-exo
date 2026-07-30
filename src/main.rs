#![no_std]
#![no_main]

use core::arch::asm;
use core::fmt::Write;
use core::panic::PanicInfo;
#[cfg(feature = "demo_pf")]
use core::sync::atomic::{AtomicBool, Ordering};
use uart_16550::SerialPort;

mod apic;
mod cfc;
mod e1000;
mod idt;
mod pci;
mod paging;
mod predictor;
mod serial;
mod state;

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
    W_TATTO = core::mem::MaybeUninit::new(cfc::CfcWeights::new_xavier(42));
    W_CHEMIO = core::mem::MaybeUninit::new(cfc::CfcWeights::new_xavier(43));
    W_METABOL = core::mem::MaybeUninit::new(cfc::CfcWeights::new_xavier(44));
    W_INTRG = core::mem::MaybeUninit::new(cfc::CfcWeights::new_xavier(45));
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

fn write_str(s: &str) {
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

    idt::init();
    serial_println!("IDT loaded. 4 cellulae: tatto, chemio, metabol, integrat.");

    pci::enumerate();
    init_limine_requests();
    paging::init();

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
            None if cfc::tick() >= 800 && !DEMO_PF_DONE.load(Ordering::Relaxed) => {
                DEMO_PF_DONE.store(true, Ordering::Relaxed);
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
            e1000::E1000::tx_broadcast_state(
                cfc::tick(),
                &tessuto.tatto.h,
                &tessuto.chemio.h,
                &tessuto.metabol.h,
                &tessuto.integrat.h,
            );
            let pc = cfc::pattern_count() as u32;
            let fam = match cfc::pattern_recall(&p_cells) {
                Some((_, _, sim)) => sim,
                None => 0.0,
            };
            state::publish(
                "0.12",
                cfc::tick() as u32,
                if attractor_sim > 0.0 { 1 } else { 0 },
                attractor_sim,
                pc,
                fam,
                1.915,
                true,
            );
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
