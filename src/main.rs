#![no_std]
#![no_main]

use core::fmt::Write;
use core::panic::PanicInfo;
use uart_16550::SerialPort;

// ── Limine base revision ────────────────────────────────────────────────

#[repr(C)]
struct BaseRevision {
    magic: [u64; 3],
}
unsafe impl Sync for BaseRevision {}

#[used]
#[link_section = ".limine_reqs"]
static BASE_REVISION: BaseRevision = BaseRevision {
    magic: [0xf9562b2d5c95a6c8, 0x6a7b384944536bdc, 0],
};

// ── Limine request list (empty, just terminator) ────────────────────────

#[used]
#[link_section = ".limine_reqs"]
static LIMINE_REQS: [u64; 1] = [0];

// ── Serial port ─────────────────────────────────────────────────────────

static mut SERIAL: SerialPort = unsafe { SerialPort::new(0x3F8) };

macro_rules! serial_println {
    ($($arg:tt)*) => {
        unsafe {
            let _ = write!(SERIAL, $($arg)*);
            let _ = SERIAL.write_str("\n");
        }
    };
}

// ── Entry point ─────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe {
        SERIAL.init();
    }
    serial_println!("Nova Exo v0.1 -- alive!");
    serial_println!("placeholder loop: testing dynamics on bare-metal...");
    placeholder_loop()
}

// ── Placeholder neural loop ─────────────────────────────────────────────
//
// NOTA: questo NON e` un CfC loop. E` uno scaffold per testare che
// UART, floating point, libm::expf(), allocazioni stack e loop
// funzionino su bare-metal.
//
// Il vero CfC (Hasani et al.) ha:
//   - equazione differenziale chiusa con soluzione analitica
//   - costanti di tempo apprese per neurone
//   - decadimento esponenziale verso stato di riposo
//   - x(t) = (x0 - A) * exp(-t * (w_tau + f)) * f + A
//
// Questo loop e` solo: new[i] = sigmoid(sum_j(w[i][j] * state[j]))
//
// Pesi differenziati e stato iniziale non uniforme per testare
// dinamiche reali a 8 dimensioni.

fn placeholder_loop() -> ! {
    const N: usize = 8;

    // Pesi differenziati: ogni neurone ha accoppiamento unico
    let w: [[f32; N]; N] = [
        [0.0,  0.15, 0.05, 0.10, 0.02, 0.08, 0.12, 0.03],
        [0.10, 0.0,  0.20, 0.04, 0.06, 0.01, 0.15, 0.07],
        [0.03, 0.12, 0.0,  0.18, 0.05, 0.09, 0.02, 0.11],
        [0.08, 0.04, 0.14, 0.0,  0.20, 0.06, 0.10, 0.01],
        [0.15, 0.07, 0.01, 0.12, 0.0,  0.18, 0.04, 0.09],
        [0.05, 0.10, 0.08, 0.02, 0.14, 0.0,  0.20, 0.06],
        [0.12, 0.03, 0.10, 0.06, 0.01, 0.15, 0.0,  0.18],
        [0.07, 0.14, 0.04, 0.09, 0.11, 0.02, 0.08, 0.0 ],
    ];

    // Stato iniziale non uniforme
    let mut state: [f32; N] = [0.1, 0.3, 0.5, 0.7, 0.2, 0.4, 0.6, 0.8];

    for cycle in 0..50 {
        let mut new = [0.0f32; N];
        for i in 0..N {
            let mut s = 0.0f32;
            for j in 0..N {
                s += w[i][j] * state[j];
            }
            new[i] = sigmoid(s);
        }
        state = new;

        if cycle < 10 || cycle % 10 == 0 {
            serial_println!("cycle {:2}: [{:4.3}, {:4.3}, {:4.3}, {:4.3}, {:4.3}, {:4.3}, {:4.3}, {:4.3}]",
                cycle, state[0], state[1], state[2], state[3],
                state[4], state[5], state[6], state[7]);
        }
    }

    serial_println!("placeholder loop: 50 cycles done. Entering deep sleep.");
    loop {
        unsafe { core::arch::asm!("hlt"); }
    }
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + libm::expf(-x))
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("PANIC: {}", info);
    loop {
        unsafe { core::arch::asm!("hlt"); }
    }
}
