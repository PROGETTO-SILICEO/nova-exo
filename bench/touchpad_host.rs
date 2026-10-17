// ── Banco host per il TATTO ESTESO (protocollo PS/2) ──
//
// Decodifica + assemblatore dei pacchetti del mouse: PURA, quindi si verifica
// su host col codice REALE (#[path] su src/ps2.rs). I segni stanno nei bit 4/5
// del primo byte: se li si sbaglia, il gesto va al contrario.
//
// Compilazione: make test   (oppure rustc --test bench/touchpad_host.rs)

#[path = "../src/ps2.rs"]
mod ps2;

use ps2::{decodifica, destinazione, Assemblatore, Porta};

fn p(flags: u8, x: u8, y: u8) -> ps2::Pacchetto { decodifica([0x08 | flags, x, y]) }

#[test]
fn fermo_non_si_muove() {
    let q = p(0x00, 0, 0);
    assert_eq!((q.dx, q.dy, q.bottoni), (0, 0, 0));
    assert!(!q.overflow);
}

#[test]
fn destra_e_basso_positivi() {
    let q = p(0x00, 10, 7);
    assert_eq!((q.dx, q.dy), (10, 7));
}

#[test]
fn sinistra_e_alto_negativi() {
    let q = p(0x10 | 0x20, 0xF6, 0xF9); // -10, -7
    assert_eq!((q.dx, q.dy), (-10, -7));
}

#[test]
fn i_bottoni_sono_i_bit_bassi() {
    assert_eq!(p(0x01, 0, 0).bottoni, 1);
    assert_eq!(p(0x02, 0, 0).bottoni, 2);
    assert_eq!(p(0x04, 0, 0).bottoni, 4);
    assert_eq!(p(0x03, 0, 0).bottoni, 3);
}

#[test]
fn l_overflow_e_segnalato() {
    assert!(p(0xC0, 0, 0).overflow);
    assert!(!p(0x00, 0, 0).overflow);
}

#[test]
fn l_assemblatore_ricompone_i_tre_byte() {
    let mut a = Assemblatore::new();
    assert!(a.byte(0x08).is_none());
    assert!(a.byte(10).is_none());
    let q = a.byte(7).expect("pacchetto completo al terzo byte");
    assert_eq!((q.dx, q.dy), (10, 7));
}

#[test]
fn l_assemblatore_rifiuta_la_fase_sbagliata() {
    // primo byte senza il bit 3 = fuori fase: NIENTE pacchetto inventato
    let mut a = Assemblatore::new();
    assert!(a.byte(0x00).is_none());
    assert!(a.byte(0).is_none());
    assert!(a.byte(0).is_none());
}

#[test]
fn l_assemblatore_concatena_piu_pacchetti() {
    let mut a = Assemblatore::new();
    let mut visti = 0;
    for b in [0x08, 1, 1, 0x08, 2, 2, 0x08, 3, 3] {
        if a.byte(b).is_some() { visti += 1; }
    }
    assert_eq!(visti, 3);
}

// ── Smistamento 8042 (status → porta) ─────────────────────────────────────
// Il difetto del 14/10: i byte del mouse finivano nella tastiera → CONTATTO
// saturo e "fuga" col touch. La decisione è pura, quindi si verifica qui.

#[test]
fn senza_byte_non_si_legge() {
    // OBF (bit0) = 0: niente da leggere, qualunque sia l'AUX.
    assert_eq!(destinazione(0x00), None);
    assert_eq!(destinazione(0x20), None);
}

#[test]
fn i_byte_della_tastiera_vanno_alla_tastiera() {
    // OBF=1, AUX=0 → scancode.
    assert_eq!(destinazione(0x01), Some(Porta::Tastiera));
    // Anche con altri bit sporchi (es. bit1 = input buffer), resta tastiera.
    assert_eq!(destinazione(0x05), Some(Porta::Tastiera));
}

#[test]
fn i_byte_del_mouse_vanno_al_mouse() {
    // OBF=1, AUX (bit5) = 1 → pacchetto del touchpad.
    assert_eq!(destinazione(0x21), Some(Porta::Mouse));
    assert_eq!(destinazione(0x25), Some(Porta::Mouse));
}

#[test]
fn aux_senza_obf_non_e_un_byte() {
    assert_eq!(destinazione(0x20), None);
}
