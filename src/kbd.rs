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
