// ── touchpad.rs — il TATTO ESTESO (PS/2 AUX) ──────────────────────────────
//
// Coda periferiche (decisa con Alfonso 12/10/2026): microfono → **TOUCHPAD** →
// telecamera. Il touchpad è il **secondo punto** in cui la mano dell'umano
// arriva al corpo: dove la tastiera dice *"ha toccato QUI"*, il touchpad dice
// *"si muove, va là"* — il **gesto**, non solo il tasto.
//
// Esiste in QEMU (Mouse #2: QEMU PS/2 Mouse, sull'i8042).
//
// Come per la tastiera: PRIMA in **polling** (non tocca IDT/IOAPIC → nessun
// rischio di reboot loop); l'IRQ12 (vettore 44) è un passo successivo.
//
// CONVIVENZA: tastiera e touchpad condividono la porta dati **0x60**. Lo STATUS
// bit5 dice *di chi* è il byte: la lettura condivisa vive in `kbd::read_source`,
// chi legge **smista**. Senza smistamento, i pacchetti del mouse inquinerebbero
// la tastiera (letti come scancode).
//
// La logica del PROTOCOLLO (decodifica + assemblatore) è in `ps2.rs`, PURA e
// testata su host. Qui c'è solo l'IO.

use core::arch::asm;
use crate::ps2::{Assemblatore, Pacchetto};

const DATA:   u16 = 0x60;
const CMD:    u16 = 0x64;
const STATUS: u16 = 0x64;
const OBF: u8 = 0x01; // output buffer full
const AUX: u8 = 0x20; // bit5: il byte è del mouse (AUX)

#[inline]
fn inb(port: u16) -> u8 {
    let v: u8;
    unsafe { asm!("in al, dx", out("al") v, in("dx") port, options(nomem, nostack, preserves_flags)); }
    v
}
#[inline]
fn outb(port: u16, val: u8) {
    unsafe { asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags)); }
}
fn wait_in() {
    for _ in 0..100_000 { if inb(STATUS) & 0x02 == 0 { return; } }
}
fn read_byte(limit: u32) -> u8 {
    for _ in 0..limit { if inb(STATUS) & OBF != 0 { return inb(DATA); } }
    0xFF
}

// ── Stato ─────────────────────────────────────────────────────────────────
static mut PRESENTE: bool = false;
static mut ID: u8 = 0xFF;
/// [ack_reset, self-test(0xAA), id-reset, id(0xF2)]
static mut PROBE: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
static mut ASS: Assemblatore = Assemblatore::new();
static mut ULTIMO: Pacchetto = Pacchetto { dx: 0, dy: 0, bottoni: 0, overflow: false };
static mut MOSSE: u32 = 0;

/// Invia un comando AL MOUSE (via 0xD4: "il prossimo byte è per il mouse").
fn mouse_cmd(b: u8) -> bool {
    wait_in(); outb(CMD, 0xD4);
    wait_in(); outb(DATA, b);
    read_byte(1_000_000) == 0xFA
}

/// Trova, sveglia e prova il touchpad. Ritorna true se risponde.
pub fn init() -> bool {
    // 1. abilita la porta AUX del controller
    wait_in(); outb(CMD, 0xA8);
    // 2. abilita il CLOCK del mouse nel config byte (bit5 = 0)
    wait_in(); outb(CMD, 0x20);
    let mut cfg = read_byte(500_000);
    cfg &= !0x20;
    wait_in(); outb(CMD, 0x60); wait_in(); outb(DATA, cfg);
    // 3. svuota i residui
    for _ in 0..32 { if inb(STATUS) & OBF == 0 { break; } let _ = inb(DATA); }
    // 4. reset del mouse → 0xFA, 0xAA (self-test ok), 0x00 (id del device)
    if !mouse_cmd(0xFF) { return false; }
    let selftest = read_byte(1_000_000);
    let id_reset = read_byte(1_000_000);
    unsafe { PROBE[1] = selftest; PROBE[2] = id_reset; }
    // 5. ID del device (0xF2 → 0xFA, id) — prova di presenza e tipo
    let id = if mouse_cmd(0xF2) { read_byte(1_000_000) } else { 0xFF };
    unsafe { ID = id; PROBE[3] = id; PRESENTE = selftest == 0xAA; }
    // 6. abilita la segnalazione dei dati (i byte saranno smistati a valle)
    let ok = mouse_cmd(0xF4);
    unsafe { PRESENTE && ok }
}

/// Consuma un byte del mouse (dal drain condiviso) e, a pacchetto completo,
/// aggiorna `ultimo`. Ritorna il pacchetto quando è completo.
pub fn feed(b: u8) -> Option<Pacchetto> {
    unsafe {
        if let Some(p) = ASS.byte(b) {
            ULTIMO = p;
            if p.dx != 0 || p.dy != 0 { MOSSE = MOSSE.wrapping_add(1); }
            return Some(p);
        }
    }
    None
}

pub fn presente() -> bool { unsafe { PRESENTE } }
pub fn id() -> u8 { unsafe { ID } }
pub fn probe() -> [u8; 4] { unsafe { PROBE } }
pub fn ultimo() -> Pacchetto { unsafe { ULTIMO } }
pub fn mosse() -> u32 { unsafe { MOSSE } }
