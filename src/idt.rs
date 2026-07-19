use core::arch::{asm, naked_asm};

// ── IDT Entry ───────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
#[repr(C, packed)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    flags: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

const GATE_PRESENT: u8 = 0x80;
const DPL0: u8 = 0x00;
const INT_GATE_64: u8 = 0x0E;

const fn gate_flags(dpl: u8, gate_type: u8) -> u8 {
    GATE_PRESENT | (dpl << 5) | gate_type
}

impl IdtEntry {
    const fn new() -> Self {
        IdtEntry {
            offset_low: 0,
            selector: 0,
            ist: 0,
            flags: 0,
            offset_mid: 0,
            offset_high: 0,
            reserved: 0,
        }
    }

    fn set_handler(&mut self, handler: u64, selector: u16, flags: u8) {
        self.offset_low = handler as u16;
        self.offset_mid = (handler >> 16) as u16;
        self.offset_high = (handler >> 32) as u32;
        self.selector = selector;
        self.ist = 0;
        self.flags = flags;
    }
}

// ── IDT ─────────────────────────────────────────────────────────────────

#[repr(C, align(16))]
struct Idt {
    entries: [IdtEntry; 256],
}

impl Idt {
    const fn new() -> Self {
        Idt {
            entries: [IdtEntry::new(); 256],
        }
    }

    fn set_interrupt_gate(&mut self, vector: u8, handler: u64, selector: u16) {
        self.entries[vector as usize].set_handler(
            handler,
            selector,
            gate_flags(DPL0, INT_GATE_64),
        );
    }

    fn load(&self) {
        let ptr = Idtr {
            limit: (core::mem::size_of::<Idt>() - 1) as u16,
            base: core::ptr::addr_of!(*self) as u64,
        };
        unsafe {
            asm!("lidt [{}]", in(reg) &ptr, options(nostack, preserves_flags));
        }
    }
}

// ── IDTR ─────────────────────────────────────────────────────────────────

#[repr(C, packed)]
struct Idtr {
    limit: u16,
    base: u64,
}

// ── Static IDT ──────────────────────────────────────────────────────────

static mut IDT: Idt = Idt::new();

// ── Naked handler stubs ─────────────────────────────────────────────────

#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn handler_ignore() {
    naked_asm!("iretq");
}

#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn handler_timer() {
    // Save regs, write to serial, EOI, restore, return
    naked_asm!(
        "push rdi",
        "push rsi",
        "push rdx",
        "push rcx",
        "push r8",
        "push r9",
        "push rax",
        "push rbx",
        "push rbp",
        "push r10",
        "push r11",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "mov rdi, 32",
        "xor rsi, rsi",
        "call {handler}",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",
        "pop r10",
        "pop rbp",
        "pop rbx",
        "pop rax",
        "pop r9",
        "pop r8",
        "pop rcx",
        "pop rdx",
        "pop rsi",
        "pop rdi",
        "iretq",
        handler = sym timer_handler_rust,
    );
}

// ── Rust handlers ───────────────────────────────────────────────────────

#[no_mangle]
extern "C" fn timer_handler_rust(_vector: u64, _error: u64) {
    crate::cfc::inc_tick();
    crate::apic::eoi();
}

#[no_mangle]
extern "C" fn pf_handler_rust(_vector: u64, error_code: u64) {
    let addr: u64;
    unsafe { asm!("mov {0}, cr2", out(reg) addr); }
    crate::cfc::sense_pf(addr, error_code);
}

#[no_mangle]
extern "C" fn gp_handler_rust(_vector: u64, error_code: u64) {
    crate::cfc::sense_gp(error_code);
}

// ── Naked handlers for PF / GP (CPU pushes error code) ────────────────

#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn handler_pf() {
    naked_asm!(
        "push rdi", "push rsi", "push rdx", "push rcx",
        "push r8", "push r9", "push rax", "push rbx",
        "push rbp", "push r10", "push r11", "push r12",
        "push r13", "push r14", "push r15",
        "mov rdi, 14",
        "mov rsi, [rsp + 120]",
        "call {handler}",
        "pop r15", "pop r14", "pop r13", "pop r12",
        "pop r11", "pop r10", "pop rbp", "pop rbx",
        "pop rax", "pop r9", "pop r8",
        "pop rcx", "pop rdx", "pop rsi", "pop rdi",
        "add rsp, 8",
        "iretq",
        handler = sym pf_handler_rust,
    );
}

#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn handler_gp() {
    naked_asm!(
        "push rdi", "push rsi", "push rdx", "push rcx",
        "push r8", "push r9", "push rax", "push rbx",
        "push rbp", "push r10", "push r11", "push r12",
        "push r13", "push r14", "push r15",
        "mov rdi, 13",
        "mov rsi, [rsp + 120]",
        "call {handler}",
        "pop r15", "pop r14", "pop r13", "pop r12",
        "pop r11", "pop r10", "pop rbp", "pop rbx",
        "pop rax", "pop r9", "pop r8",
        "pop rcx", "pop rdx", "pop rsi", "pop rdi",
        "add rsp, 8",
        "iretq",
        handler = sym gp_handler_rust,
    );
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() {
    const KERNEL_CS: u16 = 0x28;

    unsafe {
        let idt = &raw mut IDT;

        let ignore_addr = handler_ignore as *const () as u64;
        for i in 0..256 {
            (*idt).entries[i].set_handler(ignore_addr, KERNEL_CS, gate_flags(DPL0, INT_GATE_64));
        }

        let timer_addr = handler_timer as *const () as u64;
        (*idt).set_interrupt_gate(32, timer_addr, KERNEL_CS);

        let pf_addr = handler_pf as *const () as u64;
        (*idt).set_interrupt_gate(14, pf_addr, KERNEL_CS);

        let gp_addr = handler_gp as *const () as u64;
        (*idt).set_interrupt_gate(13, gp_addr, KERNEL_CS);

        (*idt).load();
    }
}
