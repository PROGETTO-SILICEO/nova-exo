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

// rx.rs (codice reale) usa crate::net::arp_request_for per l'ARP responder.
// Il banco include il net REALE (#[path]) + shim dei moduli kernel che net.rs
// consulta per la telemetria (cfc/kbd/acpi), che qui non esistono.
#[path = "../src/net.rs"]
mod net;
mod cfc {
    pub fn key_count() -> u32 { 0 }
    pub fn key_held() -> bool { false }
}
mod kbd {
    pub fn probe() -> [u8; 3] { [0xFF, 0xFF, 0xFF] }
    pub fn kbd_id() -> [u8; 4] { [0xFF, 0xFF, 0xFF, 0xFF] }
}
mod acpi {
    pub fn summary() -> [u32; 5] { [0, 0, 0, 0, 0] }
    pub fn ec_summary() -> [u32; 3] { [0, 0, 0] }
}

use rx::{
    desc_opts1, payload_to_chemio, CAP, DESC_OWN, DESC_RING_END, OFF_UDP_PAYLOAD,
    RX_DESC_BUF_SIZE,
};

/// Costruisce un pacchetto NON-IP (es. EtherType 0x88B5): payload a offset 14.
fn pkt(payload: &[u8]) -> ([u8; CAP], u16) {
    let mut d = [0u8; CAP];
    d[12] = 0x88;
    d[13] = 0xB5;
    for (i, &b) in payload.iter().enumerate() {
        if 14 + i < CAP {
            d[14 + i] = b;
        }
    }
    (d, (14 + payload.len()) as u16)
}

/// Costruisce un frame IPv4+UDP: il payload sta a offset 42, non 14.
fn pkt_udp(payload: &[u8]) -> ([u8; CAP], u16) {
    let mut d = [0u8; CAP];
    d[12] = 0x08; // EtherType IPv4
    d[13] = 0x00;
    d[14] = 0x45; // version/IHL
    d[23] = 17; // protocollo UDP
    for (i, &b) in payload.iter().enumerate() {
        if OFF_UDP_PAYLOAD + i < CAP {
            d[OFF_UDP_PAYLOAD + i] = b;
        }
    }
    (d, (OFF_UDP_PAYLOAD + payload.len()) as u16)
}

#[test]
fn ipv4_udp_legge_il_payload_non_l_intestazione() {
    // Il caso che ha bloccato tutto: prima si leggeva 0x45 0x00 (header IP),
    // identico per ogni pacchetto, quindi il corpo non sentiva mai il contenuto.
    let (d, len) = pkt_udp(&[255, 200, 150, 100]);
    let c = payload_to_chemio(len, &d);
    assert_eq!(c[0], 1.0);
    assert_eq!(c[1], 200.0 / 255.0);
    assert_eq!(c[2], 150.0 / 255.0);
    assert_eq!(c[3], 100.0 / 255.0);
    assert_ne!(c[0], 0x45 as f32 / 255.0, "sta leggendo l'header IP!");
}

#[test]
fn non_ip_legge_a_offset_14_come_prima() {
    let (d, len) = pkt(&[0, 51, 102, 255]);
    let c = payload_to_chemio(len, &d);
    assert_eq!(c[0], 0.0);
    assert_eq!(c[1], 51.0 / 255.0);
    assert_eq!(c[3], 1.0);
}

#[test]
fn ipv4_non_udp_torna_a_offset_14() {
    // TCP (proto 6): non e' UDP, si legge come prima invece di inventare.
    let mut d = [0u8; CAP];
    d[12] = 0x08;
    d[13] = 0x00;
    d[23] = 6;
    d[14] = 0x99;
    let c = payload_to_chemio(60, &d);
    assert_eq!(c[0], 0x99 as f32 / 255.0);
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

// ── Formato del descrittore RX ────────────────────────────────────────────
// Regressione del 05/10: l'init metteva la dimensione del buffer in opts1,
// il riarmo la dimenticava -> 4 pacchetti e poi il silenzio. Questi test
// bloccano quella classe di bug: la dimensione deve esserci SEMPRE.

#[test]
fn opts1_contiene_sempre_la_dimensione_del_buffer() {
    for n in [1usize, 2, 4, 256] {
        for i in 0..n {
            let o = desc_opts1(i, n);
            assert_eq!(o & 0xFFFF, RX_DESC_BUF_SIZE,
                       "descrittore {i}/{n}: manca la dimensione del buffer (opts1=0x{o:08x})");
        }
    }
}

#[test]
fn opts1_e_sempre_del_chip() {
    for n in [1usize, 4, 8] {
        for i in 0..n {
            assert_ne!(desc_opts1(i, n) & DESC_OWN, 0, "manca DescOwn su {i}/{n}");
        }
    }
}

#[test]
fn solo_l_ultimo_ha_ring_end() {
    let n = 4;
    for i in 0..n {
        let eor = desc_opts1(i, n) & DESC_RING_END != 0;
        assert_eq!(eor, i == n - 1, "RingEnd sbagliato su {i}/{n}");
    }
}

#[test]
fn opts1_ha_i_valori_attesi_dal_driver_linux() {
    // rtl8169_mark_to_asic: DescOwn | eor | 0x3FFF
    assert_eq!(desc_opts1(0, 4), 0x800007FF);
    assert_eq!(desc_opts1(3, 4), 0xC00007FF, "l'ultimo ha anche RingEnd");
}

#[test]
fn un_solo_descrittore_e_anche_l_ultimo() {
    assert_eq!(desc_opts1(0, 1), 0xC00007FF, "con n=1 e' sia primo sia ultimo");
}
