// ── Banco host per la telemetria UDP di Exo ──
//
// Perché esiste (05/10/2026): il `concept` e `imparato`/`esplorato` vivono
// SOLO sulla seriale. Per chiudere la domanda "il concetto si muove a 5
// milioni di tic?" senza che Alfonso trascriva righe dallo schermo, servono
// su rete. Ma in sandbox `no_new_privs` i frame Ethernet grezzi non si
// possono catturare: quindi un vero pacchetto IPv4/UDP, ricevibile con un
// socket normale.
//
// Test sul codice REALE (#[path] su src/net.rs), mai su una copia.
// Il costruttore è PURO: niente MMIO, niente unsafe → testabile qui.

#[path = "../src/net.rs"]
mod net;

use net::*;

const SRC_MAC: [u8; 6] = [0xc8, 0x5b, 0x76, 0xe0, 0xe0, 0x01];
const OFF_PAYLOAD: usize = 14 + 20 + 8; // eth + ip + udp
const DIAG: [u32; 8] = [11, 0, 0x870E, 2048, 0x0C, 0xF, 0, 0];
/// (c, u, p, n) — p = -0.91 come misurato sul corpo in attrattore negativo.
const CHEMIO: [f32; 4] = [0.05, 0.12, -0.91, 0.30];
/// Firma `SEMP`: i 4 byte che la chemio di Exo legge dall'ultimo pacchetto.
const SIG: [u8; 4] = *b"SEMP";

// ── Shim dei moduli del kernel che net.rs consulta per la telemetria ──
// net.rs (codice reale) legge contatori da cfc/kbd/acpi. Nel banco host quei
// moduli non esistono → valori noti e deterministici. Il banco verifica il
// FORMATO del frame, non i valori del corpo (quelli li prova il metallo).
mod cfc {
    pub fn key_count() -> u32 { 7 }
    pub fn key_held() -> bool { false }
}
mod kbd {
    pub fn probe() -> [u8; 3] { [0x14, 0x14, 0x45] }
    pub fn kbd_id() -> [u8; 4] { [0xFA, 0xAB, 0x83, 0xAB] }
}
mod acpi {
    pub fn summary() -> [u32; 5] { [1, 2, 37, 0x1000, 0x2000] }
    pub fn ec_summary() -> [u32; 3] { [1, 0xFF, 2] }
}

fn cell(v: f32) -> [f32; 16] {
    let mut c = [0.0f32; 16];
    let mut i = 0;
    while i < 16 {
        c[i] = v + i as f32;
        i += 1;
    }
    c
}

/// Costruisce un frame di prova e ritorna (lunghezza, buffer).
fn frame() -> (usize, [u8; 1518]) {
    let mut buf = [0u8; 1518];
    let cells = [cell(0.0), cell(-0.5), cell(1.25), cell(0.0)];
    let n = build_state_frame(
        &mut buf, SRC_MAC, 12345, "riposo", &cells, &CHEMIO, 2, 0.5, 3, 7, DIAG, SIG, 5,
    );
    (n, buf)
}

fn payload(buf: &[u8], n: usize) -> &str {
    core::str::from_utf8(&buf[OFF_PAYLOAD..n]).expect("payload utf8")
}

// ─────────────────────────── checksum IPv4 ───────────────────────────

#[test]
fn checksum_di_header_a_zeri_e_ffff() {
    let hdr = [0u8; 20];
    assert_eq!(ipv4_checksum(&hdr), 0xFFFF);
}

#[test]
fn checksum_su_vettore_noto() {
    // Header IPv4 con campo checksum azzerato.
    // Somma dei 16-bit: 0x4500+0x0073+0x0000+0x4000+0x4011+0x0000
    //                 +0xc0a8+0x0001+0xc0a8+0x00c7 = 0x2479c
    // fold -> 0x479e ; complemento a uno -> 0xb861
    let hdr = [
        0x45, 0x00, 0x00, 0x73, 0x00, 0x00, 0x40, 0x00, 0x40, 0x11,
        0x00, 0x00, 0xc0, 0xa8, 0x00, 0x01, 0xc0, 0xa8, 0x00, 0xc7,
    ];
    assert_eq!(ipv4_checksum(&hdr), 0xb861);
}

// ─────────────────────── formattazione numerica ───────────────────────

fn fmt_u32(n: u32) -> String {
    let mut b = [0u8; 24];
    let mut p = 0;
    write_u32_buf(&mut b, &mut p, n);
    String::from_utf8(b[..p].to_vec()).unwrap()
}

fn fmt_f32(v: f32) -> String {
    let mut b = [0u8; 32];
    let mut p = 0;
    write_f32_buf(&mut b, &mut p, v);
    String::from_utf8(b[..p].to_vec()).unwrap()
}

#[test]
fn formatta_interi() {
    assert_eq!(fmt_u32(0), "0");
    assert_eq!(fmt_u32(7), "7");
    assert_eq!(fmt_u32(100), "100");
    assert_eq!(fmt_u32(4_294_967_295), "4294967295");
}

#[test]
fn formatta_f32_con_quattro_decimali() {
    assert_eq!(fmt_f32(1.0), "1.0000");
    assert_eq!(fmt_f32(-0.1942), "-0.1942");
    assert_eq!(fmt_f32(0.0), "0.0000");
    assert_eq!(fmt_f32(0.5), "0.5000");
    assert_eq!(fmt_f32(12.3456), "12.3456");
}

// ───────────────────────── struttura del frame ─────────────────────────

#[test]
fn frame_e_un_ipv4() {
    let (n, b) = frame();
    assert!(n > 0, "frame non costruito");
    assert_eq!(u16::from_be_bytes([b[12], b[13]]), 0x0800, "ethertype IPv4");
}

#[test]
fn destinazione_e_broadcast() {
    let (_n, b) = frame();
    assert_eq!(&b[0..6], &[0xFF; 6], "MAC di destinazione broadcast");
    assert_eq!(&b[30..34], &DST_IP, "IP di destinazione = broadcast di rete");
}

#[test]
fn sorgente_e_quella_passata() {
    let (_n, b) = frame();
    assert_eq!(&b[6..12], &SRC_MAC);
}

#[test]
fn header_ip_corretto() {
    let (_n, b) = frame();
    assert_eq!(b[14], 0x45, "version 4 + IHL 5");
    assert_eq!(b[23], 17, "protocollo UDP");
    assert_eq!(b[22], 64, "TTL");
}

#[test]
fn lunghezze_coerenti() {
    let (n, b) = frame();
    let ip_len = u16::from_be_bytes([b[16], b[17]]) as usize;
    let udp_len = u16::from_be_bytes([b[38], b[39]]) as usize;
    let pl = n - OFF_PAYLOAD;

    assert_eq!(ip_len, 20 + 8 + pl, "total length IP = 20 + 8 + payload");
    assert_eq!(udp_len, 8 + pl, "length UDP = 8 + payload");
    assert!(n >= 60, "frame sotto il minimo Ethernet: {}", n);
    assert!(n <= MAX_FRAME, "frame oltre il massimo Ethernet: {}", n);
}

#[test]
fn porta_udp_giusta() {
    let (_n, b) = frame();
    assert_eq!(u16::from_be_bytes([b[34], b[35]]), UDP_PORT, "porta sorgente");
    assert_eq!(u16::from_be_bytes([b[36], b[37]]), UDP_PORT, "porta destinazione");
}

#[test]
fn checksum_ip_valido() {
    let (_n, b) = frame();
    // Proprietà dell'header IPv4: ricalcolando il checksum su un header che
    // GIÀ contiene il suo checksum, la somma in complemento a uno viene
    // 0xFFFF e il complemento (cioè il valore ritornato) viene 0.
    // (L'header tutto a zero dà 0xFFFF: somma 0, complemento 0xFFFF.)
    assert_eq!(ipv4_checksum(&b[14..34]), 0, "checksum IP non valido");
}

// ──────────────────────────── payload JSON ────────────────────────────

#[test]
fn payload_porta_tick_e_concept() {
    let (n, b) = frame();
    let p = payload(&b, n);
    assert!(p.contains("\"t\":12345"), "manca il tick: {}", p);
    assert!(p.contains("\"k\":\"riposo\""), "manca il concept: {}", p);
}

#[test]
fn payload_porta_i_quattro_tessuti_interi() {
    let (n, b) = frame();
    let p = payload(&b, n);
    assert!(p.contains("\"c\":[["), "manca il vettore dei tessuti: {}", p);
    // NEURONS_PER_CELL = 16: il payload deve portare la cella INTERA.
    // Una cella è v + 0..15, quindi deve arrivare fino a 15.0000.
    assert!(
        p.contains("[0.0000,1.0000,2.0000,3.0000,4.0000,5.0000,6.0000,7.0000,"),
        "inizio cella malformato: {}", p
    );
    assert!(p.contains(",14.0000,15.0000]"), "cella troncata (manca 14/15): {}", p);
    // Array nel payload (13): "c" = 1 esterno + 4 celle, poi w, l, r,
    // kb, ps2, acpi, ec, kbd. (Aggiornato 13/10/2026: il payload è cresciuto
    // con le sonde kb/ps2/acpi/ec/kbd del 12/10 — l'attesa era rimasta a 8.)
    assert_eq!(p.matches('[').count(), 13, "numero di array inatteso: {}", p);
    assert_eq!(p.matches(']').count(), 13, "parentesi non bilanciate: {}", p);
    assert!(p.contains("-0.5000"), "manca un valore negativo atteso: {}", p);
}

#[test]
fn payload_porta_volonta_e_apprendimento() {
    let (n, b) = frame();
    let p = payload(&b, n);
    assert!(p.contains("\"w\":[2,0.5000]"), "manca la volontà: {}", p);
    assert!(p.contains("\"l\":[3,7]"), "mancano imparato/esplorato: {}", p);
}

#[test]
fn payload_porta_la_diagnostica_rx() {
    // Sul metallo la seriale non esiste: se l'RX non funziona, la sola
    // differenza fra "porta chiusa" e "rete silenziosa" è questo campo.
    let (n, b) = frame();
    let p = payload(&b, n);
    assert!(p.contains("\"r\":[11,0,34574,2048,12,15,0,0]"), "manca la diagnostica RX: {}", p);
    assert!(p.contains("\"v\":5"), "manca l'epoca di build: {}", p);
}

#[test]
fn payload_e_una_sola_riga_json() {
    let (n, b) = frame();
    let p = payload(&b, n);
    assert!(p.starts_with('{') && p.ends_with('}'), "JSON non delimitato: {}", p);
    assert!(!p.contains('\n'), "il payload non deve avere newline");
}

// ───────────────────────────── casi limite ─────────────────────────────

#[test]
fn buffer_troppo_piccolo_non_scrive() {
    let mut small = [0u8; 40];
    let cells = [cell(0.0); 4];
    let n = build_state_frame(
        &mut small, SRC_MAC, 1, "vita", &cells, &CHEMIO, 0, 0.0, 0, 0, DIAG, SIG, 5,
    );
    assert_eq!(n, 0, "con buffer insufficiente deve tornare 0, non scrivere");
}

#[test]
fn concept_diversi_sono_riportati() {
    // net.rs NON conosce i concetti: trasporta solo una stringa. La lista
    // dei quattro nomi vive nell'interprete; qui serve solo a provare che
    // stringhe diverse passino intatte.
    for nome in ["errore", "vita", "riposo", "novita"].iter() {
        let mut buf = [0u8; 1518];
        let cells = [cell(0.0); 4];
        let n = build_state_frame(&mut buf, SRC_MAC, 1, nome, &cells, &CHEMIO, 0, 0.0, 0, 0, DIAG, SIG, 5);
        let p = payload(&buf, n);
        assert!(p.contains(nome), "concept '{}' non riportato: {}", nome, p);
    }
}

#[test]
fn json_porta_la_chemio_interpretata() {
    let (n, buf) = frame();
    let p = payload(&buf, n);
    // I tre parametri vitali che il pediatra oggi DEDUCE devono essere MISURATI.
    for campo in ["\"u\":", "\"p\":", "\"n\":"] {
        assert!(p.contains(campo), "campo {} assente nel JSON: {}", campo, p);
    }
    // Il valore di p deve essere quello passato (-0.91), non un segnaposto.
    assert!(p.contains("-0.91") || p.contains("-0.9100"),
            "la polarita' non e' riportata: {}", p);
}
