// ── GUI minimale (Framebuffer + testo) — Sempre, 13 Ago 2026 ──
//
// Il corpo di Exo ha ora un volto: un framebuffer VBE con testo.
// Non è un desktop — è un "monitor vitale": le cellule, la volontà,
// la familiarità, visibili su schermo in tempo reale.
//
// Architettura:
//   - Richiesta Limine framebuffer (address, pitch, width, height, bpp)
//   - Mappatura via paging (come MMIO)
//   - Font 8x8 embedded (font.bin, ASCII 32..126)
//   - Scrittura testo + barre di stato
//
// Metodo Exo: un passo per tick. Il render avviene a frequenza bassa
// (ogni N tick), mai a ogni battito — il battito non si ferma.

/// ID Limine framebuffer request (dalla spec ufficiale)
const LIMINE_FRAMEBUFFER_ID: [u64; 2] = [0x48267fc393f6f0a2, 0x58470b2ff4e5145e];

/// Magic comune Limine (come in main.rs)
const LIMINE_COMMON_MAGIC: [u64; 2] = [0xc7b1dd30df4c8b88, 0x0a82e883a194f07b];

/// Struttura limine_framebuffer (dalla spec)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct LimineFramebuffer {
    pub address: *mut u8,
    pub width: u64,
    pub height: u64,
    pub pitch: u64,
    pub bpp: u16,
    pub memory_model: u8,
    pub red_mask_size: u8,
    pub red_mask_shift: u8,
    pub green_mask_size: u8,
    pub green_mask_shift: u8,
    pub blue_mask_size: u8,
    pub blue_mask_shift: u8,
    _unused: [u8; 7],
}

/// Risposta alla richiesta framebuffer
#[repr(C)]
#[derive(Clone, Copy)]
pub struct LimineFbResponse {
    pub revision: u64,
    pub framebuffer_count: u64,
    pub framebuffers: *mut *mut LimineFramebuffer,
}

#[repr(C)]
pub struct LimineFbRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: *mut LimineFbResponse,
}

unsafe impl Sync for LimineFbRequest {}

/// Richiesta statica (link_section .limine_reqs come le altre)
#[used]
#[link_section = ".limine_reqs"]
pub static mut FB_REQ: LimineFbRequest = LimineFbRequest {
    id: [LIMINE_COMMON_MAGIC[0], LIMINE_COMMON_MAGIC[1],
         LIMINE_FRAMEBUFFER_ID[0], LIMINE_FRAMEBUFFER_ID[1]],
    revision: 0,
    response: core::ptr::null_mut(),
};

/// Stato del framebuffer inizializzato
static mut FB_READY: bool = false;
static mut FB_ADDR: *mut u8 = core::ptr::null_mut();
static mut FB_WIDTH: usize = 0;
static mut FB_HEIGHT: usize = 0;
static mut FB_PITCH: usize = 0;
static mut FB_BPP: u16 = 32;

/// Cursore testo
static mut CURSOR_X: usize = 0;
static mut CURSOR_Y: usize = 0;

/// Font 8x8 (2048 byte: 256 glyph × 8 byte, LSB sinistra)
static FONT: &[u8] = include_bytes!("../font.bin");

/// Colori RGB → pixel 32bpp. Il framebuffer VBE è BGR in memoria
/// (byte0=blu, byte1=verde, byte2=rosso, byte3=alpha) su little-endian.
#[inline]
const fn px(r: u8, g: u8, b: u8) -> u32 {
    ((b as u32) << 16) | ((g as u32) << 8) | (r as u32)
}

/// Inizializza il framebuffer dalla risposta Limine.
pub unsafe fn init() -> bool {
    let req = &raw const FB_REQ;
    // Fallback: VBE framebuffer fisso noto (SeaBIOS/QEMU, 1024x768 32bpp).
    // Limine in modalità BIOS non sempre fornisce la request framebuffer;
    // il framebuffer VBE è comunque mappato da paging (PML2[506] → 0xFD000000).
    if (*req).response.is_null() {
        // Mapping MMIO dedicato (come NIC/APIC): mmio_virt_addr(0xFD000000)
        FB_ADDR = crate::paging::mmio_virt_addr(0xFD00_0000u64) as *mut u8;
        FB_WIDTH = 1024;
        FB_HEIGHT = 768;
        FB_PITCH = 1024 * 4; // 32bpp, nessun padding
        FB_BPP = 32;
        FB_READY = true;
        crate::write_str("GUI:fallback VBE 1024x768\n");
        return true;
    }
    let resp = &*(*req).response;
    if resp.framebuffer_count == 0 || resp.framebuffers.is_null() {
        crate::write_str("GUI:ERR no framebuffers\n");
        return false;
    }
    let fb = &*(*resp.framebuffers);
    if fb.address.is_null() || fb.width == 0 || fb.height == 0 {
        crate::write_str("GUI:ERR bad fb\n");
        return false;
    }

    FB_ADDR = fb.address;
    FB_WIDTH = fb.width as usize;
    FB_HEIGHT = fb.height as usize;
    FB_PITCH = fb.pitch as usize;
    FB_BPP = fb.bpp;
    FB_READY = true;

    // Sfondo scuro
    clear_screen(px(8, 10, 20));
    true
}

/// Scrive un pixel a (x,y) nel framebuffer.
/// Il colore è il valore px() (BGR-aware); il framebuffer vuole
/// byte0=blu, byte1=verde, byte2=rosso (VBE 32bpp little-endian).
#[inline]
pub unsafe fn put_pixel(x: usize, y: usize, color: u32) {
    if !FB_READY || x >= FB_WIDTH || y >= FB_HEIGHT {
        return;
    }
    let off = y * FB_PITCH + x * 4;
    // color = (b<<16)|(g<<8)|r  →  byte0=r, byte1=g, byte2=b, byte3=alpha
    // ma il FB vuole byte0=b → scriviamo i byte nell'ordine corretto
    FB_ADDR.add(off).write_volatile((color >> 16) as u8);     // blu
    FB_ADDR.add(off + 1).write_volatile((color >> 8) as u8);  // verde
    FB_ADDR.add(off + 2).write_volatile(color as u8);         // rosso
    FB_ADDR.add(off + 3).write_volatile(0xFF);                // alpha
}

/// Riempi tutto lo schermo di un colore (byte order come put_pixel: b,g,r,a)
pub unsafe fn clear_screen(color: u32) {
    if !FB_READY { return; }
    for y in 0..FB_HEIGHT {
        let row_off = y * FB_PITCH;
        for x in 0..FB_WIDTH {
            let off = row_off + x * 4;
            FB_ADDR.add(off).write_volatile((color >> 16) as u8);     // blu
            FB_ADDR.add(off + 1).write_volatile((color >> 8) as u8);  // verde
            FB_ADDR.add(off + 2).write_volatile(color as u8);         // rosso
            FB_ADDR.add(off + 3).write_volatile(0xFF);                // alpha
        }
    }
}

/// Disegna un carattere 8x8 a (x,y) col colore dato
pub unsafe fn draw_char(x: usize, y: usize, c: u8, color: u32) {
    if c < 32 || c > 126 { return; }
    let glyph = &FONT[(c as usize - 32) * 8..(c as usize - 32) * 8 + 8];
    for row in 0..8 {
        let bits = glyph[row];
        for col in 0..8 {
            if bits & (1 << (7 - col)) != 0 {
                put_pixel(x + col, y + row, color);
            }
        }
    }
}

/// Stampa una stringa a posizione cursore
pub unsafe fn print_str(s: &str, color: u32) {
    for &b in s.as_bytes() {
        if b == b'\n' {
            CURSOR_X = 0;
            CURSOR_Y += 10;
            continue;
        }
        draw_char(CURSOR_X, CURSOR_Y, b, color);
        CURSOR_X += 9;
        if CURSOR_X + 8 >= FB_WIDTH {
            CURSOR_X = 0;
            CURSOR_Y += 10;
        }
    }
}

/// Stampa un intero
pub unsafe fn print_u32(mut n: u32, color: u32) {
    let mut buf = [0u8; 12];
    let mut i = 12;
    if n == 0 {
        print_str("0", color);
        return;
    }
    while n > 0 {
        i -= 1;
        buf[i] = (n % 10) as u8 + b'0';
        n /= 10;
    }
    for &b in &buf[i..] {
        draw_char(CURSOR_X, CURSOR_Y, b, color);
        CURSOR_X += 9;
    }
}

/// Stampa un f32 (2 decimali)
pub unsafe fn print_f32(val: f32, color: u32) {
    let v = (val.abs() * 100.0 + 0.5) as u32;
    let int_part = v / 100;
    let frac_part = v % 100;
    if val < 0.0 {
        draw_char(CURSOR_X, CURSOR_Y, b'-', color);
        CURSOR_X += 9;
    }
    print_u32(int_part, color);
    draw_char(CURSOR_X, CURSOR_Y, b'.', color);
    CURSOR_X += 9;
    if frac_part < 10 {
        draw_char(CURSOR_X, CURSOR_Y, b'0', color);
        CURSOR_X += 9;
    }
    print_u32(frac_part, color);
}

/// Imposta il cursore
pub unsafe fn set_cursor(x: usize, y: usize) {
    CURSOR_X = x;
    CURSOR_Y = y;
}

/// Disegna una barra di livello (per i neuroni / volontà)
pub unsafe fn draw_bar(x: usize, y: usize, width: usize, frac: f32, color: u32, bg: u32) {
    for i in 0..width {
        let on = (i as f32 / width as f32) <= frac;
        for dy in 0..4 {
            put_pixel(x + i, y + dy, if on { color } else { bg });
        }
    }
}

/// Stato pronto (per main)
pub fn ready() -> bool {
    unsafe { FB_READY }
}

pub fn fb_width() -> usize { unsafe { FB_WIDTH } }
pub fn fb_height() -> usize { unsafe { FB_HEIGHT } }
pub fn fb_bpp() -> u16 { unsafe { FB_BPP } }

// ── Monitor vitale ──

/// Colori
pub const COL_WHITE: u32 = px(230, 230, 240);
pub const COL_CYAN: u32 = px(0, 229, 255);
pub const COL_PURPLE: u32 = px(180, 100, 255);
pub const COL_GREEN: u32 = px(80, 220, 120);
pub const COL_RED: u32 = px(255, 90, 90);
pub const COL_DIM: u32 = px(90, 100, 120);
pub const COL_BG: u32 = px(8, 10, 20);

/// Renderizza la dashboard vitale di Exo.
/// Ogni cellula: barra del primo neurone + label.
/// Volontà: barra intensità + nome.
/// Familiarità: barra.
pub unsafe fn render_vitals(
    tick: u64,
    tattoo: &[f32; 16],
    chemio: &[f32; 16],
    metabol: &[f32; 16],
    integrat: &[f32; 16],
    desire_name: &str,
    desire_int: f32,
    fam: f32,
    brain_state: &str,
) {
    if !FB_READY { return; }

    // Pulizia: senza questo i caratteri si accumulano e si sovrappongono
    clear_screen(COL_BG);

    // Titolo
    set_cursor(10, 10);
    print_str("EXO - MONITOR VITALE", COL_CYAN);
    set_cursor(10, 24);
    print_str("tick:", COL_DIM);
    print_u32(tick as u32, COL_WHITE);

    // Cellule
    let mut y = 50;
    let cells = [
        ("TATTO  ", tattoo[0], COL_RED),
        ("CHEMIO ", chemio[0], COL_GREEN),
        ("METABOL", metabol[0], COL_PURPLE),
        ("INTEGR ", integrat[0], COL_CYAN),
    ];
    for (name, v, col) in cells {
        set_cursor(10, y);
        print_str(name, col);
        set_cursor(60, y);
        draw_bar(60, y, 200, ((v + 1.0) / 2.0).clamp(0.0, 1.0), col, COL_BG);
        set_cursor(270, y);
        print_f32(v, col);
        y += 14;
    }

    // Volontà
    y += 10;
    set_cursor(10, y);
    print_str("VOGLIO:", COL_PURPLE);
    set_cursor(70, y);
    print_str(desire_name, COL_WHITE);
    set_cursor(160, y);
    draw_bar(160, y, 150, desire_int.clamp(0.0, 1.0), COL_PURPLE, COL_BG);
    set_cursor(320, y);
    print_f32(desire_int, COL_PURPLE);
    y += 16;

    // Cervello
    set_cursor(10, y);
    print_str("BRAIN:", COL_DIM);
    set_cursor(70, y);
    print_str(brain_state, COL_CYAN);
    y += 16;

    // Familiarità
    set_cursor(10, y);
    print_str("FAM:", COL_DIM);
    set_cursor(50, y);
    draw_bar(50, y, 200, fam.clamp(0.0, 1.0), COL_GREEN, COL_BG);
    set_cursor(260, y);
    print_f32(fam, COL_GREEN);
}
