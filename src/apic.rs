use core::arch::asm;

static mut APIC_BASE: *mut u32 = core::ptr::null_mut();

#[allow(dead_code)]
const APIC_ID: usize = 0x020;
#[allow(dead_code)]
const APIC_VERSION: usize = 0x030;
const APIC_TPR: usize = 0x080;
const APIC_EOI: usize = 0x0B0;
const APIC_SPURIOUS: usize = 0x0F0;
const APIC_LVT_TIMER: usize = 0x320;
const APIC_TIMER_INIT_COUNT: usize = 0x380;
#[allow(dead_code)]
const APIC_TIMER_CURRENT_COUNT: usize = 0x390;
const APIC_TIMER_DIVIDE: usize = 0x3E0;

const IA32_APIC_BASE_MSR: u32 = 0x1B;

fn read_reg(offset: usize) -> u32 {
    unsafe { core::ptr::read_volatile(APIC_BASE.add(offset / 4)) }
}

fn write_reg(offset: usize, val: u32) {
    unsafe { core::ptr::write_volatile(APIC_BASE.add(offset / 4), val) }
}

pub fn eoi() {
    write_reg(APIC_EOI, 0);
}

pub fn init(mmio_base: u64) {
    unsafe {
        APIC_BASE = mmio_base as *mut u32;

        let mut apic_base: u64;
        asm!("rdmsr", out("eax") apic_base, out("edx") _, in("ecx") IA32_APIC_BASE_MSR, options(nostack));
        crate::gui::phase_steps(1); // rdmsr ok
        apic_base |= 1 << 11;
        let low = apic_base as u32;
        let high = (apic_base >> 32) as u32;
        asm!("wrmsr", in("eax") low, in("edx") high, in("ecx") IA32_APIC_BASE_MSR, options(nostack));
        crate::gui::phase_steps(2); // wrmsr ok

        let spurious = read_reg(APIC_SPURIOUS);
        crate::gui::phase_steps(3); // spurious read ok
        write_reg(APIC_SPURIOUS, spurious | (1 << 8) | 0xFF);
        crate::gui::phase_steps(4); // spurious write ok

        write_reg(APIC_TPR, 0);
        crate::gui::phase_steps(5); // TPR ok

        write_reg(0x330, 1 << 16);
        write_reg(0x340, 1 << 16);
        write_reg(0x370, 1 << 16);
        crate::gui::phase_steps(6); // LVT ok
    }
}

pub fn init_timer(vector: u8) {
    write_reg(APIC_TIMER_DIVIDE, 0x3);
    write_reg(APIC_LVT_TIMER, (1 << 17) | vector as u32);
    write_reg(APIC_TIMER_INIT_COUNT, 62_500);
}

pub fn read_id() -> u32 {
    read_reg(APIC_ID)
}

// ── Timer PIT + IO-APIC (v0.27) ───────────────────────────────────────
// Il timer LAPIC su AMD Kabini (Lenovo G50) non genera interrupt: Linux
// lo evita (usa PIT via IO-APIC, vedi /proc/interrupts). Sostituiamo la
// sorgente del tick con il PIT 8254 (universale) inoltrato dall'IO-APIC
// al LAPIC sul vector 32. L'handler IDT resta identico (inc_tick + EOI).

const PIT_CMD: u16 = 0x43;
const PIT_CH0: u16 = 0x40;
/// Divisore per ~100 Hz (10 ms/tick): 1193182 / 100 ≈ 11932 = 0x2E9C
const PIT_DIVISOR: u16 = 11932;

static mut IOAPIC_BASE: *mut u32 = core::ptr::null_mut();

fn outb(port: u16, val: u8) {
    unsafe { core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nostack)); }
}

fn ioapic_write(reg: u8, val: u32) {
    unsafe {
        core::ptr::write_volatile(IOAPIC_BASE, reg as u32);
        core::ptr::write_volatile(IOAPIC_BASE.add(0x10 / 4), val);
    }
}

fn ioapic_read(reg: u8) -> u32 {
    unsafe {
        core::ptr::write_volatile(IOAPIC_BASE, reg as u32);
        core::ptr::read_volatile(IOAPIC_BASE.add(0x10 / 4))
    }
}

/// Inoltra l'IRQ del PIT (pin 2 su ICH9/Q35 — "IO-APIC 2-edge timer",
/// vedi /proc/interrupts di Linux) al LAPIC 0 sul vector dato.
pub fn init_ioapic(mmio_base: u64, vector: u8) {
    unsafe {
        IOAPIC_BASE = mmio_base as *mut u32;
        // Redirection entry 2 = IRQ 0 (PIT): low = vector | fixed | physical | edge
        ioapic_write(0x14, vector as u32);
        // high: dest field = 0 (LAPIC 0)
        ioapic_write(0x15, 0);
        // Anche entry 0 per i sistemi che cablano il PIT sul pin 0
        ioapic_write(0x10, vector as u32);
        ioapic_write(0x11, 0);
    }
}

/// Avvia il PIT channel 0 in mode 2 (rate generator) a ~100 Hz.
pub fn init_pit() {
    outb(PIT_CMD, 0x34); // ch0, lobyte/hibyte, mode 2, binary
    outb(PIT_CH0, (PIT_DIVISOR & 0xFF) as u8);
    outb(PIT_CH0, (PIT_DIVISOR >> 8) as u8);
}
