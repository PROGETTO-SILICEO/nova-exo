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
        apic_base |= 1 << 11;
        let low = apic_base as u32;
        let high = (apic_base >> 32) as u32;
        asm!("wrmsr", in("eax") low, in("edx") high, in("ecx") IA32_APIC_BASE_MSR, options(nostack));

        let spurious = read_reg(APIC_SPURIOUS);
        write_reg(APIC_SPURIOUS, spurious | (1 << 8) | 0xFF);

        write_reg(APIC_TPR, 0);

        write_reg(0x330, 1 << 16);
        write_reg(0x340, 1 << 16);
        write_reg(0x370, 1 << 16);
    }
}

pub fn init_timer(vector: u8) {
    write_reg(APIC_TIMER_DIVIDE, 0x3);
    write_reg(APIC_LVT_TIMER, (1 << 17) | vector as u32);
    write_reg(APIC_TIMER_INIT_COUNT, 62_500);
}
