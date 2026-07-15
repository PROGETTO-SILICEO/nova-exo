#![no_std]
#![no_main]

use core::arch::asm;
use core::fmt::Write;
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicBool, Ordering};
use uart_16550::SerialPort;

mod cfc;
mod idt;
mod serial;

use serial::LineReader;

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

#[used]
#[link_section = ".limine_reqs"]
static LIMINE_REQS: [u64; 1] = [0];

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
        unsafe {
            let _ = write!(SERIAL, $($arg)*);
            let _ = SERIAL.write_str("\n");
        }
    };
}

// ── I/O port helpers ────────────────────────────────────────────────────

unsafe fn outb(port: u16, val: u8) {
    asm!("out dx, al", in("dx") port, in("al") val);
}

// ── CfC weights (Xavier seed per cellula) ───────────────────────────────
// Generated: seed 42=tatto, 43=chemio, 44=metabol, 45=integrat

static W_TATTO: cfc::CfcWeights = cfc::CfcWeights {
    w_f: [
        [-0.153656, 0.552010, 0.284133, 0.120831, -0.421289, -0.421319, -0.541235, 0.448472],
        [0.123840, 0.254836, -0.587162, 0.575520, 0.407157, -0.352311, -0.389683, -0.387749],
        [-0.239753, 0.030320, -0.083350, -0.255691, 0.136991, -0.441528, -0.254570, -0.163673],
        [-0.053803, 0.349268, -0.367823, 0.017434, 0.113184, -0.555483, 0.131715, -0.403524],
        [-0.532701, 0.549770, 0.570280, 0.377708, -0.239298, -0.492749, 0.225638, -0.073298],
        [-0.462907, -0.005907, -0.570255, 0.501313, -0.295433, 0.199048, -0.230606, 0.024578],
        [0.057208, -0.385973, 0.575121, 0.336968, 0.538274, 0.483563, 0.119902, 0.516688],
        [-0.503992, -0.372343, -0.556981, -0.213926, -0.136342, -0.280039, 0.402620, -0.175441]
    ],
    w_f_in: [
        [0.568601, 0.081752, 0.220596, 0.470186],
        [-0.424428, -0.527142, 0.383873, -0.095758],
        [-0.162235, 0.401132, 0.403803, -0.476861],
        [-0.544312, -0.236850, 0.143647, 0.534293],
        [0.162461, -0.107475, 0.647492, 0.500751],
        [0.387700, 0.531219, -0.596296, -0.661910],
        [0.291671, 0.074911, 0.502424, 0.642205],
        [-0.035389, 0.068119, 0.331669, 0.054634]
    ],
    b_f: [0.000000, 0.000000, 0.000000, 0.000000, 0.000000, 0.000000, 0.000000, 0.000000],
    w_g: [
        [0.226194, -0.544761, 0.180741, 0.097648, -0.199869, -0.480728, 0.153994, 0.416220],
        [-0.415804, -0.535203, -0.445754, 0.547735, -0.265709, -0.437678, 0.032651, -0.535857],
        [0.068924, 0.035786, -0.577190, 0.563371, -0.592554, 0.442336, -0.546619, 0.437407],
        [-0.061452, -0.574716, -0.601221, -0.461290, 0.197320, -0.368838, 0.315975, 0.057781],
        [-0.398386, 0.375100, 0.170239, 0.277397, 0.333587, 0.440780, -0.535304, -0.387206],
        [-0.088787, -0.362710, -0.529149, 0.362380, 0.231996, -0.574923, -0.567952, -0.207542],
        [0.376092, 0.283072, -0.199285, -0.586147, -0.374714, -0.289710, -0.600801, 0.008346],
        [-0.550294, 0.475078, -0.038785, 0.057348, -0.527057, 0.006142, 0.314748, 0.543580]
    ],
    w_g_in: [
        [-0.615134, 0.455821, -0.649402, 0.108662],
        [0.614166, -0.253270, -0.095410, -0.175055],
        [0.156794, -0.183251, -0.201028, -0.380470],
        [0.541332, 0.296886, -0.393322, 0.465626],
        [0.151572, -0.601160, -0.178822, -0.338658],
        [0.375138, -0.049544, -0.699137, -0.214297],
        [0.106663, 0.586838, 0.336829, 0.287465],
        [-0.251266, 0.535382, -0.251986, 0.566026]
    ],
    b_g: [0.500000, -0.400000, 0.000000, 0.000000, 0.000000, 0.000000, 0.000000, 0.000000],
};

static W_CHEMIO: cfc::CfcWeights = cfc::CfcWeights {
    w_f: [
        [-0.471460, 0.133579, -0.449003, -0.317712, -0.211711, 0.439852, 0.203418, 0.050413],
        [-0.576838, 0.286282, -0.128659, 0.369931, -0.300771, -0.542703, 0.449051, -0.341668],
        [-0.116364, -0.225235, -0.518480, 0.420363, 0.427361, 0.577424, -0.140384, 0.556632],
        [-0.066432, 0.207869, -0.511331, 0.486344, -0.247394, -0.291116, -0.606090, 0.052912],
        [-0.029888, 0.167023, 0.585678, 0.500508, 0.502333, 0.030932, -0.484976, -0.390798],
        [0.554859, -0.107835, 0.447053, 0.210873, 0.157730, -0.274883, 0.485914, -0.358983],
        [-0.117079, 0.604500, 0.288706, -0.067286, 0.074297, -0.108689, 0.278002, -0.123458],
        [0.208384, 0.250724, 0.134183, 0.049032, -0.359975, -0.368450, 0.362205, -0.256789]
    ],
    w_f_in: [
        [0.336236, -0.226338, -0.045127, -0.107568],
        [0.096619, 0.539213, -0.660233, -0.694684],
        [-0.425342, -0.470553, 0.663253, -0.627104],
        [-0.580452, -0.311635, 0.154567, -0.607296],
        [0.102514, -0.236848, -0.337076, -0.576160],
        [0.228879, -0.581897, -0.395508, -0.654536],
        [-0.467295, 0.169279, -0.144241, 0.653337],
        [0.560564, -0.346368, 0.461823, 0.467828]
    ],
    b_f: [0.000000, 0.000000, 0.000000, 0.000000, 0.000000, 0.000000, 0.000000, 0.000000],
    w_g: [
        [-0.358072, 0.345368, 0.130924, -0.188575, -0.117233, -0.066305, 0.284954, -0.223365],
        [-0.447776, 0.386653, 0.591442, -0.315025, -0.094301, -0.050567, 0.185902, 0.387557],
        [-0.213949, 0.105771, -0.482584, 0.547526, 0.543213, 0.518983, -0.408232, 0.496242],
        [0.162931, 0.248758, -0.051830, -0.045745, 0.160760, -0.149405, 0.128346, -0.226018],
        [-0.471302, 0.425268, -0.396158, 0.384565, -0.043289, 0.034091, 0.576127, -0.515503],
        [-0.543444, -0.263380, 0.465453, -0.539331, 0.212297, 0.315429, 0.535277, 0.404996],
        [-0.561241, 0.545607, -0.528638, 0.036153, 0.255064, -0.579161, 0.088729, 0.558407],
        [-0.269452, -0.517594, -0.299431, -0.033839, 0.138674, -0.134327, -0.025273, 0.187378]
    ],
    w_g_in: [
        [-0.600962, 0.206502, 0.221043, -0.113093],
        [-0.369085, 0.595697, 0.610702, -0.553562],
        [-0.516726, -0.458956, 0.314733, -0.601841],
        [0.228601, 0.262538, -0.363727, 0.705714],
        [0.645925, 0.507317, -0.461735, 0.506084],
        [0.492038, -0.380215, -0.704680, -0.243355],
        [0.105231, -0.244465, 0.313417, -0.341961],
        [0.443510, -0.512175, 0.167757, 0.225359]
    ],
    b_g: [0.000000, 0.000000, 0.000000, 0.000000, 0.000000, 0.000000, 0.000000, 0.000000],
};

static W_METABOL: cfc::CfcWeights = cfc::CfcWeights {
    w_f: [
        [0.410096, -0.484024, 0.299622, -0.170851, -0.172308, 0.133789, -0.130093, -0.111363],
        [0.012128, 0.257378, 0.564027, -0.053128, -0.088608, -0.473408, -0.345502, 0.560287],
        [0.542992, 0.467637, 0.179316, -0.350492, 0.167584, -0.441954, -0.050577, 0.457887],
        [-0.295837, 0.201901, 0.444184, -0.430072, 0.077098, -0.417448, -0.400620, -0.484971],
        [-0.363826, -0.054882, 0.360779, 0.601133, 0.373568, -0.150135, 0.019274, -0.540236],
        [0.258538, -0.523569, 0.468602, 0.276863, 0.408289, 0.257479, 0.241801, 0.526766],
        [0.467452, -0.496056, -0.053281, -0.008912, -0.478908, -0.424087, 0.593124, -0.279781],
        [0.486410, -0.411227, -0.450267, -0.223695, -0.235858, -0.095463, -0.206986, 0.083375]
    ],
    w_f_in: [
        [-0.575098, 0.552877, -0.032902, -0.461710],
        [0.298550, 0.011278, -0.057921, 0.434696],
        [0.254498, -0.289671, 0.129299, -0.574545],
        [0.422252, -0.330108, 0.166187, -0.287509],
        [0.692409, -0.153499, -0.122719, 0.067295],
        [0.687303, 0.062574, 0.299569, 0.566551],
        [-0.427010, -0.626220, 0.615629, -0.565645],
        [0.441520, 0.222719, -0.036714, -0.328554]
    ],
    b_f: [0.000000, 0.000000, 0.000000, 0.000000, 0.000000, 0.000000, 0.000000, 0.000000],
    w_g: [
        [-0.256366, -0.490783, -0.307402, -0.126949, -0.523709, -0.345813, 0.600718, 0.160488],
        [0.180471, 0.013924, 0.270974, -0.536616, -0.113978, -0.512001, -0.343901, 0.390518],
        [-0.493016, 0.187906, 0.467222, -0.547306, 0.600637, -0.557354, -0.366362, 0.393242],
        [0.121748, 0.187912, 0.490456, 0.362754, 0.538182, 0.321137, 0.487090, -0.540267],
        [0.170000, -0.093724, -0.596043, 0.312183, 0.363491, -0.196149, -0.312680, -0.457031],
        [-0.061950, 0.048170, 0.073761, -0.586846, 0.516622, -0.516874, -0.442108, 0.214318],
        [-0.454525, -0.290557, -0.546450, -0.299305, -0.390738, 0.457612, -0.367001, 0.075861],
        [0.146977, -0.217866, -0.244643, 0.215858, 0.100182, -0.575306, 0.527524, 0.446317]
    ],
    w_g_in: [
        [-0.263205, 0.380949, -0.278381, -0.681030],
        [-0.389196, -0.234724, 0.107337, -0.010427],
        [-0.538982, -0.379637, 0.706538, 0.305122],
        [-0.575928, -0.329497, 0.414679, -0.291576],
        [0.469628, 0.290023, -0.095217, 0.277298],
        [-0.643991, -0.254321, 0.056654, 0.428571],
        [0.300449, 0.578110, -0.454493, 0.152584],
        [0.418351, -0.475621, 0.601839, -0.330429]
    ],
    b_g: [0.100000, -0.100000, 0.000000, 0.000000, 0.000000, 0.000000, 0.000000, 0.000000],
};

static W_INTRG: cfc::CfcWeights = cfc::CfcWeights {
    w_f: [
        [0.598914, 0.060680, -0.267671, -0.517712, -0.068011, -0.033303, -0.552945, -0.412342],
        [-0.470362, 0.156022, 0.436232, 0.183837, 0.601009, -0.036313, 0.144881, -0.266177],
        [0.582983, 0.211964, -0.072834, -0.257579, 0.011880, -0.474637, -0.334411, -0.026268],
        [-0.315056, -0.137193, 0.390539, -0.521079, 0.518250, -0.336932, 0.252752, -0.476958],
        [0.123704, -0.114155, 0.412530, -0.306149, -0.051760, 0.070340, -0.303798, -0.477355],
        [0.277573, -0.232611, 0.398966, -0.059196, -0.497076, 0.474225, 0.296087, -0.463037],
        [0.435849, -0.530390, -0.387078, -0.401512, 0.517183, 0.203715, -0.299993, -0.304574],
        [0.592821, 0.218979, -0.119570, -0.533778, -0.068740, -0.358738, -0.217282, 0.183082]
    ],
    w_f_in: [
        [-0.306825, -0.704236, 0.043245, -0.593459],
        [0.215325, 0.382883, 0.138790, -0.245106],
        [0.279415, -0.006584, 0.633884, 0.102807],
        [-0.460783, -0.576725, -0.610347, 0.592060],
        [0.222299, -0.124798, -0.074222, 0.051893],
        [-0.081227, 0.535929, -0.597999, 0.162618],
        [-0.204489, -0.575541, -0.656384, 0.320352],
        [-0.422463, -0.319253, -0.588910, -0.064068]
    ],
    b_f: [0.000000, 0.000000, 0.000000, 0.000000, 0.000000, 0.000000, 0.000000, 0.000000],
    w_g: [
        [0.498292, -0.474552, -0.489530, -0.519037, 0.541976, 0.427868, 0.553384, -0.390470],
        [0.491477, -0.139763, -0.046163, -0.479955, -0.198958, 0.293333, 0.106243, 0.527251],
        [0.344306, 0.593014, -0.562752, 0.073616, -0.096835, 0.064669, 0.607323, 0.165611],
        [0.260609, -0.509948, 0.468724, -0.001916, -0.173647, 0.601021, -0.215615, -0.299399],
        [-0.554276, -0.158688, -0.293552, -0.323236, -0.498293, 0.405764, -0.306228, -0.381429],
        [-0.206406, 0.469459, 0.600256, -0.334924, -0.610416, -0.448854, -0.216000, -0.288261],
        [0.564093, -0.344461, -0.034214, -0.354711, 0.214609, -0.239297, 0.195469, -0.091873],
        [-0.191604, -0.173135, -0.068247, -0.226697, -0.233103, 0.130899, 0.183071, -0.102700]
    ],
    w_g_in: [
        [-0.182537, 0.233182, -0.004726, 0.144362],
        [-0.361777, -0.007427, 0.297749, -0.068742],
        [-0.640585, 0.043809, 0.075035, -0.545271],
        [-0.359954, 0.476168, 0.573881, -0.351339],
        [-0.420509, 0.188533, -0.673110, 0.305630],
        [0.562907, -0.459243, 0.030337, -0.396039],
        [0.508536, 0.123544, -0.208468, -0.042723],
        [-0.316257, -0.497986, -0.230197, -0.404993]
    ],
    b_g: [0.300000, -0.250000, 0.000000, 0.000000, 0.000000, 0.000000, 0.000000, 0.000000],
};



// ── PIC + PIT init ──────────────────────────────────────────────────────

unsafe fn pic_pit_init() {
    outb(0x20, 0x11);
    asm!("nop");
    outb(0xA0, 0x11);
    asm!("nop");
    outb(0x21, 0x20);
    asm!("nop");
    outb(0xA1, 0x28);
    asm!("nop");
    outb(0x21, 0x04);
    asm!("nop");
    outb(0xA1, 0x02);
    asm!("nop");
    outb(0x21, 0x01);
    asm!("nop");
    outb(0xA1, 0x01);
    asm!("nop");
    outb(0x21, 0xFE);
    asm!("nop");
    outb(0xA1, 0xFF);
    asm!("nop");

    serial_println!("PIC remapped: IRQ0 → vector 32");

    outb(0x43, 0x36u8);
    let divisor: u16 = 11931u16;
    outb(0x40, (divisor & 0xFF) as u8);
    asm!("nop");
    outb(0x40, ((divisor >> 8) & 0xFF) as u8);
}

// ── Serial output helpers ───────────────────────────────────────────────

fn serial_putc(c: u8) {
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
            for j in 0..32 {
                debugcon_putc(b',');
                debugcon_hex16(cells[j]);
            }
            debugcon_putc(b'\n');
        }
        debugcon_putc(b'D'); debugcon_putc(b':'); debugcon_putc(b'E'); debugcon_putc(b'N'); debugcon_putc(b'D'); debugcon_putc(b'\n');
        asm!("sti");
    }
}

fn write_hex64(val: u64) {
    write_str("0x");
    for shift in (0..64).rev().step_by(4) {
        let nibble = ((val >> shift) & 0xF) as u8;
        serial_putc(if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 });
    }
}

fn write_cell_line(prefix: &str, h: &[f32; 8]) {
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
    for i in 0..8 {
        write_f32(h[i]);
        if i < 7 { serial_putc(b','); }
    }
    serial_putc(b'\n');
}

// ── Entry point ─────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe {
        SERIAL.init();
    }
    serial_println!("Nova Exo v0.7 -- Battito timer-driven.");

    idt::init();
    serial_println!("IDT loaded. 4 cellulae: tatto, chemio, metabol, integrat.");

    let mut tessuto = cfc::Tessuto::new();
    let mut line_reader = LineReader::new();
    let mut dump_requested = false;
    let dt_tatto = 0.001f32;
    let dt_rest = 0.01f32;

    unsafe {
        serial_println!("Initializing PIC + PIT timer...");
        pic_pit_init();
        serial_println!("Enabling interrupts. Tessuto loop starts.");
        asm!("sti");
    }

    loop {
        unsafe { asm!("hlt"); }

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
        let chemio_input;
        if line_reader.has_line {
            let raw = line_reader.line();
            if raw == b"DUMP" {
                dump_requested = true;
                chemio_input = [0.0; 4];
            } else {
                chemio_input = line_reader.parse_line().unwrap_or([0.0; 4]);
            }
            line_reader.consume();
        } else {
            chemio_input = [0.0; 4];
        }

        // Tessuto step (all 4 cells via axon bundles)
        tessuto.step(&FASCI, &chemio_input, sense.as_ref(),
            &W_TATTO, &W_CHEMIO, &W_METABOL, &W_INTRG,
            dt_tatto, dt_rest);

        // Log current state (lab: circular buffer, always recording)
        cfc::log_record(&tessuto.tatto.h, &tessuto.chemio.h,
            &tessuto.metabol.h, &tessuto.integrat.h);

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
    }
}

// ── Panic handler ───────────────────────────────────────────────────────

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    unsafe {
        let _ = write!(SERIAL, "PANIC: ");
        let _ = write!(SERIAL, "{}", info);
        let _ = SERIAL.write_str("\n");
    }
    loop {
        unsafe { core::arch::asm!("hlt"); }
    }
}
