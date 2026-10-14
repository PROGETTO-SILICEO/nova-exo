// ── kbd.rs — tastiera PS/2 (controller i8042) ────────────────────────────
//
// Exo sapeva già SCRIVERE alla tastiera (i LED, comando 0xED su 0x60), ma non
// LEGGERE: non esisteva né la lettura della porta dati né un handler IRQ1.
// Questo è il canale del contatto: il tasto premuto è la mano dell'umano che
// arriva al corpo. Atterra sul Tatto (slot liberi 2 e 3, cfc.rs).
//
// Prima versione in POLLING, di proposito: non tocca IDT né IOAPIC, quindi non
// può causare reboot loop. Si verifica che il tasto arrivi, e solo dopo lo si
// collega all'interrupt (vettore 33, IOAPIC entry 1).

use core::arch::asm;

const DATA: u16 = 0x60;
const STATUS: u16 = 0x64;

const OBF: u8 = 0x01; // Output Buffer Full: c'è un byte da leggere in 0x60

#[inline]
fn inb(port: u16) -> u8 {
    let v: u8;
    unsafe {
        asm!("in al, dx",
             out("al") v,
             in("dx") port,
             options(nomem, nostack, preserves_flags));
    }
    v
}

/// Svuota eventuali byte rimasti nel buffer (ACK dei LED, residui di boot).
/// Senza questo, il primo tasto letto sarebbe un residuo e non un tasto.
pub fn init() {
    for _ in 0..32 {
        if inb(STATUS) & OBF == 0 {
            break;
        }
        let _ = inb(DATA);
    }
}

/// Legge uno scancode se la tastiera ne ha uno pronto. Non blocca mai:
/// se non c'è nulla, ritorna None (il loop non deve mai fermarsi).
///
/// Salta gli ACK: il loop scrive i LED ogni 50 tick (heartbeat caps lock,
/// main.rs:1161) e OGNI comando genera un 0xFA nel buffer. Senza questo
/// filtro il primo "tasto" letto sarebbe l'ACK di un LED.
pub fn read() -> Option<u8> {
    while inb(STATUS) & OBF != 0 {
        let b = inb(DATA);
        match b {
            0xFA => continue, // ACK di un comando (LED) — non è un tasto
            0xAA => continue, // self-test ok
            0xE0 => continue, // prefisso esteso: il tasto vero è il byte dopo
            _ => return Some(b),
        }
    }
    None
}

/// Legge un byte dal controller 8042 e dice **di chi è**: `true` = mouse (AUX),
/// `false` = tastiera. Tastiera e touchpad condividono la porta 0x60: chi legge
/// DEVE smistare, altrimenti i pacchetti del mouse diventano scancode.
pub fn read_source() -> Option<(bool, u8)> {
    let st = inb(STATUS);
    if st & OBF == 0 { return None; }
    Some((st & 0x20 != 0, inb(DATA)))
}

/// Distingue una PRESSIONE da un RILASCIO.
///
/// Nel set 1 il rilascio è la pressione con il bit 7 alzato (0x80 | code).
/// Per il corpo conta il contatto: la pressione. Il rilascio è il contatto
/// che finisce — per ora lo si ignora, come una mano che si alza.
pub fn pressione(sc: u8) -> Option<u8> {
    if sc & 0x80 == 0 {
        Some(sc)
    } else {
        None
    }
}

/// Nome leggibile di alcuni tasti (set 1), per il registro e la telemetria.
/// Non serve a decidere nulla: serve a poter dire "mi ha toccato QUI".
pub fn nome(sc: u8) -> &'static str {
    match sc {
        0x01 => "ESC",
        0x1C => "INVIO",
        0x39 => "SPAZIO",
        0x0E => "BACKSPACE",
        0x2A | 0x36 => "MAIUSC",
        0x1D | 0xE0 => "CTRL",
        0x38 => "ALT",
        0x0F => "TAB",
        b if b >= 0x02 && b <= 0x0B => "1..0",
        b if b >= 0x10 && b <= 0x19 => "QWERTY",
        b if b >= 0x1E && b <= 0x26 => "ASDF",
        b if b >= 0x2C && b <= 0x32 => "ZXCV",
        _ => "?",
    }
}

#[inline]
fn outb(port: u16, val: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
    }
}

/// Attende che il buffer d'ingresso dell'8042 sia vuoto (bit 1 = 0). Non blocca mai.
fn wait_in() {
    for _ in 0..100_000 {
        if inb(STATUS) & 0x02 == 0 {
            return;
        }
    }
}

/// Abilita la tastiera e imposta la traduzione set 2 -> set 1.
///
/// MOLTI controller, al boot, hanno la tastiera DISABILITATA nel config byte
/// (bit 4 e bit 0). Senza questo non arriva NESSUNO scancode — e dall'esterno
/// sembra "la tastiera non funziona". Diagnosi resa visibile dalla telemetria
/// (campo `kb`) e dalla riga CONTATTO a schermo.
pub fn enable() {
    wait_in();
    outb(0x64, 0xAE); // enable keyboard interface
    wait_in();
    outb(0x64, 0x20); // comando: leggi config byte
    for _ in 0..100_000 {
        if inb(STATUS) & 0x01 != 0 {
            break;
        }
    }
    let mut cfg = inb(DATA);
    cfg |= 0x40; // bit6: traduzione set 2 -> set 1
    cfg &= !0x10; // bit4: keyboard ENABLED
    cfg &= !0x01; // bit0: interfaccia tastiera ENABLED
    wait_in();
    outb(0x64, 0x60); // comando: scrivi config byte
    wait_in();
    outb(DATA, cfg);
    wait_in();
    outb(0x64, 0xAE); // ri-abilita interfaccia
    init(); // svuota i residui
}

/// RILASCIO: nel set 1 il rilascio è la pressione col bit 7 alzato.
/// Ritorna il codice del tasto rilasciato (senza bit 7), oppure None.
pub fn rilascio(sc: u8) -> Option<u8> {
    if sc & 0x80 != 0 {
        Some(sc & 0x7F)
    } else {
        None
    }
}

/// Legge il registro di stato (0x64) — diagnostica: se è sempre 0xFF, il
/// controller i8042 non risponde (tastiera dietro l'EC o su USB).
pub fn read_status() -> u8 {
    inb(STATUS)
}

/// Legge il config byte dell'8042 (comando 0x20 su 0x64). 0xFF se non risponde.
pub fn read_config() -> u8 {
    wait_in();
    outb(0x64, 0x20);
    for _ in 0..100_000 {
        if inb(STATUS) & 0x01 != 0 {
            return inb(DATA);
        }
    }
    0xFF
}

/// Esito della sonda del controller PS/2, per la telemetria e la diagnosi.
/// `[status_prima, status_dopo_enable, config_byte]` — 0xFF ripetuto = assente.
static mut PS2_PROBE: [u8; 3] = [0xFF, 0xFF, 0xFF];

pub fn probe_store(a: u8, b: u8, c: u8) {
    unsafe {
        PS2_PROBE = [a, b, c];
    }
}

pub fn probe() -> [u8; 3] {
    unsafe { PS2_PROBE }
}

/// Legge un byte dal buffer dati (0x60) entro `limit` tentativi. 0xFF = nulla.
fn read_byte(limit: u32) -> u8 {
    for _ in 0..limit {
        if inb(STATUS) & 0x01 != 0 {
            return inb(DATA);
        }
    }
    0xFF
}

/// Esito dell'interrogazione della tastiera: [ACK, 0xAB, id0, id1].
static mut KBD_ID: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];

pub fn kbd_id() -> [u8; 4] {
    unsafe { KBD_ID }
}

/// Interroga la tastiera PS/2: read ID (0xF2 → ACK, 0xAB, id0, id1) e poi
/// **enable scanning** (0xF4). Se la tastiera risponde, il canale è vivo; se
/// era in "scanning off", con 0xF4 i tasti cominciano ad arrivare.
pub fn identify_and_enable() {
    for _ in 0..64 {
        if inb(STATUS) & 0x01 == 0 {
            break;
        }
        let _ = inb(DATA);
    }
    wait_in();
    outb(DATA, 0xF2); // Read ID
    let a = read_byte(500_000);
    let b = read_byte(500_000);
    let c = read_byte(500_000);
    let d = read_byte(500_000);
    unsafe { KBD_ID = [a, b, c, d]; }
    wait_in();
    outb(DATA, 0xF4); // Enable scanning
}
