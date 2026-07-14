#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[repr(C)]
pub struct BaseRevision {
    magic: [u64; 3],
}
unsafe impl Sync for BaseRevision {}

#[used]
static BASE_REVISION: BaseRevision = BaseRevision {
    magic: [
        0xf9562b2d5c95a6c8,
        0x6a7b384944536bdc,
        0,
    ],
};

#[used]
#[link_section = ".limine_reqs"]
static LIMINE_REQS: [u64; 2] = [0, 0];

#[no_mangle]
#[link_section = ".text.startup"]
pub extern "C" fn _start() -> ! {
    write_serial(b'N');
    write_serial(b'O');
    write_serial(b'V');
    write_serial(b'A');
    write_serial(b'!');
    write_serial(b'\r');
    write_serial(b'\n');
    loop {
        unsafe { core::arch::asm!("hlt"); }
    }
}

fn write_byte(port: u16, byte: u8) {
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") port,
            in("al") byte,
            options(att_syntax)
        );
    }
}

fn read_byte(port: u16) -> u8 {
    let byte: u8;
    unsafe {
        core::arch::asm!(
            "in al, dx",
            out("al") byte,
            in("dx") port,
            options(att_syntax)
        );
    }
    byte
}

fn write_serial(c: u8) {
    let port: u16 = 0x3F8;
    unsafe {
        core::arch::asm!(
            "mov dx, {0:x}",
            "in al, dx",
            "test al, $0x20",
            "jne 2f",
            "1:",
            "pause",
            "in al, dx",
            "test al, $0x20",
            "je 1b",
            "2:",
            "mov al, {1:l}",
            "mov dx, {0:x}",
            "out dx, al",
            in(reg) port,
            in(reg) c,
            out("dx") _,
            out("al") _,
            options(nostack, preserves_flags)
        );
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        unsafe { core::arch::asm!("hlt"); }
    }
}
