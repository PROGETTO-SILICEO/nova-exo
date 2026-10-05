// ── Banco host per la porta d'ingresso di rete ──
//
// Perché esiste (05/10/2026): il cablaggio "pacchetto ricevuto → chemio_input"
// era legato a e1000. Sul 110-15ACL la NIC è Realtek e non aveva RX: la cella
// chemio restava a zero perché non entrava mai niente. La porta ora è
// condivisa (src/rx.rs). Questo banco esercita la parte PURA, senza toccare i
// globali: i test girano in parallelo e non devono litigare sullo stato.
//
// Test sul codice REALE (#[path] su src/rx.rs), mai su una copia.

#[path = "../src/rx.rs"]
mod rx;

use rx::{payload_to_chemio, CAP};

/// Costruisce un pacchetto di prova: header Ethernet di 14 byte + payload.
fn pkt(payload: &[u8]) -> ([u8; CAP], u16) {
    let mut d = [0u8; CAP];
    for (i, &b) in payload.iter().enumerate() {
        if 14 + i < CAP {
            d[14 + i] = b;
        }
    }
    (d, (14 + payload.len()) as u16)
}

#[test]
fn i_primi_quattro_byte_del_payload_diventano_chemio() {
    let (d, len) = pkt(&[0, 51, 102, 255]);
    let c = payload_to_chemio(len, &d);
    assert_eq!(c[0], 0.0);
    assert_eq!(c[1], 51.0 / 255.0);
    assert_eq!(c[2], 102.0 / 255.0);
    assert_eq!(c[3], 1.0);
}

#[test]
fn pacchetto_troppo_corto_non_inventa_valori() {
    // Solo l'header, niente payload: len = 14 -> nessun byte di payload.
    let (d, len) = pkt(&[]);
    assert_eq!(len, 14);
    assert_eq!(payload_to_chemio(len, &d), [0.0; 4]);
}

#[test]
fn pacchetto_parziale_riempie_solo_quello_che_c_e() {
    // Solo 2 byte di payload: i primi due valori sì, gli altri restano 0.
    let (d, len) = pkt(&[255, 0]);
    let c = payload_to_chemio(len, &d);
    assert_eq!(c[0], 1.0);
    assert_eq!(c[1], 0.0);
    assert_eq!(c[2], 0.0, "il terzo byte non esiste: deve restare 0");
    assert_eq!(c[3], 0.0);
}

#[test]
fn i_valori_stanno_in_zero_uno() {
    for b in [0u8, 1, 127, 254, 255] {
        let (d, len) = pkt(&[b, b, b, b]);
        let c = payload_to_chemio(len, &d);
        for v in c {
            assert!((0.0..=1.0).contains(&v), "valore fuori [0,1]: {}", v);
        }
    }
}

#[test]
fn la_porta_tiene_solo_sessantaquattro_byte() {
    // Un payload enorme: la porta non deve traboccare.
    let (mut d, _) = pkt(&[7u8; 200]);
    let len = 1000u16; // dichiarato più lungo del buffer
    let c = payload_to_chemio(len, &d);
    assert_eq!(c.len(), 4);
    // e nessun accesso fuori dal buffer (se ci fosse, il test andrebbe in panic
    // per indice fuori range: è il vero controllo)
    d[CAP - 1] = 0;
}
